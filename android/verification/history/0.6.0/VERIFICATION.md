# Android verification — 19 November 2026

OpenRemote **0.6.0, version code 6** builds with six-digit code + PIN connections
and a shared public service profile. This is **build-verified development software**.
No physical Android, Windows-to-phone or Internet session has been executed.
The checked-in profile has no deployed service origin.

## Final result

The final strict/offline run passed in **3 minutes 47 seconds**: 105 tasks,
23 executed and 82 up to date. It assembled debug and minified unsigned release
APKs for ARM64, ARMv7, x86 and x86_64, explicitly reran the JVM tests, ran debug
lint and generated signing information.

- **76 JVM tests passed** across 12 suites; zero failures, errors or skips.
- Lint: **0 errors, 3 warnings**, all pinned-dependency update suggestions.
- **51 source inputs**, including the shared service profile, were hash-checked
  before and after the entire final run; all stayed unchanged.
- The ARM64 preview APK signature, package, version, label and manifest were
  inspected. ARM64 preview and release APKs pass ZIP 16 KiB alignment and both
  bundled native libraries pass ELF LOAD alignment/congruence checks.
- Strict dependency verification and the existing lockfile remained enabled.
  All 619 previous artifact pins are unchanged. Three reviewed metadata-only
  pins were added when restoring a fresh dependency cache.

Source-manifest SHA-256:
`ab80d35c6d77449ef24775e8172e00b25128ab6929f52d8352bc9992fda912b4`

The final Gradle tasks were `:app:assembleDebug :app:testDebugUnitTest --rerun
:app:lintDebug :app:assembleRelease :app:signingReport`, with
`--dependency-verification strict --offline --no-daemon --max-workers=2`.
Toolchain: Temurin JDK 21.0.13+9, Gradle 8.13, SDK 36 / Build Tools 35.0.0,
AGP 8.13.2, Kotlin/Compose compiler 2.2.21 and WebRTC 150.7871.04. Managed proxy
and certificate trust settings were retained; no TLS verification was disabled.

## What the new tests establish

The new pairing suite contains 12 tests and discovery gained one additional test.
They verify leading zeroes and credential validation; strict public-profile parsing
and preservation of existing settings; decoding the actual compiled build profile;
capability discovery before credentials; the authenticated JSON-only redemption;
no automatic POST replay after 401, 408 or retryable 503 responses; cancellation
with cleanup of a real delayed HTTP result; rejected/cleaned-up unsafe grants;
pairing-identity/permission ceilings on later polls; and bounded recovery messages
without reflected secrets. Existing tests still exercise an actual P-256 signature.

MockWebServer/JVM coverage does not prove native Keystore, Compose lifecycle,
Activity installation, video decoding, desktop control or real Internet behavior.
The implementation and its cancellation limits are detailed in
[CODE_PIN_0.6.md](CODE_PIN_0.6.md).

## Preview identity and artifacts

The installable preview uses **com.openremote.mobile.dev**, named **OpenRemote
Preview**, and installs beside the older test app. The original debug signing key
was unavailable. Existing app settings and keys are not deleted. Both apps may
appear in the setup-link chooser; select OpenRemote Preview. The direct code + PIN
screen does not use that link handler.

The final development certificate has alias **AndroidDebugKey** and SHA-256:
`24c0441ceca6f73ea0e6de8a0958be6229cfcf2518328e8f7f9b71d98b980b28`.
It is valid from 2026-11-19 08:42:16 UTC until 2056-11-11 08:42:16 UTC. The key is
retained in a separate private checkpoint for later .dev updates and excluded from
source/public downloads. No production signing credential is supplied. The
unsigned release variant remains **com.openremote.mobile**.

| ARM64 artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `app/build/outputs/apk/debug/app-arm64-v8a-debug.apk` | 24,467,087 | `431b6593ba0f977e7839231668ab7519c37da81b14ea35c50393e76a1b792480` |
| `app/build/outputs/apk/release/app-arm64-v8a-release-unsigned.apk` | 13,796,938 | `f145749887abe87a9de9305129d40bd5c3e86f94e5dcad47cc3bf7bc1d99dbcf` |

## Evidence and remaining gates

Current evidence is under `verification/code-pin-0.6.0/`: final build log/JSON,
source hashes, raw JUnit XML and totals, artifacts, signature/package inspection,
ZIP/ELF checks, toolchain provenance, metadata-pin review and build summary.

The initial missing-pin failure, runtime-only Java failure (no javac), interrupted
attempt before configuring the managed JDK trust store, and successful warm build
are retained. The three added files are Guava parent 33.3.1-jre POM, JUnit BOM
5.10.2 module metadata and coroutines BOM 1.8.0 POM, fetched from official Maven
Central and checked against its published checksum bytes. These metadata pins
are first-seen provenance, not publisher signatures or a full supply-chain audit.
The previous verification narrative is retained at
`verification/history/0.5.0/VERIFICATION.md`.

Physical installation/update, native UI/Keystore execution, Windows-to-Android
video/input, Internet/TURN/cellular handoff, production signing and publication
remain open. A deployed coordinator and relay plus a configured origin are
required for Internet use. Existing native WebRTC provenance/licensing and wider
project release gates remain unchanged.
