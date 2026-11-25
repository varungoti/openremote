# OpenRemote Android controller

Native Android controller for the OpenRemote host and control plane. This module is the
controller, not an Android screen-sharing host. An enrolled Windows PC and an HTTPS
OpenRemote server are required.

## Build

Use JDK 17 or 21 and Android SDK Platform 36 / Build Tools 35.0.0. The checked-in
Gradle wrapper is pinned to 8.13 and verifies the official distribution SHA-256.

```sh
./gradlew :app:assembleDebug :app:testDebugUnitTest :app:lintDebug
./gradlew :app:assembleRelease
```

`assembleRelease` produces an **unsigned** APK for each supported ABI. Use the `arm64-v8a`
APK for current 64-bit phones; each split includes only its required native library. It must be signed with the owner's
protected release key before distribution. No signing password or key is stored here.
Do not distribute the automatically generated debug signing identity as a production
release. Configure release signing in a protected CI job, then verify the resulting APK
with `apksigner verify --verbose --print-certs`.

The supported dependency set is intentionally internally compatible:

- Android Gradle Plugin 8.13.2 / Gradle 8.13
- Kotlin and Compose compiler plugin 2.2.21
- Compose BOM 2025.10.01, Activity 1.11.0, Lifecycle 2.9.4
- compile/target API 36; minimum API 26
- WebRTC SDK Android 150.7871.04
- OkHttp 4.12.0; Coroutines 1.10.2

These are exact pins, not a claim that every dependency is the newest available.
`androidx.compose:compose-bom:2026.08.00` is not used because its API 37 requirement is
incompatible with this AGP 8.13/API 36 build. Changing to AGP 9 also requires its Kotlin
migration; do not merely bump that version string.

### Locked and verified dependencies

`app/gradle.lockfile` records transitive versions for resolved app, release, JVM-test,
instrumentation-APK and Kotlin tooling configurations. Locking uses `LockMode.STRICT`:
a newly used configuration without reviewed lock state fails resolution.
`gradle/verification-metadata.xml` adds SHA-256 verification for resolved library/plugin
artifacts and their metadata. Ordinary builds verify these files; they must not write
new lock state or checksums merely to make a verification failure disappear.

The initial checksums were captured from the already resolved build cache. They pin
first-seen bytes; they are **not publisher attestations** or an independent audit of
the first download. Dependency updates require a deliberate review of expected versions,
source provenance, checksum changes and applicable licenses before changing these files.
The full release runtime dependency graph is in `verification/release-runtime-dependencies.txt`.

The build-only environment did not have every optional connected-device runner/UTP
dependency in its offline cache. The all-configuration bootstrap report retains those
unresolved entries rather than treating them as verified. The required debug/release,
JVM-test, lint and instrumentation-APK configurations are covered and tested; before a
connected device run, the device-test CI must resolve and review any additional runner
dependencies and commit their lock/checksum state. Keep strict verification enabled.

## First connection

1. Deploy the server with a valid HTTPS certificate and configured STUN/TURN service.
2. Enroll the Windows host against that server.
3. In the phone app, open **Settings**, enter the same HTTPS origin, and save.
4. Enter the host's nine-digit PC ID, find it, and request view/control or view-only.
   Clipboard and file upload are separate switches, both off by default. The PC must
   independently enable `allow_clipboard` / `allow_file_transfer` and approve those grants.
5. Compare this phone's SHA-256 identity fingerprint from Settings with the request
   displayed on the PC, then approve there. For unattended enrollment, use **Copy phone
   public key** in Settings and add that exact Base64 P-256 SPKI key to the PC's
   trusted-controller setup. Only the public key is copied; the private key remains in
   Android Keystore. Enable unattended trust only on the host.
6. The phone waits for the server-approved permission set before creating a WebRTC offer.

The 0.3.0 controller requests optional clipboard and file-upload permissions only when
you enable their switches. The actual permission set granted by the host, its capabilities,
an open tools channel and fresh decoded video must all allow an action. Audio, PC-to-phone
downloads, filesystem browsing, power actions and monitor selection are not implemented.

## Clipboard and file uploads

During a live session, open **Clipboard & files**. **Read PC clipboard** performs one
explicit text read (up to 4,096 UTF-8 bytes), then shows a preview. **Copy received text
to phone** is a separate action. Enter or explicitly paste text into the field and tap
**Set PC clipboard** to replace the PC clipboard; an empty field labels that action
**Clear PC clipboard**. The app never polls or reads the phone clipboard in the background.

**Choose file to send** opens Android's document picker. Only the selected content URI is
read; no broad storage permission or persistent URI grant is requested. Input pauses while
the picker is open, and this explicit picker flow may retain the remote session for at most
two minutes. If it does not return by then, the session disconnects. Ordinary app
backgrounding still disconnects immediately.

The phone snapshots and hashes the document into its private cache before upload, bounded
to 64 MiB. It sends 8,192-byte chunks over a separate reliable, ordered `tools` data channel
and waits for each exact acknowledgement. The response timeout is 15 seconds and a timed-out
chunk is never replayed. Progress is acknowledged bytes, and success appears only after the
PC confirms final size and SHA-256. Zero-byte files are supported. The per-session upload
allowance is 256 MiB, including reservations consumed by cancelled or failed uploads.

Cancel, disconnect and failures delete the phone's private copy. Leftovers from a killed
process are removed on the next launch. Cancellation attempts to remove the PC's incomplete
file; if the acknowledgement cannot arrive within three seconds, only the tools channel
closes so the host can clean up. The desktop remains usable. If final confirmation was lost,
check the PC's ReceivedFiles folder before sending again; the app does not claim completion
without confirmation.

The PC receives files under `%LOCALAPPDATA%\OpenRemote\ReceivedFiles`, applies Internet-zone
metadata and never opens them automatically. Filename validation rejects path components,
reserved Windows device names, control characters and unsupported punctuation. Use matching
0.3.0-or-compatible Windows and Android components for this tools protocol.

## What the controller implements

- P-256 Android Keystore identity and proof of possession: SHA256withECDSA DER over the
  server's decoded challenge. The identity key never leaves Android Keystore.
- TLS-only control plane, certificate verification, no redirects, no built-in endpoint
  or fixed TURN password. Access/session tokens remain in memory.
- AES-256-GCM private settings storage with a Keystore key; backups disabled. Server
  changes clear server-specific saved PCs.
- Host permission narrowing and continuous session-status checks after approval.
- Unified Plan receive-only video transceiver before offer creation; real native SDP
  completion callbacks; queued early ICE candidates; bounded signaling retries.
- Server-issued short-lived ICE credentials; up to three ICE recovery attempts on
  connection failure or network changes. Healthy credential rotation preserves a fresh
  existing transport, so it does not interrupt a slow file upload simply because TURN
  credentials are renewed.
- Direct touch or trackpad control, two-finger scroll, long-press right click,
  double-tap-and-hold drag, text entry and keyboard shortcuts. Letterboxing is accounted
  for when mapping input to the desktop.
- Reliable ordered control channel with frame/backpressure limits and `release_all`.
- Separate reliable ordered tools channel with strict bounded UTF-8 JSON, duplicate-key
  rejection, request correlation, approved-permission gates and file digest verification.
- Decoded video freshness: input and tools pause after five seconds without frame progress;
  a thirty-second capture stall ends the session. Transport connection alone never enables input.
- Foreground session indicator, immediate disconnect, desktop pixels excluded from
  Android screenshots/recent-app previews, and deterministic renderer/peer disposal.
- Rotation retains the session. Backgrounding the app closes it, except for the explicitly
  launched, input-paused document picker described above. No background service
  is claimed or silently started.

## Tests

JVM tests exercise challenge signing against a real P-256 verifier, bearer scoping,
invalid server/ID input, session error propagation, cancellation and letterbox input
mapping. Tools tests exercise strict results and malformed Unicode/JSON, permissions,
ACK correlation, timeout without replay, private-file cleanup on cancellation/native-close
failure, quota reservations, oversized provider streams and zero-byte uploads. Frame
freshness tests cover initial readiness, stalls and recovery. The Keystore instrumentation
test requires a real Android device or emulator:

```sh
./gradlew :app:connectedDebugAndroidTest
```

Prepare the additional connected-device runner dependency pins described above before
that first device run. This task was not executed in the build-only environment.

Before a release, also run the device/network matrix from the root release report:
landscape/portrait changes, Wi-Fi↔cellular ICE recovery, direct and forced TURN sessions,
long press/drag/release while losing the network, host permission revocation, host
reject/end, Android background/resume, and hardware decoder behavior on actual phones.
Also verify document selection/cancellation/cloud providers, picker rotation and its
two-minute bound; explicit clipboard actions; empty, Unicode-named and 64 MiB files;
cancel/connection loss during upload; digest verification and Windows received-file metadata.
An APK compilation is not evidence of those device behaviors.

## Sources for toolchain/API decisions

- https://developer.android.com/build/releases/agp-8-13-0-release-notes
- https://developer.android.com/develop/ui/compose/setup-compose-dependencies-and-compiler
- https://github.com/webrtc-sdk/android/releases/tag/v150.7871.04
- https://webrtc.googlesource.com/src/+/refs/heads/main/sdk/android/api/org/webrtc/PeerConnection.java
- https://developer.android.com/privacy-and-security/keystore
- https://developer.android.com/reference/androidx/activity/result/contract/ActivityResultContracts.OpenDocument
- https://developer.android.com/training/data-storage/shared/documents-files
- https://docs.gradle.org/current/userguide/dependency_locking.html
- https://docs.gradle.org/current/userguide/dependency_verification.html

No local toolchain paths or environment credentials belong in version control.
