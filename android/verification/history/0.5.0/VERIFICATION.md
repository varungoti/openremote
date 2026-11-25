# Android verification — 18 November 2026

## Result

OpenRemote Android **0.5.0 (version code 5)** builds debug and unsigned release APKs
for arm64-v8a, armeabi-v7a, x86 and x86_64. This increment adds explicit PC setup-link
review/import and staged connection checks, alongside the existing remote desktop,
monitor selection, keyboard/mouse, clipboard and phone-to-PC upload implementation.

This is **build-verified development software**. No physical phone or emulator was
available. Compilation and JVM tests do not demonstrate installation, native UI,
Keystore execution, Windows-to-Android video/input, Internet relay behavior or
production release readiness.

## Final checks executed

The source manifest pins **48 inputs** and was checked before and after the build.
Its SHA-256 is `59222880985370d0daaf8aa185f00b2b0561eee7adf27841dc0be48079bced58`. No build input changed during the
final run. The build retained strict dependency locking and checksum verification:

```sh
gradle -p android :app:assembleDebug :app:testDebugUnitTest --rerun \
  :app:lintDebug :app:assembleRelease :app:assembleDebugAndroidTest \
  --dependency-verification strict --offline --no-daemon \
  --max-workers=2 --stacktrace --console=plain
```

- **BUILD SUCCESSFUL in 1m 32s**: 133 tasks, 28 executed and 105 up-to-date.
- **63 JVM tests passed**, zero failures, errors or skips. The complete test task
  was explicitly rerun. Raw XML is in `verification/junit/`; totals and source
  pins are in `verification/tests.json`.
- Lint: **0 errors, 5 warnings**, all existing pinned dependency-version suggestions.
- Four debug and four unsigned release APKs were assembled. The Keystore
  instrumentation APK compiled and was not executed on a device.
- ARM64 debug APK signature verified. Its certificate matches the 0.4 test APK:
  `a0cc38598018756a992fb04c5aa327060f124e8464becc1f9a2f757f5186d21f`. This is a development identity;
  the matching certificate is not evidence that a physical upgrade succeeded.
- ARM64 debug and release APKs passed `zipalign -c -P 16 4`.
- Both ARM64 release native libraries passed ELF LOAD alignment and offset/address
  congruence checks for 16 KiB pages. Native runtime on such a device remains open.
- Compiled APK metadata reports package `com.openremote.mobile`, version code 5,
  version name 0.5.0 and the intended custom-scheme activity routing. Raw package
  and manifest output is retained; runtime Android task routing was not tested.

Toolchain: Temurin JDK 21.0.13+9, Gradle 8.13, Android SDK Platform 36, Build Tools
35.0.0, AGP 8.13.2, Kotlin/Compose compiler 2.2.21 and WebRTC 150.7871.04.

## Requirement-based verification

| Suite | Tests |
| --- | ---: |
| ApiClientTest | 7 |
| ConnectionChecksTest | 9 |
| DisplayProtocolTest | 4 |
| DisplayReadinessTest | 7 |
| FileUploadTest | 10 |
| PhoneSetupTest | 8 |
| RemoteCoordinatesTest | 3 |
| SessionLifecycleTest | 2 |
| SessionToolsClientTest | 4 |
| ToolProtocolTest | 6 |
| VideoFreshnessTest | 3 |

New setup tests use the actual production parser, review gate and settings
transaction primitive. They cover exact Windows-generated origin/PC vectors,
custom ports and IPv6, unknown and encoded duplicate fields, malformed UTF-8,
ambiguous encodings, overlong links, credentials/fragments, raw paths that URL
normalization would otherwise discard, staged review during initial loading,
wrong/stale confirmation, loading/saving/pending-review action gates, and confirmed
favorite persistence with same-origin name preservation, changed-origin clearing,
ID deduplication and the 100-PC bound.

A real coroutine cancellation test blocks a disk-writer substitute, cancels its
caller, verifies another server cannot reserve a settings change until the first
write exits, and then verifies the final saved snapshot belongs to the new server.
A separate generation test verifies a retired lookup cannot replace the current
server or clear the current operation's busy state. These are not native AtomicFile
or whole-ViewModel instrumentation tests.

Connection-check tests run the actual API/check orchestration against an isolated
MockWebServer with an explicit test-only loopback HTTP allowance. They verify
strict server info, no identity proof after wrong/legacy/redirecting endpoints,
fresh proof on each check, authenticated lookup and matching PC ID, offline/missing
relay warnings, optional no-PC behavior, cancellation before authentication, and
local error messages without reflected remote secrets. Existing API tests still
verify real P-256 signatures. This harness does not prove genuine TLS/DNS failure
handling on Android or live Internet relay access.

## Implemented first-use and lifecycle behavior

A setup URL carries only version, HTTPS origin and nine-digit PC number. Parsing
or reviewing one performs no network, identity, persistence, trust or connection
operation. Full origin and PC number are shown before **Save PC setup**. A server
change warns that the previous server's saved PC list will be removed. When you confirm, the app
persists the origin and a favorite with a neutral `PC 123456789` label, preserving
an already-known name only on the same server. The favorite is available after a
process restart even before the first connection. **Check connection** or **Find
PC** remains another explicit user action. Clipboard text is read only on the explicit paste button and never
resolved as a content URI. The full wire contract is `../docs/phone-setup.md`.

The custom scheme is claimable by other apps and is not a verified Android App
Link or proof of ownership. No secret or permission is embedded. The activity uses
`singleTask` and `onNewIntent` to route a warm link to the existing ViewModel;
a link received during a session or busy operation cannot change its server.
Rotation preserves a pending review in the ViewModel; process recreation may
require reopening the link instead of automatically replaying it.

Settings load updates existing UI fields instead of replacing a pending review.
Every settings mutation reserves a single transaction synchronously, snapshots
its origin/list, and retains the gate until the blocking IO call actually exits,
including cancellation. Cancelling a pre-session request invalidates both request
and session generations; stale results/finalizers cannot alter the next action.

Connection checks first fetch `/v1/info` without credentials, then perform a fresh
phone signature proof, then optionally query PC presence. Advertised TURN UDP,
plain TCP and TLS bits are **configuration information**, not a successful relay
or media test. An incompatible/legacy endpoint fails before identity disclosure.
Error messages identify actionable categories without copying server bodies,
exception details, bearer tokens, keys, SDP or TURN credentials into reports.
The explicit copied report contains the configured origin and optional PC ID.

## Artifacts and custody

| Artifact | Bytes | Purpose |
| --- | ---: | --- |
| `app/build/outputs/apk/debug/app-arm64-v8a-debug.apk` | 24,726,168 | Installable development-signed ARM64 test APK |
| `app/build/outputs/apk/release/app-arm64-v8a-release-unsigned.apk` | 13,796,554 | Minified release variant, owner signing required |
| `app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk` | 360,615 | Keystore instrumentation package, compiled only |

- ARM64 debug SHA-256: `de856ea9917927af0354a46e3c43cc2e7b5203222c96f3470ac4b0e1c54d0a16`
- ARM64 unsigned release SHA-256: `bd64d756ed233fa142402c7f931d2c97028864a66133b6158bb99d47f8afe0df`

`verification/artifacts.sha256` pins all nine APKs; `verification/build-summary.json`
records sizes, hashes, checks and explicit not-run gates. From the repository root:

```sh
sha256sum -c android/verification/source-inputs.sha256
sha256sum -c android/verification/artifacts.sha256
sha256sum -c android/verification/build-evidence.sha256
```

The prior final 0.4 verification is preserved under `verification/history/0.4.0/`.
The first 62-test 0.5 candidate is superseded and preserved under
`verification/history/0.5-before-setup-favorite/`, including the affected source
bytes. Review found that its PC number was not persisted until a first connection;
the final candidate corrects that gap. Earlier history remains intact. The current evidence manifest covers final reports,
raw JUnit XML and this documentation; it is not a signed reproducibility attestation.

## Dependency provenance

No dependency was added or upgraded in 0.5. The unchanged strict lock file covers
91 configurations / 133 components. Verification metadata pins 367 components /
619 artifacts, with no trusted-artifact exclusions. The final build did not write
new lock state or checksum pins. These are first-seen cache checksums, not publisher
attestations; native WebRTC/source/notice completeness remains a release gate.
Optional connected-device runner/UTP configurations were absent from the original
offline cache and are not counted as verified. Device CI must review and pin any
required additions before executing connected tests; keep strict verification on.

## Open native and release gates

A physical phone must still verify install/upgrade, Keystore and encrypted settings,
actual Compose layout/touch/keyboard, video decoding, media/input and relay paths,
clipboard/files, real certificates/DNS, multiple monitors and Wi-Fi/cellular roaming.
For this increment explicitly test cold/warm incoming links, review during loading,
rotation, background/relaunch, process death, conflicting rapid setup actions,
and **a new link while the Storage Access Framework picker is active**. Confirm one
activity/task remains, no hidden import or connection occurs, and picker/session
cleanup still obeys its existing two-minute bound. The singleTask lifecycle has not
been exercised on Android here.

Existing acceptance requirements remain: monitor binding before video/input,
repeated switches and local consent, view-only/trusted modes, wrong/missing catalog,
hot unplug and layout change, current-renderer ownership, rotation after a long
session and during upload, and SAF recreation near its deadline. The root pilot
matrix supplies Windows and Internet requirements; no native or relay pass is
asserted by a successful setup report.

Owner-controlled release signing, distribution, native provenance/notices, security
review and supported device/network validation are still required. Audio, iOS,
PC-to-phone downloads, remote reboot, automatic clipboard polling and in-stream
monitor switching are not implemented.
