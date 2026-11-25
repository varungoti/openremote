# Android verification — 18 November 2026

## Result

OpenRemote Android **0.4.0 (version code 4)** builds actual debug and unsigned release
APKs for arm64-v8a, armeabi-v7a, x86 and x86_64. This increment adds monitor selection
through a new remote session, strict display binding and renderer ownership to the
existing screen/input, text, clipboard and phone-to-PC upload implementation.

The result is **build-verified development software**. No physical phone or emulator
was available. These checks do not establish production release readiness, actual
Windows-to-Android video/input, Internet relay behavior, or native UI correctness.

## Final checks executed successfully

The final build used the exact **44 inputs** in `verification/source-inputs.sha256`,
strict dependency locking and strict artifact verification. Hashes were verified
before and after the build; no input changed during execution.

```sh
gradle -p android :app:assembleDebug :app:testDebugUnitTest --rerun \
  :app:lintDebug :app:assembleRelease :app:assembleDebugAndroidTest \
  --dependency-verification strict --offline --no-daemon \
  --max-workers=2 --stacktrace --console=plain
```

- **BUILD SUCCESSFUL in 1m 35s**; 133 tasks, 24 executed and 109 up-to-date.
- **46 JVM tests passed**, zero failures, errors or skipped cases. The complete test
  task was explicitly rerun. Raw JUnit XML is retained in `verification/junit/` and
  the aggregate is `verification/tests.json`.
- Debug lint: **0 errors, 5 warnings**, all existing dependency-version suggestions
  for the deliberately pinned Compose/Lifecycle/MockWebServer set. No new permission,
  API, context or display UI lint error remained.
- Debug and unsigned release APKs were assembled for all four ABIs. The Keystore
  instrumentation APK compiled; it was **not executed on a device**.
- The final ARM64 debug APK passed `apksigner verify --verbose --print-certs`, using
  APK Signature Scheme v2 and the local debug identity. It is not an owner-controlled
  production signing identity.
- Final ARM64 debug and release APKs passed `zipalign -c -P 16 4`.
- Both native ARM64 libraries extracted from the final release APK passed checks for
  16 KiB-compatible ELF LOAD alignment and offset/address congruence. A physical or
  emulator run with 16 KiB pages is still required.

Toolchain: checksummed Temurin JDK 21.0.13+9, Gradle 8.13, Android SDK Platform 36,
Build Tools 35.0.0, AGP 8.13.2, Kotlin/Compose compiler 2.2.21 and native WebRTC
150.7871.04. These toolchains and resolved dependencies were actually used.

## Observable contracts covered by tests

| Suite | Tests | Verification |
| --- | ---: | --- |
| ApiClientTest | 7 | Real P-256 challenge proof, bearer scope, HTTPS/PC IDs, cancellable polling, selected-monitor request/echo, immutable status binding, missing-echo cleanup without primary fallback |
| DisplayProtocolTest | 4 | Valid portrait catalog, older host capabilities, strict types/Unicode/UTF-8 size/fields, duplicate keys and IDs, unique primary/selected membership, list/dimension/identifier bounds |
| DisplayReadinessTest | 7 | Exact binding before video publication/input readiness, selected-monitor rejection of legacy hosts, fixed topology, stale renderer frames/disposal, view-only/tool UI gates, clocked rotation and SAF handoff deadlines |
| RemoteCoordinatesTest | 3 | Letterbox mapping and invalid coordinates |
| SessionLifecycleTest | 2 | Pending/approved/connecting/active progression and terminal states |
| ToolProtocolTest | 6 | Strict JSON/Unicode/IDs, Windows filenames, frame limits, exact ACKs and final size/hash/name |
| SessionToolsClientTest | 4 | One in-flight request, ordinary denial, retired IDs, no timeout replay, prompt close and scoped malformed-frame failure |
| FileUploadTest | 10 | Chunk/progress, empty files, wrong ACKs, cancellation, bounded provider reads, quotas, native-close failure, denied begin and shared-cache recovery |
| VideoFreshnessTest | 3 | First decoded progress, five-second pause, thirty-second failure and recovery |

The display reducer is used by the production peer. Unit tests exercise its readiness
and timer decisions, plus the actual `SessionUi` availability properties. They do not
instantiate a native WebRTC peer or render a Compose/SurfaceView screen. Those native
callbacks and UI interactions remain device-test obligations.

## Display and lifecycle behavior implemented

The display picker is advertised only after a compatible host supplies its strict
catalog. It shows display labels, resolution, primary and current state. A switch
closes the old peer/renderer, waits for the coordinator's end acknowledgement, then
requests a new session with the original requested permissions and selected opaque
monitor identity. Local approval may be required again. Switching is unavailable
while a clipboard/file operation or document-picker handoff is active.

The coordinator must echo the monitor binding at creation and every status check.
Binding violations fail immediately instead of entering transient network retries.
The host must advertise compatible capabilities and a matching catalog. Unconfirmed
video is not attached to the renderer. A legacy host may serve a normal primary
session, but an explicitly selected monitor cannot silently become its primary
screen. Missing host binding fails ten seconds after control-channel open.

Fresh decoded frames and the current renderer's own first-frame callback are both
required for new input and tools. Renderer tokens and session generations prevent
callbacks or disposal from older renderers/peers changing a new session. Rotation
pauses input and new tools, while an already active upload may continue if its
approved binding, transport and decoded-frame progress remain healthy.

An independent source review found that retaining the initial renderer timestamp
could cause immediate timeout after a long-lived session rotated. The corrected
implementation starts a fresh thirty-second wait when the current renderer is
removed/replaced; stale tokens cannot reset or extend it. Clock-driven regressions
cover the gap between old disposal and new attachment, failure when no replacement
arrives, slow transport establishment and obsolete callbacks.

The explicitly launched Storage Access Framework picker keeps its independent
**two-minute** limit and pauses input/new tools. During that handoff only the
renderer deadline is suspended; ordinary backgrounding, network/capture failure
and the picker limit still retain their existing termination behavior. On return,
a new renderer receives a fresh wait budget. A selected URI waits for that
renderer's first frame before snapshot/upload if Activity recreation occurred.
Actual picker rotation/provider behavior is not claimed verified by JVM tests.

## Dependency provenance

No dependency was added or upgraded for 0.4.0. The committed strict lock file still
covers **91 configurations / 133 components**. Verification metadata records
**367 components / 619 SHA-256-pinned artifacts**, with no trusted-artifact exclusions.
The ordinary final build did not use `--write-locks` or regenerate verification pins.

Checksums were originally captured from already resolved cache bytes. They provide
first-seen provenance pins, **not publisher attestations**. The unchanged runtime
dependency graph and the original bootstrap/coverage reports remain recorded.
Optional connected-device runner/UTP configurations were absent from the offline
cache in the original bootstrap; they are not counted as verified. Device CI must
resolve, review and pin them before its first connected test. Do not weaken strict
verification to make an unresolved configuration pass.

## Final artifacts

| Artifact | Bytes | Purpose |
| --- | ---: | --- |
| `app/build/outputs/apk/debug/app-arm64-v8a-debug.apk` | 24,631,759 | Installable ARM64 development APK, debug signed |
| `app/build/outputs/apk/release/app-arm64-v8a-release-unsigned.apk` | 13,780,002 | Minified release variant; owner signing required |
| `app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk` | 360,615 | Keystore instrumentation package; compiled only |

ARM64 SHA-256:

- Debug: `c9b78473a65ade3ec64dd444ecec83c201ca4d90a76723abbc4f57efefe13e0e`
- Unsigned release: `3c5868cefb1ccbcabd88880c2b58be2e56c31f41366080d4202834fd9cbf3df0`
- Source-input manifest: `b3a56713276d984fa93d0bd1e09ef6075c7f4689f6bfa65345b11638d7563a27`

`verification/artifacts.sha256` records all **nine APKs**. From the repository root:

```sh
sha256sum -c android/verification/source-inputs.sha256
sha256sum -c android/verification/artifacts.sha256
sha256sum -c android/verification/build-evidence.sha256
```

The evidence manifest pins the reports, raw JUnit XML and these documents. Source
changes invalidate the matching build evidence until required gates are rerun.
The prior 0.3 reports remain under `verification/history/0.3.0/`; the successful
intermediate 43-test 0.4 build before the renderer-deadline correction is retained
under `verification/history/0.4-before-renderer-deadline-fix/`. Neither overrides
this final 46-test build.

## Open release gates

No actual Android Keystore execution, rendered remote desktop, hardware decoder,
monitor-picker UI, touch/keyboard accuracy, Windows clipboard, file transfer over
WebRTC, installation/upgrade, mixed-DPI/negative-origin monitor selection, hot
unplug, Wi-Fi/cellular roaming, carrier NAT or forced TURN path was executed on a
phone. Compile/lint and model tests do not prove these behaviors.

Physical tests must cover repeated monitor switching with local consent and
trusted unattended access; view-only mode; host/process/layout changes; wrong or
missing catalog; current-renderer input gating; rotation after several minutes;
rotation during upload; and SAF Activity recreation near the two-minute deadline.
Use the root release matrix for Internet and Windows integration tests.

Owner-controlled signing/distribution, native dependency notices/provenance,
security review and a supported device/network matrix remain prerequisites to
release. Audio, PC-to-phone download, remote reboot, automatic clipboard polling
and live in-stream monitor switching are not implemented.
