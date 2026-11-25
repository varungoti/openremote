# License evidence directory

Start with `../THIRD_PARTY_NOTICES.md`. This directory is an evidence inventory;
its current completeness status is **BLOCKED** for public redistribution.

- `dependency-inventory.json`: exact dependency metadata and content hashes.
  It also binds the final Gradle inputs/lockfile/verification metadata by hash,
  compares the 98 runtime components with the lock and checks 64 cached runtime
  AAR/JAR hashes against the existing verification pins.
- `dependency-inventory.md`: package, direct-declaration and resolved-runtime tables.
- `rust/`: 575 available license/notice files from the exact cached locked crates.
- `rust-upstream/`: 11 exact-commit upstream text copies, published VCS metadata
  and provenance records; four packages still lack retrievable notice text.
- `android/poms/`: exact cached POMs for direct Android declarations and all 98
  selected release runtime components.
- `android/releaseRuntimeClasspath.txt`: captured successful Gradle dependency report.
- `android/notices/`: 41 available embedded notice files from cached AAR/JAR archives.
- `ffmpeg/`: exact FFmpeg 9.0.2 LGPL/source-license and LLVM/MinGW runtime texts,
  provenance and build recipe. The exact source archive accompanies the installer.
- `go/`: official toolchain license for the standard-library-only coordinator.
- `upstream/`: exact fetched WebRTC/packaging reference texts and provenance.
- `collect_inventory.py`: local, read-only source collection plus output writing.

Example rerun on a machine with those caches already available:

```sh
python3 licenses/collect_inventory.py \
  --cargo-source "$CARGO_HOME/registry/src" \
  --gradle-cache "$GRADLE_USER_HOME/caches/modules-2/files-2.1" \
  --go-root "$(go env GOROOT)"
```

This script never chooses a license for a package, downloads dependencies,
substitutes generic license text, or declares legal/redistribution approval.
It records alternative expressions exactly as supplied by publishers.

Some packages include target- or build-specific dependencies that may not ship
in the released binary. A final binary-specific dependency audit remains needed.
The collector reads `android/verification/release-runtime-dependencies.txt` by
default; `--android-runtime-report` selects another successful Gradle report.
