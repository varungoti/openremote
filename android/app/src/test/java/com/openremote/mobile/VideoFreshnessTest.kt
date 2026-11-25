package com.openremote.mobile

import org.junit.Assert.assertEquals
import org.junit.Test

class VideoFreshnessTest {
    @Test fun connectedTransportNeedsNewDecodedFramesAndStallsAtFiveSeconds() {
        val monitor = VideoFreshness()
        monitor.connected(1000, 40)
        monitor.observe(1100, 40)
        assertEquals(VideoFreshness.State.WAITING, monitor.state(1100))
        monitor.observe(2000, 41)
        assertEquals(VideoFreshness.State.LIVE, monitor.state(6999))
        assertEquals(VideoFreshness.State.STALLED, monitor.state(7000))
        monitor.observe(9000, 41) // A stats callback by itself is not progress.
        assertEquals(VideoFreshness.State.STALLED, monitor.state(9000))
        assertEquals(VideoFreshness.State.FAILED, monitor.state(32000))
    }
    @Test fun freshFramesRecoverStalledCaptureAndRestartRequiresAnotherFreshFrame() {
        val monitor = VideoFreshness()
        monitor.connected(0, 0); monitor.observe(1000, 5)
        assertEquals(VideoFreshness.State.STALLED, monitor.state(6000))
        monitor.observe(7000, 6)
        assertEquals(VideoFreshness.State.LIVE, monitor.state(7000))
        monitor.disconnected(); assertEquals(VideoFreshness.State.DISCONNECTED, monitor.state(8000))
        monitor.connected(9000, 6); monitor.observe(9001, 6)
        assertEquals(VideoFreshness.State.WAITING, monitor.state(9001))
        monitor.observe(10000, 7); assertEquals(VideoFreshness.State.LIVE, monitor.state(10000))
    }
    @Test fun noFirstFrameFailsThirtySecondsAfterTransportConnection() {
        val monitor = VideoFreshness(); monitor.connected(100, 0)
        assertEquals(VideoFreshness.State.WAITING, monitor.state(30099))
        assertEquals(VideoFreshness.State.FAILED, monitor.state(30100))
    }
}
