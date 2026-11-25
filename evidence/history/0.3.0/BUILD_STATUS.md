# OpenRemote 0.3.0 — build and release status

**18 November 2026 · Development build · Public release blocked**

The project now produces an actual Windows x64 executable, Android APKs and a
Linux coordinator. It implements native WebRTC peers, authenticated authorization,
Windows capture/input, mobile controls, explicit clipboard actions and phone-to-PC
file uploads. Compilation and focused tests are verified below. **A successful
physical Windows-to-Android remote desktop session has not been demonstrated in
this environment.**

The expanded scope, priorities and acceptance targets are in
[PRODUCT_PLAN.md](PRODUCT_PLAN.md). The complete release criteria are in
[docs/release-gates.md](docs/release-gates.md). A prepared PR description is included;
no destination repository or public deployment has been configured.

## Implemented behavior

| Component | Current implementation |
| --- | --- |
| Coordinator | Permanent nine-digit PC IDs, P-256 phone proof, protected host enrollment, local approval, permission narrowing, presence, SDP/ICE routing, temporary TURN credentials, durable trust/revocation/audit and monotonic event sequence |
| Windows host | Current-user DPAPI identity, explicit local consent and disconnect, exact-key unattended trust, real H.264 WebRTC peer, primary-desktop capture, mouse/keyboard/Unicode input, bounded recovery and input release |
| Android | HTTPS configuration, saved PCs, Keystore identity, native renderer, direct touch/trackpad/keyboard, real-frame readiness, bounded recovery, diagnostics and explicit lifecycle handling |
| Clipboard | User-triggered PC text read/write, phone preview and explicit copy, 4,096 UTF-8 byte limit, separate host enablement and session permission; no background clipboard monitoring |
| File upload | Android document picker and private snapshot; phone-to-PC only; 8 KiB chunks, 64 MiB/file, 256 MiB/session reservations, progress/cancel, exact offsets/length/SHA-256, fixed receive folder, no overwrite or automatic execution |
| Installation | Verified standalone FFmpeg preflight, staged version directory, effective configuration checks, preserved enrollment, caught-failure activation rollback, separate received-file deletion choice |
| Build/deployment | Locked Rust/Android dependency inputs, Gradle checksum verification, manually triggered owner-controlled CI, Caddy/coturn configuration and operational runbook |

Host input, clipboard and received-file operations are native source implementations;
Linux tests use explicit adapters for native effects. They still need Windows
execution. The video profile is software H.264 up to **1280×720 at 30 FPS**.

## Executed verification

| Check | Observed result | Boundary |
| --- | --- | --- |
| Go unit/race suite | Passed; 17 top-level test functions plus subcases; app package coverage 75.1% | Coordinator logic and races; command package coverage is separately 0% |
| Go vet | Passed | Static analysis |
| Independent HTTPS integration | Eight recorded scenarios passed, including real process restart, wrong-key/replay rejection, host consent, grant narrowing, TURN HMAC and ended-session rejection | Verified temporary TLS CA; control-plane behavior only |
| Rust host tests | **42 passed**, 0 failed/ignored; includes 24 tools tests | Linux; actual temporary-file I/O, explicit native clipboard/zone test adapters |
| Rust Clippy/format | Linux and Windows target `-D warnings` passed; formatter passed | Source/static checks |
| Windows executable | Optimized gnullvm x64 binary compiled and linked | Unsigned; not executed on Windows |
| Android JVM tests | **32 passed**, 0 failures/errors | API, lifecycle, touch mapping, tools validation, transfer/cancellation/cache behavior and frame freshness |
| Android lint | **0 errors, 5 warnings** | Warnings concern newer pinned-dependency versions |
| Android packages | Debug and minified unsigned release APKs for four ABIs; instrumentation APK compiled | No connected-device test executed |
| Android package checks | ARM64 debug signature, debug/release 16 KiB ZIP alignment and final native ELF LOAD alignment verified | Does not establish execution on a 16 KiB-page device |
| Strict Android dependency build | Passed with existing lock state and strict checksum verification, offline | 91 locked configurations; 367 components / 619 checksum-pinned artifacts across build scopes |
| Deployment configuration | **3 Python tests passed** | Secret consistency, private files, input checks and no overwrite; Docker/coturn not run |
| Installer file activation | **4 cases passed** using actual replacement/rollback operations | Linux PowerShell 7.7; no Windows COM/signing/installation execution |
| PowerShell syntax | All four scripts parse without errors | Native Windows behavior still untested |

The final Android build took 1 minute 35 seconds: 133 tasks, 31 executed and 102
up to date. Its test task reran the final 32 tests. Optional connected-device
runner/UTP configurations unavailable in the offline cache are documented in
`android/VERIFICATION.md`; those paths are not counted as verified.

### Prior encoder evidence retained

The unchanged production encoder/access-unit assembler path previously produced
**30 complete H.264 units, 30 decoded frames and 30 distinct frame hashes** from a
synthetic moving source. This was executed during 0.2 and was not unnecessarily
rerun for the 0.3 tools change. It proves that network-free path; it is not evidence
of transported desktop video, native Windows capture or Android decoding.

## Regressions found and corrected

The original foundation's unauthenticated controller request and pre-approval
signaling failures were reproduced before correction. Subsequent tests/review
found and corrected:

- Host event numbers reused after a coordinator restart, silently skipping new requests.
- A pending long poll publishing an event after its durable storage write failed.
- Host heartbeat/name bounds inconsistent with the coordinator.
- Historical/orphan sessions and transient status-fetch errors interfering with new requests.
- Successful ICE recovery leaving coordinator state at `connecting`.
- NUL text and Windows filename validation differing between clients.
- Frozen video leaving phone input enabled.
- A file-finalization race allowing publication during local revocation.
- Another Android Activity's cache recovery removing a live upload snapshot.
- PowerShell passing an empty backup path during rollback; the failing actual-file
  regression is retained before the corrected four-case pass.

These are focused fixes, not a completed independent security audit.

## Exact principal artifacts

| Artifact | Bytes | Signing state |
| --- | ---: | --- |
| Windows host x64 | 9,224,192 | Unsigned test executable |
| Android ARM64 debug APK | 24,587,694 | Local debug identity; installable test build |
| Android ARM64 release variant | 13,763,614 | Unsigned; owner signing required before installation/distribution |
| Linux amd64 coordinator | 10,712,827 | Native executable; tested through isolated HTTPS integration |

```text
Windows host:
950d68915594e3413fb55aeb26cbe6e8e0e64c08cc27980babf4db06c0442162

Android ARM64 debug:
f1289b55b1c5b99548979473f5b43c6ee678f443106f31b1818643e58392e53a

Android ARM64 unsigned release:
4adc6462f1bc814d75dae56ecfbbdc7237148cdc7ee471ae247d9320ae128e9f

Linux coordinator:
fe1c5bb5887b94d3c441f64051f4c2077123f50d7e81996802c9ee648b8ba55a
```

Build evidence matches 18 host source inputs and 41 Android build inputs, plus all
nine APKs and 13 Android evidence files. The final server's recorded 11 input hashes
and its binary were checked. See [final-build-verification.json](evidence/final-build-verification.json),
[host build evidence](agent-windows/BUILD_EVIDENCE.json),
[Android verification](android/VERIFICATION.md) and
[server regression evidence](server/verification/cursor-fix-verification.json).

The Windows PE imports Windows/UCRT components only; no third-party compiler-runtime
DLL is listed. The app still requires a compatible, separately supplied FFmpeg
executable. No FFmpeg binary is bundled or represented as license-cleared.

## Blocked or unexecuted runtime gates

1. **Networked media:** the independent Pion/Rust fixture was denied network-interface
   discovery (`netlinkrib: operation not permitted`). Direct, TURN/UDP and TURN/TCP
   cases received **zero packets/decoded frames**. The browser fixture was blocked
   at Chromium startup by a denied local socket. Neither is a pass. These 0.2
   attempts were retained and not retried or bypassed for 0.3.
2. **Physical endpoints:** no Windows desktop or attached Android phone/emulator
   was available. DPAPI/Keystore, native capture/input, consent, clipboard, ACLs,
   reparse handling, zone metadata, handle rename, document picker and lifecycle
   must run on actual devices.
3. **Internet deployment:** no public coordinator or coturn instance was deployed.
   Carrier NAT, UDP blocking, TURN/TCP/TLS/443, Wi-Fi/mobile handoff, renewal and
   multi-PC operation still need recorded Internet tests.
4. **Installation and crash recovery:** no native install/upgrade/uninstall matrix,
   power-loss activation test or host crash cleanup is complete. A host process
   crash can leave a private `.part` upload file. Normal cancellation/drop cleanup
   tests do not close this crash-recovery gate.
5. **Production signing and supply chain:** no owner signing keys or signed updater
   exist. Native WebRTC source/build/complete notices and four exact Rust notice
   gaps remain. First-seen Gradle hashes are not publisher attestations. The full
   inventory is under `licenses/` and `THIRD_PARTY_NOTICES.md`.

## Advanced product work still required

Monitor selection, hardware/adaptive encoding, measured 1080p60 and higher profiles,
PC audio, reverse/resumable transfers, host settings/trust UI, startup/service and
reboot recovery, secure-desktop policy, independent peer-signed negotiation,
biometric phone gating and signed updates are unfinished product requirements.
Fleet/RBAC, Wake-on-LAN, privacy screen, invitations, recording and more platforms
are later milestones. The current coordinator is trusted to bind identity to SDP;
a compromised coordinator is outside the current trust boundary.

## Source provenance and handoff

The original supplied `openremote-foundation.zip` is preserved as the `main` Git
baseline, commit `c97543f6f988c14046228a0313882b5fe1269f5e`. Its SHA-256 is:

```text
179962e6d3c3ed83f0e9cf24b0aac591a08aa80cfdf356ada877ddc7d282522a
```

Implementation is on `work/release-hardening`. The source handoff includes the
history, complete source, lockfiles, scripts, product plan, protocol contracts and
verification evidence. Binaries are separate from source history. The destination
repository, owned device runners and public deployment details are needed to
publish the PR and execute the remaining physical/Internet release gates.
