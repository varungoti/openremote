package com.openremote.mobile

/** Control-plane states are separate from transport/media readiness. */
sealed interface SessionDecision {
    data object AwaitApproval : SessionDecision
    data object StartPeer : SessionDecision
    data object KeepPeer : SessionDecision
    data class End(val reason: String) : SessionDecision
}

fun sessionDecision(status: String, peerStarted: Boolean): SessionDecision = when (status) {
    "pending" -> if (peerStarted) SessionDecision.End("Approval was withdrawn") else SessionDecision.AwaitApproval
    "approved", "connecting", "active" -> if (peerStarted) SessionDecision.KeepPeer else SessionDecision.StartPeer
    "rejected" -> SessionDecision.End("Connection declined on the PC")
    "ended", "expired" -> SessionDecision.End("The remote session ended")
    else -> SessionDecision.End("Unsupported session state")
}
