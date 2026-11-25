# OpenRemote 0.5.1 — Windows startup repair and installer

**19 November 2026 · Unsigned development installer · Native Windows execution remains outstanding**

The reported disappearing Windows executable exposed a first-run defect: version
0.5.0 wrote an incomplete sample configuration and returned an error before
creating any GUI. This source path is removed. The new graphical setup window
keeps missing/incomplete setup and startup errors visible, and an executable
installer now supplies normal Start menu and uninstall entries.

Download/use **OpenRemote-0.5.1-Windows-x64-Setup.exe**. Read
[the setup guide](docs/windows-installer-0.5.1.md). The Android app and coordinator
remain at 0.5.0; their source and prior evidence were preserved, not rebuilt in
this Windows-only repair. No public server or relay was provisioned.

## Changes that address the reported failure

- A visible first-run/configuration/progress window opens before encoder checks
  or enrollment. `--configure` returns to setup. Existing registered settings
  automatically undergo startup checks while progress remains visible.
- The form collects the HTTPS server origin, first-enrollment PC name, trusted
  FFmpeg executable and SHA-256, and a masked enrollment key. The key is kept
  out of configuration, phone links, command arguments and encoder children.
- Failed checks stay in the form for correction/retry. Fatal initialization and
  runtime authorization revocation retain visible errors. GUI launch no longer
  interprets a failed console-signal registration as a Stop request.
- Existing valid permission/trust settings and encrypted PC identity are kept.
  The authoritative server binding cannot be reassigned by editing the form.
  Malformed settings receive a recovery copy only on deliberate repair; saved
  JSON is bounded to the same 64 KiB limit used by the reader.
- Cancel before enrollment prevents it. An already-sent enrollment request
  finishes its bounded transaction and protected credential save before closing;
  cancellation still prevents host startup. A lost response/duplicate-registration
  condition explains the need for owner recovery.
- The per-user installer stages a new release, holds the host's exclusive lock,
  creates host/settings shortcuts, offers a desktop shortcut and registers an
  uninstall entry. Caught activation errors restore prior launch settings.
  Edited shortcuts, user data and earlier release directories are preserved.
- The installer rejects unsupported CPU/Windows versions before activation.
  Minimum: Intel/AMD x64 Windows 10 version 1703 (build 15063) or newer, or Windows 11.

## Executed checks on the final candidate

| Check | Result | What this establishes |
| --- | --- | --- |
| Rust tests | **76 passed**, 0 failed/ignored; 11 new startup regressions | Linux execution of configuration/persistence, protocol/tools and existing lifecycle/process tests |
| Rust formatting | Passed | Source formatting |
| Linux and Windows Clippy | Passed, all targets, warnings denied | Static compilation/lint for both target sets |
| Windows optimized build | Passed in **101.912 seconds** | AMD64 graphical PE linked; no native Windows execution |
| Installer admission/custody tests | **10 passed** | Reject stale source/version/hash, console/DLL/wrong architecture, malformed PE, missing runtime DLL and changed/duplicate extracted payload |
| NSIS compilation | Passed with warnings treated as errors | Native setup and generated uninstaller compiled |
| Independent extraction | Passed with 7-Zip 26.06 | Four payloads byte-identical; generated GUI uninstaller present |
| Artifact/source matching | All 22 Rust inputs match the implementation commit and review pins | Exact checked source and delivered host custody |
| Negative controls | Old console host rejected; removing the serialized-size guard makes its regression fail | The relevant admission and persistence checks detect their intended defects |
| Independent review | No open blocking findings in the pinned startup/installer slice | Requirements and engineering source review, not a complete security audit |

The first-party Cargo package was rebuilt after an isolated negative-control
experiment mistakenly shared its target directory with the production checks.
The failed run, cache diagnosis and subsequent fresh positive check are retained;
no test assertion or production guard was weakened. An earlier successfully
linked host before the final size guard is archived and is **not** delivered.

The initially installed 7-Zip 25.01 could not parse a newer NSIS command form and
hid its generated uninstaller. Official 7-Zip 26.06 resolves that extractor defect;
the complete extraction gate was retained. Both failed and corrected evidence
are included. This tool correction is not native installer execution.

## Exact artifact hashes

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| Windows installer | 3,291,954 | `fe593f19f80b45647e6f434ad6951d5a245e4ea328e51a4a0093db56bbbce5b9` |
| Embedded Windows host | 9,655,296 | `47c5c0a386be1f9b9d75950aefe56a0abc519a643c975bea3561bfe8c986f96b` |
| Generated uninstaller | 141,168 | `719a62fc20f883ef4cb11e47794dbecd265f15f508d198f2c9878258cd6f46b0` |

The NSIS bootstrap/uninstaller are x86 GUI executables that install the AMD64
host. Host imports are Windows system DLLs, including the new common-dialog
import. No separately installed Rust toolchain or custom runtime DLL is needed.
A compatible, separately supplied FFmpeg executable is still required.

## Source and verification records

- Windows implementation and final persistence guard:
  `b25975bf632902a6f65d2f6e71c3722b55010e19`.
- Installer minimum-version and guide capture:
  `058918a8769c62f6d05413a411616a5deaa0b631`.
- Rust input aggregate: `21cff28037ef411a562cfa0823a420408fb1c2fb2086705aa20498237d26a725`.
- [Host build evidence](agent-windows/BUILD_EVIDENCE.json).
- [Final check sequence](evidence/windows-startup-0.5.1/final-checks.json).
- [Final package evidence](evidence/windows-startup-0.5.1/package/installer-build-evidence.json).
- [Independent startup review](evidence/windows-startup-0.5.1/independent-startup-review.md).
- [Independent installer review](evidence/windows-startup-0.5.1/installer-review.md).
- [Baseline symptom and preserved source](evidence/windows-startup-0.5.1/baseline.json).

The complete previous 0.5.0 build status and Windows build record are archived
under `evidence/windows-startup-0.5.1/history/0.5.0`. Earlier Android, coordinator,
media and pilot records retain their original scope and dates.

## Remaining release gates and practical prerequisites

**Native Windows installation, launch, upgrade, rollback and uninstall have not
run here. A Windows-to-phone remote desktop session over the Internet has not
been demonstrated.** Physical setup/focus/DPI/file-picker/DPAPI/capture/input,
Android rendering and direct/forced TURN network cases remain required.

The installer is unsigned. Publisher signing, complete third-party notice
review, operational validation and the broader release gates remain open. The
available notices are embedded unchanged; unresolved existing notice gaps are
not silently declared resolved.

Remote use requires a deployed HTTPS coordinator/relay, its enrollment key and
a trusted standalone FFmpeg executable with a verified expected hash. The
installer does not supply that Internet service or redistribute FFmpeg. It
installs no service or automatic startup and supports the signed-in user's
desktop only. The new setup flow explains missing prerequisites instead of
closing before a window appears.
