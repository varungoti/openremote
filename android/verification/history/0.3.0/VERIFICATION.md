# Android verification — 18 November 2026

## Result

OpenRemote Android **0.3.0 (version code 3)** produces actual debug and unsigned
release APKs for arm64-v8a, armeabi-v7a, x86 and x86_64. Screen/control, text,
explicit clipboard actions and phone-to-PC file upload are implemented in source.
This is **build-verified**. It is not a claim of production release readiness or
successful phone-to-Windows device integration.

## Final checks executed successfully

The final ordinary build used strict dependency verification, committed dependency
lock state and the exact **41 build inputs** in `verification/source-inputs.sha256`.
The input hashes were checked before and after the build; none changed during it.

```sh
gradle -p android :app:assembleDebug :app:testDebugUnitTest --rerun \
  :app:lintDebug :app:assembleRelease :app:assembleDebugAndroidTest \
  --dependency-verification strict --offline --no-daemon \
  --max-workers=2 --stacktrace --console=plain
```

- `BUILD SUCCESSFUL in 1m 35s`; 133 tasks, 31 executed and 102 up-to-date.
- **32 JVM tests passed**, zero failures/errors. The final test task was explicitly
  rerun rather than only accepting an up-to-date result.
- Debug lint: **0 errors, 5 warnings**. Warnings identify newer versions of the
  intentionally pinned Compose/Lifecycle dependencies and MockWebServer. No
  permission, backup, context-leak or new tools API error remained.
- Android instrumentation-test APK compiled. It was **not executed on a device**.
- ARM64 debug signature verified with SDK `apksigner`, APK Signature Scheme v2 and
  the local debug identity. This is not an owner-controlled production signing key.
- ARM64 debug and release APKs passed `zipalign -c -P 16 4`.
- ARM64 native ELF load-segment alignment was checked from the **final release APK**:
  all LOAD alignments support 16 KiB pages. A real 16 KiB-page Android run is still required.
- The final resolved `releaseRuntimeClasspath` graph passed ordinary strict
  verification with no unresolved entries. It is retained in
  `verification/release-runtime-dependencies.txt`.

Toolchain: checksummed Temurin JDK 21.0.13+9, Gradle 8.13, Android SDK Platform 36,
Build Tools 35.0.0; AGP 8.13.2, Kotlin/Compose compiler 2.2.21 and native WebRTC
150.7871.04. Compiler, SDK, native dependencies and build plugins were actually
resolved and used; this was not syntax-only source inspection.

## What the tests cover

| Suite | Tests | Observable contracts |
| --- | ---: | --- |
| ApiClientTest | 4 | Real P-256 verification of the signed challenge, bearer scope, HTTPS/PC IDs, cancelled signaling |
| RemoteCoordinatesTest | 3 | Letterbox input mapping and invalid coordinates |
| SessionLifecycleTest | 2 | Pending/approved/connecting/active progression and fail-closed terminal states |
| ToolProtocolTest | 6 | Strict JSON/Unicode/IDs, duplicate fields, Windows names, UTF-8/frame limits, exact ACKs and final digest/length/name confirmation |
| SessionToolsClientTest | 4 | Single in-flight request, ordinary denial, retired request IDs, no timeout replay, prompt close and scoped malformed-frame failure |
| FileUploadTest | 10 | Chunk sequence/progress, empty files, wrong ACKs, cancellation/partial-copy cleanup, oversized streams, quota reservations, native-close failure, denied begin, shared-cache recovery |
| VideoFreshnessTest | 3 | Fresh frames required after connection, five-second stall gate, thirty-second failure and recovery |

A current decoded frame is required before input or tools are enabled. After five
seconds without decoded progress, held input is released and input/tools pause;
a thirty-second capture stall ends the session. Fresh frames recover readiness.
A healthy existing encrypted path remains usable during TURN credential rotation,
so renewal alone does not cancel a long upload.

## Dependency reproducibility and its limits

`app/gradle.lockfile` records **91 configurations**, including the required debug,
release, JVM-test and instrumentation-APK compile/runtime paths. The application
uses `LockMode.STRICT`. `gradle/verification-metadata.xml` records **619 artifacts
from 367 components**, each with SHA-256; metadata verification is enabled and no
trusted-artifact exclusions were added. The final build above did **not** use
`--write-locks` or `--write-verification-metadata`.

These checksums were captured from the already resolved build cache. They are
**first-seen provenance pins, not publisher signatures or attestations**. Future
dependency changes require review; checksum failures must not be silenced by
regenerating metadata without establishing the intended source.

The separate bootstrap ran the required tasks with `--write-locks` and
`--write-verification-metadata sha256`, plus an all-configuration dependency report.
That bootstrap completed, but its report contains unresolved **optional connected-device
runner/UTP and unused generic metadata configurations** missing from the offline
cache (for example `com.android.tools.utp:android-device-provider-ddmlib:31.13.2`
and `com.google.testing.platform:core:0.0.9-alpha03`). Those entries are retained in
`verification/dependency-bootstrap.log`; they are not counted as verified. Device
CI must resolve, review and pin any additional runner dependencies before its first
connected test. The required build paths and final release runtime graph did pass
strict verification. See `verification/dependency-coverage.json` for the exact coverage.

## Final artifacts

| Artifact | Bytes | Purpose |
| --- | ---: | --- |
| `app/build/outputs/apk/debug/app-arm64-v8a-debug.apk` | 24,587,694 | Installable development build for ARM64 phones |
| `app/build/outputs/apk/release/app-arm64-v8a-release-unsigned.apk` | 13,763,614 | Minified release variant; owner signing still required |
| `app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk` | 360,615 | Keystore instrumentation test package |

ARM64 SHA-256:

- Debug: `f1289b55b1c5b99548979473f5b43c6ee678f443106f31b1818643e58392e53a`
- Unsigned release: `4adc6462f1bc814d75dae56ecfbbdc7237148cdc7ee471ae247d9320ae128e9f`

Exact hashes for all **nine APKs** are in `verification/artifacts.sha256`.
From the repository root:

```sh
sha256sum -c android/verification/source-inputs.sha256
sha256sum -c android/verification/artifacts.sha256
sha256sum -c android/verification/build-evidence.sha256
```

The last manifest also pins the resolved dependency graph, reports and this document.
If a source or dependency pin changes, these results describe old inputs until the
relevant gates are rerun. Earlier build logs and pre-tools input/artifact hashes
are retained in `verification/history/` and do not override this final evidence.

## Review corrections included

- Accepted the server's `connecting` state without creating a second peer.
- Rejected NUL/malformed Unicode and oversized encoded text before control delivery.
- Matched Windows filename rules, including CONIN$/CONOUT$, reserved stems with
  trailing whitespace and C0/C1 controls.
- Made cancellation delete the private snapshot even when native channel close throws.
- Made cache recovery run once per app process so another Activity cannot erase an
  active upload; rejected file-begin responses leave the tools channel usable.
- Added the explicit public-key export required by unattended trusted-controller setup.

An earlier 0.2.0 adaptive-icon folder rename exposed a stale incremental Android
resource-merger result. A clean build of that same source corrected it. It is
historical, not a failure of the final 0.3.0 locked/verifying build.

## Not executed / release gates

No connected Android device or emulator/KVM surface was available. There is no
claimed evidence for real Android Keystore execution, rendered remote pixels,
hardware decoder behavior, gesture accuracy, rotation/background transitions,
Wi-Fi/cellular roaming, CGNAT or forced TURN from a phone. JVM networking tests use
synthetic local servers; they are not an Internet deployment test.

The Storage Access Framework flow, cloud document providers, cancellation during a
provider read, real Windows clipboard effects, file transfer over actual WebRTC,
Windows received-file metadata and multi-window behavior remain device integration
gates. The explicitly launched document picker pauses input and retains the session
for at most **two minutes**; ordinary app backgrounding still closes it. Both behaviors
need device verification. The source tests do not claim those platform interactions passed.

Owner-controlled release signing, distribution/store review, complete dependency/native
license notices, release security review and the full real-device/network matrix remain
required. Audio, PC-to-phone downloads, remote reboot, automatic clipboard polling and
multi-monitor selection are not implemented or advertised as working features.
