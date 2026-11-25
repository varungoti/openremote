package com.openremote.mobile

import android.content.Context
import android.annotation.SuppressLint
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.view.GestureDetector
import android.view.MotionEvent
import android.view.View
import android.view.ViewConfiguration
import android.widget.FrameLayout
import org.webrtc.EglBase
import org.webrtc.RendererCommon
import org.webrtc.SurfaceViewRenderer
import org.webrtc.VideoTrack
import kotlin.math.abs

/** Renderer and touch overlay have a single, explicit disposal owner. */
// Constructed only by Compose after a live EGL context exists, never inflated from XML.
@SuppressLint("ViewConstructor")
class RemoteDisplay(
    context: Context,
    egl: EglBase.Context,
    private val peer: () -> RemotePeer?,
    private val onFirstFrame: () -> Unit,
    private val onReleased: () -> Unit,
) : FrameLayout(context) {
    private val renderer = SurfaceViewRenderer(context)
    private val input = InputOverlay(context)
    private var track: VideoTrack? = null
    private var released = false
    private var videoWidth = 0
    private var videoHeight = 0
    var trackpad = true
    var inputEnabled = false
        set(value) { if (field && !value) input.cancelTouches(); field = value }
    init {
        setBackgroundColor(Color.BLACK)
        renderer.init(egl, object : RendererCommon.RendererEvents {
            override fun onFirstFrameRendered() { post { if (!released && track != null) onFirstFrame() } }
            override fun onFrameResolutionChanged(width: Int, height: Int, rotation: Int) {
                post { if (!released) { videoWidth = if (rotation % 180 == 0) width else height; videoHeight = if (rotation % 180 == 0) height else width } }
            }
        })
        renderer.setScalingType(RendererCommon.ScalingType.SCALE_ASPECT_FIT)
        renderer.setEnableHardwareScaler(true)
        addView(renderer, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT))
        addView(input, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT))
        input.contentDescription = "Remote desktop. Trackpad mode: move a finger to move the pointer, tap to click, hold for right click, two fingers to scroll."
    }
    fun bind(value: VideoTrack?) {
        if (released || track === value) return
        track?.removeSink(renderer); track = value; value?.addSink(renderer)
    }
    fun release() {
        if (released) return
        released = true
        val previous = track; track = null
        runCatching { previous?.removeSink(renderer) }
        runCatching { input.cancelTouches() }
        try { renderer.release() } finally { onReleased() }
    }

    private inner class InputOverlay(context: Context) : View(context) {
        private var cursor = NormalizedPoint(.5f, .5f)
        private var downPoint: NormalizedPoint? = null
        private var pointerHeld = false
        private var longPressed = false
        private var twoFinger = false
        private var lastX = 0f
        private var lastY = 0f
        private var startX = 0f
        private var startY = 0f
        private var moved = false
        private val slop = ViewConfiguration.get(context).scaledTouchSlop
        private val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = Color.WHITE; strokeWidth = 2f; style = Paint.Style.STROKE }
        private val longPress = Runnable {
            if (inputEnabled && !moved && !twoFinger && downPoint != null) {
                if (pointerHeld) peer()?.pointer("up", cursor.x, cursor.y, 1)
                pointerHeld = false; longPressed = true
                click(2)
            }
        }
        private var lastTapAt = 0L
        override fun performClick(): Boolean { super.performClick(); return true }
        private fun content() = RemoteCoordinates.fit(width, height, videoWidth, videoHeight)
        private fun click(button: Int) {
            peer()?.pointer("down", cursor.x, cursor.y, button)
            peer()?.pointer("up", cursor.x, cursor.y, button)
        }
        fun cancelTouches() {
            removeCallbacks(longPress)
            if (pointerHeld) peer()?.pointer("up", cursor.x, cursor.y, 0)
            pointerHeld = false; downPoint = null; twoFinger = false
        }
        override fun onTouchEvent(event: MotionEvent): Boolean {
            if (!inputEnabled || released) { cancelTouches(); return false }
            val rect = content() ?: return true
            when (event.actionMasked) {
                MotionEvent.ACTION_DOWN -> {
                    val p = RemoteCoordinates.map(event.x, event.y, rect) ?: return true
                    startX = event.x; startY = event.y; lastX = event.x; lastY = event.y
                    moved = false; longPressed = false; twoFinger = false; downPoint = p
                    if (!trackpad) cursor = p
                    // Second tap held is a drag. The first tap already produced a click.
                    val doubleTap = event.eventTime - lastTapAt in 1..ViewConfiguration.getDoubleTapTimeout().toLong()
                    if (!trackpad || doubleTap) { peer()?.pointer("down", cursor.x, cursor.y, 1); pointerHeld = true }
                    postDelayed(longPress, ViewConfiguration.getLongPressTimeout().toLong())
                }
                MotionEvent.ACTION_POINTER_DOWN -> {
                    twoFinger = true; removeCallbacks(longPress)
                    if (pointerHeld) peer()?.pointer("up", cursor.x, cursor.y, 0)
                    pointerHeld = false
                    lastX = event.x; lastY = event.y
                }
                MotionEvent.ACTION_MOVE -> {
                    if (downPoint == null) return true
                    val dx = event.x - lastX; val dy = event.y - lastY
                    if (abs(event.x - startX) > slop || abs(event.y - startY) > slop) { moved = true; removeCallbacks(longPress) }
                    if (event.pointerCount >= 2) {
                        peer()?.scroll(-dx * 3, -dy * 3)
                    } else if (!twoFinger && !longPressed) {
                        cursor = if (trackpad) NormalizedPoint((cursor.x + dx / rect.width).coerceIn(0f, 1f), (cursor.y + dy / rect.height).coerceIn(0f, 1f))
                            else RemoteCoordinates.map(event.x, event.y, rect, clamp = true) ?: cursor
                        peer()?.pointer("move", cursor.x, cursor.y, if (pointerHeld) 1 else 0)
                    }
                    lastX = event.x; lastY = event.y; invalidate()
                }
                MotionEvent.ACTION_UP -> {
                    removeCallbacks(longPress)
                    if (downPoint == null) return true
                    if (pointerHeld) peer()?.pointer("up", cursor.x, cursor.y, 1)
                    else if (!moved && !longPressed && !twoFinger) click(1)
                    if (!moved && !longPressed && !twoFinger) { lastTapAt = event.eventTime; performClick() } else lastTapAt = 0
                    pointerHeld = false; downPoint = null; invalidate()
                }
                MotionEvent.ACTION_CANCEL -> cancelTouches()
            }
            return true
        }
        override fun onDraw(canvas: Canvas) {
            super.onDraw(canvas)
            if (!trackpad || !inputEnabled) return
            val rect = content() ?: return
            val x = rect.left + cursor.x * rect.width; val y = rect.top + cursor.y * rect.height
            canvas.drawCircle(x, y, 8f, paint)
            canvas.drawLine(x - 13f, y, x + 13f, y, paint); canvas.drawLine(x, y - 13f, x, y + 13f, paint)
        }
    }
}
