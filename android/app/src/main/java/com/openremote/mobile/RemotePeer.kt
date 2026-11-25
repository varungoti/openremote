package com.openremote.mobile

import android.content.Context
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import org.json.JSONObject
import org.webrtc.*
import java.nio.ByteBuffer
import java.time.Instant
import java.time.Duration
import java.util.concurrent.atomic.AtomicBoolean
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException

/** One foreground session, serialized on Main. HTTP callbacks never retain an Activity. */
class RemotePeer(
    context: Context,
    private val api: ApiClient,
    private val session: RemoteSession,
    private val onVideo: (VideoTrack?) -> Unit,
    private val onState: (String, Boolean) -> Unit,
    private val onRenderReady: (Boolean) -> Unit,
    private val onDiagnostics: (String) -> Unit,
    private val onCapabilities: (HostCapabilities) -> Unit,
    private val onDisplays: (DisplayCatalog) -> Unit,
    private val onToolsState: (Boolean, String?) -> Unit,
    private val onFatal: (String) -> Unit,
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val closed = AtomicBoolean(false)
    private val failureReported = AtomicBoolean(false)
    private val egl = EglBase.create()
    val eglContext: EglBase.Context get() = egl.eglBaseContext
    private val factory: PeerConnectionFactory
    private var peer: PeerConnection? = null
    private var control: DataChannel? = null
    private var toolsChannel: DataChannel? = null
    private var capabilities = HostCapabilities()
    private val displayReadiness = DisplayReadiness(session.monitorId)
    private var controlOpened = false
    private val toolsClient = SessionToolsClient(
        send = { request ->
            runCatching {
                val channel = toolsChannel
                val allowed = when {
                    request.kind == "file_cancel" -> true
                    request.kind.startsWith("file_") -> videoReady && permission.fileTransfer && capabilities.fileTransfer &&
                        (request.kind != "file_begin" || displayReadiness.ready(videoReady))
                    else -> displayReadiness.ready(videoReady) && permission.clipboard && capabilities.clipboard
                }
                allowed && !closed.get() && channel?.state() == DataChannel.State.OPEN && channel.bufferedAmount() <= 65_536 &&
                    channel.send(DataChannel.Buffer(ByteBuffer.wrap(request.bytes), false))
            }.getOrDefault(false)
        },
        closeChannel = { reason ->
            try { toolsChannel?.close() } finally { onToolsState(false, reason) }
        },
    )
    private val uploader = FileUploader(toolsClient)
    private var video: VideoTrack? = null
    private var publishedVideo: VideoTrack? = null
    private var cursor = 0L
    private var remoteReady = false
    private val earlyIce = ArrayDeque<IceCandidate>()
    private val localIce = ArrayDeque<IceCandidate>()
    private var localDescriptionSent = false
    private var restarting = false
    private var restarts = 0
    private var reconnectJob: Job? = null
    private var renewIceJob: Job? = null
    private var usesRelay = false
    private var transportConnected = false
    private var videoReady = false
    private var decodedFrames = 0L
    private val freshness = VideoFreshness()
    private val negotiation = Mutex()
    private var permission = session.permissions
    private var lastBytes = 0L
    private var lastStatsTime = 0.0

    init {
        synchronized(RemotePeer::class.java) {
            if (!initialized) {
                PeerConnectionFactory.initialize(PeerConnectionFactory.InitializationOptions.builder(context.applicationContext).createInitializationOptions())
                initialized = true
            }
        }
        factory = PeerConnectionFactory.builder()
            .setVideoDecoderFactory(DefaultVideoDecoderFactory(egl.eglBaseContext))
            .setVideoEncoderFactory(DefaultVideoEncoderFactory(egl.eglBaseContext, true, true))
            .createPeerConnectionFactory()
    }

    suspend fun start() = withContext(Dispatchers.Main.immediate) {
        check(!closed.get())
        val ice = api.iceConfiguration(session)
        scheduleIceRenewal(ice)
        val config = configuration(ice)
        val pc = factory.createPeerConnection(config, observer()) ?: error("WebRTC could not start")
        peer = pc
        // A data-only offer has no video m-line. recvonly MUST exist before offer creation.
        pc.addTransceiver(MediaStreamTrack.MediaType.MEDIA_TYPE_VIDEO,
            RtpTransceiver.RtpTransceiverInit(RtpTransceiver.RtpTransceiverDirection.RECV_ONLY))
            ?: error("Unable to request remote video")
        control = pc.createDataChannel("control", DataChannel.Init().apply { ordered = true })
            ?: error("Unable to create control channel")
        installChannel(control!!)
        toolsChannel = pc.createDataChannel("tools", DataChannel.Init().apply { ordered = true })
            ?: error("Unable to create tools channel")
        installToolsChannel(toolsChannel!!)
        createOffer(false)
        scope.launch { poll() }
        scope.launch {
            while (isActive) {
                delay(2_000)
                refreshVideoReadiness()
                peer?.getStats { report -> scope.launch { publishStats(report) } }
            }
        }
        scope.launch {
            delay(30_000)
            if (peer?.connectionState() != PeerConnection.PeerConnectionState.CONNECTED) {
                fail("The remote video connection timed out. Check the host and relay configuration.")
            }
        }
    }

    private fun configuration(ice: IceConfiguration): PeerConnection.RTCConfiguration {
        val servers = ice.servers.map {
            PeerConnection.IceServer.builder(it.urls).setUsername(it.username).setPassword(it.credential).createIceServer()
        }
        return PeerConnection.RTCConfiguration(servers).apply {
            sdpSemantics = PeerConnection.SdpSemantics.UNIFIED_PLAN
            iceTransportsType = if (ice.relayOnly) PeerConnection.IceTransportsType.RELAY else PeerConnection.IceTransportsType.ALL
            continualGatheringPolicy = PeerConnection.ContinualGatheringPolicy.GATHER_CONTINUALLY
            bundlePolicy = PeerConnection.BundlePolicy.MAXBUNDLE
        }
    }

    private fun observer() = object : PeerConnection.Observer {
        override fun onSignalingChange(state: PeerConnection.SignalingState?) {}
        override fun onIceConnectionChange(state: PeerConnection.IceConnectionState?) {}
        override fun onIceConnectionReceivingChange(receiving: Boolean) {}
        override fun onIceGatheringChange(state: PeerConnection.IceGatheringState?) {}
        override fun onIceCandidate(candidate: IceCandidate) {
            scope.launch {
                if (!localDescriptionSent) {
                    if (localIce.size >= 256) fail("Too many local network candidates") else localIce.addLast(candidate)
                } else sendIce(candidate)
            }
        }
        override fun onIceCandidatesRemoved(candidates: Array<out IceCandidate>?) {}
        override fun onAddStream(stream: MediaStream?) {}
        override fun onRemoveStream(stream: MediaStream?) {}
        override fun onDataChannel(channel: DataChannel) {
            // Controller owns the reliable channel; close unexpected remote-created channels.
            channel.close()
        }
        override fun onRenegotiationNeeded() {}
        override fun onAddTrack(receiver: RtpReceiver, streams: Array<out MediaStream>?) {
            (receiver.track() as? VideoTrack)?.let { track -> scope.launch { attachVideo(track) } }
        }
        override fun onTrack(transceiver: RtpTransceiver) {
            (transceiver.receiver.track() as? VideoTrack)?.let { track -> scope.launch { attachVideo(track) } }
        }
        override fun onConnectionChange(state: PeerConnection.PeerConnectionState) {
            scope.launch {
                if (closed.get()) return@launch
                when (state) {
                    PeerConnection.PeerConnectionState.CONNECTED -> {
                        reconnectJob?.cancel(); reconnectJob = null
                        restarting = false; restarts = 0
                        transportConnected = true; videoReady = false; freshness.connected(monotonicMillis(), decodedFrames)
                        displayReadiness.transportConnected(monotonicMillis())
                        onState("Encrypted link connected. Waiting for desktop…", false)
                    }
                    PeerConnection.PeerConnectionState.DISCONNECTED -> {
                        transportConnected = false; videoReady = false; freshness.disconnected()
                        onState("Connection interrupted. Reconnecting…", false)
                        scheduleRestart(2_000)
                    }
                    PeerConnection.PeerConnectionState.FAILED -> { transportConnected = false; videoReady = false; freshness.disconnected(); scheduleRestart(0) }
                    PeerConnection.PeerConnectionState.CONNECTING -> {
                        transportConnected = false; videoReady = false; freshness.disconnected()
                        onState("Establishing encrypted connection…", false)
                    }
                    PeerConnection.PeerConnectionState.CLOSED -> if (!closed.get()) fail("Remote connection closed")
                    else -> Unit
                }
            }
        }
    }

    private fun attachVideo(track: VideoTrack) {
        if (closed.get() || video === track) return
        video = track
        track.setEnabled(true)
        publishBoundVideo()
    }
    private fun publishBoundVideo() {
        // Never show primary-desktop pixels from a legacy host that ignored a selected monitor.
        // Matching the immutable catalog precedes renderer attachment; rendering precedes input.
        if (closed.get() || !displayReadiness.bindingReady) return
        val value = video ?: return
        if (publishedVideo === value) return
        publishedVideo = value
        onVideo(value)
    }

    private fun installChannel(channel: DataChannel) {
        var windowStarted = System.nanoTime()
        var received = 0
        fun opened() {
            if (closed.get() || controlOpened) return
            controlOpened = true
            scope.launch {
                delay(10_000)
                if (!displayReadiness.bindingReady) fail("The PC did not confirm the selected display. Reconnect to the primary display.")
            }
        }
        channel.registerObserver(object : DataChannel.Observer {
            override fun onBufferedAmountChange(previousAmount: Long) {}
            override fun onStateChange() {
                scope.launch {
                    if (closed.get()) return@launch
                    if (channel.state() == DataChannel.State.OPEN) opened()
                    if (channel.state() == DataChannel.State.CLOSED && peer?.connectionState() == PeerConnection.PeerConnectionState.CONNECTED) fail("The host closed the control channel")
                }
            }
            override fun onMessage(buffer: DataChannel.Buffer) {
                if (buffer.binary || buffer.data.remaining() > 16_384) {
                    scope.launch { fail("The PC sent an invalid control frame") }; return
                }
                val bytes = ByteArray(buffer.data.remaining()); buffer.data.get(bytes)
                scope.launch {
                    if (closed.get()) return@launch
                    val now = System.nanoTime()
                    if (now - windowStarted >= 1_000_000_000L) { windowStarted = now; received = 0 }
                    if (++received > 128) { fail("The PC exceeded the control message limit"); return@launch }
                    try {
                        when (val message = DisplayProtocol.decode(bytes)) {
                            is HostControlMessage.Capabilities -> {
                                displayReadiness.capabilities(message.value)
                                capabilities = message.value
                                onCapabilities(capabilities)
                            }
                            is HostControlMessage.Displays -> {
                                displayReadiness.catalog(message.value)
                                onDisplays(message.value)
                            }
                        }
                        publishBoundVideo()
                        refreshVideoReadiness()
                    } catch (_: Exception) { fail("The PC did not provide a valid, matching display binding. Reconnect to the primary display.") }
                }
            }
        })
        if (channel.state() == DataChannel.State.OPEN) opened()
    }

    private fun installToolsChannel(channel: DataChannel) {
        var windowStarted = System.nanoTime()
        var received = 0
        channel.registerObserver(object : DataChannel.Observer {
            override fun onBufferedAmountChange(previousAmount: Long) {}
            override fun onStateChange() { scope.launch {
                if (closed.get()) return@launch
                when (channel.state()) {
                    DataChannel.State.OPEN -> { toolsClient.opened(); onToolsState(true, null) }
                    DataChannel.State.CLOSING, DataChannel.State.CLOSED -> {
                        toolsClient.closed("Session tools closed. Reconnect to use clipboard or file transfer.")
                        onToolsState(false, "Session tools closed. The desktop remains available.")
                    }
                    else -> Unit
                }
            } }
            override fun onMessage(buffer: DataChannel.Buffer) {
                if (buffer.binary || buffer.data.remaining() > ToolProtocol.MAX_FRAME_BYTES) {
                    scope.launch { toolsClient.abort("The PC sent an invalid tools frame. The desktop remains available.") }; return
                }
                val bytes = ByteArray(buffer.data.remaining()); buffer.data.get(bytes)
                scope.launch {
                    if (closed.get()) return@launch
                    val now = System.nanoTime()
                    if (now - windowStarted >= 1_000_000_000L) { windowStarted = now; received = 0 }
                    if (++received > 128) toolsClient.abort("The PC exceeded the tools message limit. The desktop remains available.")
                    else toolsClient.receive(bytes)
                }
            }
        })
    }

    suspend fun clipboardRead(): String {
        check(displayReadiness.ready(videoReady) && permission.clipboard && capabilities.clipboard) { "Clipboard access is not available for this session" }
        return (toolsClient.request(ToolProtocol.clipboardRead()) as ToolResult.ClipboardText).text
    }
    suspend fun clipboardWrite(text: String) {
        check(displayReadiness.ready(videoReady) && permission.clipboard && capabilities.clipboard) { "Clipboard access is not available for this session" }
        toolsClient.request(ToolProtocol.clipboardWrite(text))
    }
    suspend fun upload(snapshot: UploadSnapshot, progress: (Long, Long) -> Unit): ToolResult.FileFinished {
        if (!(displayReadiness.ready(videoReady) && permission.fileTransfer && capabilities.fileTransfer)) {
            snapshot.close(); error("File transfer is not available for this session")
        }
        return uploader.upload(snapshot, progress)
    }

    private suspend fun createOffer(iceRestart: Boolean) = negotiation.withLock {
        val pc = peer ?: return@withLock
        check(pc.signalingState() == PeerConnection.SignalingState.STABLE) { "Previous negotiation is still pending" }
        localDescriptionSent = false
        val constraints = MediaConstraints().apply {
            if (iceRestart) mandatory.add(MediaConstraints.KeyValuePair("IceRestart", "true"))
        }
        val offer = withTimeout(15_000) { pc.createOfferAwait(constraints) }
        withTimeout(15_000) { pc.setDescriptionAwait(offer, local = true) }
        // Only advertise an SDP after native setLocalDescription has succeeded.
        api.sendSignal(session, "offer", JSONObject().put("type", "offer").put("sdp", offer.description))
        localDescriptionSent = true
        while (localIce.isNotEmpty()) sendIce(localIce.removeFirst())
    }

    private suspend fun sendIce(candidate: IceCandidate) {
        try {
            api.sendSignal(session, "ice", JSONObject().put("sdpMid", candidate.sdpMid)
                .put("sdpMLineIndex", candidate.sdpMLineIndex).put("candidate", candidate.sdp))
        } catch (e: CancellationException) { throw e }
        catch (_: Exception) { scheduleRestart(1_000) }
    }

    private suspend fun poll() {
        var failures = 0
        while (scope.isActive && !closed.get()) {
            try {
                val messages = api.pollSignals(session, cursor)
                for (message in messages) {
                    if (message.seq <= cursor) continue
                    when (message.kind) {
                        "answer" -> {
                            require(message.payload.getString("type") == "answer")
                            val answer = SessionDescription(SessionDescription.Type.ANSWER, message.payload.getString("sdp"))
                            withTimeout(15_000) { peer?.setDescriptionAwait(answer, local = false) }
                            remoteReady = true
                            while (earlyIce.isNotEmpty()) require(peer?.addIceCandidate(earlyIce.removeFirst()) == true) { "Remote network candidate rejected" }
                        }
                        "ice" -> {
                            val p = message.payload
                            val candidate = IceCandidate(p.optString("sdpMid", "0"), p.optInt("sdpMLineIndex", 0), p.getString("candidate"))
                            if (!remoteReady) {
                                require(earlyIce.size < 256) { "Too many remote network candidates" }
                                earlyIce.addLast(candidate)
                            } else require(peer?.addIceCandidate(candidate) == true) { "Remote network candidate rejected" }
                        }
                        "renegotiate" -> requestRestart()
                        else -> error("Unexpected signaling message from host")
                    }
                    // Advance only after success. A failed SDP is never silently skipped.
                    cursor = message.seq
                }
                failures = 0
            } catch (e: CancellationException) { throw e }
            catch (e: Exception) {
                if (e is ApiException && e.status in setOf(401, 403, 404, 409, 410)) { fail(e.message ?: "Session ended"); return }
                failures += 1
                if (failures > 6) { fail("Signaling connection failed: ${e.message?.take(120)}"); return }
                delay(minOf(8_000L, 500L shl (failures - 1)))
            }
        }
    }

    fun updatePermissions(value: PermissionSet) {
        if (!value.viewScreen) { fail("The PC removed screen access"); return }
        if (permission.controlInput && !value.controlInput) releaseAll()
        if ((permission.clipboard && !value.clipboard) || (permission.fileTransfer && !value.fileTransfer))
            toolsClient.abort("Tools permission was removed. Reconnect to request it again.")
        permission = value
    }

    fun requestRestart() { if (!closed.get()) scheduleRestart(700) }
    private fun scheduleRestart(afterMs: Long) {
        if (closed.get() || reconnectJob?.isActive == true || restarting) return
        reconnectJob = scope.launch {
            delay(afterMs)
            if (restarts >= 3) { fail("Connection could not recover. Please reconnect to the PC."); return@launch }
            restarting = true; restarts++
            // Credential rotation can negotiate a new path while the old encrypted path is healthy.
            // Preserve that fresh stream so a slow upload is not cancelled at every TURN renewal.
            val live = videoReady && transportConnected && freshness.state(monotonicMillis()) == VideoFreshness.State.LIVE
            if (!live) videoReady = false
            onState(if (live) "Refreshing connection…" else "Recovering connection ($restarts/3)…", live)
            try {
                val pc = peer ?: return@launch
                val refreshed = api.iceConfiguration(session)
                require(pc.setConfiguration(configuration(refreshed))) { "ICE configuration rejected" }
                scheduleIceRenewal(refreshed)
                remoteReady = false; earlyIce.clear()
                createOffer(true)
                delay(20_000)
                restarting = false
                if (pc.connectionState() != PeerConnection.PeerConnectionState.CONNECTED) {
                    reconnectJob = null
                    scheduleRestart(1_000)
                } else {
                    restarts = 0
                    if (videoReady) onState("Live desktop", true)
                }
            } catch (e: CancellationException) { throw e }
            catch (_: Exception) {
                restarting = false; reconnectJob = null; scheduleRestart(2_000)
            }
        }
    }


    private fun scheduleIceRenewal(ice: IceConfiguration) {
        renewIceJob?.cancel()
        val expiry = ice.expiresAt ?: return
        renewIceJob = scope.launch {
            delay(Duration.between(Instant.now(), expiry.minusSeconds(60)).toMillis().coerceAtLeast(15_000))
            if (closed.get()) return@launch
            if (usesRelay) {
                // TURN credentials are rotated before expiration and a fresh allocation is negotiated.
                requestRestart()
            } else {
                // Keep healthy direct links; fresh credentials are fetched for every future restart.
                try {
                    val fresh = api.iceConfiguration(session)
                    peer?.setConfiguration(configuration(fresh))
                    scheduleIceRenewal(fresh)
                } catch (e: CancellationException) { throw e }
                catch (_: Exception) { /* Next ICE restart always refreshes credentials again. */ }
            }
        }
    }

    fun pointer(action: String, x: Float, y: Float, buttons: Int = 0) {
        if (!x.isFinite() || !y.isFinite() || action !in setOf("move", "down", "up")) return
        send(JSONObject().put("v", 1).put("type", "pointer").put("action", action)
            .put("x", x.coerceIn(0f, 1f)).put("y", y.coerceIn(0f, 1f)).put("buttons", buttons))
    }
    fun scroll(dx: Float, dy: Float) {
        if (!dx.isFinite() || !dy.isFinite()) return
        send(JSONObject().put("v", 1).put("type", "scroll").put("dx", dx.coerceIn(-1200f, 1200f)).put("dy", dy.coerceIn(-1200f, 1200f)))
    }
    fun key(code: String, down: Boolean) {
        if (code.length !in 1..32) return
        send(JSONObject().put("v", 1).put("type", "key").put("code", code).put("down", down).put("modifiers", 0))
    }
    fun text(text: String): Boolean = ToolProtocol.validTypedText(text) && send(JSONObject().put("v", 1).put("type", "text").put("text", text))
    fun releaseAll() {
        runCatching {
            val channel = control ?: return
            if (closed.get() || channel.state() != DataChannel.State.OPEN || channel.bufferedAmount() > 262_144) return
            val bytes = "{\"v\":1,\"type\":\"release_all\"}".toByteArray(Charsets.UTF_8)
            // Best effort only: the host must also release held input on channel/peer teardown.
            channel.send(DataChannel.Buffer(ByteBuffer.wrap(bytes), false))
        }
    }
    private fun send(json: JSONObject): Boolean {
        if (closed.get() || !permission.controlInput || !displayReadiness.ready(videoReady)) return false
        val channel = control ?: return false
        if (channel.state() != DataChannel.State.OPEN) return false
        val bytes = json.toString().toByteArray(Charsets.UTF_8)
        if (bytes.size > 16_384) return false
        if (channel.bufferedAmount() > 262_144) {
            // Do not enqueue unbounded stale input. End, so host cleanup releases keys/buttons.
            fail("Remote input is congested. Reconnect to continue.")
            return false
        }
        val sent = runCatching { channel.send(DataChannel.Buffer(ByteBuffer.wrap(bytes), false)) }.getOrDefault(false)
        if (!sent) fail("Remote input could not be delivered")
        return sent
    }

    private fun publishStats(report: RTCStatsReport) {
        val transport = report.statsMap.values.firstOrNull { it.type == "transport" && it.members.containsKey("selectedCandidatePairId") }
        val pair = transport?.members?.get("selectedCandidatePairId")?.let { report.statsMap[it.toString()] }
        val local = pair?.members?.get("localCandidateId")?.let { report.statsMap[it.toString()] }
        val remote = pair?.members?.get("remoteCandidateId")?.let { report.statsMap[it.toString()] }
        usesRelay = local?.members?.get("candidateType") == "relay" || remote?.members?.get("candidateType") == "relay"
        val inbound = report.statsMap.values.firstOrNull { it.type == "inbound-rtp" && it.members["kind"] == "video" } ?: return
        val members = inbound.members
        decodedFrames = (members["framesDecoded"] as? Number)?.toLong() ?: decodedFrames
        freshness.observe(monotonicMillis(), decodedFrames)
        refreshVideoReadiness()
        val received = (members["bytesReceived"] as? Number)?.toLong() ?: 0
        val seconds = (report.timestampUs - lastStatsTime) / 1_000_000.0
        val rate = if (lastStatsTime > 0 && seconds > 0) (received - lastBytes) * 8.0 / seconds / 1_000_000 else 0.0
        lastStatsTime = report.timestampUs; lastBytes = received
        val width = (members["frameWidth"] as? Number)?.toInt() ?: 0
        val height = (members["frameHeight"] as? Number)?.toInt() ?: 0
        val fps = (members["framesPerSecond"] as? Number)?.toInt() ?: 0
        onDiagnostics("${width}×${height} · $fps fps · ${"%.1f".format(rate.coerceAtLeast(0.0))} Mbps · ${if (usesRelay) "Relay" else "Direct"}")
    }

    private fun monotonicMillis() = System.nanoTime() / 1_000_000
    fun attachRenderer(): Long {
        val token = displayReadiness.attachRenderer(monotonicMillis())
        pauseForRenderer()
        return token
    }
    fun rendererFirstFrame(token: Long) {
        if (closed.get()) return
        displayReadiness.rendered(token)
        onRenderReady(displayReadiness.renderedReady)
        refreshVideoReadiness()
    }
    fun detachRenderer(token: Long) {
        if (closed.get()) return
        if (displayReadiness.detachRenderer(token, monotonicMillis())) pauseForRenderer()
    }
    fun documentPickerHandoff(active: Boolean) { displayReadiness.pickerHandoff(active, monotonicMillis()) }
    private fun pauseForRenderer() {
        if (closed.get()) return
        if (videoReady) releaseAll()
        onRenderReady(false)
    }
    private fun refreshVideoReadiness() {
        if (closed.get()) return
        if (transportConnected && displayReadiness.rendererTimedOut(monotonicMillis())) {
            fail("The desktop did not render within 30 seconds. Reconnect to continue.")
            return
        }
        when (freshness.state(monotonicMillis())) {
            VideoFreshness.State.LIVE -> if (!videoReady && displayReadiness.bindingReady) {
                videoReady = true
                val selected = displayReadiness.catalog?.selected?.name
                onState(if (selected == null) "Live desktop" else "Live desktop · $selected", true)
            }
            VideoFreshness.State.STALLED -> if (videoReady) {
                videoReady = false
                releaseAll()
                onState("Desktop frames paused. Input and tools are disabled…", false)
            }
            VideoFreshness.State.FAILED -> fail("Desktop capture stopped sending decoded frames for 30 seconds. Reconnect after checking the PC capture and encoder.")
            else -> Unit
        }
    }

    private fun fail(message: String) { if (!closed.get() && failureReported.compareAndSet(false, true)) onFatal(message) }
    /** UI must remove renderer sinks before calling this on Main. Idempotent. */
    fun close() {
        if (closed.get()) return
        releaseAll()
        if (!closed.compareAndSet(false, true)) return
        scope.cancel()
        try { onVideo(null) } finally {
            toolsClient.closed("The remote session ended")
            listOfNotNull(toolsChannel, control).forEach { channel ->
                runCatching { channel.unregisterObserver() }; runCatching { channel.close() }; runCatching { channel.dispose() }
            }
            toolsChannel = null; control = null
            runCatching { peer?.close() }; runCatching { peer?.dispose() }; peer = null
            video = null; publishedVideo = null
            runCatching { factory.dispose() }; runCatching { egl.release() }
            earlyIce.clear(); localIce.clear()
        }
    }
    companion object { private var initialized = false }
}

private suspend fun PeerConnection.createOfferAwait(constraints: MediaConstraints): SessionDescription = suspendCancellableCoroutine { c ->
    createOffer(object : SdpObserver {
        override fun onCreateSuccess(sdp: SessionDescription) { if (c.isActive) c.resume(sdp) }
        override fun onCreateFailure(error: String) { if (c.isActive) c.resumeWithException(IllegalStateException("Offer failed: $error")) }
        override fun onSetSuccess() {}
        override fun onSetFailure(error: String) {}
    }, constraints)
}
private suspend fun PeerConnection.setDescriptionAwait(sdp: SessionDescription, local: Boolean) = suspendCancellableCoroutine<Unit> { c ->
    val observer = object : SdpObserver {
        override fun onCreateSuccess(sdp: SessionDescription) {}
        override fun onCreateFailure(error: String) {}
        override fun onSetSuccess() { if (c.isActive) c.resume(Unit) }
        override fun onSetFailure(error: String) { if (c.isActive) c.resumeWithException(IllegalStateException("SDP rejected: $error")) }
    }
    if (local) setLocalDescription(observer, sdp) else setRemoteDescription(observer, sdp)
}
