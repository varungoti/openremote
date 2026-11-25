package com.openremote.mobile

import java.io.ByteArrayOutputStream
import java.net.URI
import java.nio.ByteBuffer
import java.nio.charset.CodingErrorAction
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/** A locator only. Importing one never grants access, trusts a phone, or starts a request. */
data class PhoneSetup(val endpoint: String, val pcId: String)

internal object PhoneSetupLink {
    const val MAX_BYTES = 2048
    fun parse(value: String): PhoneSetup {
        require(value.length in 1..MAX_BYTES && value.all { it.code in 33..126 }) { "Use the complete OpenRemote setup link, without spaces or line breaks." }
        val uri = try { URI(value) } catch (_: Exception) { throw IllegalArgumentException("The setup link is not a valid URL.") }
        require(uri.scheme == "openremote" && uri.rawAuthority == "connect" && uri.rawPath.isNullOrEmpty() && uri.rawFragment == null) { "Use an openremote://connect setup link from your PC." }
        val fields = mutableMapOf<String, String>()
        val query = requireNotNull(uri.rawQuery) { "The setup link is missing its fields." }
        query.split('&').forEach { part ->
            val at = part.indexOf('=')
            require(at > 0 && at == part.lastIndexOf('=') && at < part.lastIndex) { "The setup link has an invalid field." }
            val name = decode(part.substring(0, at))
            val content = decode(part.substring(at + 1))
            require(name in setOf("v", "server", "pc") && fields.put(name, content) == null) { "The setup link has unknown or repeated fields." }
        }
        require(fields.keys == setOf("v", "server", "pc") && fields["v"] == "1") { "This setup link version is not supported." }
        val server = fields.getValue("server")
        require(server.isNotEmpty() && server.all { it.code in 33..126 } && '\\' !in server && '%' !in server) { "The setup link must contain an ASCII HTTPS server origin." }
        val origin = try { URI(server) } catch (_: Exception) { throw IllegalArgumentException("The setup link has an invalid server origin.") }
        require(origin.scheme == "https" && !origin.rawAuthority.isNullOrEmpty() && origin.host != null && origin.rawUserInfo == null &&
            origin.rawPath in setOf("", "/") && origin.rawQuery == null && origin.rawFragment == null) { "The setup link must contain an HTTPS origin without credentials or a path." }
        val endpoint = ApiClient.validateEndpoint(server).toString().trimEnd('/')
        val pc = fields.getValue("pc")
        require(pc.length == 9 && pc.all { it in '0'..'9' }) { "The setup link must contain a nine-digit PC number." }
        return PhoneSetup(endpoint, pc)
    }

    private fun decode(value: String): String {
        val out = ByteArrayOutputStream(value.length)
        var i = 0
        while (i < value.length) {
            val char = value[i++]
            if (char == '%') {
                require(i + 1 < value.length) { "The setup link contains invalid URL encoding." }
                val high = value[i++].digitToIntOrNull(16)
                val low = value[i++].digitToIntOrNull(16)
                require(high != null && low != null) { "The setup link contains invalid URL encoding." }
                out.write(high * 16 + low)
            } else {
                require(char != '+') { "The setup link contains ambiguous URL encoding." }
                out.write(char.code)
            }
        }
        return try {
            Charsets.UTF_8.newDecoder().onMalformedInput(CodingErrorAction.REPORT)
                .onUnmappableCharacter(CodingErrorAction.REPORT).decode(ByteBuffer.wrap(out.toByteArray())).toString()
        } catch (_: Exception) { throw IllegalArgumentException("The setup link contains invalid UTF-8.") }
    }
}

/** Staging survives Activity recreation in the ViewModel; consumption requires the exact reviewed value. */
internal class SetupInbox {
    var pending: PhoneSetup? = null
        private set
    fun stage(raw: String, blocked: Boolean): PhoneSetup {
        require(!blocked) { "Finish the current action or disconnect before importing another PC." }
        val value = PhoneSetupLink.parse(raw)
        require(pending == null || pending == value) { "Confirm or cancel the current setup before opening another link." }
        pending = value
        return value
    }
    fun approved(reviewed: PhoneSetup, ready: Boolean): PhoneSetup {
        require(ready && pending == reviewed) { "Review the current setup after the app finishes loading." }
        return reviewed
    }
    fun dismiss() { pending = null }
}

/** A cancelled lookup/check is never allowed to publish into another server's UI. */
internal class HomeRequestEpoch {
    private var generation = 0L
    fun next(): Long = ++generation
    fun invalidate() { ++generation }
    fun current(token: Long): Boolean = token == generation
}

/** Reserve synchronously before launching work. A cancelled blocking disk write retains its slot until it exits. */
internal class SettingsTransactions(private val persist: (UserSettings) -> Unit) {
    var busy: Boolean = false
        private set
    fun reserve(snapshot: UserSettings): Pending {
        check(!busy) { "Settings are already being saved" }
        busy = true
        return Pending(snapshot)
    }
    inner class Pending internal constructor(private val snapshot: UserSettings) {
        private var started = false
        suspend fun commit() {
            check(!started) { "This settings change was already used" }
            started = true
            try { withContext(Dispatchers.IO) { persist(snapshot) } }
            finally { busy = false }
        }
    }
}
