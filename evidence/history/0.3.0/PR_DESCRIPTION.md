# Build native remote desktop clients and permissioned session tools

The supplied foundation could route a simulated session but could not establish a
real Windows-to-Android desktop. Its controller identity was not proven, signaling
could occur before approval, and native capture/WebRTC integration was unfinished.
Those gaps prevented meaningful device testing and made the release claim unsafe.

This change builds a Go HTTPS coordinator, Rust Windows host and native Android
controller around explicit identity, consent and session state. Phones sign
one-use challenges; hosts enroll with a private deployment token and protect
credentials with DPAPI. The coordinator persists PC identity, trust, revocation
and monotonic event sequence, and issues temporary TURN credentials.

The host now produces real H.264 WebRTC video and validates mouse/keyboard/Unicode
input. Android negotiates native receive-only video, waits for decoded frames,
provides phone controls and performs bounded recovery. Both clients implement the
shared tools contract for explicit clipboard actions and phone-to-PC uploads with
separate permissions, file integrity and cancellation cleanup.

Installer/deployment scripts, manually triggered owner-controlled CI, dependency
provenance, source fingerprints and exact build artifacts make the change
reviewable. PRODUCT_PLAN.md expands the intended product into concrete milestones
without presenting planned features as implemented ones.

## Relevant evidence

- Go race/unit tests and vet; independent HTTPS process/restart and authorization test.
- Rust tests, Linux/Windows-target Clippy, formatter and linked Windows executable.
- Android JVM tests, lint, native APK builds, package signature/alignment checks,
  dependency lock state and captured checksum verification metadata.
- Four actual installer file-activation/rollback tests, including a preserved
  failing regression and fix.
- Previous 30-frame encode/assemble/decode smoke on the unchanged production media path.
- Source and binary digests in component evidence and BUILD_STATUS.md.

## Review boundaries

This is not a release approval. The workspace blocks the networked media fixtures
before packets flow; no successful physical PC-to-phone session is asserted.
Native Windows clipboard/ACL/zone/rename behavior, Android SAF/Keystore/video,
public TURN UDP/TCP/TLS and installation/upgrade must run on suitable devices.

The coordinator is a trusted SDP-binding authority. Secure-desktop/service support,
independent peer negotiation signatures, production signing/updates, advanced
media/audio/monitors, reverse/resumable transfers and complete native redistribution
provenance remain open. A host crash can leave private `.part` files. See
`docs/release-gates.md` for the exact remaining acceptance criteria.

## Publishing

Prepared on `work/release-hardening` with the original archive as `main`.
No destination repository was supplied, so this description accompanies the
reviewable source handoff and has not been posted to an unrelated repository.
