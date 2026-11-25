# Third-party dependency notices and inventory

**Inventory checked: 18 November 2026; encoder update: 19 November 2026; agent bridge inventory: 20 November 2026. Public redistribution notice gate: BLOCKED.**

This file describes verified package metadata and the license/notice text copies
available in this source tree. It is not a complete legal notice bundle for a
signed Windows installer or Android release. No conclusion about license
compatibility, patent coverage or redistribution permission is asserted here.

## Evidence included

| Component | Evidence collected | Scope and limitation |
| --- | --- | --- |
| Production Go coordinator | `server/go.mod` has no third-party modules; official Go toolchain license copied to `licenses/go/GO-LICENSE.txt` | Standard-library application; this does not include the separate Pion test tools |
| Rust Windows host | All **319 locked registry packages** have exact cached manifests; all **319 cached archive SHA-256 hashes match Cargo.lock**; **575 cached license/notice files** and **11 exact-commit upstream text copies** collected | Cargo.lock includes build, development and target-specific packages; this is not an exact list of objects linked into one Windows build |
| Android direct dependencies | **19 declarations** with exact cached Maven POM license metadata; versionless Compose declarations resolved through the declared BOM | Includes runtime, debug, unit-test and instrumentation declarations |
| Android resolved release runtime | **98 selected components** in a successful Gradle `releaseRuntimeClasspath` report; all **98 exact cached POMs** read; **64 cached AAR/JAR files** hashed; **41 embedded notice files** copied | Includes transitive components and platforms/metadata modules. This is not a final signed APK binary SBOM or complete native component notice bundle |
| Android native WebRTC | Exact AAR and all four native libraries hashed; repository/upstream license and patent texts copied | The AAR and its nested classes.jar contain **no license/notice entries**. Complete native component notices tied to the exact binary remain unavailable |

The detailed [inventory](licenses/dependency-inventory.md) lists every Rust
package and Android declaration. Its [JSON version](licenses/dependency-inventory.json)
records metadata, package/archive/file hashes, source coordinates, copied notice
paths and unresolved gates. [The collector](licenses/collect_inventory.py)
reproduces the local inventory from the Cargo source/archive cache, Gradle cache
and official Go toolchain; it does not infer missing licenses.

The captured [Android runtime report](licenses/android/releaseRuntimeClasspath.txt)
records the selected versions after dependency conflict resolution. The collector
excludes constraint-only rows and deduplicates repeated dependency subtrees.
Matching POMs are copied to `licenses/android/poms/`; available notice text from
cached AAR/JAR files, including nested `classes.jar`, is preserved under
`licenses/android/notices/`. A module without a cached AAR/JAR may be a platform
or metadata component; its inclusion in a final APK is not inferred. POM license
declarations and copied embedded files still need completeness review against
the exact signed artifacts that will ship.

The final Android declarations include `kotlinx-coroutines-test:1.10.2` and total
**19 direct declarations**. The app lockfile covers **91 configurations / 133
components**; its **98 release runtime components** match the captured report.
The existing Gradle verification metadata covers **367 components / 619
artifacts** across build scopes. All **64 cached runtime AAR/JAR hashes** match
those entries. The inventory records exact hashes for the relevant Gradle
scripts, wrapper settings, lockfile and verification metadata. These first-seen
checksum pins are not publisher signatures or a complete license inventory of
all build tooling; they do not change the redistribution gate.

## Agent bridge dependencies added in 0.8

The new local Node.js MCP bridge has a separate exact-lock inventory under
[licenses/agent-bridge](licenses/agent-bridge/README.md). Its
[dependency record](licenses/agent-bridge/dependencies.json) covers six locked
packages and five locally installed packages, with nine original LICENSE,
NOTICE or ThirdPartyNotices files copied without changes. The package lock and
bridge source hashes are recorded. Direct pins are MCP server 2.4.0, Playwright
1.62.1 and Zod 4.6.7; version labels are inventory facts, not update advice.

The optional Darwin-only `fsevents` package was not installed in this Linux
workspace and has no copied local original notice. The Chromium runtime is a
separate binary download: its exact distribution notice bundle is not established
by the npm package inventory. Third-party agent-client redistribution and a
complete signed bridge/browser binary SBOM are also outside this inventory.
These open items join the existing release notice gates below; the source
checkpoint does not claim complete public-redistribution compliance.

## Rust license copies

Available files named `LICENSE`, `LICENCE`, `COPYING`, `NOTICE` or `PATENTS`
(including nested variants and declared license-file paths) are copied beneath
`licenses/rust/<crate>-<version>/`. Their original text is preserved. The raw
license expression in each published manifest is recorded without interpreting
or choosing between alternative license terms.

The initial filename scan left eleven packages without copied notice text.
Seven now have additional verified evidence:

- `asn1-rs-impl 0.2.0`, `crc-catalog 2.5.0`, `webrtc 0.17.3` and `yasna 0.5.2`:
  upstream license files retrieved from the exact commit recorded in the
  published crate's `.cargo_vcs_info.json`.
- `precis-tools 0.1.10`: the exact-commit repository root MIT `LICENSE` copied.
  Its manifest declares MIT/Apache-2.0 alternatives, while no Apache license
  text was found in that inspected commit tree. No alternative was chosen.
- `r-efi 5.3.0` and `r-efi 6.0.0`: their `AUTHORS` files contain the project's
  license declarations, MIT permission text and copyright notices. These files
  are now collected from the published crates and from their exact upstream
  commits; the bytes match. The declared alternatives are preserved.

The [Rust upstream provenance record](licenses/rust-upstream/provenance.json)
records original URLs, commits, source paths, checked dates, download outcomes
and hashes. All nine available VCS metadata records were checked against the
checksum-verified published crate archives. Retrieved ordinary license files
also match their exact-commit Git blob hashes; both `r-efi` `AUTHORS` files match
the archived crate contents. These source records do not establish complete
binary notice compliance.

The following **four packages** still have no copied notice text:

- `precis-core 0.1.11` and `precis-profiles 0.1.13`: the recorded commits could
  not be retrieved from the declared repository. GitHub tree requests returned
  422 and exact-commit root `LICENSE` requests returned 404.
- `winapi-i686-pc-windows-gnu 0.4.0` and
  `winapi-x86_64-pc-windows-gnu 0.4.0`: the published crates lack VCS metadata;
  an exact corresponding source revision has not been established.

This is a gap in the assembled notice package, not an assertion that those
projects have no license. Their exact corresponding source notices still need
to be collected and reviewed. Source headers or vendored components may also
carry notices that filename-based collection does not identify.

## Android WebRTC provenance

Declared artifact:

```text
io.github.webrtc-sdk:android:150.7871.04
AAR SHA-256:
1872dda4e3bf7d6ea6f7c6e11861332081c57081a1e62fc82fab72415c69c84c
```

Verified sources and exact downloaded files:

- Android packaging repository, exact tag `v150.7871.04`:
  [source](https://github.com/webrtc-sdk/android/tree/v150.7871.04),
  [MIT license copy](licenses/upstream/webrtc-sdk-android-LICENSE.txt), and
  [release metadata](licenses/upstream/webrtc-sdk-android-release.json).
- Native fork reference commit
  `4de68c8241d1c04d866aa1efd64f0be71317ad79`:
  [source](https://github.com/webrtc-sdk/webrtc/tree/4de68c8241d1c04d866aa1efd64f0be71317ad79),
  [BSD license copy](licenses/upstream/webrtc-sdk-webrtc-LICENSE.txt),
  [patent text](licenses/upstream/webrtc-sdk-webrtc-PATENTS.txt),
  [fork NOTICE](licenses/upstream/webrtc-sdk-webrtc-NOTICE.txt), and
  [upstream README](licenses/upstream/webrtc-sdk-webrtc-README.md).
- Official Google WebRTC current upstream reference:
  [license source](https://webrtc.googlesource.com/src/+/refs/heads/main/LICENSE),
  [license copy](licenses/upstream/google-webrtc-LICENSE.txt),
  [patent source](https://webrtc.googlesource.com/src/+/refs/heads/main/PATENTS), and
  [patent copy](licenses/upstream/google-webrtc-PATENTS.txt).

The Maven POM describes the Android artifact as using the 3-Clause BSD License.
The Android packaging repository's own license file is MIT. The native fork
README identifies the Google BSD code and Apache-licensed patches/changes.
These statements describe different source scopes; none establishes a complete
license inventory of the compiled JNI libraries.

The pinned native fork commit is an inspected reference, **not a proven source
revision for the published AAR**. The retrieved Android release metadata does
not establish that mapping. A request for `LICENSE_THIRD_PARTY` in the fork
returned 404; no replacement was invented. The complete build-specific native
third-party notice bundle, actual source/dependency revisions and build flags
must be obtained from the publisher or a reproducible build before public
redistribution. Individual JNI hashes are recorded in the JSON inventory.

[Upstream provenance](licenses/upstream/provenance.json) records source URLs,
checked dates, download outcomes and SHA-256 hashes. Maven POM license metadata
was read from the exact cached artifacts; canonical Maven URLs are provided,
without claiming fresh online retrieval of every POM during this inventory task.

## Bundled FFmpeg and other excluded materials

The 0.7.0 Windows installer bundles a separate, minimal FFmpeg 9.0.2 executable
built from the exact signature-verified upstream archive. The selected build uses
Windows Media Foundation `h264_mf`; GPL/nonfree configuration and external codec
libraries are disabled. This build is LGPL-2.1-or-later. That statement describes
these recorded bytes and flags, not every possible FFmpeg distribution.

Exact upstream LGPL/source-license texts, LLVM/MinGW runtime notices and provenance
are in `licenses/ffmpeg/`. The installer also includes the unmodified corresponding
source archive and the complete build recipe/evidence. See
`licenses/ffmpeg/BUILD.md` and `evidence/one-button-0.7.0/encoder/` for source,
encoder and toolchain hashes, enabled components and signature verification.
Actual Windows encoding and performance were not executed in this workspace.

The Go Pion media/relay test harnesses, Python test dependencies, Gradle/Android
build tooling and operating-system libraries are not an exhaustive part of this
production inventory. Deployment container images have their own contents and
notice requirements; the source package does not flatten those images into the
client notices.

## Remaining release-notice work

1. Obtain the complete native Android WebRTC notice bundle and source/build
   provenance for the exact AAR hashes being shipped.
2. Resolve the four remaining Rust package notice-text gaps, review source headers and
   native/vendor notices, and inventory the exact final Windows target/features.
3. Bind the captured resolved Android runtime graph to the final signed release
   artifacts and complete the corresponding component notice review.
4. Review the supplied exact FFmpeg source, notices and build evidence against the final signed installer; the 0.7 preview now includes those materials.
5. Attach the reviewed notices to the actual installers/apps and retain the
   corresponding source/provenance records.

These are recorded unresolved release gates; the existence of copied license
texts is not being used as a substitute for completing them.
