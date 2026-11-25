package com.openremote.mobile

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.TimeoutCancellationException
import java.io.IOException
import java.net.ConnectException
import java.net.SocketTimeoutException
import java.net.UnknownHostException
import java.security.GeneralSecurityException
import javax.net.ssl.SSLException

data class RelayConfiguration(val configured: Boolean, val udp: Boolean, val tcp: Boolean, val tls: Boolean)
data class ServerInfo(val version: String, val monitorSelection: Boolean, val relay: RelayConfiguration, val relayOnly: Boolean, val pairingCodeVersion: Int = 0, val webController: Boolean = false, val automaticHostRegistrationVersion: Int = 0, val activationVersion: Int = 0, val maxActivationConnections: Int = 0) {
    companion object {
        fun decode(bytes: ByteArray): ServerInfo {
            val value = StrictToolJson.parse(bytes)
            val required = setOf("service", "protocolVersion", "version", "phoneSetupVersion", "monitorSelection", "relay", "relayOnly")
            require(value.keys.containsAll(required) && value.keys.all { it in required || it in setOf("pairingCodeVersion", "webController", "automaticHostRegistrationVersion", "activationVersion", "maxActivationConnections") }) { "Invalid server information" }
            val pairing = if (value.containsKey("pairingCodeVersion")) value["pairingCodeVersion"] as? Long
                ?: throw IllegalArgumentException("Invalid pairing capability") else 0L
            require(pairing in 0L..1L) { "Incompatible pairing version" }
            val web = if (value.containsKey("webController")) value["webController"] as? Boolean
                ?: throw IllegalArgumentException("Invalid web capability") else false
            val registration = if (value.containsKey("automaticHostRegistrationVersion")) value["automaticHostRegistrationVersion"] as? Long
                ?: throw IllegalArgumentException("Invalid registration capability") else 0L
            require(registration in 0L..1L) { "Incompatible registration version" }
            val activation = if (value.containsKey("activationVersion")) value["activationVersion"] as? Long
                ?: throw IllegalArgumentException("Invalid activation capability") else 0L
            val capacity = if (value.containsKey("maxActivationConnections")) value["maxActivationConnections"] as? Long
                ?: throw IllegalArgumentException("Invalid activation capacity") else 0L
            require(activation in 0L..1L && ((activation == 0L && capacity == 0L) ||
                (activation == 1L && capacity == 4L))) { "Incompatible activation capability" }
            require(value["service"] == "openremote" && value["protocolVersion"] == 1L && value["phoneSetupVersion"] == 1L) { "Incompatible OpenRemote server" }
            val version = value["version"] as? String ?: throw IllegalArgumentException("Invalid server version")
            require(Regex("[A-Za-z0-9.+_-]{1,64}").matches(version)) { "Invalid server version" }
            val selection = value["monitorSelection"] as? Boolean ?: throw IllegalArgumentException("Invalid server feature")
            val only = value["relayOnly"] as? Boolean ?: throw IllegalArgumentException("Invalid relay policy")
            val relay = value["relay"] as? Map<*, *> ?: throw IllegalArgumentException("Invalid relay information")
            require(relay.keys == setOf("configured", "udp", "tcp", "tls") && relay.values.all { it is Boolean }) { "Invalid relay fields" }
            val result = RelayConfiguration(relay["configured"] == true, relay["udp"] == true, relay["tcp"] == true, relay["tls"] == true)
            require(result.configured == (result.udp || result.tcp || result.tls) && (!only || result.configured)) { "Inconsistent relay configuration" }
            return ServerInfo(version, selection, result, only, pairing.toInt(), web, registration.toInt(), activation.toInt(), capacity.toInt())
        }
    }
}

enum class CheckStatus { RUNNING, PASSED, INFO, WARNING, FAILED }
data class ConnectionCheck(val title: String, val status: CheckStatus, val detail: String)
data class ConnectionCheckReport(val endpoint: String, val pcId: String?, val checks: List<ConnectionCheck> = emptyList(), val running: Boolean = true) {
    fun asText(): String = buildString {
        append("OpenRemote connection checks\nServer: ").append(endpoint)
        pcId?.let { append("\nPC: ").append(it) }
        for (check in checks) append("\n").append(check.title).append(" [").append(check.status.name.lowercase()).append("]: ").append(check.detail)
        append("\nThese checks do not test desktop video, input, or an actual relay connection.")
    }
}
data class ConnectionCheckResult(val report: ConnectionCheckReport, val device: RemoteDevice?)

/** Explicit checks stop before identity authentication if the target is not a compatible coordinator. */
internal object ConnectionChecks {
    suspend fun run(client: ApiClient, endpoint: String, pcId: String?, publish: (ConnectionCheckReport) -> Unit): ConnectionCheckResult {
        val rows = mutableListOf<ConnectionCheck>()
        var current = "Server"
        var device: RemoteDevice? = null
        fun update(status: CheckStatus, text: String) {
            val row = ConnectionCheck(current, status, text)
            if (rows.lastOrNull()?.title == current) rows[rows.lastIndex] = row else rows += row
            publish(ConnectionCheckReport(endpoint, pcId, rows.toList()))
        }
        try {
            update(CheckStatus.RUNNING, "Checking the HTTPS address and OpenRemote protocol…")
            val info = client.serverInfo()
            update(CheckStatus.PASSED, "Reached OpenRemote ${info.version} using the configured HTTPS connection.")
            current = "Internet relay configuration"
            val transports = buildList { if (info.relay.udp) add("UDP"); if (info.relay.tcp) add("TCP"); if (info.relay.tls) add("TLS") }
            update(if (info.relay.configured) CheckStatus.INFO else CheckStatus.WARNING,
                if (info.relay.configured) "Server advertises ${transports.joinToString(", ")}${if (info.relayOnly) " (relay required)" else ""}. Reachability is checked only during a real session."
                else "No relay is configured. Connections between different networks may fail. Configure TURN on your server.")
            current = "Phone identity"
            update(CheckStatus.RUNNING, "Verifying a fresh proof from this phone…")
            client.verifyIdentity()
            update(CheckStatus.PASSED, "The server accepted this phone's signed challenge. PC approval is still required.")
            current = "PC availability"
            if (pcId == null) update(CheckStatus.INFO, "Enter a nine-digit PC number and run the checks again to check that PC.")
            else {
                update(CheckStatus.RUNNING, "Checking the PC's current heartbeat…")
                device = client.lookup(pcId)
                update(if (device.online) CheckStatus.PASSED else CheckStatus.WARNING,
                    if (device.online) "This PC is online. Request a connection and approve it on the PC."
                    else "This PC is offline. Wake it, start the OpenRemote host, and confirm it uses this server.")
            }
        } catch (e: TimeoutCancellationException) { update(CheckStatus.FAILED, connectionError(e, current)) }
        catch (e: CancellationException) { throw e }
        catch (e: Exception) { update(CheckStatus.FAILED, connectionError(e, current)) }
        val report = ConnectionCheckReport(endpoint, pcId, rows.toList(), running = false)
        publish(report)
        return ConnectionCheckResult(report, device)
    }
}

/** Do not reflect arbitrary server, TLS, path, token or key material into support messages. */
internal fun connectionError(error: Exception, stage: String = "Connection"): String = when (error) {
    is UnknownHostException -> "The server name could not be found. Check its spelling, your Internet connection, and the server's DNS record."
    is SSLException -> "The HTTPS certificate could not be verified. Check the phone's date and the server's certificate; a valid trusted certificate is required."
    is SocketTimeoutException, is TimeoutCancellationException -> "The connection timed out. Check your Internet connection and whether the server is running, then try again."
    is ConnectException -> "The server could not be reached. Check its address, HTTPS port, firewall, and whether it is running."
    is ApiException -> when {
        error.status == 404 && stage == "Server" -> "This server does not support connection checks. Confirm the address and upgrade the coordinator to OpenRemote 0.5 or later."
        error.status == 404 -> "That PC number is not registered on this server. Check the number and use the same server as the PC."
        error.status == 401 || error.status == 403 -> "The server rejected this phone's authentication or access. Check the server policy and whether this phone was revoked."
        error.status == 429 -> "The server is limiting requests. Wait a short time, then try again."
        error.status >= 500 -> "The server is unavailable or cannot save its state. Check its service and storage before retrying."
        error.status in 300..399 -> "The server redirected this request. Enter its final HTTPS origin; redirects are not followed."
        else -> "The server rejected this request. Check the PC number and coordinator configuration."
    }
    is GeneralSecurityException, is SecurityException -> "Android could not use this phone's identity key. Unlock the phone and try again."
    is IllegalArgumentException -> if (stage == "Server") "The address did not return compatible OpenRemote information. Confirm the HTTPS origin and coordinator version." else "The server returned an invalid response. Check that the PC, server, and phone use compatible OpenRemote versions."
    is IOException -> "The connection was interrupted or the response was invalid. Check your network and server, then try again."
    else -> "The $stage check could not complete. Check the server and phone setup, then try again."
}
