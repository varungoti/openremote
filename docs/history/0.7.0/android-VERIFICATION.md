# Android verification — 19 November 2026

OpenRemote **0.7.0, version code 7** provides the managed home screen with **Server
ID**, a masked **six-digit PIN**, and one **Connect remote desktop** button. It
becomes Cancel while requesting a session, followed by a visible Stop remote session
control. No technical setup or URL form is exposed.

This is an **unconfigured development preview**, not a customer-ready Internet
release. The shared service profile remains empty. A fresh installation displays a
plain provider-unavailable message and disables connection. No physical Android,
Windows-to-phone or Internet session has been executed.

## Final build and tests

The final strict/offline Gradle run passed in **3 minutes 28 seconds** (wrapper
elapsed 210.065 seconds): **106 tasks, 33 executed, 73 up to
date**. It assembled debug and minified unsigned preview release APKs for ARM64,
ARMv7, x86 and x86_64, explicitly reran the JVM tests, ran debug lint and generated
signing information.

- **83 JVM tests passed** across **13 suites**, with no failures, errors or skips.
- Lint: **0 errors, 3 warnings**, all dependency-update suggestions on pinned versions.
- **52 build inputs**, including the public service profile, were hashed before
  and after the final run. All remained unchanged.
- All four preview APK signatures use the retained 0.6 certificate. All four
  release-variant APKs are unsigned. Compiled package/version/label checks passed
  for every artifact.
- All eight APKs pass ZIP 16 KiB alignment. Both libraries in each ARM64 and x86_64
  APK pass ELF LOAD alignment and offset/address congruence checks: eight native
  library records across four APKs.
- Both ARM64 manifests were inspected. They retain MAIN/LAUNCHER and contain no
  ACTION_VIEW or BROWSABLE setup filter. MainActivity has no intent import handler.
- Strict dependency verification and locking stayed enabled. The lockfile and
  verification metadata are byte-identical to the 0.6 base commit.

Source manifest SHA-256:
`95c326714e615113327a1d220c454ba3ed564fe32326a8f77492e9bda216107f`

Source base: `865e0a69de9a8e0df3957db33713fac08b783991`. The manifest pins the modified working inputs;
it does not claim that the base commit already contains these changes.

The final tasks were `:app:assembleDebug :app:testDebugUnitTest --rerun
:app:lintDebug :app:assembleRelease :app:signingReport`, with
`-PopenremoteUnconfiguredPreview=true --dependency-verification strict --offline
--no-daemon --max-workers=2`. The exception is explicit in the build log and JSON.
The unsigned release variant is a development inspection artifact.

Toolchain remains Temurin JDK 21.0.13+9, Gradle 8.13, SDK 36 / Build Tools 35.0.0,
AGP 8.13.2, Kotlin/Compose compiler 2.2.21 and WebRTC 150.7871.04. The restored
0.6 toolchain/cache was reused. The final run was offline; managed proxy and trust
configuration remain intact and TLS verification was not weakened.

## Release safeguards

Two deliberate negative checks passed:

1. Default `:app:assembleRelease :app:bundleRelease` fails specifically at
   `verifyReleaseServiceProfile` because the embedded origin is empty.
2. An explicit preview exception together with an injected signing alias also
   fails specifically at that guard, because unconfigured preview output must
   remain unsigned. The injected alias is an inert test value, not a credential.

Only the explicit `-PopenremoteUnconfiguredPreview=true` run with no release signing
was permitted. The exception does not configure a service or authorize customer
distribution. Raw logs and exact commands are in `release-guard-checks.json`.
The guard also rejects a configured release signing object when the empty-profile
exception is used; that branch was inspected in source, not exercised with a real
production signing credential.

## What the tests and review establish

The six new managed-home tests cover disabled connection without an origin, a
configured build without setup actions, Cancel availability after clearing PIN and
locking fields, rejection of every legacy 7–12-digit PIN without prefix truncation,
validation/retention of an earlier authorized origin, and nontechnical bounded
error messages. A seventh new test accepts optional numeric automatic-registration
version 0/1 and rejects other values/types. Existing pairing API tests now also
exercise the actual 0.7 discovery fields and prove a seven-digit PIN causes no HTTP
request.

The existing tests still cover authenticated one-shot redemption, capability checks,
no replay after HTTP failures, real delayed-response cancellation cleanup, permission
ceilings, actual P-256 signature verification, generation/transaction helpers,
display/video readiness and legacy protocol behavior.

Static review confirms two local, non-saveable fields and one home button. PIN
clearing is wired to submit, Cancel, Back during a request, Activity pause and home
disposal. No setup/import/settings/list/check navigation remains. Paired sessions
hide tools and display switching and show Connected only after current-renderer
readiness. JVM helpers and manifest checks do **not** execute Compose gestures,
Activity lifecycle, Keystore or native video on a phone.

## Preview identity and artifacts

The installable package is **com.openremote.mobile.dev**, label **OpenRemote Preview**,
version code **7**. It reuses the exact 0.6 preview certificate and is eligible for an
in-place update under Android's package/signature/version rules. That update was
not installed on a device here. It remains a separate app from older
`com.openremote.mobile` test builds and preserves their settings and keys. The 0.7
preview no longer handles `openremote://connect` links.

Certificate alias: **AndroidDebugKey**. Certificate SHA-256:
`24c0441ceca6f73ea0e6de8a0958be6229cfcf2518328e8f7f9b71d98b980b28`.
Validity: **2026-11-19 08:42:16 UTC to 2056-11-11 08:42:16 UTC**. The retained
private preview keystore is required for future .dev updates and must remain in
its separate private checkpoint, excluded from source/public downloads. No
production signing key is supplied. The unsigned release package remains
**com.openremote.mobile**.

| ARM64 artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `app/build/outputs/apk/debug/app-arm64-v8a-debug.apk` | 24,417,783 | `9a844518ede8e646c2ac15fd4379ca613532ea4948ac2161909463da77cdf0b9` |
| `app/build/outputs/apk/release/app-arm64-v8a-release-unsigned.apk` | 13,780,402 | `fb4456c82f744b52abec0eac0a865965b54073d0da1980e823825e4306e64fad` |

## Evidence and remaining gates

Evidence is under `verification/one-button-0.7.0/`: frozen source manifest,
final build log/JSON, raw JUnit XML and totals, lint, artifact hashes, per-APK
signature/package inspection, manifest inspection, ZIP/ELF checks, dependency-pin
comparison, public certificate information and release-guard negative checks.

The initial missing-import compilation failure and the initial nullable-map
release-guard script compilation failure are preserved separately. Both were fixed
before the source freeze and successful final run. Deliberate guard rejections
remain recorded as expected exit-code-1 results. The previous complete narrative
and README are preserved under `verification/history/0.6.0/`; prior evidence
under `verification/code-pin-0.6.0/` is unchanged.

Open gates are deployed HTTPS/relay infrastructure and public profile configuration,
physical installation/update, native home/keyboard/Back/lifecycle and Keystore,
Windows-to-phone video/input, real Internet/TURN/cellular handoff, production
signing and publication. No compilation or JVM result closes those gates.
See [ONE_BUTTON_0.7.md](ONE_BUTTON_0.7.md) for behavior and security boundaries.
