# Development changelog

## 0.4.0 — 18 November 2026

This is a development build. Physical Windows/Android and Internet release gates
remain open; the build does not claim a production release.

### Added

- GDI monitor enumeration and selection, with bounded opaque monitor IDs and a
  strict catalog carrying display labels, dimensions and the selected monitor.
- A phone **Displays** picker with **Switch and reconnect**. Each selection uses
  fresh authorization, peer and renderer ownership, preserves the original
  requested permissions and waits for the previous session to end.
- Immutable optional coordinator monitor selectors without expanded grants;
  old primary-monitor request behavior remains available.
- Physical-pixel capture/input binding, negative virtual-origin mapping,
  observed topology generation invalidation and bounded display-change shutdown.
- First-render input gating, deferred selected-display video presentation,
  stale-callback rejection and explicit renderer/file-picker wait handling.
- A 17-scenario physical-device/Internet pilot record and strict artifact/evidence
  validator. Every initial scenario is `not_run`; a valid record does not attest
  execution or establish release readiness.

### Corrected during review

- Pointer input re-reading a different primary monitor than the one being captured.
- Old selectors becoming usable again after an observed layout changed and returned.
- Duplicate control JSON fields being silently overwritten during parsing.
- New sessions racing unjoined video-task/encoder cleanup; unverified teardown
  now prevents replacement sessions instead of silently marking the host ready.
- Failed key, Unicode or mouse-button releases losing their tracking before retry.
- Old renderer deadlines causing immediate disconnect during a long-lived session's
  rotation, and renderer waits overriding the separately bounded document picker.

### Verification and remaining boundaries

Exact final tests, artifact hashes and native execution limits are recorded in
BUILD_STATUS.md. No new third-party dependency was introduced. Mixed-DPI behavior
must be tested with the exact separately supplied FFmpeg build; host DPI settings
do not configure that process. Software H.264 remains at up to 720p30. Audio,
hardware/adaptive encoding, reverse/resumable files, host settings/service lifecycle,
production signing/updates and complete native provenance remain open.

## 0.3.0 — 18 November 2026

This is a development build. It is not a production release.

### Added

- Explicit, separately permissioned PC text clipboard read/write with phone preview
  and deliberate copy; neither endpoint monitors the clipboard in the background.
- Phone-to-PC file upload over an independent reliable tools DataChannel: Android
  document picker, private hashed snapshot, progress/cancel, acknowledged chunks,
  bounded file/session quota and exact length/SHA-256 completion.
- Windows scoped receive directory, exclusive no-overwrite publication,
  Internet-zone metadata and cleanup/rollback on normal transfer failure or revocation.
- Staged per-user installer activation and executable/encoder verification;
  actual file-rollback regression tests; truthful source provenance for builds.
- An expanded product plan with staged delivery, requirement IDs, measurable
  acceptance criteria and explicit advanced-feature/release gates.

### Corrected

- Controller key-possession proof and one-use challenge handling; no copied-public-key login.
- Signaling before approval, permission escalation, stale authorization and durable phone revocation.
- Coordinator event-sequence reuse after restart and publication after persistence failure.
- Host heartbeat/presence bounds, historical-event false busy, orphan sessions,
  transient event-status lookup loss and active-state reporting after ICE recovery.
- Android connecting-state handling, exported trust key, NUL validation, stale-video
  input/tool gating, document-picker lifecycle and multi-instance snapshot cleanup.
- Cross-client filename rules, upload cancellation/finalization races and bounded tool errors.
- Installer rollback's PowerShell null argument, effective-name validation and
  preserving existing enrollment/configuration during staging.

### Release boundaries

The real WebRTC peers, desktop capture/input and Android renderer are implemented,
with compiled native packages. Network-free synthetic encoding has produced moving
frames through the production encoder/assembler path. Physical Windows/Android
behavior and actual networked media/tools remain unverified here: the media
fixtures are blocked by workspace socket/interface policy, and no devices are attached.

Audio, monitor switching, hardware/adaptive encoding, bidirectional/resumable files,
a Windows service/reboot recovery, independent peer-signed negotiation, biometric
consent and signed updates remain planned. Abrupt host crashes may leave private
partial upload files. Production signing, native dependency provenance and full
installation/Internet test matrices are still open.

## Original foundation baseline

The supplied foundation archive is preserved as the `main` Git baseline. It
contained a control-plane skeleton and native client placeholders. The implemented
branch is `work/release-hardening`; the archive SHA-256 is recorded in BUILD_STATUS.md.
