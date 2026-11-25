# Android verification — 20 November 2026

OpenRemote **0.8.0, version code 8** now accepts the permanent nine-digit PC ID
and six-digit PIN. The current shared profile is empty: this is an unconfigured
engineering preview, with Connect disabled on a fresh installation. No physical
Android or Windows-to-phone Internet session has run.

## Final executed checks

- Strict/offline full build completed successfully in **334.281 seconds**.
- **90 JVM tests** passed across **14 suites**, with no failures, errors or skips.
  Seven new activation tests cover ID/PIN parsing, strict capability discovery
  before PIN disclosure, identity matching, view-only grants, device-name bounds
  and refresh invariants.
- Debug lint reported **0 errors and 0 warnings** in this run.
- Debug and R8-minified unsigned release APKs built for ARM64, ARMv7, x86_64 and x86.
- All eight APKs passed compiled package/version/label and ZIP 16 KiB alignment
  inspection. All four preview signatures match the retained certificate; all
  four release artifacts were independently confirmed unsigned.
- Eight ARM64/x86_64 native library instances passed ELF LOAD alignment and
  offset/address congruence checks. ARM64 launcher manifests contain no setup
  URL import filter.
- All **53 build inputs** stayed unchanged throughout the final build and guard
  checks. Dependency verification and strict locks were not weakened or changed.

Source manifest SHA-256:
`a62d8f215dd455a365e7f64af7dd9f0e9e7692a01637fe5d2236567a0f9c82dd`.
The recorded base revision is `55d0dd83697c51df017ee2b1c790c8cab78c9808`;
individual hashes pin the modified inputs rather than claiming that the base
already included this work.

The exact command, environment, timestamps and before/after hashes are in
`verification/activation-0.8.0/final-build.json`. The final tasks assembled debug,
ran JVM tests and lint, assembled release and recorded signing information. The
explicit `-PopenremoteUnconfiguredPreview=true` allowed only unsigned release
inspection. It is not permission for customer distribution.

## Release guard checks

Default `assembleRelease`/`bundleRelease` rejected the empty provider profile.
An empty-profile preview exception combined with an injected signing alias was
also rejected at the required guard. The injected value was inert test data,
not a credential. Both observed exits and exact messages are recorded in the
component evidence. Explicit unsigned preview build was permitted and marked
unconfigured; no production signature was applied.

## Artifact identity

The ARM64 debug artifact SHA-256 is
`4ac99f04f81b3b2cd20e33b5801bf6f3a85174e31f983767279c253e1684503a`,
size **24,417,783 bytes**, package `com.openremote.mobile.dev`, label
**OpenRemote Preview**. The certificate remains
`24c0441ceca6f73ea0e6de8a0958be6229cfcf2518328e8f7f9b71d98b980b28`.
All ABI hashes are in `verification/activation-0.8.0/artifacts.json` and
`artifacts.sha256`. The private key is excluded from source and public delivery.

The release package is `com.openremote.mobile`; its four generated APKs are
unsigned, unconfigured engineering artifacts. The R8 mapping hash is recorded
in `build-summary.json`.

## Retained failure and execution limits

The first build failed one old recovery-message assertion out of 90 tests.
Its expected “create a new Server ID” wording no longer matched the permanent-ID
activation flow. Four related assertions were updated to the intended recovery
messages, retaining secret-redaction checks; production inputs were unchanged.
The initial log/JUnit/source snapshot are retained under `initial-failure/`.
The final complete run passed on the updated source snapshot.

JVM tests and APK inspection do not execute native Activity/Compose, Keystore,
installation/update, decoding or input. Those require a physical phone or suitable
native runner, a real configured service and a Windows host. Wi-Fi/cellular
handoff, carrier NAT/TURN, multi-peer resource use, production signing and store
publication also remain open. Historical 0.7 evidence and documentation are
preserved and are not substituted for current checks.
