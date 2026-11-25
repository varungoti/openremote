# Restored build tools for OpenRemote 0.8.0

The task-local build environment is ready. Source it before building:

```bash
source /workspace/scratch/72f8c6db0182/toolchains/env.sh
```

| Tool | Verified version |
| --- | --- |
| Rust | rustc 1.100.0 (0ef1bc386 2026-11-09) |
| Cargo | 1.100.0 (3acc79c0c 2026-11-05) |
| rustfmt | 1.11.0-stable |
| Clippy | 0.1.100 |
| Windows linker | LLVM-MinGW 20261104 UCRT, Clang 23.1.5 |
| Windows target | x86_64-pc-windows-gnullvm |
| NSIS | v3.11-1 |
| 7-Zip | 26.06 |
| Go | go1.27.4 linux/amd64 |

The environment selects task-local Cargo and Go caches and sets
`GOTOOLCHAIN=local`. It configures only the Windows target's Rust linker,
C compiler, C++ compiler, and archiver, so native Linux test builds can continue
using the system compiler. The NSIS resource directory is set through `NSISDIR`.

## Verification

- Every downloaded archive matched the exact SHA-256 and byte count from the
  repository's saved provenance. The archive URLs, expected and actual hashes,
  and timestamps are in `restored-downloads.json`.
- A Rust hello-world program linked for `x86_64-pc-windows-gnullvm` and was
  independently identified as an AMD64 Windows PE by `llvm-readobj`.
- A minimal NSIS installer compiled with `-WX` and was extracted with 7-Zip
  26.06. Its generated uninstaller was present and identified as a Windows GUI
  PE. Commands and results are in `toolchain-verification.json` and its logs.
- Both `cargo fetch --locked` and `cargo fetch --locked --offline` passed.
  All 319 cached registry archives were independently hashed against the
  current `Cargo.lock`; see `cargo-cache-verification.json`.

## Reproduction inputs

The pinned downloads come from these existing source records:

- `evidence/windows-startup-0.5.1/toolchain-provenance.json`
- `evidence/windows-startup-0.5.1/extractor-repair-provenance.json`
- `server/verification/automatic-registration-0.7.0/go-toolchain-provenance.json`

Task-local restoration and verification helpers are in
`/workspace/scratch/72f8c6db0182/toolchains/`. All HTTP requests used inherited
proxy and CA settings with TLS verification. No networking policy, global
compiler installation, repository source file, or lockfile was changed by this
restoration.

## Android restoration

The later Android update uses a separate environment:

```bash
source /workspace/scratch/72f8c6db0182/toolchains/android-0.8/env.sh
python3 /workspace/scratch/72f8c6db0182/toolchains/android-0.8/run-build.py <Gradle tasks>
```

JDK Temurin 21.0.13+9, Gradle 8.13, Android SDK Platform 36 revision 2,
Build Tools 35.0.0, command-line tools 23.0, and Platform Tools 37.0.3 were
restored from the repository's exact saved download pins. See
`android-restored-downloads.json`. All 622 dependency artifacts matched the
unchanged checked-in SHA-256 verification metadata. See
`android-dependency-restore.json` for exact URLs and hashes.

The initial dependency pass found 27 expected metadata aliases whose filenames
are not their repository URLs. The pinned Gradle `.module` files supplied their
canonical URLs. All 27 then matched their original SHA-256 pins; no checksum,
dependency version, or source dependency configuration changed. The initial
attempt and alias-specific results are retained for provenance.

The runner uses a task-local Maven file repository containing those verified
bytes, with Gradle's strict dependency verification and offline mode supplied by
the final build command. A task-local Java truststore preserves the JDK roots
and adds only the inherited configured CA certificate. Java proxy properties
are derived from the existing proxy environment. The application build keeps
its original repository files, lock state, and verification metadata.

The separately restored preview keystore retains certificate SHA-256
`24c0441ceca6f73ea0e6de8a0958be6229cfcf2518328e8f7f9b71d98b980b28`.
The runner refuses to build with a missing or mismatching key; it never creates
a replacement. No private key bytes are included in these records.

## Browser check

The existing primary runtime provides Playwright 1.62.1, matching the repository
pin. An ordinary headless launch of installed Chromium 156.0.8078.79 with its
Chromium sandbox enabled failed before page creation: the environment rejected
the process-singleton socket and ptrace operations with `EPERM`. No custom
browser flags or policy changes were attempted. See `chromium-launch.json`.

This directory records build-tool availability and its limits. Android application
build evidence is recorded separately under `android/verification/activation-0.8.0`.
Neither these checks nor cross compilation establish native Windows or Android
execution. FFmpeg was recovered separately by the parent agent.

## Preserved helper and review records

`helpers/` contains the exact task-local restoration/build verification scripts,
with SHA-256 values in `helper-snapshots.json`. They contain no private key or
generated proxy credentials. `independent-review.json` records bounded read-only
Windows, backend and MCP review scope, exact reviewed hashes, the corrected
small-window roster layout finding and execution limits.
