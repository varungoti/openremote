# OpenRemote Android controller 0.8

OpenRemote 0.8.0 has a managed connection screen: **PC ID**, a
masked **six-digit PIN**, and **Connect remote desktop**. The app provider embeds
the service configuration in the build. The phone does not ask the user for a
server URL, certificate, enrollment token, setup link, or other technical setting.

The current checked-in service profile is empty. A fresh installation therefore
shows a plain provider-unavailable message and disables Connect. These artifacts
are build-verified development previews; they cannot establish Internet sessions
until a working service is configured and the device/network acceptance work is
completed. See [current verification](VERIFICATION.md) and
[the current activation guide](../docs/windows-installer-0.8.0.md).

## Using a configured build

1. On the Windows host, locate **Your PC ID**, select **Activate Connection**,
   enter a six-digit PIN, then select the same action again or press Enter.
   A configured host registers before asking for a PIN.
2. Enter that permanent nine-digit **PC ID** and six-digit PIN on the phone.
3. Tap **Connect remote desktop**. The same button becomes **Cancel** while the
   authenticated connection request is in progress. Once a session is admitted,
   **Stop remote session** remains visible while connecting and viewing.
4. Stop from either device when finished. Normal phone backgrounding ends that
   phone's session. Other peers can remain connected until the host deactivates.

A PC ID is a stable locator, while a PIN authorizes the current activation for
at most 24 hours. The phone preserves leading zeroes and accepts exactly nine
ASCII ID digits or a legacy six-digit invitation; PINs remain exactly six digits.
Nine-digit redemption requires activation capability before the PIN is sent.
The resulting response must carry the same PC ID and activation mode; a known
mismatched or cancelled session is ended. Refresh cannot change that binding.

Four pending/live peers are allowed, with one input controller and additional
view-only peers. The phone preserves the server's narrowed permissions. Its
sanitized Android manufacturer/model name appears in the host roster. Names
are display claims, not verified identities. Requests are not automatically
replayed after an uncertain response; the user is directed to check the PC's
connection list before retrying.

The PIN is cleared on submission, cancellation, Activity pause and removal of the
home screen. It is not put into saved state, private settings or logs. It can
remain transiently in the bounded in-flight request; this is not a claim of
cryptographic memory erasure. The activation authorizes screen viewing and the host's
allowed keyboard/mouse control. It does not authorize clipboard, file transfer,
audio or power actions, or permanently trust the phone.

## Provider configuration and release gate

The shared public file `../config/service-profile.json` is embedded as a generated
BuildConfig value. It may contain only `schemaVersion`, `serviceName` and
`apiOrigin`, with schema version 1 and service name `OpenRemote`. The origin must
be a deployed HTTPS origin without credentials, query, fragment or a non-root
path. Placeholders, HTTP and ambiguous encodings are rejected at startup. No
secret belongs in this file.

A valid origin already saved by an earlier, explicitly configured installation
is retained after validation. A new external intent cannot change it: the app has
no setup-link intent filter and MainActivity does not process external setup data.
There are no settings/import, diagnostics, QR or saved-PC screens
in the 0.8 home flow. Permanent PC ID entry is the primary connection path. Legacy protocol helpers and their tests remain internal.

Normal `assembleRelease` and `bundleRelease` reject an empty embedded origin.
A publisher must configure the service before producing customer artifacts. There
is a deliberately explicit exception for inspecting an **unsigned, unconfigured
development preview**:

```sh
./gradlew :app:assembleRelease -PopenremoteUnconfiguredPreview=true \
  --dependency-verification strict
```

That exception rejects a release signing configuration or injected signing
properties. It does not configure a service, test Internet connectivity, or make
the result a customer release. Debug builds remain available with the controlled
unavailable message. No URL-entry workaround is offered to ordinary users.

## Build and package identity

Use JDK 17 or 21, SDK Platform 36 and Build Tools 35.0.0. The checked-in Gradle 8.13
wrapper verifies the official distribution SHA-256. From this directory:

```sh
./gradlew :app:assembleDebug :app:testDebugUnitTest :app:lintDebug \
  --dependency-verification strict
# Only after the provider profile is configured:
./gradlew :app:assembleRelease :app:bundleRelease \
  --dependency-verification strict
```

APK splits target ARM64, ARMv7, x86 and x86_64. `arm64-v8a` is the relevant split
for current 64-bit Android phones. Minimum Android API is 26; compile/target is 36.

The installable development package remains **com.openremote.mobile.dev**, named
**OpenRemote Preview**, with version code **8** and version **0.8.0**. This build
reuses the retained 0.6 preview certificate. It is eligible to update the retained 0.6/0.7 preview when
Android's ordinary package/version/signature checks pass; installation on a phone
has not been executed here. It is a separate app from the older
`com.openremote.mobile` test package. Neither that app's settings nor identity keys
are overwritten. The 0.8 app does not claim `openremote://connect` links.

The release variant remains **com.openremote.mobile** and is unsigned in the
verification artifacts. The production owner must supply protected signing and
complete deployment/device acceptance before customer distribution. The preview
keystore is retained separately and excluded from public artifacts; later preview
updates must reuse it. No production signing credential is included.

## Implementation retained for the connection

- Android Keystore P-256 identity and fresh signed challenge authentication.
- TLS certificate verification, no redirects, memory-only session/access tokens,
  fresh pairing capability discovery and a single-attempt credential redemption.
- Strict bounded responses, permission ceilings and cleanup of known sessions
  returned after cancellation. Unknown outcomes never trigger automatic redemption
  replay. Generation ownership prevents old requests from changing newer UI.
- Encrypted private settings and backup exclusion. No shared enrollment secret.
- Receive-only native WebRTC video, short-lived relay credentials, queued ICE and
  bounded ICE recovery after connectivity changes.
- Trackpad/direct-touch interaction, scrolling, drag, text input and keyboard
  shortcuts. Input requires granted permissions, the expected display binding,
  fresh decoded video and a first frame from the current renderer.
- Foreground session status and visible stop, screenshot/recent-preview exclusion,
  key-release cleanup and deterministic peer/renderer disposal.
- Rotation retains the session but waits for the newly owned renderer before input.
  Ordinary backgrounding ends the remote session. The app is not a background host.

The older optional display-switch and tools protocols remain internal compatibility
code. The current activation flow does not expose display switching, clipboard or files.
Audio, downloads, filesystem browsing and power controls are not implemented. The
0.6 guide and earlier README are historical under `CODE_PIN_0.6.md` and
`verification/history/0.6.0/`; their Advanced/setup UI instructions do not apply to 0.8.

## Dependency and test policy

Versions remain pinned: AGP 8.13.2, Kotlin/Compose compiler 2.2.21, Compose BOM
2025.10.01, Activity 1.11.0, Lifecycle 2.9.4, WebRTC SDK Android 150.7871.04,
OkHttp 4.12.0 and Coroutines 1.10.2. These are compatible exact pins, not a claim
that they are the latest versions. No dependency was changed for 0.8.

`app/gradle.lockfile` uses strict locking. `gradle/verification-metadata.xml` verifies
artifact and metadata hashes. Never weaken verification or automatically rewrite
pins to resolve a failed build. The original pins are recorded first-seen bytes,
not independent publisher attestations. The three metadata additions needed by
the 0.6 cache restoration are documented in that historical evidence.

JVM tests cover authenticated pairing, single-use/no-replay/cancellation behavior,
strict discovery and response validation, encrypted-settings transaction ordering,
media/display readiness helpers and prior protocol compatibility. Retained 0.7 tests
cover the one-button state, disabled unconfigured build, strict six-digit input,
retention of validated authorized origins, plain recovery messages and optional
numeric automatic-registration discovery. Seven new 0.8 tests cover permanent-ID
parsing, activation capabilities before PIN disclosure, matching response identity,
view-only preservation, named-device bounds and refresh invariants. They do not execute native Compose,
Keystore, installation or real WebRTC video.

A connected-device Keystore test exists, but its optional runner dependencies need
the reviewed pin bootstrap described in the historical README before first use:

```sh
./gradlew :app:connectedDebugAndroidTest --dependency-verification strict
```

Remaining device acceptance includes fresh install and 0.6/0.7 preview update, small
phone layout/keyboard and accessibility, PIN clearing on Back/pause/rotation, rapid
Cancel/new Connect, setup-intent non-rebinding, actual Keystore operations,
Windows-to-phone video/control, Wi-Fi/cellular handoff, direct/forced-TURN sessions,
permission revocation and host-end/background cleanup. No physical device or
Internet session was available during this build verification.
