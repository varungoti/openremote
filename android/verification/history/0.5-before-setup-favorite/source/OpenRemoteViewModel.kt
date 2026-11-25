package com.openremote.mobile

import android.app.Application
import android.content.Context
import android.net.ConnectivityManager
import android.net.Network
import android.net.Uri
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import org.webrtc.VideoTrack
import java.lang.ref.WeakReference

class OpenRemoteViewModel(application: Application) : AndroidViewModel(application) {
    private val settings = SecureSettings(application)
    private val settingsTransactions = SettingsTransactions(settings::save)
    private val identity = ControllerIdentity()
    private val _ui = MutableStateFlow(AppUiState())
    val ui = _ui.asStateFlow()
    private val cleanupScope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val setupInbox = SetupInbox()
    private val homeEpoch = HomeRequestEpoch()
    private var api: ApiClient? = null
    private var sessionJob: Job? = null
    private var requestJob: Job? = null
    private var toolJob: Job? = null
    private var pickerTimeout: Job? = null
    private var pickerGeneration: Long? = null
    private var peer: RemotePeer? = null
    private var displayRef: WeakReference<RemoteDisplay>? = null
    private var track: VideoTrack? = null
    private var sessionGeneration = 0L
    private var textSupported = false
    private var network: Network? = null
    private val connectivity = application.getSystemService(ConnectivityManager::class.java)
    private val networkCallback = object : ConnectivityManager.NetworkCallback() {
        override fun onAvailable(value: Network) { viewModelScope.launch {
            if (network != null && network != value) peer?.requestRestart()
            network = value
        } }
        override fun onLost(value: Network) { viewModelScope.launch {
            if (network == value) peer?.requestRestart()
        } }
    }
    init {
        connectivity.registerDefaultNetworkCallback(networkCallback)
        viewModelScope.launch {
            try {
                // Recover private leftovers after a killed process; no user document is touched.
                withContext(Dispatchers.IO) { prepareUploadCache(application) }
                val saved = withContext(Dispatchers.IO) { settings.load() }
                if (saved.endpoint.isNotBlank()) api = ApiClient(saved.endpoint, identity)
                // A launch intent can stage a setup while encrypted settings are loading.
                _ui.update { it.copy(endpoint = saved.endpoint, savedPcs = saved.savedPcs, settingsLoaded = true) }
            } catch (_: Exception) {
                _ui.update { it.copy(settingsLoaded = true, error = "Saved settings could not be decrypted. Enter your server again to continue.") }
            }
        }
    }
    private fun homeAvailable(allowPending: Boolean = false): Boolean = _ui.value.let {
        it.settingsLoaded && !settingsTransactions.busy && !it.settingsSaving && !it.busy && it.session == null && (allowPending || it.pendingSetup == null)
    }
    fun setPcInput(value: String) {
        if (!homeAvailable()) return
        val id = value.filter { it in '0'..'9' }.take(9)
        _ui.update { it.copy(pcInput = id, device = it.device?.takeIf { d -> d.displayId == id }, connectionChecks = null, setupMessage = null) }
    }
    fun stageSetupLink(value: String) {
        try {
            val state = _ui.value
            val pending = setupInbox.stage(value, state.session != null || state.busy || state.settingsSaving)
            _ui.update { it.copy(pendingSetup = pending, error = null, setupMessage = null) }
        } catch (e: IllegalArgumentException) { _ui.update { it.copy(error = e.message?.take(200) ?: "Invalid setup link") } }
    }
    fun dismissSetup() { if (!_ui.value.settingsSaving) { setupInbox.dismiss(); _ui.update { it.copy(pendingSetup = null) } } }
    fun confirmSetup(reviewed: PhoneSetup) {
        if (!homeAvailable(allowPending = true)) return
        try {
            val approved = setupInbox.approved(reviewed, ready = true)
            persistEndpoint(approved.endpoint, approved)
        } catch (e: IllegalArgumentException) { _ui.update { it.copy(error = e.message?.take(200)) } }
    }
    fun saveEndpoint(value: String) {
        if (!homeAvailable()) return
        try { persistEndpoint(ApiClient.validateEndpoint(value).toString().trimEnd('/')) }
        catch (e: IllegalArgumentException) { _ui.update { it.copy(error = e.message?.take(200)) } }
    }
    private fun persistEndpoint(endpoint: String, setup: PhoneSetup? = null) {
        val state = _ui.value
        val sameServer = endpoint == state.endpoint
        val saved = if (sameServer) state.savedPcs else emptyList()
        val next = UserSettings(endpoint, saved)
        val transaction = settingsTransactions.reserve(next)
        homeEpoch.invalidate()
        _ui.update { it.copy(settingsSaving = true, device = null, error = null, connectionChecks = null) }
        viewModelScope.launch {
            try {
                transaction.commit()
                api = ApiClient(endpoint, identity)
                if (setup != null) setupInbox.dismiss()
                _ui.update { it.copy(endpoint = endpoint, savedPcs = saved, device = null, error = null,
                    pcInput = setup?.pcId ?: if (sameServer) it.pcInput else "", pendingSetup = if (setup != null) null else it.pendingSetup,
                    setupMessage = if (setup != null) "PC setup saved. Run connection checks or find the PC when you are ready." else "Server settings saved.") }
            } catch (e: CancellationException) { throw e }
            catch (_: Exception) { _ui.update { it.copy(error = "Settings could not be saved securely. Check your phone's available storage and try again.") } }
            finally { _ui.update { it.copy(settingsSaving = false) } }
        }
    }
    fun findPc(id: String) {
        if (!homeAvailable()) return
        val client = api ?: run { _ui.update { it.copy(error = "Set your HTTPS server address first") }; return }
        val normalized = try { ApiClient.normalizePcId(id) } catch (e: IllegalArgumentException) { _ui.update { it.copy(error = e.message) }; return }
        val generation = homeEpoch.next()
        _ui.update { it.copy(busy = true, error = null, device = null, pcInput = normalized, connectionChecks = null, setupMessage = null) }
        requestJob = viewModelScope.launch {
            try {
                val d = client.lookup(normalized)
                if (homeEpoch.current(generation)) _ui.update { it.copy(device = d) }
            } catch (e: CancellationException) { throw e }
            catch (e: Exception) { if (homeEpoch.current(generation)) showError(e) }
            finally { if (homeEpoch.current(generation)) _ui.update { it.copy(busy = false) } }
        }
    }
    fun checkConnection() {
        if (!homeAvailable()) return
        val client = api ?: run { _ui.update { it.copy(error = "Set your HTTPS server address first") }; return }
        val state = _ui.value
        val id = if (state.pcInput.isBlank()) null else try { ApiClient.normalizePcId(state.pcInput) }
            catch (_: IllegalArgumentException) { _ui.update { it.copy(error = "Enter all nine PC digits, or clear the PC number to check only the server.") }; return }
        val generation = homeEpoch.next()
        _ui.update { it.copy(busy = true, error = null, device = null, setupMessage = null, connectionChecks = ConnectionCheckReport(state.endpoint, id)) }
        requestJob = viewModelScope.launch {
            try {
                val result = ConnectionChecks.run(client, state.endpoint, id) { report ->
                    if (homeEpoch.current(generation)) _ui.update { it.copy(connectionChecks = report) }
                }
                if (homeEpoch.current(generation)) _ui.update { it.copy(device = result.device) }
            } catch (e: CancellationException) { throw e }
            finally { if (homeEpoch.current(generation)) _ui.update { it.copy(busy = false) } }
        }
    }
    fun cancelHomeRequest() {
        if (_ui.value.session != null) return
        // This also covers a connection POST before its session exists. Invalidate both
        // generations so its late result/finally cannot affect the next lookup or request.
        disconnectLocal()
    }
    private fun stoppedChecks(report: ConnectionCheckReport?): ConnectionCheckReport? = report?.let { value ->
        if (!value.running) value else value.copy(running = false, checks = value.checks.map {
            if (it.status == CheckStatus.RUNNING) it.copy(status = CheckStatus.INFO, detail = "Check stopped. Run connection checks again to continue.") else it
        })
    }
    fun removePc(id: String) {
        if (!homeAvailable()) return
        val state = _ui.value
        val updated = state.savedPcs.filterNot { it.displayId == id }
        val next = UserSettings(state.endpoint, updated)
        val transaction = settingsTransactions.reserve(next)
        _ui.update { it.copy(settingsSaving = true, error = null) }
        viewModelScope.launch {
            try {
                transaction.commit()
                _ui.update { it.copy(savedPcs = updated, device = it.device?.takeUnless { d -> d.displayId == id }) }
            } catch (e: CancellationException) { throw e }
            catch (_: Exception) { _ui.update { it.copy(error = "The saved PC list could not be updated. Try again after checking phone storage.") } }
            finally { _ui.update { it.copy(settingsSaving = false) } }
        }
    }
    fun connect(controlInput: Boolean, clipboard: Boolean = false, fileTransfer: Boolean = false) {
        val d = _ui.value.device ?: return
        val client = api ?: return
        if (!homeAvailable() || !d.online) return
        beginConnection(client, d, PermissionSet(controlInput = controlInput, clipboard = clipboard, fileTransfer = fileTransfer))
    }
    fun switchMonitor(monitorId: String) {
        val current = _ui.value.session ?: return
        val client = api ?: return
        val catalog = current.displays ?: return
        if (!current.canSwitchDisplay || monitorId == catalog.selectedMonitorId || catalog.monitors.none { it.id == monitorId }) return
        // The original user request, not newly added permissions, is repeated for fresh host approval.
        disconnectLocal(endOnServer = false)
        beginConnection(client, current.device, current.requestedPermissions, monitorId, current.session)
    }
    private fun beginConnection(client: ApiClient, d: RemoteDevice, requested: PermissionSet, monitorId: String? = null, previous: RemoteSession? = null) {
        _ui.update { it.copy(busy = true, error = null) }
        val generation = ++sessionGeneration
        requestJob = viewModelScope.launch {
            var created: RemoteSession? = null
            var previousEnded = previous == null
            try {
                if (previous != null) {
                    withTimeout(10_000) { client.end(previous) }
                    previousEnded = true
                }
                val session = client.createSession(d.displayId, identity.label(), requested, monitorId)
                created = session
                if (generation != sessionGeneration) { cleanupScope.launch { runCatching { client.end(session) } }; return@launch }
                _ui.update { it.copy(session = SessionUi(d, session, requestedPermissions = requested), busy = false) }
                val saved = (listOf(SavedPc(d.displayId, d.name)) + _ui.value.savedPcs.filterNot { it.displayId == d.displayId }).take(100)
                val savedSettings = UserSettings(_ui.value.endpoint, saved)
                val transaction = settingsTransactions.reserve(savedSettings)
                // Cancellation waits for an already executing AtomicFile write to finish. Keep home
                // mutations locked until then, even if disconnect removes the session meanwhile.
                _ui.update { it.copy(settingsSaving = true) }
                try { transaction.commit() }
                finally { _ui.update { it.copy(settingsSaving = false) } }
                if (generation != sessionGeneration) return@launch
                _ui.update { it.copy(savedPcs = saved) }
                sessionJob = viewModelScope.launch { runSession(client, session, generation) }
            } catch (e: CancellationException) {
                created?.let { s -> cleanupScope.launch { runCatching { client.end(s) } } }
                throw e
            } catch (e: Exception) {
                created?.let { s -> cleanupScope.launch { runCatching { client.end(s) } } }
                if (generation == sessionGeneration) {
                    if (_ui.value.session != null) disconnect(e.message ?: "Unable to start session") else showError(e)
                }
            } finally {
                if (!previousEnded && previous != null) cleanupScope.launch { runCatching { withTimeout(5_000) { client.end(previous) } } }
                if (generation == sessionGeneration) _ui.update { it.copy(busy = false) }
            }
        }
    }
    private suspend fun runSession(client: ApiClient, session: RemoteSession, generation: Long) {
        var pendingPolls = 0
        var failures = 0
        try {
            while (generation == sessionGeneration) {
                val snapshot = try { client.sessionStatus(session) } catch (e: CancellationException) { throw e }
                catch (e: Exception) {
                    failures++
                    if (e is IllegalArgumentException || e is ApiException && e.status in setOf(401, 403, 404, 409, 410) || failures > 5) throw e
                    delay(minOf(8_000L, 500L shl failures)); continue
                }
                failures = 0
                if (generation != sessionGeneration) return
                when (val decision = sessionDecision(snapshot.status, peer != null)) {
                    SessionDecision.AwaitApproval -> {
                        pendingPolls++
                        if (pendingPolls > 180) error("The PC did not approve the connection in time")
                    }
                    SessionDecision.StartPeer, SessionDecision.KeepPeer -> {
                        if (peer == null) {
                            require(snapshot.permissions.viewScreen) { "The PC did not grant screen access" }
                            val granted = session.copy(permissions = snapshot.permissions)
                            _ui.update { state -> state.copy(session = state.session?.copy(session = granted)) }
                            peer = RemotePeer(getApplication(), client, granted,
                                onVideo = { value -> if (generation == sessionGeneration) { track = value; displayRef?.get()?.bind(value) } },
                                onState = { label, connected ->
                                    if (generation == sessionGeneration) {
                                        _ui.update { state -> state.copy(session = state.session?.let { current -> current.copy(stage = label, connected = connected, inputEnabled = connected && current.rendered && current.session.permissions.controlInput && !current.tools.pickingDocument) }) }
                                        if (!connected && toolJob?.isActive == true) cancelTools("The desktop is not visible or the connection was interrupted. The file or clipboard action was cancelled.")
                                    }
                                },
                                onRenderReady = { rendered -> if (generation == sessionGeneration) _ui.update { state -> state.copy(session = state.session?.let { current -> current.copy(
                                    rendered = rendered, inputEnabled = current.connected && rendered && current.session.permissions.controlInput && !current.tools.pickingDocument) }) } },
                                onDiagnostics = { text -> if (generation == sessionGeneration) _ui.update { it.copy(session = it.session?.copy(diagnostics = text)) } },
                                onCapabilities = { value -> if (generation == sessionGeneration) { textSupported = value.text; updateTools { it.copy(capabilities = value) } } },
                                onDisplays = { value -> if (generation == sessionGeneration) _ui.update { it.copy(session = it.session?.copy(displays = value)) } },
                                onToolsState = { open, reason -> if (generation == sessionGeneration) updateTools { it.copy(channelOpen = open, status = reason ?: it.status, error = reason != null || it.error) } },
                                onFatal = { message -> if (generation == sessionGeneration) disconnect(message) })
                            withTimeout(45_000) { peer!!.start() }
                        }
                        peer?.updatePermissions(snapshot.permissions)
                        _ui.update { state -> state.copy(session = state.session?.let { current -> current.copy(
                            session = current.session.copy(permissions = snapshot.permissions),
                            inputEnabled = current.connected && current.rendered && snapshot.permissions.controlInput && !current.tools.pickingDocument) }) }
                    }
                    is SessionDecision.End -> error(decision.reason)
                }
                delay(if (snapshot.status == "pending") 1_000 else 3_000)
            }
        } catch (e: CancellationException) { throw e }
        catch (e: Exception) { if (generation == sessionGeneration) disconnect(e.message ?: "Session failed") }
    }
    fun disconnect(message: String? = null) = disconnectLocal(message)
    private fun disconnectLocal(message: String? = null, endOnServer: Boolean = true) {
        val current = _ui.value.session?.session
        val client = api
        sessionGeneration++
        homeEpoch.invalidate()
        sessionJob?.cancel(); sessionJob = null
        requestJob?.cancel(); requestJob = null
        toolJob?.cancel(); toolJob = null
        pickerTimeout?.cancel(); pickerTimeout = null; pickerGeneration = null
        val previousPeer = peer
        val previousDisplay = displayRef?.get()
        peer = null; displayRef = null; track = null; textSupported = false
        runCatching { previousPeer?.releaseAll() }
        runCatching { previousDisplay?.release() }
        runCatching { previousPeer?.close() }
        _ui.update { it.copy(session = null, busy = false, error = message, connectionChecks = stoppedChecks(it.connectionChecks)) }
        if (endOnServer && current != null && client != null) cleanupScope.launch { runCatching { withTimeout(5_000) { client.end(current) } } }
    }
    fun newDisplay(context: Context): RemoteDisplay {
        val p = requireNotNull(peer) { "Video connection is not ready" }
        val generation = sessionGeneration
        runCatching { displayRef?.get()?.release() }
        val token = p.attachRenderer()
        return RemoteDisplay(context, p.eglContext,
            peer = { p.takeIf { generation == sessionGeneration && peer === p } },
            onFirstFrame = { if (generation == sessionGeneration && peer === p) p.rendererFirstFrame(token) },
            onReleased = { if (generation == sessionGeneration && peer === p) p.detachRenderer(token) },
        ).also { displayRef = WeakReference(it); it.bind(track) }
    }
    fun displayReady(): Boolean = peer != null
    fun releaseDisplay(value: RemoteDisplay) {
        runCatching { value.release() }
        if (displayRef?.get() === value) displayRef = null
    }
    fun key(code: String) { if (_ui.value.session?.inputEnabled == true) { peer?.key(code, true); peer?.key(code, false) } }
    fun shortcut(vararg codes: String) {
        if (_ui.value.session?.inputEnabled != true) return
        codes.forEach { peer?.key(it, true) }; codes.reversed().forEach { peer?.key(it, false) }
    }
    fun sendText(value: String): Boolean {
        if (!textSupported || _ui.value.session?.inputEnabled != true || !ToolProtocol.validTypedText(value)) return false
        return peer?.text(value) == true
    }
    private fun updateTools(change: (SessionToolsUi) -> SessionToolsUi) { _ui.update { state -> state.copy(session = state.session?.let { it.copy(tools = change(it.tools)) }) } }
    private fun startTool(status: String, action: suspend (RemotePeer) -> Unit) {
        val current = _ui.value.session ?: return
        val remote = peer ?: return
        if (!current.canClipboard || current.tools.busy) return
        val generation = sessionGeneration
        updateTools { it.copy(busy = true, status = status, error = false, progress = null) }
        toolJob = viewModelScope.launch {
            try { action(remote) }
            catch (e: kotlinx.coroutines.TimeoutCancellationException) { if (generation == sessionGeneration) updateTools { it.copy(status = safeToolError(e), error = true) } }
            catch (e: CancellationException) { throw e }
            catch (e: Exception) { if (generation == sessionGeneration) updateTools { it.copy(status = safeToolError(e), error = true) } }
            finally { if (generation == sessionGeneration) updateTools { it.copy(busy = false) } }
        }
    }
    fun readPcClipboard() = startTool("Reading PC clipboard…") { remote ->
        val text = remote.clipboardRead()
        updateTools { it.copy(clipboardPreview = text, status = "PC clipboard received. Review it before copying to this phone.") }
    }
    fun writePcClipboard(text: String) {
        if (!ToolProtocol.validText(text)) { updateTools { it.copy(status = "Clipboard text must be valid Unicode without NUL and at most 4,096 UTF-8 bytes.", error = true) }; return }
        startTool("Updating PC clipboard…") { remote -> remote.clipboardWrite(text); updateTools { it.copy(status = "PC clipboard updated.") } }
    }
    fun clearClipboardPreview() = updateTools { it.copy(clipboardPreview = null) }
    fun noteClipboardCopied() = updateTools { it.copy(status = "Text copied to this phone's clipboard.", error = false) }
    fun documentPickerActive(): Boolean = pickerGeneration == sessionGeneration && pickerGeneration != null
    fun beginDocumentPicker(): Boolean {
        val current = _ui.value.session ?: return false
        if (!current.canUpload || current.tools.busy) return false
        pickerGeneration = sessionGeneration
        peer?.documentPickerHandoff(true)
        peer?.releaseAll()
        updateTools { it.copy(busy = true, pickingDocument = true, progress = null, error = false, status = "Choose a file. Desktop input is paused while the document picker is open.") }
        _ui.update { it.copy(session = it.session?.copy(inputEnabled = false)) }
        pickerTimeout = viewModelScope.launch { delay(120_000); if (documentPickerActive()) disconnect("The file picker was open for two minutes. Reconnect to continue.") }
        return true
    }
    fun pickedDocument(uri: Uri?) {
        if (!documentPickerActive()) return
        pickerGeneration = null; pickerTimeout?.cancel(); pickerTimeout = null
        peer?.documentPickerHandoff(false)
        updateTools { it.copy(pickingDocument = false) }
        _ui.update { state -> state.copy(session = state.session?.let { it.copy(inputEnabled = it.connected && it.rendered && it.session.permissions.controlInput) }) }
        val current = _ui.value.session
        val remote = peer
        val uploadGranted = current?.let { it.connected && it.session.permissions.fileTransfer && it.tools.channelOpen && it.tools.capabilities.fileTransfer } == true
        if (uri == null || !uploadGranted || remote == null) {
            updateTools { it.copy(busy = false, status = if (uri == null) "No file selected." else "File transfer is no longer available.", error = uri != null) }
            return
        }
        val generation = sessionGeneration
        updateTools { it.copy(status = "Preparing a private file copy and SHA-256 digest…", error = false) }
        toolJob = viewModelScope.launch {
            var snapshot: UploadSnapshot? = null
            try {
                // A recreated Activity may receive the SAF result before its own first frame.
                // Preserve the explicit selection, but start no transfer against an unseen display.
                if (_ui.value.session?.canUpload != true) {
                    updateTools { it.copy(status = "Waiting for the selected display before sending the file…") }
                    withTimeout(30_000) { ui.first { generation != sessionGeneration || it.session?.canUpload == true } }
                    if (generation != sessionGeneration) return@launch
                    updateTools { it.copy(status = "Preparing a private file copy and SHA-256 digest…") }
                }
                withTimeout(120_000) { snapshot = snapshotDocument(getApplication(), uri) }
                val prepared = requireNotNull(snapshot)
                val finished = remote.upload(prepared) { confirmed, size ->
                    if (generation == sessionGeneration) updateTools { it.copy(
                        status = if (confirmed == size) "Verifying ${prepared.name} on the PC…" else "Sending ${prepared.name}: ${formatBytes(confirmed)} / ${formatBytes(size)}",
                        progress = if (size == 0L) 1f else confirmed.toFloat() / size.toFloat(),
                    ) }
                }
                updateTools { it.copy(status = "Sent ${prepared.name} (${formatBytes(finished.size)}). Saved in the PC's OpenRemote ReceivedFiles folder.", progress = 1f) }
            } catch (e: kotlinx.coroutines.TimeoutCancellationException) { if (generation == sessionGeneration) updateTools { it.copy(status = safeToolError(e), error = true, progress = null) } }
            catch (e: CancellationException) { throw e }
            catch (e: Exception) { if (generation == sessionGeneration) updateTools { it.copy(status = safeToolError(e), error = true, progress = null) } }
            finally {
                snapshot?.close()
                if (generation == sessionGeneration) updateTools { it.copy(busy = false) }
            }
        }
    }
    fun cancelTools(reason: String = "Action cancelled. Any incomplete upload is being removed.") {
        updateTools { it.copy(status = reason, progress = null, error = false) }
        toolJob?.cancel()
    }
    private fun safeToolError(error: Exception): String = when (error) {
        is kotlinx.coroutines.TimeoutCancellationException -> "The PC did not confirm the action in time. Any incomplete upload is cancelled; if verification had started, check the PC's ReceivedFiles folder."
        is SecurityException -> "Android did not grant access to that document. Choose it again."
        is java.io.IOException -> "The document could not be read or the private file copy could not be saved."
        else -> error.message?.take(240) ?: "The action could not be completed."
    }
    fun identityFingerprint(): String = identity.fingerprint()
    fun identityPublicKey(): String = identity.publicKeyBase64()
    private fun showError(e: Exception) { _ui.update { it.copy(error = connectionError(e)) } }
    override fun onCleared() {
        disconnect()
        connectivity.unregisterNetworkCallback(networkCallback)
        // Let the bounded /end request finish independently of ViewModel cancellation.
        cleanupScope.launch { delay(6_000); cleanupScope.cancel() }
        super.onCleared()
    }
}
