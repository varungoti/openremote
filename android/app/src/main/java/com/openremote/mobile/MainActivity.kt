package com.openremote.mobile

import android.os.Bundle
import android.content.ClipData
import android.content.ClipboardManager
import android.content.ClipDescription
import android.os.Build
import android.os.PersistableBundle
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.viewModels
import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.lifecycle.compose.collectAsStateWithLifecycle

class MainActivity : ComponentActivity() {
    private val model: OpenRemoteViewModel by viewModels()
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // Remote desktop pixels never enter Android's recent-app preview or screenshots.
        window.addFlags(WindowManager.LayoutParams.FLAG_SECURE)
        // Launcher content never imports or applies addresses from external intents.
        setContent {
            MaterialTheme(colorScheme = lightColorScheme(primary = Color(0xFF2463EB), secondary = Color(0xFF008577))) {
                val ui by model.ui.collectAsStateWithLifecycle()
                DisposableEffect(ui.session != null) {
                    if (ui.session != null) window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
                    onDispose { window.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON) }
                }
                Surface(Modifier.fillMaxSize()) {
                    if (ui.session != null) key(ui.session!!.session.id) { SessionScreen(model, ui.session!!) } else HomeScreen(model, ui)
                }
            }
        }
    }
    override fun onStop() {
        super.onStop()
        // An explicit SAF picker pauses input and has a two-minute bound; ordinary backgrounding ends the session.
        if (!isChangingConfigurations && !model.documentPickerActive()) model.disconnect()
    }
}

@Composable
private fun HomeScreen(model: OpenRemoteViewModel, ui: AppUiState) {
    // Neither field is saved across process/Activity recreation. PIN copies only live
    // in this UI and the existing bounded one-shot request, never persistent settings.
    var serverId by remember { mutableStateOf("") }
    var pin by remember { mutableStateOf("") }
    val lifecycleOwner = LocalLifecycleOwner.current
    DisposableEffect(lifecycleOwner) {
        val observer = LifecycleEventObserver { _, event -> if (event == Lifecycle.Event.ON_PAUSE) pin = "" }
        lifecycleOwner.lifecycle.addObserver(observer)
        onDispose { lifecycleOwner.lifecycle.removeObserver(observer); pin = "" }
    }
    LaunchedEffect(ui.homeLocked) { if (ui.homeLocked) pin = "" }
    BackHandler(enabled = ui.busy) { pin = ""; model.cancelHomeRequest() }
    val action = managedHomeAction(ui, serverId, pin)
    Box(Modifier.fillMaxSize().safeDrawingPadding().imePadding()) {
        Column(Modifier.align(Alignment.Center).widthIn(max = 480.dp).fillMaxWidth()
            .verticalScroll(rememberScrollState()).padding(24.dp), verticalArrangement = Arrangement.spacedBy(20.dp)) {
            Text("OpenRemote", style = MaterialTheme.typography.headlineLarge, fontWeight = FontWeight.Bold)
            OutlinedTextField(serverId, { serverId = it.take(32) },
                label = { Text("PC ID") }, singleLine = true,
                supportingText = { Text("Find Your PC ID in OpenRemote on the PC you want to control.") },
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
                isError = serverId.isNotEmpty() && !PairingInput.partialPcId(serverId),
                modifier = Modifier.fillMaxWidth(), enabled = !ui.homeLocked && ui.endpoint.isNotEmpty())
            OutlinedTextField(pin, { pin = it.take(32) },
                label = { Text("PIN") }, singleLine = true,
                visualTransformation = PasswordVisualTransformation(),
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.NumberPassword, autoCorrectEnabled = false),
                isError = pin.isNotEmpty() && !PairingInput.partialDigits(pin),
                modifier = Modifier.fillMaxWidth(), enabled = !ui.homeLocked && ui.endpoint.isNotEmpty())
            Button(onClick = {
                if (ui.busy) {
                    pin = ""
                    model.cancelHomeRequest()
                } else {
                    val submittedPin = pin
                    pin = ""
                    model.connectWithCode(serverId, submittedPin)
                }
            }, enabled = action.enabled, modifier = Modifier.fillMaxWidth().heightIn(min = 56.dp)) {
                Text(if (ui.busy) "Cancel" else "Connect remote desktop")
            }
            if (!ui.settingsLoaded || ui.settingsSaving || ui.busy) LinearProgressIndicator(Modifier.fillMaxWidth())
            val message = if (ui.settingsLoaded && ui.endpoint.isEmpty()) ui.error ?: ManagedClientText.UNAVAILABLE else ui.error
            message?.let { Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodyMedium) }
        }
    }
}

@Composable
private fun SessionScreen(model: OpenRemoteViewModel, ui: SessionUi) {
    var trackpad by remember { mutableStateOf(true) }
    var textDialog by remember { mutableStateOf(false) }
    var disconnectDialog by remember { mutableStateOf(false) }
    var toolsDialog by remember { mutableStateOf(false) }
    var displaysDialog by remember { mutableStateOf(false) }
    val filePicker = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri -> model.pickedDocument(uri) }
    BackHandler { disconnectDialog = true }
    Column(Modifier.fillMaxSize().background(Color(0xFF0B1020)).safeDrawingPadding()) {
        Row(Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 6.dp), verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) {
                Text(ui.device.name, color = Color.White, fontWeight = FontWeight.SemiBold)
                Text(if (ui.connected && ui.rendered) "Connected" else "Connecting…", color = Color(0xFFB8C8E4), style = MaterialTheme.typography.bodySmall)
            }
            TextButton(onClick = { model.disconnect() }) { Text("Stop remote session", color = Color(0xFFFFB4AB)) }
        }
        Box(Modifier.weight(1f).fillMaxWidth()) {
            if (model.displayReady()) {
                AndroidView(factory = { context -> model.newDisplay(context) },
                    update = { view -> view.trackpad = trackpad; view.inputEnabled = ui.inputEnabled },
                    onRelease = { view -> model.releaseDisplay(view) }, modifier = Modifier.fillMaxSize())
                if (!ui.rendered) Column(Modifier.align(Alignment.Center).padding(24.dp), horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    CircularProgressIndicator()
                    Text("Waiting for ${ui.displays?.selected?.name ?: "desktop"} to render…", color = Color.White)
                }
            } else {
                Column(Modifier.align(Alignment.Center).padding(24.dp), horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.spacedBy(16.dp)) {
                    CircularProgressIndicator()
                    Text("Connecting to your computer…", color = Color.White)
                }
            }
        }
        if ((ui.session.permissions.clipboard || ui.session.permissions.fileTransfer) && (ui.tools.busy || ui.tools.status.isNotEmpty())) {
            Text(ui.tools.status, color = if (ui.tools.error) Color(0xFFFFB4AB) else Color(0xFFB8C8E4), style = MaterialTheme.typography.bodySmall,
                maxLines = 2, modifier = Modifier.padding(horizontal = 12.dp, vertical = 4.dp))
        }
        Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            FilterChip(selected = trackpad, onClick = { trackpad = !trackpad }, label = { Text(if (trackpad) "Trackpad" else "Direct touch") })
            Spacer(Modifier.width(6.dp))
            TextButton(onClick = { textDialog = true }, enabled = ui.inputEnabled) { Text("Type text") }
            if (ui.session.pairingId == null && ui.displays != null) TextButton(onClick = { displaysDialog = true }, enabled = ui.canSwitchDisplay) { Text("Displays") }
            if (ui.session.permissions.clipboard || ui.session.permissions.fileTransfer || ui.tools.busy) {
                TextButton(onClick = { toolsDialog = true }, enabled = ui.connected || ui.tools.busy) { Text("Clipboard & files") }
            }
            TextButton(onClick = { model.key("Escape") }, enabled = ui.inputEnabled) { Text("Esc") }
            TextButton(onClick = { model.key("Tab") }, enabled = ui.inputEnabled) { Text("Tab") }
            TextButton(onClick = { model.key("Enter") }, enabled = ui.inputEnabled) { Text("Enter") }
            TextButton(onClick = { model.key("Backspace") }, enabled = ui.inputEnabled) { Text("⌫") }
            TextButton(onClick = { model.shortcut("AltLeft", "Tab") }, enabled = ui.inputEnabled) { Text("Alt+Tab") }
            TextButton(onClick = { model.shortcut("ControlLeft", "KeyC") }, enabled = ui.inputEnabled) { Text("Ctrl+C") }
            TextButton(onClick = { model.shortcut("ControlLeft", "KeyV") }, enabled = ui.inputEnabled) { Text("Ctrl+V") }
            listOf("ArrowLeft" to "←", "ArrowDown" to "↓", "ArrowUp" to "↑", "ArrowRight" to "→").forEach { (key, label) ->
                TextButton(onClick = { model.key(key) }, enabled = ui.inputEnabled) { Text(label) }
            }
        }
    }
    if (textDialog) {
        var text by remember { mutableStateOf("") }
        var error by remember { mutableStateOf(false) }
        AlertDialog(onDismissRequest = { textDialog = false }, title = { Text("Type on your PC") },
            text = { Column { OutlinedTextField(text, { if (it.isEmpty() || ToolProtocol.validTypedText(it)) text = it }, label = { Text("Text to type") }, minLines = 2)
                Text("This sends typing to the PC's focused app. Check where the cursor is first.", style = MaterialTheme.typography.bodySmall)
                if (error) Text("This host does not support text input, or input is unavailable.", color = MaterialTheme.colorScheme.error)
            } },
            confirmButton = { Button(onClick = { if (model.sendText(text)) textDialog = false else error = true }, enabled = text.isNotEmpty()) { Text("Type") } },
            dismissButton = { TextButton(onClick = { textDialog = false }) { Text("Cancel") } })
    }
    if (toolsDialog) SessionToolsDialog(model, ui, onDismiss = { toolsDialog = false }, onPickFile = {
        if (model.beginDocumentPicker()) {
            try { filePicker.launch(arrayOf("*/*")) }
            catch (_: Exception) { model.pickedDocument(null) }
        }
    })
    if (displaysDialog && ui.displays != null) DisplayPickerDialog(ui, onDismiss = { displaysDialog = false }, onSwitch = { id ->
        displaysDialog = false
        model.switchMonitor(id)
    })
    if (disconnectDialog) AlertDialog(onDismissRequest = { disconnectDialog = false }, title = { Text("Disconnect from this PC?") },
        text = { Text("Your programs stay open on the PC.") }, confirmButton = { Button(onClick = { model.disconnect() }) { Text("Disconnect") } },
        dismissButton = { TextButton(onClick = { disconnectDialog = false }) { Text("Stay connected") } })
}

@Composable
private fun DisplayPickerDialog(ui: SessionUi, onDismiss: () -> Unit, onSwitch: (String) -> Unit) {
    val catalog = requireNotNull(ui.displays)
    var selected by remember(catalog) { mutableStateOf(catalog.selectedMonitorId) }
    AlertDialog(onDismissRequest = onDismiss, title = { Text("PC displays") }, text = {
        Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Text(if (ui.session.pairingId != null) "This code authorizes one session only. Switching ends it and sends a new request that must be approved on the PC."
                else "Switching ends this session and reconnects to the selected display. You may need to approve the phone again on the PC.", style = MaterialTheme.typography.bodySmall)
            catalog.monitors.forEach { monitor ->
                OutlinedCard(onClick = { selected = monitor.id }, modifier = Modifier.fillMaxWidth(), enabled = ui.canSwitchDisplay) {
                    Row(Modifier.padding(10.dp), verticalAlignment = Alignment.CenterVertically) {
                        RadioButton(selected = selected == monitor.id, onClick = { selected = monitor.id }, enabled = ui.canSwitchDisplay)
                        Column(Modifier.weight(1f)) {
                            Text(monitor.name + if (monitor.id == catalog.selectedMonitorId) " · Current" else "", fontWeight = FontWeight.SemiBold)
                            Text("${monitor.width} × ${monitor.height}" + if (monitor.primary) " · Primary" else "", style = MaterialTheme.typography.bodySmall)
                        }
                    }
                }
            }
            Text("If a monitor is unplugged or the PC display layout changes, reconnect to the primary display to refresh this list.", style = MaterialTheme.typography.bodySmall)
        }
    }, confirmButton = {
        Button(onClick = { onSwitch(selected) }, enabled = ui.canSwitchDisplay && selected != catalog.selectedMonitorId) { Text("Switch and reconnect") }
    }, dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } })
}

@Composable
private fun SessionToolsDialog(model: OpenRemoteViewModel, ui: SessionUi, onDismiss: () -> Unit, onPickFile: () -> Unit) {
    var clipboardDraft by remember { mutableStateOf("") }
    val context = LocalContext.current
    AlertDialog(onDismissRequest = onDismiss, title = { Text("Clipboard & files") }, text = {
        Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text("PC clipboard", style = MaterialTheme.typography.titleMedium)
            if (ui.canClipboard) {
                Text("Text only, up to 4,096 UTF-8 bytes. Actions happen only when you tap a button.", style = MaterialTheme.typography.bodySmall)
                OutlinedButton(onClick = { model.readPcClipboard() }, enabled = !ui.tools.busy, modifier = Modifier.fillMaxWidth()) { Text("Read PC clipboard") }
                OutlinedTextField(clipboardDraft, { if (ToolProtocol.validText(it)) clipboardDraft = it }, label = { Text("Text for PC clipboard") },
                    supportingText = { Text("${clipboardDraft.toByteArray(Charsets.UTF_8).size} / 4096 bytes") }, minLines = 2, maxLines = 5,
                    enabled = !ui.tools.busy, modifier = Modifier.fillMaxWidth())
                Button(onClick = { model.writePcClipboard(clipboardDraft) }, enabled = !ui.tools.busy, modifier = Modifier.fillMaxWidth()) {
                    Text(if (clipboardDraft.isEmpty()) "Clear PC clipboard" else "Set PC clipboard")
                }
            } else Text("Clipboard access is unavailable. Request it before connecting, enable it in the PC settings, and approve it on the PC.", style = MaterialTheme.typography.bodySmall)
            ui.tools.clipboardPreview?.let { preview ->
                Text("Received from PC — review before copying", fontWeight = FontWeight.SemiBold)
                Surface(color = MaterialTheme.colorScheme.surfaceVariant, shape = RoundedCornerShape(8.dp)) {
                    SelectionContainer { Text(preview.ifEmpty { "(Empty clipboard)" }, modifier = Modifier.fillMaxWidth().heightIn(max = 180.dp).verticalScroll(rememberScrollState()).padding(10.dp)) }
                }
                OutlinedButton(onClick = {
                    val clip = ClipData.newPlainText("OpenRemote clipboard text", preview)
                    if (Build.VERSION.SDK_INT >= 33) clip.description.extras = PersistableBundle().apply { putBoolean(ClipDescription.EXTRA_IS_SENSITIVE, true) }
                    context.getSystemService(ClipboardManager::class.java)?.setPrimaryClip(clip)
                    model.noteClipboardCopied()
                }, modifier = Modifier.fillMaxWidth()) { Text("Copy received text to phone") }
                TextButton(onClick = { model.clearClipboardPreview() }) { Text("Clear preview") }
            }
            HorizontalDivider()
            Text("Send a file to PC", style = MaterialTheme.typography.titleMedium)
            Text("Choose a document with Android's picker. Maximum 64 MiB per file and 256 MiB per session, including cancelled uploads. Files are verified and saved without opening them.", style = MaterialTheme.typography.bodySmall)
            if (!ui.canUpload) Text("File transfer requires permission in the connection request and the PC's local settings.", style = MaterialTheme.typography.bodySmall)
            Button(onClick = onPickFile, enabled = ui.canUpload && !ui.tools.busy, modifier = Modifier.fillMaxWidth()) { Text("Choose file to send") }
            if (ui.tools.status.isNotEmpty()) Text(ui.tools.status, color = if (ui.tools.error) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurface, style = MaterialTheme.typography.bodySmall)
            if (ui.tools.busy) {
                ui.tools.progress?.let { progress -> LinearProgressIndicator(progress = { progress }, modifier = Modifier.fillMaxWidth()) }
                    ?: LinearProgressIndicator(modifier = Modifier.fillMaxWidth())
                if (!ui.tools.pickingDocument) OutlinedButton(onClick = { model.cancelTools() }, modifier = Modifier.fillMaxWidth()) { Text("Cancel action") }
            }
        }
    }, confirmButton = { TextButton(onClick = onDismiss) { Text("Close") } })
}
