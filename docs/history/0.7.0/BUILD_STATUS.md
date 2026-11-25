# OpenRemote 0.7.0 — one-button build status

19 November 2026. **Unconfigured development preview; not release-ready.**

## What changed for the user

The Windows host uses one persistent window and one primary action, **Connect
remote desktop**. Choose exactly six PIN digits, then the same action prepares
the host automatically and displays its six-digit **Server ID**. It becomes
**Stop remote desktop** during preparation/hosting. The phone and PC browser home
ask only for Server ID and six-digit PIN with one current connection action.

The normal server address, enrollment credential, fingerprint, encoder-path and
checksum forms are gone. Android Advanced/setup imports and the Windows Settings
shortcut are removed. The installer bundles a pinned Windows encoder. An existing
legitimate service binding can be reused without silently changing identity/origin.

**No public coordinator/relay has been deployed for this project.** Both apps'
shared profile is empty. A fresh phone install disables Connect and shows provider
unavailable; the Windows host reports that a configured installer is needed. These
builds cannot yet make a fresh Internet connection. This work moved configuration
into publisher deployment/build inputs; it did not make Internet infrastructure
unnecessary. Default release packaging rejects an empty profile. The files here
use the explicit, labelled unconfigured-preview exception.

## Automatic registration and connection authorization

The host creates its own Ed25519 identity and proves ownership using a 60-second,
one-use challenge bound to purpose, the configured HTTPS origin, its public key,
name and platform. The administrator/enrollment secret stays on the server. Valid
existing credentials need no registration request. Same-key startup recovery can
rotate eligible automatic credentials, ending old sessions and invitations; public
recovery cannot restore an administrator-revoked host or take over legacy records.
Revocation persists across real process restarts. Server snapshot version 2 reads
legacy version 1 conservatively and retains ownership/tombstone state.

The visible Server ID is a cryptographically random six-digit invitation, unique
among active invitations on the same service. It expires after ten minutes, works
once and locks after five wrong PINs. It is not a permanent PC number. PINs and
pairing grants are not persisted. The coordinator applies rate/capacity limits and
the host must still consume its matching in-memory owner grant before approving.
The grant permits view and optionally input only. It does not create permanent
unattended access or grant clipboard, files, audio or power actions.

Stop/close invalidates the local grant and cancels active work. Failed cleanup
requires restart instead of overlapping attempts. PINs clear on submission and
relevant cancellation/lifecycle paths. One-use redemption is not blindly retried;
a known late session is ended, while a completely lost response still needs host
Stop or coordinator expiry.

## Executed verification

The aggregate record is `evidence/one-button-0.7.0/verification.json`. It links
component commands, exact source hashes, artifact hashes and retained failures.

| Area | Result | Scope |
| --- | --- | --- |
| Windows host | 99 Rust tests; format and Linux/Windows Clippy with warnings denied | State, identity, registration, framing and cleanup logic; not native UI execution. |
| Coordinator | 64 top-level tests / 150 including subcases, in both normal and race runs; vet and static build | Registration, pairing, session, storage, limits and concurrency. |
| Automatic registration over real HTTPS | 8 grouped checks passed | Independent Python Ed25519/P-256 clients against the compiled Go process, verified ephemeral CA, recovery/revocation and two real restarts. |
| Existing session/PIN over real HTTPS | 12 grouped checks passed | Actual isolated coordinator APIs, wrong PIN, identity, signaling, ICE credentials and restart; no desktop pixels. |
| Browser protocol/lifecycle | 15 core tests and 14 VM lifecycle tests passed | Actual production modules with explicit DOM/RTC doubles; no rendered browser. |
| Browser protocol to real Go HTTP | 6 grouped checks passed | Node WebCrypto proof, PIN denial/redemption, host approval, replay, ICE and stop. |
| Deployment/profile validation | 19 Python tests passed | Generated configuration and validation logic; no deployed server. |
| Installer admission | 16 tests passed | GUI host/source custody, encoder/source pins and unconfigured-release safeguards. |
| Android | 83 JVM tests, 13 suites; lint 0 errors/3 existing dependency-update warnings; strict/offline builds for four ABIs | All eight APK identity and ZIP alignment checks; retained preview signing and 64-bit native alignment; no physical phone. |
| Bundled encoder | Minimal FFmpeg 9.0.2 cross-build, component admission, source signature and AMD64 PE/import inspection passed | LGPL-only configured build with OS Media Foundation encoder; no Windows encoding/performance run. |

The optimized Windows x64 GUI executable linked successfully. NSIS compilation
with warnings treated as errors and independent 7-Zip extraction passed: all seven
owned payload files plus the generated native uninstaller match their expected
bytes. Final package hashes and command exits are in the aggregate evidence.

### Review findings and retained failures

Independent lifecycle review found a concrete panic edge: an attempt could unwind
while child session tasks remained detached, without canceling their token before
the UI entered restart-required. The shared supervisor now cancels child work
before returning success/error/panic and also on owner-task abort. Focused panic
and owner-abort regressions cover this boundary, with the failing-before mutation
retained alongside the corrected run. This is runtime/state evidence, not native
Windows reproduction.

The first encoder build completed compilation but failed required-component
admission because the configure name differs from the runtime `dump_extra` filter.
A clean corrected build passed. Intermediate Android/import and Windows compile
errors and the deliberate empty-profile/signing rejection checks remain in the
component evidence. The earlier Chromium launch restriction remains unbypassed.

## Distribution and unfinished release checks

The Windows installer is unsigned and targets Intel/AMD Windows x64. The existing
minimum target is Windows 10 version 1703 or Windows 11; the new Media Foundation
path still requires the supported-version/device matrix. The encoder's exact
source, build recipe and license/runtime notices accompany the installer. Remaining
Rust and Android WebRTC notice/provenance gaps still block public release.

The Android ARM64 preview targets Android 8 or later, retains package
`com.openremote.mobile.dev` and the exact 0.6 preview certificate. Its certificate
SHA-256 is `24c0441ceca6f73ea0e6de8a0958be6229cfcf2518328e8f7f9b71d98b980b28`.
It is built for updating 0.6 without removing data, but that physical update has
not run. Production identity remains `com.openremote.mobile` with no production
signing supplied. No iOS build or store publication is included.

No physical Windows install/upgrade/uninstall, native Win32/DPAPI/PIN interaction,
Media Foundation capture, SendInput, Android Compose/Keystore/media, live browser
rendering or Windows-to-phone session ran here. No public Internet/cellular/TURN
UDP/TCP/TLS/443, network-handoff or outage session was performed. The host must
remain in the logged-in interactive desktop; Windows service, UAC/lock-screen
control and reboot recovery remain outside this preview.

Before a customer release: deploy this project's real coordinator/relay, embed its
verified public origin in both apps, complete native/Internet and installer tests,
resolve final dependency notices, apply production signing and validate the
commercial release gates. No client-side configuration form is part of that plan.
