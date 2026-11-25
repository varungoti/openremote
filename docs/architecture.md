# Architecture and implemented trust boundaries

## Implemented components

The **Go coordinator** owns a private deployment's PC registry, cryptographic
controller login, host inbox, session authorization, SDP/ICE routing, expiring
TURN credentials and audit metadata. It persists identity/trust/revocation in a
single-writer atomic file. It never receives desktop video in its HTTP API.

The **interactive Windows Rust host** enrolls once and protects its bearer and
private identity with current-user DPAPI. It runs in an already logged-in desktop
session. Native approval and a visible emergency-stop window govern control. It
starts a verified external FFmpeg process for selected-monitor capture/encoding and
sends complete H.264 access units through webrtc-rs. The data-channel handler
validates every message and checks the approved input permission before SendInput.
A separate tools channel handles explicit clipboard actions and scoped file uploads
with independent local enable flags and server-approved permissions. No HTTP
endpoint handles file contents or clipboard data.

The **Android controller** uses Kotlin/Compose and native libwebrtc. It stores its
P-256 key in Keystore, signs one-use login challenges, requests a PC session, then
negotiates receive-only video and a control channel. Saved PC metadata is encrypted;
login/session bearers are memory-only. Media rendering and all input controls have
explicit lifecycle disposal. Ordinary backgrounding closes the session; the explicit
document picker has a two-minute, input-paused exception. Stalled decoded frames
disable input and tools after five seconds and end the session after thirty seconds.

The **coturn relay** is a separately deployed encrypted-packet forwarding service.
Caddy supplies HTTPS for the API. TURN/TLS needs its own valid certificate and an
available port/address. Neither has been deployed by this workspace run.

## Session contract

1. Locally configured host enrollment bearer authorizes creating a PC record.
2. The host receives its stable 9-digit ID and a private host bearer.
3. The phone proves ownership of a P-256 key by signing decoded random challenge
   bytes with SHA256withECDSA. Replayed or wrong-key proofs are rejected.
4. The coordinator derives the requester identity from that proof. The short PC
   ID does not authenticate anyone.
5. The host approves locally, or an explicitly pre-trusted local key is allowed
   under the unattended flag. It approves only implemented requested capabilities.
6. Only then may the peers fetch temporary TURN credentials and exchange real
   SDP/ICE. SDP exchange is `connecting`, never evidence of live desktop video.
7. WebRTC supplies encrypted media/data between endpoints. The Android UI waits
   for decoded video before enabling input.
8. Each input message is bounded, validated and authorized at the host. Disconnect
   releases held inputs and terminates capture. A limited disconnected grace period
   permits an authenticated ICE restart, then closes if recovery fails.
9. Persistent revocation blocks later requests even if the host's local trust
   configuration still lists that phone. Explicit host re-trust clears the tombstone.

## Coordinator trust

The coordinator is a trusted authorization and SDP-binding component. HTTPS and
WebRTC encryption protect the connections, but peers do not yet exchange
independent signatures binding every SDP/certificate fingerprint to out-of-band
identity keys. Do not claim resistance to a malicious coordinator or a full
independently verifiable peer-pairing protocol. The displayed fingerprint provides
human identity comparison, not a replacement for that missing protocol.

## Privilege scope

This version has no LocalSystem service, secure desktop access, unattended login
screen agent, reboot orchestration, remote terminal or update service. Remote
input remains subject to Windows integrity/UIPI boundaries. Per-user session
access is the implemented scope. The intended future service/session-agent split
must be independently reviewed and tested before extending those privileges.

## Planned capabilities

Hardware video acceleration, adaptive quality, multi-monitor selection, PC audio,
automatic clipboard sync, bidirectional/resumable files, durable reboot recovery,
biometric consent and signed updates
remain product requirements. They are not hidden behind working-looking buttons
or fake responses in this build. The feature/release gate matrix is authoritative.
