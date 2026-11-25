package com.openremote.mobile

/** Fixed-session display binding; changing a monitor requires a new peer and renderer. */
data class RemoteMonitor(val id: String, val name: String, val width: Int, val height: Int, val primary: Boolean)
data class DisplayCatalog(val selectedMonitorId: String, val monitors: List<RemoteMonitor>) {
    val selected: RemoteMonitor get() = monitors.first { it.id == selectedMonitorId }
}

internal sealed interface HostControlMessage {
    data class Capabilities(val value: HostCapabilities) : HostControlMessage
    data class Displays(val value: DisplayCatalog) : HostControlMessage
}

internal object DisplayProtocol {
    private val idPattern = Regex("[0-9a-f]{64}")
    fun validId(value: String): Boolean = idPattern.matches(value)

    fun decode(bytes: ByteArray): HostControlMessage {
        val value = StrictToolJson.parse(bytes, allowArrays = true)
        require(value["v"] == 1L) { "Unsupported control version" }
        return when (value["type"]) {
            "capabilities" -> {
                val allowed = setOf("v", "type", "text", "clipboard", "fileTransfer", "multiMonitor", "audio", "power")
                require(value.keys.all { it in allowed }) { "Unknown capability fields" }
                for (key in allowed - setOf("v", "type")) require(!value.containsKey(key) || value[key] is Boolean) { "Invalid capability flag" }
                HostControlMessage.Capabilities(HostCapabilities(
                    text = value["text"] == true, clipboard = value["clipboard"] == true,
                    fileTransfer = value["fileTransfer"] == true, multiMonitor = value["multiMonitor"] == true,
                ))
            }
            "display_catalog" -> {
                require(value.keys == setOf("v", "type", "selectionMode", "selectedMonitorId", "monitors") && value["selectionMode"] == "reconnect") { "Invalid display catalog" }
                val selected = value["selectedMonitorId"] as? String ?: error("Invalid selected display")
                require(validId(selected)) { "Invalid selected display" }
                val entries = value["monitors"] as? List<*> ?: error("Invalid display list")
                require(entries.size in 1..16) { "Invalid display count" }
                val monitors = entries.map { entry ->
                    val monitor = entry as? Map<*, *> ?: error("Invalid display entry")
                    require(monitor.keys == setOf("id", "name", "width", "height", "primary")) { "Invalid display fields" }
                    val id = monitor["id"] as? String ?: error("Invalid display identity")
                    val name = monitor["name"] as? String ?: error("Invalid display label")
                    val width = monitor["width"] as? Long ?: error("Invalid display width")
                    val height = monitor["height"] as? Long ?: error("Invalid display height")
                    val primary = monitor["primary"] as? Boolean ?: error("Invalid primary display flag")
                    require(validId(id) && name.isNotEmpty() && name.toByteArray(Charsets.UTF_8).size <= 80 && name.none(Character::isISOControl)) { "Invalid display identity or label" }
                    require(width in 1..32768 && height in 1..32768) { "Invalid display dimensions" }
                    RemoteMonitor(id, name, width.toInt(), height.toInt(), primary)
                }
                require(monitors.map { it.id }.toSet().size == monitors.size && monitors.count { it.primary } == 1 && monitors.any { it.id == selected }) { "Ambiguous display catalog" }
                HostControlMessage.Displays(DisplayCatalog(selected, monitors))
            }
            else -> throw IllegalArgumentException("Unsupported host control message")
        }
    }
}

/** Pure state used by RemotePeer: neither a catalog nor a decoded frame alone enables input. */
internal class DisplayReadiness(private val requestedMonitorId: String?) {
    var capabilities: HostCapabilities? = null
        private set
    var catalog: DisplayCatalog? = null
        private set
    private var rendererSequence = 0L
    private var activeRenderer: Long? = null
    private var rendered = false
    private var rendererWaitingSince: Long? = null
    private var pickerHandoff = false
    init { require(requestedMonitorId == null || DisplayProtocol.validId(requestedMonitorId)) }

    fun capabilities(value: HostCapabilities) {
        require(capabilities == null || capabilities == value) { "The host changed its capabilities during this session" }
        require(requestedMonitorId == null || value.multiMonitor) { "The PC did not confirm the selected display" }
        capabilities = value
    }
    fun catalog(value: DisplayCatalog) {
        require(capabilities?.multiMonitor == true) { "The PC did not advertise display selection" }
        require(requestedMonitorId == null || value.selectedMonitorId == requestedMonitorId) { "The PC selected a different display" }
        require(catalog == null || catalog == value) { "The PC changed its display layout. Reconnect to the primary display." }
        catalog = value
    }
    val bindingReady: Boolean get() = capabilities?.let { if (it.multiMonitor) catalog != null else requestedMonitorId == null } == true
    val renderedReady: Boolean get() = activeRenderer != null && rendered
    fun attachRenderer(nowMillis: Long = 0): Long {
        activeRenderer = ++rendererSequence; rendered = false
        rendererWaitingSince = nowMillis
        return rendererSequence
    }
    fun rendered(token: Long) { if (activeRenderer == token) { rendered = true; rendererWaitingSince = null } }
    fun detachRenderer(token: Long, nowMillis: Long = 0): Boolean {
        if (activeRenderer != token) return false
        activeRenderer = null; rendered = false; rendererWaitingSince = nowMillis
        return true
    }
    fun transportConnected(nowMillis: Long) { if (!renderedReady) rendererWaitingSince = nowMillis }
    fun pickerHandoff(active: Boolean, nowMillis: Long) {
        if (pickerHandoff && !active && !renderedReady) rendererWaitingSince = nowMillis
        pickerHandoff = active
    }
    fun rendererTimedOut(nowMillis: Long): Boolean = !pickerHandoff && !renderedReady &&
        rendererWaitingSince?.let { nowMillis - it >= 30_000 } == true
    fun ready(freshDecodedFrames: Boolean): Boolean = bindingReady && renderedReady && !pickerHandoff && freshDecodedFrames
}
