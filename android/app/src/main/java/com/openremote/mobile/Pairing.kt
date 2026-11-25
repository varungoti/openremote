package com.openremote.mobile

import java.net.URI

/** These values are capabilities for one session, never saved PC identities. */
internal object PairingInput {
    fun validCode(code: String) = code.length in setOf(6, 9) && code.all { it in '0'..'9' }
    fun validPin(pin: String) = pin.length == 6 && pin.all { it in '0'..'9' }
    fun partialDigits(value: String) = value.length <= 6 && value.all { it in '0'..'9' }
    fun partialPcId(value: String) = value.length <= 9 && value.all { it in '0'..'9' }
    fun validate(code: String, pin: String) {
        require(validCode(code)) { "Enter Your PC ID shown on the computer you want to control." }
        require(validPin(pin)) { "Enter the six-digit PIN set on your computer." }
    }
    fun safeId(value: String) = value.length in 16..64 && value.all { it.isAsciiLetterOrDigit() || it in "-_" }
    private fun Char.isAsciiLetterOrDigit() = this in 'a'..'z' || this in 'A'..'Z' || this in '0'..'9'
}

data class PairedConnection(val device: RemoteDevice, val session: RemoteSession)

/** The shared build profile is public configuration. Existing encrypted settings always win. */
internal object ServiceProfile {
    fun endpoint(bytes: ByteArray): String {
        val profile = StrictToolJson.parse(bytes)
        require(profile.keys == setOf("schemaVersion", "serviceName", "apiOrigin") &&
            profile["schemaVersion"] == 1L && profile["serviceName"] == "OpenRemote") { "Invalid service profile" }
        val origin = profile["apiOrigin"] as? String ?: throw IllegalArgumentException("Invalid service origin")
        return validateOrigin(origin)
    }

    private fun validateOrigin(origin: String): String {
        if (origin.isEmpty()) return ""
        require(origin.length <= 2048 && origin.all { it.code in 33..126 } && origin.none { it in "\\%+@?#" }) { "Invalid service origin" }
        val uri = URI(origin)
        require(uri.scheme == "https" && uri.host != null && uri.rawUserInfo == null &&
            uri.rawPath in setOf("", "/") && uri.rawQuery == null && uri.rawFragment == null) { "Invalid service origin" }
        val host = uri.host.lowercase().trimEnd('.')
        require(listOf("example.com", "example.net", "example.org", "example", "invalid", "test", "localhost").none {
            host == it || host.endsWith(".$it")
        }) { "Configure a deployed service origin" }
        return ApiClient.validateEndpoint(origin).toString().trimEnd('/')
    }

    fun initialSettings(saved: UserSettings, profileBytes: ByteArray): UserSettings =
        if (saved.endpoint.isNotEmpty()) saved.copy(endpoint = validateOrigin(saved.endpoint))
        else UserSettings(endpoint = endpoint(profileBytes))
}

internal object ManagedClientText {
    const val UNAVAILABLE = "This copy of OpenRemote isn't ready to connect. Please contact your app provider."
    const val STARTUP_UNAVAILABLE = "OpenRemote couldn't start. Please reopen the app. If this keeps happening, contact your app provider."
    const val SESSION_STOPPED = "The remote session stopped. Check that connection is active on your PC, then reconnect with its PC ID and PIN."
}

internal enum class ManagedHomeAction {
    Disabled, Connect, Cancel;
    val enabled: Boolean get() = this != Disabled
}

/** Clearing the PIN after submission must never disable the single Cancel action. */
internal fun managedHomeAction(ui: AppUiState, serverId: String, pin: String): ManagedHomeAction = when {
    ui.busy -> ManagedHomeAction.Cancel
    ui.homeLocked || ui.endpoint.isEmpty() || !PairingInput.validCode(serverId) || !PairingInput.validPin(pin) -> ManagedHomeAction.Disabled
    else -> ManagedHomeAction.Connect
}

/** User-facing recovery never exposes service addresses, keys or configuration instructions. */
internal fun pairingError(error: Exception): String = when {
    error is ApiException && error.code == "pairing_unavailable" ->
        "Check the PC ID and PIN, and make sure connection is active on your PC."
    error is ApiException && error.status == 429 ->
        "Please wait a minute before trying again. Check that connection is still active on your PC."
    error is PairingNotSupportedException || error is ApiException && error.status == 404 ->
        "This connection isn't available yet. Please contact your app provider."
    error is PairingResultUnknownException || error is ApiException && (error.status == 408 || error.status >= 500) ->
        "We couldn't confirm the connection. Check the connection list on your PC before trying again."
    else -> "We couldn't connect. Check your Internet connection and try again. If this continues, contact your app provider."
}

internal class PairingNotSupportedException : IllegalArgumentException("Pairing capability unavailable")
internal class PairingResultUnknownException(cause: Exception) : java.io.IOException("Pairing result unavailable", cause)

/** Display name only, not a proof of device ownership. Never include identity keys or credentials. */
internal fun controllerDeviceName(manufacturer: String, model: String): String {
    val raw = (manufacturer + " " + model).trim().replace(Regex("\\s+"), " ")
    val result = StringBuilder("Android")
    var size = 7 // "Android" in UTF-8
    val suffix = raw.codePoints().toArray().filter { !Character.isISOControl(it) && Character.getType(it) != Character.FORMAT.toInt() }
    if (suffix.isNotEmpty()) {
        result.append(" · "); size += 4
        for (point in suffix) {
            val text = String(Character.toChars(point))
            val bytes = text.toByteArray(Charsets.UTF_8).size
            if (size + bytes > 80) break
            result.append(text); size += bytes
        }
    }
    return result.toString().trim()
}
