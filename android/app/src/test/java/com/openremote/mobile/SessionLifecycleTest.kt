package com.openremote.mobile

import org.junit.Assert.*
import org.junit.Test

class SessionLifecycleTest {
    @Test fun negotiationDoesNotEndOrDuplicatePeerWhenServerMovesToConnecting() {
        var starts = 0
        var ended = false
        for (status in listOf("pending", "approved", "connecting", "connecting", "active")) {
            when (sessionDecision(status, starts > 0)) {
                SessionDecision.StartPeer -> starts++
                is SessionDecision.End -> ended = true
                else -> Unit
            }
        }
        assertEquals("One native peer for one session", 1, starts)
        assertFalse("SDP negotiation is not a terminal state", ended)
        assertTrue(sessionDecision("ended", true) is SessionDecision.End)
    }
    @Test fun revokedPendingAndUnrecognizedStatesFailClosed() {
        assertTrue(sessionDecision("pending", true) is SessionDecision.End)
        assertTrue(sessionDecision("unknown", false) is SessionDecision.End)
        assertEquals(SessionDecision.AwaitApproval, sessionDecision("pending", false))
    }
}
