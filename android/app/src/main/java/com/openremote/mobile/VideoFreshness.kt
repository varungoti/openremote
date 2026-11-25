package com.openremote.mobile

/** A connected transport is insufficient: input is safe only while decoded video continues advancing. */
internal class VideoFreshness {
    enum class State { DISCONNECTED, WAITING, LIVE, STALLED, FAILED }
    private var connectedAt: Long? = null
    private var advancedAt: Long? = null
    private var frames = 0L
    fun connected(nowMillis: Long, decodedFrames: Long) {
        connectedAt = nowMillis; advancedAt = null; frames = decodedFrames
    }
    fun disconnected() { connectedAt = null; advancedAt = null }
    fun observe(nowMillis: Long, decodedFrames: Long) {
        if (connectedAt == null) return
        if (decodedFrames > frames) advancedAt = nowMillis
        frames = decodedFrames
    }
    fun state(nowMillis: Long): State {
        val start = connectedAt ?: return State.DISCONNECTED
        val elapsed = (nowMillis - (advancedAt ?: start)).coerceAtLeast(0)
        return when {
            elapsed >= 30_000 -> State.FAILED
            advancedAt == null -> State.WAITING
            elapsed >= 5_000 -> State.STALLED
            else -> State.LIVE
        }
    }
}
