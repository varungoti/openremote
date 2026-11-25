# OpenRemote 0.4.0 — build and release status

**18 November 2026 · Development build · Public release blocked**

This continuation adds selected-monitor sessions to the native Windows host and
Android controller, and corrects session/renderer cleanup defects found during
review. Both native packages and the coordinator are built. **A physical
Windows-to-Android remote desktop session over the Internet has not been
demonstrated in this environment.**

The [product plan](PRODUCT_PLAN.md) retains the full requested advanced scope.
The [release gates](docs/release-gates.md) distinguish implemented source, executed
engineering checks and the physical-device/Internet work still required. No
destination repository or public deployment is configured.

## New in this build

- **Monitor selection:** up to 16 enumerated GDI displays, generic names,
  dimensions, primary/current indicators and opaque topology-generation IDs.
  Capture and input share one immutable physical-pixel selection.
- **Switch and reconnect:** the phone ends the old session, obtains its server
  acknowledgement and opens a new authorized peer/renderer for the chosen display.
  A PC approval may be required again. View-only choice cannot add control.
- **No silent fallback:** unknown/stale selectors fail; the phone withholds
  selected-session video until a valid catalog confirms the requested monitor.
  Observed layout changes release input and end the session. A restored layout
  cannot revive an earlier observed generation's token.
- **Lifecycle corrections:** cancelled callbacks recheck access under the input
  lock; failed virtual-key, Unicode and button releases remain tracked for retry;
  child tasks are joined and encoder termination must be confirmed. Unverified
  teardown blocks replacement sessions and requires local host restart.
- **Android renderer ownership:** old peer/renderer callbacks cannot authorize
  replacement input. Fresh decoded video and the current renderer's first-frame
  event gate input. Rotation has a fresh render deadline; the explicit two-minute
  document-picker handoff is preserved without enabling input in the background.
- **Pilot evidence workflow:** 17 initially unattempted physical-device/Internet
  scenarios, candidate/evidence byte checks and an explicit incomplete-gate failure.
  The validator checks records; it cannot attest execution or release readiness.

Existing permanent PC numbers, phone key-possession login, local approval,
unattended trust/revocation, native WebRTC, touch/keyboard controls, explicit text
clipboard, bounded phone-to-PC upload and deployment/installer tooling remain.

## Executed verification

| Check | Result | What it establishes |
| --- | --- | --- |
| Coordinator Go suite | 19 top-level tests plus subcases; race tests pass; app coverage 76.3% | Authentication, state, authorization, storage and immutable monitor selection; command package coverage remains separately 0% |
| Go vet | Passed | Static analysis |
| Independent HTTPS process test | 9 scenarios pass with verified temporary CA and actual restart | Coordinator behavior only; no desktop/media/input |
| Rust tests | **58 passed**, none failed/ignored | Geometry, strict parsing, lifecycle, tools and real temporary-file effects; native input uses explicit test adapters |
| Real child-process cleanup | Passed on Linux | Production termination helper actually waits for a spawned OS child; not Windows capture validation |
| Rust checks and Windows build | Formatter and Linux/Windows Clippy `-D warnings` pass; optimized x64 gnullvm executable linked | Compile/static gates; unsigned and not run on Windows |
| Encoder smoke | 30 complete access units, 30 decoded frames and 30 distinct frame hashes | Fresh network-free synthetic FFmpeg/assembler/decode path; no WebRTC or Windows capture |
| Android JVM tests | **46 passed**, no failures/errors/skipped | API/catalog/renderer-state/time/transfer/lifecycle behavior |
| Android lint | **0 errors, 5 existing dependency-version warnings** | Static checks, not a security audit |
| Android packages | Debug and minified unsigned release for four ABIs; instrumentation APK built | Instrumentation/device tests were not executed |
| Android artifact checks | ARM64 debug signature, debug/release 16 KiB ZIP alignment and native ELF LOAD alignment pass | Package compatibility checks; not execution on a 16 KiB-page device |
| Strict dependency build | Offline locked/checksum-verified build passes | 91 locked configurations, 133 locked components; 367 components / 619 artifact hashes across verification scopes |
| Python suite | **16 passed**: 13 pilot-record and 3 deployment-config tests | Synthetic record rejection and private deployment configuration; no pilot/cloud deployment |
| Independent source review | Concrete findings corrected and final source pins recorded | Focused engineering review; not a completed independent security audit |

The final Android build took **1m 35s**, with 133 tasks (24 executed, 109 up to
date). Its 44 source inputs were unchanged across the final run. The earlier
43-test intermediate run is retained separately; it does not substitute for the
final corrected result.

Four installer activation/rollback tests and PowerShell syntax checks passed in
0.3. All four script byte streams are unchanged and were compared again; those
results are carried forward, not represented as newly executed native Windows
installation tests. The fresh 0.4 encoder smoke supersedes the earlier carried
encoder result because session/encoder teardown changed in this continuation.

## Exact principal artifacts

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| Windows x64 test executable | 9,356,800 | `37ea1ec8177abffd9353c0f5cd59047bcdf68924772874225d43c908076200cf` |
| Linux amd64 coordinator | 10,711,963 | `ba9336b949b2dc7a78766e512c9662b168958e1b2a1a621d403347ddc8413951` |
| Android ARM64 debug APK | 24,631,759 | `c9b78473a65ade3ec64dd444ecec83c201ca4d90a76723abbc4f57efefe13e0e` |
| Android ARM64 unsigned release APK | 13,780,002 | `3c5868cefb1ccbcabd88880c2b58be2e56c31f41366080d4202834fd9cbf3df0` |

The debug APK uses the same development certificate as the 0.3 test APK and has
versionCode 4. This supports the intended upgrade path but does not establish
that a real-device upgrade succeeded. The release APK is unsigned and must not
be described as installable production distribution. The Windows executable is
unsigned. No FFmpeg executable or signing key is bundled.

Component records bind **19 host inputs, 44 Android inputs and 12 coordinator
inputs** to their content hashes. Android also records nine APKs and 22 evidence
files. Source/byte comparisons passed against those records. See
[consolidated evidence](evidence/final-build-verification.json),
[host evidence](agent-windows/BUILD_EVIDENCE.json),
[Android verification](android/VERIFICATION.md),
[coordinator evidence](server/verification/monitor-verification.json) and
[review findings](evidence/monitor-review-0.4.md).

## Physical and Internet gates still open

1. **Windows and Android devices:** no interactive Windows desktop or attached
   Android phone/emulator was available. Native capture/input, DPAPI/Keystore,
   consent, renderer/SAF lifecycle, clipboard, ACL/zone metadata, file rename and
   installation require actual execution.
2. **Multiple monitors:** negative origins, portrait, mixed DPI and unplug during
   held input must run with the exact external FFmpeg binary. Host DPI awareness
   does not set that process's DPI behavior. OS changes and topology reads are not
   atomic; a zero-frame/zero-event hardware guarantee is not claimed.
3. **Networked media:** prior independent native fixtures were blocked by
   `netlinkrib: operation not permitted` before media flowed. Chromium was blocked
   at local-socket initialization. They received no verified remote frames and
   were not retried or bypassed for0.4.
4. **Public connectivity:** no coordinator/coturn public deployment exists here.
   Direct Internet, TURN UDP/TCP/TLS443, carrier NAT, credential renewal and
   Wi-Fi/cellular handoff remain unexecuted.
5. **Distribution and operations:** production signing identities, full native
   dependency provenance/notices, supported-system performance, backup/restore,
   installation/upgrade and the independent security review remain open.

The [pilot runbook](docs/pilot-validation.md) supplies 17 candidate-bound cases.
All begin as `not_run`; `--require-pass` must reject that initial record.

## Current limitations and next scope

The host runs in an already logged-in user desktop. Software H.264 is limited to
up to **1280×720 / 30 FPS**; optional DXGI capture still requires one monitor.
Monitor changes reconnect, rather than switching inside an existing video stream.
Topology failures end access; the user reconnects to refresh available displays.
Explicit unattended trust can select any offered monitor; per-monitor unattended
allowlists are not implemented.

Clipboard is explicit text only (4 KiB). File transfer remains phone-to-PC only,
64 MiB/file and 256 MiB reservations/session; no arbitrary filesystem browsing,
automatic execution or resume. Abrupt host crash/power loss can leave private
partial upload files. No new claim of crash cleanup was introduced by this build.

Hardware/adaptive encoding, PC audio, reverse/resumable transfers, host settings
and trust-management UI, service/reboot/secure-desktop support, independent peer
negotiation signatures, biometric gating, signed updates and iOS remain planned.
The next media implementation slice follows the product plan; physical validation
is still required before these existing components can be called release-ready.

## Dependency and source provenance

No new third-party package was introduced. The refreshed inventory contains 319
locked Rust registry packages and 98 Android release-runtime components; all
743 unique referenced local notice/metadata/input files matched their recorded
bytes. Four Rust notice gaps and the exact packaged native WebRTC source/build/
transitive notice mapping remain unresolved. First-seen Gradle checksums do not
constitute publisher attestation or redistribution clearance.

The original foundation remains the `main` baseline, archive SHA-256
`179962e6d3c3ed83f0e9cf24b0aac591a08aa80cfdf356ada877ddc7d282522a`.
Implementation continues on `work/release-hardening`. Previous 0.3 records and
intermediate evidence are retained with explicit historical scope. No PR was
posted because no destination repository was supplied.

The reviewed implementation is captured in commit
`53dccfc7ddcd8460bfbed43d9a363a08c58021ae`. The 19 Windows, 44 Android and
12 coordinator build inputs match those committed bytes. These unsigned development
binaries were built before that consolidation commit; this is exact input custody,
not an independently reproducible or publisher-signed build attestation. The
candidate-bound pilot record contains 17 `not_run` cases and was verified to fail
the explicit completion gate.
