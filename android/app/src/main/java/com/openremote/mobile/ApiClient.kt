package com.openremote.mobile

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.isActive
import kotlinx.coroutines.withContext
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withTimeout
import okhttp3.Call
import okhttp3.Callback
import okhttp3.HttpUrl
import okhttp3.HttpUrl.Companion.toHttpUrlOrNull
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody
import okio.BufferedSink
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.Response
import org.json.JSONArray
import org.json.JSONObject
import java.io.IOException
import java.time.Instant
import java.util.Base64
import java.util.concurrent.TimeUnit
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException

class ApiException(val status: Int, val code: String) : IOException(code.replace('_', ' '))

/** All credentials stay in memory and every request is cancellable with its owning coroutine. */
class ApiClient(
    endpoint: String,
    private val identity: ChallengeSigner,
    private val http: OkHttpClient = OkHttpClient.Builder()
        .connectTimeout(10, TimeUnit.SECONDS).readTimeout(30, TimeUnit.SECONDS)
        .callTimeout(35, TimeUnit.SECONDS).followRedirects(false).followSslRedirects(false).build(),
    private val now: () -> Instant = Instant::now,
    allowLoopbackHttpForTest: Boolean = false,
) {
    private val base: HttpUrl = validateEndpoint(endpoint, allowLoopbackHttpForTest)
    private val singleAttemptHttp = http.newBuilder().retryOnConnectionFailure(false).build()
    private val authMutex = Mutex()
    private var access: ControllerAccess? = null

    private suspend fun request(method: String, path: String, token: String? = null, body: JSONObject? = null): JSONObject =
        requestDecoded(method, path, token, body) { bytes ->
            val text = String(bytes, Charsets.UTF_8)
            if (text.isBlank()) JSONObject() else JSONObject(text)
        }

    private suspend fun <T> requestDecoded(method: String, path: String, token: String? = null, body: JSONObject? = null,
                                           maxBytes: Int = 1_048_576, oneShot: Boolean = false, decode: (ByteArray) -> T): T {
        val url = requireNotNull(base.resolve(path))
        val request = Request.Builder().url(url).header("Accept", "application/json")
            .apply { token?.let { header("Authorization", "Bearer $it") } }
            .method(method, if (method == "POST") {
                val encoded = (body?.toString() ?: "{}").toRequestBody("application/json; charset=utf-8".toMediaType())
                if (oneShot) object : RequestBody() {
                    override fun contentType() = encoded.contentType()
                    override fun contentLength() = encoded.contentLength()
                    override fun isOneShot() = true
                    override fun writeTo(sink: BufferedSink) = encoded.writeTo(sink)
                } else encoded
            } else null).build()
        val call = (if (oneShot) singleAttemptHttp else http).newCall(request)
        return suspendCancellableCoroutine { continuation ->
            continuation.invokeOnCancellation { call.cancel() }
            call.enqueue(object : Callback {
                override fun onFailure(call: Call, e: IOException) {
                    if (continuation.isActive) continuation.resumeWithException(e)
                }
                override fun onResponse(call: Call, response: Response) {
                    response.use {
                        try {
                            val source = response.body?.source() ?: throw IOException("Empty server response")
                            val buffer = okio.Buffer()
                            var remaining = maxBytes.toLong() + 1
                            while (remaining > 0) {
                                val n = source.read(buffer, minOf(remaining, 16_384))
                                if (n < 0) break
                                remaining -= n
                            }
                            if (buffer.size > maxBytes) throw IOException("Server response too large")
                            val bytes = buffer.readByteArray()
                            if (!response.isSuccessful) {
                                val code = runCatching { JSONObject(String(bytes, Charsets.UTF_8)).optString("error") }.getOrNull()
                                // Remote text is never displayed as an error, including HTML proxy responses.
                                throw ApiException(response.code, code?.takeIf { Regex("[a-z][a-z0-9_]{0,63}").matches(it) } ?: "http_error")
                            }
                            val decoded = decode(bytes)
                            if (continuation.isActive) continuation.resume(decoded)
                        } catch (e: Exception) {
                            if (continuation.isActive) continuation.resumeWithException(e)
                        }
                    }
                }
            })
        }
    }

    private suspend fun controllerToken(forceRefresh: Boolean = false): String = authMutex.withLock {
        if (!forceRefresh) access?.takeIf { it.expiresAt.isAfter(now().plusSeconds(30)) }?.let { return@withLock it.token }
        val challenge = request("POST", "/v1/auth/challenge", body = JSONObject()
            .put("publicKey", identity.publicKeyBase64()).put("algorithm", "p256"))
        val nonce = Base64.getDecoder().decode(challenge.getString("challenge"))
        require(nonce.size in 16..512) { "Invalid server challenge" }
        val verified = request("POST", "/v1/auth/verify", body = JSONObject()
            .put("challengeId", challenge.getString("challengeId"))
            .put("signature", identity.signChallenge(nonce)))
        val value = ControllerAccess(verified.getString("accessToken"), Instant.parse(verified.getString("expiresAt")))
        require(value.token.isNotBlank() && value.expiresAt.isAfter(now())) { "Expired authentication response" }
        access = value
        value.token
    }

    suspend fun serverInfo(): ServerInfo = requestDecoded("GET", "/v1/info", maxBytes = 16_384, decode = ServerInfo::decode)
    /** An explicit connection check signs a fresh challenge even when an earlier bearer is cached. */
    suspend fun verifyIdentity() { controllerToken(forceRefresh = true) }

    private suspend fun authenticated(method: String, path: String, body: JSONObject? = null): JSONObject {
        val token = controllerToken()
        return try { request(method, path, token, body) } catch (e: ApiException) {
            if (e.status != 401) throw e
            authMutex.withLock { if (access?.token == token) access = null }
            request(method, path, controllerToken(), body)
        }
    }

    suspend fun lookup(displayId: String): RemoteDevice {
        val id = normalizePcId(displayId)
        val j = authenticated("GET", "/v1/devices/$id")
        require(j.get("displayId") == id && j.get("online") is Boolean) { "The server returned a different or invalid PC" }
        val name = j.getString("name")
        val platform = j.getString("platform")
        require(name.toByteArray(Charsets.UTF_8).size in 1..80 && name.none(Character::isISOControl) &&
            platform.length in 1..32 && platform.all { it.code in 33..126 }) { "Invalid PC details" }
        return RemoteDevice(id, name, platform, j.getBoolean("online"), j.optString("lastSeen"))
    }

    suspend fun createSession(displayId: String, controllerName: String, permissions: PermissionSet, monitorId: String? = null): RemoteSession {
        require(monitorId == null || DisplayProtocol.validId(monitorId)) { "Invalid selected display" }
        val j = authenticated("POST", "/v1/sessions", JSONObject()
            .put("displayId", normalizePcId(displayId)).put("controllerName", controllerName.take(80))
            .put("permissions", permissions.toJson()).apply { monitorId?.let { put("monitorId", it) } })
        val s = j.getJSONObject("session")
        val session = RemoteSession(s.getString("id"), s.getString("status"), j.getString("controllerToken"), PermissionSet.fromJson(s.getJSONObject("permissions")), monitorId)
        try {
            require(responseMonitor(s) == monitorId) { "The server did not confirm the selected display" }
        } catch (e: Exception) {
            // A legacy or inconsistent coordinator must not leave a silently primary-bound request active.
            runCatching { withTimeout(5_000) { end(session) } }
            throw e
        }
        return session
    }

    /**
     * A one-use redemption is never automatically replayed, including by OkHttp. Once sent,
     * settle its bounded response even if the UI cancels so any known late session can be ended.
     * A lost response cannot be recovered from the code: the owner must generate another.
     */
    suspend fun redeemPairing(code: String, pin: String, controllerName: String): PairedConnection {
        PairingInput.validate(code, pin)
        require(controllerName.toByteArray(Charsets.UTF_8).size in 1..80 &&
            controllerName.trim() == controllerName && controllerName.none(Character::isISOControl)) { "Invalid controller name" }
        val info = serverInfo()
        if (info.pairingCodeVersion != 1 || (code.length == 9 && info.activationVersion != 1)) throw PairingNotSupportedException()
        val token = controllerToken(forceRefresh = true)
        val owner = currentCoroutineContext()
        owner.ensureActive()
        return withContext(NonCancellable) {
            owner.ensureActive()
            var provisional: RemoteSession? = null
            try {
                val j = try {
                    requestDecoded("POST", "/v1/pairings/redeem", token,
                        JSONObject().put("code", code).put("pin", pin).put("controllerName", controllerName),
                        maxBytes = 16_384, oneShot = true) { bytes ->
                        StrictToolJson.parse(bytes)
                        JSONObject(String(bytes, Charsets.UTF_8))
                    }
                } catch (e: ApiException) { throw e }
                catch (e: Exception) { throw PairingResultUnknownException(e) }
                val raw = j.getJSONObject("session")
                val id = raw.getString("id")
                val bearer = j.getString("controllerToken")
                require(PairingInput.safeId(id) && bearer.length in 32..128 && bearer.all { it in 'a'..'z' || it in 'A'..'Z' || it in '0'..'9' || it in "-_" }) { "Invalid pairing session" }
                // Capture only path-safe cleanup credentials before validating the rest of the grant.
                provisional = RemoteSession(id, "pending", bearer, PermissionSet(controlInput = false))
                val permission = raw.getJSONObject("permissions")
                require(permission.keys().asSequence().toSet() == setOf("viewScreen", "controlInput", "clipboard", "fileTransfer", "audio", "power") &&
                    permission.keys().asSequence().all { permission.get(it) is Boolean }) { "Invalid pairing permissions" }
                val granted = PermissionSet.fromJson(permission)
                require(granted.viewScreen && !granted.clipboard && !granted.fileTransfer && !granted.audio && !granted.power) { "Invalid pairing permission ceiling" }
                val pairingId = raw.getString("pairingId")
                require(PairingInput.safeId(pairingId) && raw.getString("status") == "pending" && responseMonitor(raw) == null) { "Invalid pairing grant" }
                val hostId = raw.getString("hostDisplayId")
                require(hostId.length == 9 && hostId.all { it in '0'..'9' }) { "Invalid host identity" }
                val activation = raw.has("mode")
                if (activation) require(raw.get("mode") == "activation") { "Invalid session mode" }
                if (code.length == 9) require(activation && hostId == code) { "The server returned a different PC" }
                require(Base64.getDecoder().decode(raw.getString("hostPublicKey")).size == 32) { "Invalid host key" }
                val name = raw.getString("hostName")
                require(name.toByteArray(Charsets.UTF_8).size in 1..80 && name.trim() == name && name.none(Character::isISOControl)) { "Invalid host name" }
                val session = provisional.copy(permissions = granted, pairingId = pairingId, activation = activation, hostDisplayId = hostId)
                if (!owner.isActive) throw CancellationException("Pairing request cancelled")
                PairedConnection(RemoteDevice(hostId, name, "windows", true, ""), session)
            } catch (e: Exception) {
                provisional?.let { runCatching { withTimeout(5_000) { end(it) } } }
                throw e
            }
        }
    }

    suspend fun sessionStatus(session: RemoteSession): SessionSnapshot {
        val j = request("GET", "/v1/sessions/${session.id}", session.controllerToken).getJSONObject("session")
        require(responseMonitor(j) == session.monitorId) { "The server changed the selected display during the session" }
        if (session.pairingId != null) {
            require(j.optString("pairingId") == session.pairingId) { "The server changed the pairing grant" }
            require(if (session.activation) j.opt("mode") == "activation" else !j.has("mode")) { "The server changed the connection mode" }
            if (session.activation) require(j.optString("hostDisplayId") == session.hostDisplayId) { "The server changed the connected PC" }
            val permissions = PermissionSet.fromJson(j.getJSONObject("permissions"))
            require(permissions.viewScreen && (!permissions.controlInput || session.permissions.controlInput) &&
                !permissions.clipboard && !permissions.fileTransfer && !permissions.audio && !permissions.power) { "The server expanded the pairing permissions" }
        }
        return SessionSnapshot(j.getString("status"), PermissionSet.fromJson(j.getJSONObject("permissions")))
    }
    private fun responseMonitor(value: JSONObject): String? {
        if (!value.has("monitorId")) return null
        val raw = value.get("monitorId")
        require(raw is String) { "The server returned an invalid display identity" }
        val id = raw
        require(DisplayProtocol.validId(id)) { "The server returned an invalid display identity" }
        return id
    }
    suspend fun sendSignal(session: RemoteSession, kind: String, payload: JSONObject) {
        require(kind in setOf("offer", "answer", "ice", "renegotiate"))
        require(payload.toString().toByteArray().size <= 262_144)
        request("POST", "/v1/sessions/${session.id}/signal", session.controllerToken, JSONObject().put("kind", kind).put("payload", payload))
    }
    suspend fun pollSignals(session: RemoteSession, after: Long): List<SignalMessage> {
        require(after >= 0)
        val a = request("GET", "/v1/sessions/${session.id}/signals?after=$after", session.controllerToken).optJSONArray("signals") ?: JSONArray()
        return (0 until a.length()).map { i -> a.getJSONObject(i).let { j ->
            SignalMessage(j.getLong("seq"), j.getString("from"), j.getString("kind"), j.getJSONObject("payload"))
        } }
    }
    suspend fun iceConfiguration(session: RemoteSession): IceConfiguration {
        val j = request("GET", "/v1/sessions/${session.id}/ice", session.controllerToken)
        val a = j.getJSONArray("iceServers")
        val servers = (0 until a.length()).map { i ->
            val s = a.getJSONObject(i)
            val urls = when (val v = s.get("urls")) {
                is JSONArray -> (0 until v.length()).map { v.getString(it) }
                is String -> listOf(v)
                else -> error("Invalid ICE server list")
            }
            require(urls.all { it.startsWith("stun:") || it.startsWith("stuns:") || it.startsWith("turn:") || it.startsWith("turns:") }) { "Unsupported ICE URL" }
            IceServerConfig(urls, s.optString("username"), s.optString("credential"))
        }
        return IceConfiguration(servers, j.optString("iceTransportPolicy") == "relay",
            j.optString("expiresAt").takeIf { it.isNotEmpty() }?.let(Instant::parse))
    }
    suspend fun end(session: RemoteSession) {
        try { request("POST", "/v1/sessions/${session.id}/end", session.controllerToken) }
        catch (e: ApiException) { if (e.status != 404 && e.status != 410 && e.status != 409) throw e }
    }

    companion object {
        fun validateEndpoint(value: String, allowLoopbackHttpForTest: Boolean = false): HttpUrl {
            val url = value.trim().toHttpUrlOrNull() ?: throw IllegalArgumentException("Enter a valid HTTPS server address")
            val loopbackTest = allowLoopbackHttpForTest && url.scheme == "http" && url.host in setOf("127.0.0.1", "localhost", "::1")
            require(url.isHttps || loopbackTest) { "The server must use HTTPS" }
            require(url.username.isEmpty() && url.password.isEmpty() && url.query == null && url.fragment == null) { "Server address must not contain credentials, a query, or a fragment" }
            require(url.encodedPath == "/") { "Enter the server origin without a path" }
            return url
        }
        fun normalizePcId(value: String): String {
            require(value.all { it in '0'..'9' || it == ' ' || it == '-' }) { "PC ID must contain nine digits" }
            val id = value.filter { it in '0'..'9' }
            require(id.length == 9) { "PC ID must contain nine digits" }
            return id
        }
    }
}
