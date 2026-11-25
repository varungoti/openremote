# Development changelog

## 0.8.0 — permanent PC identity, activation tray and agent access (engineering)

- Windows resolves service/identity before PIN, displays permanent nine-digit PC ID,
  and uses one Activate/Deactivate primary action with a receiving-PC launcher.
- Added confirmed activation-to-tray, claimed device names, actual active count,
  connection states, native tray actions and high-DPI/short-window scrolling.
- Added reusable 24-hour activation grants with four pending/live peers, one input
  owner, view-only followers, bounded orphan recovery and revocation on Stop/lockout.
- Android and browser now accept permanent PC ID/PIN and preserve activation identity
  and granted permissions; legacy one-use codes retain compatibility.
- Added a local stdio MCP bridge and production-browser screenshot/input interface,
  with named agent sessions, one-use fresh observation tokens and cancellation.
- Removed the Windows installer's unconfigured-preview bypass. An empty service
  profile always blocks customer installer packaging.
- Added exact build/source evidence and real isolated API checks. Public service,
  native media/tray/device sessions, signing and commercial release remain open.

## 0.7.0 — one-button connection (unconfigured development preview)

- Removed technical client setup fields and the normal Settings/Advanced/import paths.
- Windows uses one persistent Connect/Stop action, six-digit PIN and Server ID.
- Android and browser home use Server ID/PIN only; preview Android signing is retained.
- Added opt-in Ed25519 automatic host registration, same-key recovery and durable revoke.
- Bundled a pinned LGPL FFmpeg Media Foundation encoder, exact source and notices.
- Added strict empty-profile packaging gates with a labelled development exception.
- No public service deployment, native device session or production signing is claimed.

## 0.6.0 — code/PIN connection and receiving-PC browser viewer (development)

- Added owner-generated six-digit codes and chosen 6–12 digit PINs. Codes expire
  after ten minutes, are consumed once, and can be replaced or revoked.
- Added bounded salted PIN verification, wrong-PIN lockout, independent request
  budgets, concurrent-consumption protection and restart-ephemeral grants.
- Added native Windows Generate/Revoke/Copy controls and an in-memory local grant
  that must match before automatic acceptance; ordinary consent policy remains.
- Made code and PIN the primary Android flow, retained advanced PC lookup, and
  added careful one-use POST cancellation and late-session cleanup.
- Added an embedded same-origin browser viewer with P-256 proof, WebRTC video,
  display/fresh-frame input gating, mouse/keyboard/text and visible disconnect.
- Added an explicit Windows button opening the configured receiving-PC viewer.
- Added a shared public service profile and an owner-side profile generator;
  existing saved/enrolled origins take precedence. The delivered profile is empty.
- Added a separate OpenRemote Preview Android identity for safe side-by-side
  installation with the previous development app; release identity is unchanged.
- Updated the commercial release plan and exact build/package verification.
  Live deployment, physical device sessions, production signing, turnkey encoder
  packaging and the remaining advanced capabilities are still release gates.

## 0.5.1 — Windows first-run and installer repair (development)

- Replaced the default-config-and-exit startup path with a visible first-run
  setup/progress window and an explicit `--configure` settings entry.
- Added local server, PC name, verified encoder and masked first-enrollment key
  inputs. Failures stay visible for correction and retry before remote access.
- Added an executable per-user Windows installer with Start menu/settings
  shortcuts and an uninstall entry, preserving local identity and user data.
- Built the host as a Windows graphical application so normal launch does not
  rely on a transient console for setup errors.
- Android and coordinator remain at 0.5.0. Native Windows installation and
  Windows-to-phone remote operation remain release gates; see current evidence.

## 0.5.0 — phone setup and connection checks (development)

- Added a persistent visible Windows host home with PC number, full server
  origin, selectable setup link, explicit copy and Stop host. Closing it cancels
  remote access. Native layout, clipboard and shutdown still need Windows tests.
- Added Android setup-link paste/intent handling with strict parsing and explicit
  origin/PC confirmation. Import carries no credential or permission and does not
  authenticate, connect or grant trust before the user's subsequent action.
- Added staged server discovery, phone identity and optional PC availability
  checks with actionable local errors and configuration-only relay reporting.
- Corrected Android initial-settings, overlapping-write and cancelled-request
  ownership races found during independent review.
- Added bounded public `/v1/info` discovery and strict supported ICE URL validation.
  Unknown/duplicate/encoded transport values and noncanonical schemes fail startup.
- Updated installation, phone setup, first-use acceptance and release gate runbooks.
  Physical PC/phone, Internet/TURN, signing and general-release gates remain open.

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
