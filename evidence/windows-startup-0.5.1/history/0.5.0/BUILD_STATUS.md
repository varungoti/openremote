# OpenRemote 0.5.0 — build and release status

**18 November 2026 · Development candidate · Public release blocked**

The Android test APK, Windows host and Linux coordinator are built. This pass adds
phone setup import, explicit connection checks, a persistent visible PC home and
fixes for settings/request ownership races. **A real Windows-to-phone session
over the Internet has not been demonstrated.** Production readiness is not claimed.

## What is implemented in this continuation

- The Windows home shows the PC number, full origin and setup link. Copy is an
  explicit local action. Stop, close or Escape cancels remote access. A scrollable
  content pane and separate footer accommodate small/high-scale work areas.
- Android accepts paste or a custom-scheme intent, shows the full server/PC for
  review, and waits for confirmation. Parsing/review performs no network or login.
  Confirmation saves a persistent favorite with a neutral PC-number label; it
  does not trust the phone or connect. Ordinary host approval still applies.
- Explicit checks distinguish server compatibility, phone proof and PC presence.
  Advertised UDP/TCP/TLS relay flags describe configuration; they do not prove a
  working allocation, candidate route, decoded desktop or remote input.
- Settings loads, saves and session-favorite writes retain ownership through
  cancellation. Stale requests cannot replace a newer server's UI state. Malformed
  or ambiguous ICE configuration fails coordinator startup.

See [the installation guide](docs/start-here-0.5.md), [wire and native acceptance
contract](docs/phone-setup.md), and [expanded plan](PRODUCT_PLAN.md).

## Current executed checks

| Check | Result | Boundary |
| --- | --- | --- |
| Go tests | **23 top-level tests plus subcases pass**, race suite and static-build suite | Coordinator behavior; app coverage 77.4% |
| Go vet/static package | Passed; statically linked Linux amd64, CGO disabled | Binary embeds clean implementation revision below |
| HTTPS integration | **10 scenarios pass** with verified temporary CA and actual coordinator restart | No Windows pixels, phone rendering or relay/media transport |
| Rust | **65 tests pass**, formatter, Linux/Windows Clippy and release link pass | Native Windows executable cross-compiled; not executed on Windows |
| Android JVM | **63 tests pass**, 0 failures/errors/skipped | Includes 17 new setup/check tests; not an Activity/device run |
| Android lint | **0 errors**, 5 existing pinned-version suggestions | Static checks, not a security audit |
| Android packages | Four debug, four unsigned release and one instrumentation APK | Strict locked/checksum-verified offline build; instrumentation not executed |
| Android package checks | ARM64 signature, 16 KiB ZIP alignment and release native ELF alignment pass | Device installation, upgrade and 16 KiB runtime remain untested |
| Python | **16 tests pass** | Synthetic pilot-record rejection and deployment configuration |
| Independent review | 22 reviewed source/test/build-input files match pins; concrete findings resolved | Focused source review, no full security or native runtime attestation |

Final Android build: **BUILD SUCCESSFUL in 1m 32s**, 133 tasks
(28 executed, 105 up to date).
Windows release link: **49.167 seconds**. The superseded 62-test Android candidate
is archived and is not the delivered APK.

## Principal artifacts

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| windows | 9,433,088 | `ef0b0470444edd6c4f15435a2155094ac7e9dbaa1985242578faa163cfc1948f` |
| android-test | 24,726,168 | `de856ea9917927af0354a46e3c43cc2e7b5203222c96f3470ac4b0e1c54d0a16` |
| android-release | 13,796,554 | `bd64d756ed233fa142402c7f931d2c97028864a66133b6158bb99d47f8afe0df` |
| coordinator | 10,718,249 | `8b1d24ea5613bc3417498bec1ffd98e7a9c50791bff859f7ba8dc23d521f43b9` |

The Android test APK uses versionCode 5 and the same development certificate as
0.4 (`a0cc38598018756a992fb04c5aa327060f124e8464becc1f9a2f757f5186d21f`). Signature continuity
supports the intended upgrade path; it does not establish a device upgrade.
Install the **Test.apk**, not the unsigned owner-signing input. The Windows host
is unsigned; the installer requires explicit `-TestBuild`. FFmpeg is external
and must be checksum-verified. No production signing key is supplied or generated.

## Source and artifact custody

Implementation capture: **`cc296948a90efa1b066ceec18015632699c38fcc`** on `work/release-hardening`.
All **20 Windows, 48 Android and 14 coordinator inputs** match that commit and
their build records. Android's nine APKs and 27 evidence files were independently
checked. Windows source/evidence manifests were also checked. The final static
coordinator embeds this revision with `vcs.modified=false`; its exact bytes passed
the fresh ten-scenario HTTPS integration test.

The final handoff commit/tag and bundle checksum are added to the downloadable
verification file after the evidence commit. This custody check does not attest
independent reproducible builds, publisher identity or hardware execution.

- [Consolidated results](evidence/final-build-verification.json)
- [Committed source comparison](evidence/committed-source-verification-0.5.json)
- [Windows build record](agent-windows/BUILD_EVIDENCE.json)
- [Android verification](android/VERIFICATION.md)
- [Coordinator record](server/verification/setup-verification.json)
- [Independent review](evidence/setup-review-0.5.md)

## Evidence carried forward

The 0.4 network-free encoder fixture produced 30 complete access units, 30 decoded
frames and 30 distinct hashes. Its encoder/configuration/fixture bytes are unchanged;
that evidence is **carried forward**, not rerun or presented as native/networked
media. Four 0.3 PowerShell file activation/rollback tests are also carried forward
after comparing the four unchanged scripts. They do not prove real Windows
installation. The old results and intermediate failed/superseded runs are retained.

## Release gates still open

1. **Actual devices:** install on the phone and Windows PCs, exercise Keystore,
   DPAPI, the visible home, intent delivery, UI scaling, desktop capture/rendering,
   input, clipboard, file metadata, consent, rotation and shutdown.
2. **Internet service:** configure an authorized HTTPS coordinator and TURN relay
   with real DNS/TLS, then demonstrate direct Internet and TURN UDP/TCP/TLS443 on
   separate networks, credential renewal, handoff and service restarts.
3. **Media fixtures:** the earlier native fixture was blocked by
   `netlinkrib: operation not permitted` before media; Chromium was blocked at
   local-socket initialization. Neither restriction was bypassed or rerun for 0.5.
4. **Phone setup:** the eight native acceptance steps in `docs/phone-setup.md`
   remain unexecuted, including cancellation, process restart, an active session
   or SAF picker, origin changes, and host Stop during streaming.
5. **Distribution/operations:** publisher signing, complete native provenance,
   independent security review, supported-system performance, installation/upgrade
   and backup/restore are incomplete.

The exact-candidate pilot record has **17 `not_run`, zero passed** cases. It is
structurally valid and its completion gate was actually invoked and rejected it.
Its validator checks recorded assertions and files; it cannot attest execution.
The phone setup supplement is also required before release.

## Dependency provenance and remaining product scope

No package dependency was added or upgraded. The host enabled standard controls
in the existing `windows-sys` crate. The inventory contains 319 Rust packages and
98 Android runtime components; **755 file references / 743 unique local files**
matched their hashes. Four Rust notice gaps and the exact native Android WebRTC
source/build/transitive-notice mapping remain unresolved. No FFmpeg binary is
redistributed. First-seen checksums are not publisher attestation.

The host needs a logged-in desktop. Current software video is up to 720p30, with
fresh-session monitor changes. Clipboard is explicit text (4 KiB). Uploads are
phone-to-PC only (64 MiB/file, 256 MiB reserved/session); a crash can leave private
partial files. Audio, hardware/adaptive encoding, reverse/resumable transfers,
full host settings/trust management, service/reboot/secure-desktop access,
independent peer negotiation signatures, biometrics, signed updates and iOS remain
planned. See the plan for their acceptance criteria.

No destination GitHub repository or public deployment is configured. No PR,
production deployment or successful remote session is asserted by this handoff.
