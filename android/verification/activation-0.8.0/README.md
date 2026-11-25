# Android activation 0.8.0 verification

The frozen Android source built successfully as an **unconfigured development
preview**. This record does not establish a working deployed service or native
Android execution.

## Results

- 90 JVM tests passed across 14 suites, including all 7 activation contract tests.
  There were zero failures, errors or skipped tests.
- Debug lint reported zero errors and zero warnings. The pinned Gradle build
  emitted its ordinary Gradle 9 deprecation notice; that is separate from lint.
- Debug and R8-minified unsigned release APKs were built for `arm64-v8a`,
  `armeabi-v7a`, `x86_64` and `x86`.
- All eight APKs passed compiled package, version, label and ABI inspection and
  `zipalign -c -P 16 4`. All eight 64-bit native library instances passed
  ELF `PT_LOAD` alignment and offset/address congruence checks for 16 KiB pages.
- All four debug APK signatures matched the retained 0.6/0.7 preview certificate.
  All four release APKs were independently checked as unsigned, including
  absence of an APK signing block and signature entries.
- ARM64 debug and release manifests retained only the intended launcher entry;
  no VIEW or BROWSABLE setup-link activity appeared.
- The ordinary empty-profile release build failed at the intended profile guard.
  The explicit preview exception also rejected injected release signing. The
  successful full build used the unsigned preview exception.

The full successful build took **334.281 seconds**. Its exact
command, exit status, timestamps and all 53 before/after input hashes are in
`final-build.json`. The source manifest is
`a62d8f215dd455a365e7f64af7dd9f0e9e7692a01637fe5d2236567a0f9c82dd`.
All inputs remained unchanged throughout the successful build and both guard
checks. `build-summary.json` is the compact machine-readable result.

## Build and dependency provenance

The build used Temurin JDK 21.0.13+9, Gradle 8.13, Android SDK Platform 36
revision 2, Build Tools 35.0.0, command-line tools 23.0 and Platform Tools 37.0.3.
The six tool archives matched their previously recorded exact SHA-256 pins.
All 622 dependency artifacts were independently verified against the unchanged
checked-in SHA-256 metadata. That metadata's SHA-256 remains
`17030e89e7f48a35cbf131d587253171d2ca5014bc1c3e44eb51c54cabf03642`.

The final Gradle invocation used strict dependency verification and offline mode
against a task-local mirror of the verified original artifacts. Repository
configuration and dependency locks were unchanged. The restoration records and
exact helper snapshots are under `evidence/build-tools-0.8.0/` at repository
root. Inherited proxy/CA settings and TLS verification were preserved.

## Preserved initial failure and correction

The first full invocation compiled the debug variants and ran all 90 tests,
but `PairingTest.pairingErrorsProvideRecoveryWithoutSecrets` still expected
one-use Server ID recovery messages. All seven new activation tests passed.
The first result, log and original test/source snapshot are retained in
`build-attempt-1.*` and `initial-failure/`.

With the owning agent's approval, only that test's recovery-message assertions
were updated to the permanent PC ID/active-host flow and the instruction to
check the PC connection list after an ambiguous outcome. Secret-redaction and
other recovery assertions were retained. No production input was changed by
this correction. `recovery-message-test-correction.json` records the exact
before/after source manifests. The complete second invocation then passed.

## Package and signing identity

The signed preview package is `com.openremote.mobile.dev`, labelled
**OpenRemote Preview**, version name `0.8.0`, version code `8`.
The minified unsigned inspection package is `com.openremote.mobile`.
Both target Android 36 and require Android 26 or later.

The preview certificate SHA-256 is:

`24c0441ceca6f73ea0e6de8a0958be6229cfcf2518328e8f7f9b71d98b980b28`

It matches the retained 0.6/0.7 identity and uses the same preview package with a
higher version code. No physical in-place update was performed. No private key
or signing password is included in this evidence directory.

## Artifact hashes

Complete paths and metadata are in `artifacts.json`; `artifacts.sha256` is ready
for an independent checksum check against the build outputs.

| ABI | Variant | Bytes | SHA-256 |
| --- | --- | ---: | --- |
| arm64-v8a | debug | 24,417,783 | `4ac99f04f81b3b2cd20e33b5801bf6f3a85174e31f983767279c253e1684503a` |
| armeabi-v7a | debug | 18,943,559 | `74345b59a030b21c44aa2a38219aa6b83a8d89c0e155f9a916db7e7c332b6f6c` |
| x86_64 | debug | 28,296,577 | `7f87d800e4c862b117beca13d3c880ff6c1c9adf2fe7cc817fcb5354ed92c7b5` |
| x86 | debug | 24,950,067 | `efcfba9769d5ddcc5299ca3578593ca1fcf55bcd6d706131b8b9081e2f358a10` |
| arm64-v8a | release | 13,780,402 | `13cfe47bffcd3a7970865f0254154ad631b5f7109bca1a4365d9b867a4deed59` |
| armeabi-v7a | release | 8,306,178 | `a83ff22d124b2222c6e263fc37bec9f10c843e70fc5ab94fd3b2eb4bf1ddb4d8` |
| x86_64 | release | 17,659,196 | `8bdf785ae47f8ada7defd47e6414739099b90e5e8954aadd2809e1cec753a65f` |
| x86 | release | 14,312,686 | `b9e7cedc3afd30debd58fe6173e23be18d485321266f13cb40a98081a3d00605` |

## Execution limits

The source service profile remains empty. These artifacts cannot establish an
Internet connection until an authorized provider configures and deploys the
service and rebuilds the configured distribution. The unsigned release outputs
are inspection artifacts, not installable customer releases.

This run did not perform a physical Android installation or update, emulator
Activity/Compose or Android Keystore execution, Windows-to-Android video/input,
real Internet/carrier NAT/TURN relay testing, production signing, or store
publication. Build and JVM results must not be described as those checks.
