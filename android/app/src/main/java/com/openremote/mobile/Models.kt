package com.openremote.mobile

import org.json.JSONObject
import java.time.Instant

data class RemoteDevice(
    val displayId: String,
    val name: String,
    val platform: String,
    val online: Boolean,
    val lastSeen: String,
)

data class PermissionSet(
    val viewScreen: Boolean = true,
    val controlInput: Boolean = true,
    val clipboard: Boolean = false,
    val fileTransfer: Boolean = false,
    val audio: Boolean = false,
    val power: Boolean = false,
) {
    fun toJson() = JSONObject().put("viewScreen", viewScreen).put("controlInput", controlInput)
        .put("clipboard", clipboard).put("fileTransfer", fileTransfer).put("audio", audio).put("power", power)
    companion object {
        fun fromJson(j: JSONObject) = PermissionSet(j.optBoolean("viewScreen"), j.optBoolean("controlInput"),
            j.optBoolean("clipboard"), j.optBoolean("fileTransfer"), j.optBoolean("audio"), j.optBoolean("power"))
    }
}

data class RemoteSession(val id: String, val status: String, val controllerToken: String, val permissions: PermissionSet, val monitorId: String? = null, val pairingId: String? = null, val activation: Boolean = false, val hostDisplayId: String? = null)
data class SessionSnapshot(val status: String, val permissions: PermissionSet)
data class SignalMessage(val seq: Long, val from: String, val kind: String, val payload: JSONObject)
data class IceServerConfig(val urls: List<String>, val username: String, val credential: String)
data class IceConfiguration(val servers: List<IceServerConfig>, val relayOnly: Boolean, val expiresAt: Instant? = null)
data class ControllerAccess(val token: String, val expiresAt: Instant)
data class SavedPc(val displayId: String, val name: String)
data class UserSettings(val endpoint: String = "", val savedPcs: List<SavedPc> = emptyList())

data class HostCapabilities(val text: Boolean = false, val clipboard: Boolean = false, val fileTransfer: Boolean = false, val multiMonitor: Boolean = false)
data class SessionToolsUi(
    val channelOpen: Boolean = false,
    val capabilities: HostCapabilities = HostCapabilities(),
    val busy: Boolean = false,
    val pickingDocument: Boolean = false,
    val status: String = "",
    val progress: Float? = null,
    val clipboardPreview: String? = null,
    val error: Boolean = false,
)

data class SessionUi(
    val device: RemoteDevice,
    val session: RemoteSession,
    val stage: String = "Waiting for approval on PC",
    val connected: Boolean = false,
    val rendered: Boolean = false,
    val inputEnabled: Boolean = false,
    val diagnostics: String = "",
    val error: String? = null,
    val tools: SessionToolsUi = SessionToolsUi(),
    val requestedPermissions: PermissionSet = session.permissions,
    val displays: DisplayCatalog? = null,
) {
    val canClipboard: Boolean get() = connected && rendered && !tools.pickingDocument && session.permissions.clipboard && tools.channelOpen && tools.capabilities.clipboard
    val canUpload: Boolean get() = connected && rendered && !tools.pickingDocument && session.permissions.fileTransfer && tools.channelOpen && tools.capabilities.fileTransfer
    val canSwitchDisplay: Boolean get() = connected && rendered && displays != null && !tools.busy && !tools.pickingDocument
}

data class AppUiState(
    val settingsLoaded: Boolean = false,
    val settingsSaving: Boolean = false,
    val endpoint: String = "",
    val pcInput: String = "",
    val pendingSetup: PhoneSetup? = null,
    val setupMessage: String? = null,
    val connectionChecks: ConnectionCheckReport? = null,
    val savedPcs: List<SavedPc> = emptyList(),
    val device: RemoteDevice? = null,
    val session: SessionUi? = null,
    val busy: Boolean = false,
    val error: String? = null,
) {
    val homeLocked: Boolean get() = !settingsLoaded || settingsSaving || busy || session != null || pendingSetup != null
}

internal fun formatBytes(bytes: Long): String = when {
    bytes >= 1_048_576 -> String.format(java.util.Locale.US, "%.1f MiB", bytes / 1_048_576.0)
    bytes >= 1024 -> String.format(java.util.Locale.US, "%.1f KiB", bytes / 1024.0)
    else -> "$bytes bytes"
}
