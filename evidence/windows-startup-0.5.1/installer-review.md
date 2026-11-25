# OpenRemote 0.5.1 installer source review

Date: 19 November 2026. Reviewer: separate packaging-review task, read-only review of
installer and builder implementation. This reviewer implemented the host startup
change but did not author the installer. This record does not replace the
separate host-source review.

**Result: no outstanding blocking source findings in the pinned files below.**
This is source review, not a Windows installation test. Installation, upgrade,
uninstall, native shortcut COM operations, and injected Windows rollback tests
remain **not_run** in this Linux environment.

## Exact reviewed inputs

| File | SHA-256 |
| --- | --- |
| `packaging/windows/installer/OpenRemote.nsi` | `ac4640578b38a854ccaaabada829936aec41d4cc1ea2d6e68f6bb54c0a832cba` |
| `scripts/build-windows-installer.py` | `aa07c9bfe7b8f8b25a1f3e5c7b970bf28b7554c5d7962cb9740574e843a8388b` |

The reviewed bytes were rechecked before writing this record. The final artifact
assembly must retain these hashes or obtain review of subsequent changes.

## Concrete findings and resolutions

1. **Desktop shortcut ownership on upgrade.** The draft selected the desktop
   checkbox from a previous installation flag, then replaced the shortcut on a
   successful upgrade without checking whether the user had repointed it. The
   final source auto-selects only when the saved target and arguments still
   match the previous host. It rechecks under the exclusive lock before mutation.
   A selected but foreign or edited existing shortcut causes a visible failure
   with rename/uncheck guidance. The unselected foreign shortcut remains intact.
   Resolved in the pinned source; native execution remains untested.
2. **Staging-directory ownership during rollback.** The draft assigned
   `ReleaseDir` before checking for an existing directory and cleaned up whenever
   that variable was nonempty. An explicit collision failure could consequently
   remove payload files from a preexisting release. The final `StageCreated`
   flag starts false and becomes true only after the absent-directory guard and
   successful directory creation. Cleanup requires that flag and switches its
   current directory away from staging before removing it. Resolved in the
   pinned source; a forced-collision Windows acceptance test remains unexecuted.

## Requirements and control-flow review

- The installer is per-user, requests ordinary user privileges, checks Windows
  10 version 1703/build 15063 or later and native AMD64, and installs the host
  plus documentation/notices.
  It does not install a service, configure automatic startup, alter host
  permissions, deploy the coordinator, or silently supply an encoder.
- Installation and uninstallation acquire the same exclusive host-instance
  file lock used before the application's setup window. Failure leaves a
  visible message and does not terminate a running host. Completion and GUI
  exit release the lock.
- Releases are staged in fresh directories, and old executables are not
  overwritten. Shortcut and registration backups precede activation. Caught
  activation failures restore the previous values; rollback failure is surfaced
  and retains release files for recovery. This is not a power-loss transaction.
- The registry owner is checked before adoption and rechecked under the lock.
  A newer installed version is not replaced by an older version. The normal
  host shortcut has no arguments; the settings shortcut uses `--configure`.
- Uninstallation validates its release location and marker. It deletes named
  application payloads only, uses no recursive state-folder removal, and retains
  the PC identity, settings, received files, and other release directories.
  An old release's uninstaller only changes active shortcuts/registration if
  the active registry location still points to that release.
- Shortcut deletion uses a read-only target/argument check without resolving or
  executing the target. Failed ownership lookup conservatively preserves the
  shortcut.
- The builder binds the supplied host to its version, binary digest, exact Rust
  input set and each input digest. It rejects the old console subsystem,
  non-AMD64 executable, DLL image, and unbundled non-system import DLLs.
- The compiled installer is independently extracted with 7-Zip. Payload presence,
  uniqueness and byte hashes are checked against the staged payload before the
  setup EXE is published. Its records explicitly mark native installation as
  unexecuted and release readiness as false.

## Verification boundary

This review observed source and consulted the NSIS documentation; it did not run
Setup.exe on Windows. The parent task owns compiler logs, extracted payload
verification, final artifact hashes and candidate custody. Physical Windows
checks must cover clean installation, first launch, update with preserved
identity, a running-host lock refusal, user-edited desktop shortcut preservation,
forced staging collision, activation failure/rollback, and old-release uninstall
without damage to the active newer release.

Reference semantics checked against primary documentation:

- [NSIS installation-directory variable](https://nsis.sourceforge.io/Reference/%24INSTDIR)
- [NSIS CopyFiles](https://nsis.sourceforge.io/Reference/CopyFiles)
- [NSIS WriteUninstaller](https://nsis.sourceforge.io/Reference/WriteUninstaller)
- [NSIS command-line and uninstaller behavior](https://nsis.sourceforge.io/Docs/Chapter3.html)

## Narrow minimum-Windows-build addendum

Reviewed the change committed as
`058918a8769c62f6d05413a411616a5deaa0b631`. The installer now checks
`AtLeastBuild 15063` immediately after its Windows 10 version check, before
installed application files, shortcuts, or registry entries are written.
Unsupported versions receive a visible explanation and exit status 2. The
builder's prerequisite text matches this gate. The host uses
`SetProcessDpiAwarenessContext` and per-monitor-v2 awareness; the API-specific
Microsoft documentation and the per-monitor-v2 documentation require Windows
10 version 1703 for this combination. No additional blocking source finding.

This addendum updates the two exact source hashes in the input table. The
previous reviewed NSIS hash was
`3e976f3b16dd56d7aa421e413bd80a4271029abade8d3ff5241ff22f3ae26352` and builder hash
was `f4303eeee13cbfd60022d75070380752af5dac271142923ec5748fa110903c0a`.
No host code changed during this review, and no Windows runtime test was run.

Primary references checked for this change:

- [SetProcessDpiAwarenessContext requirements](https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-setprocessdpiawarenesscontext)
- [DPI awareness contexts and per-monitor-v2 availability](https://learn.microsoft.com/en-us/windows/win32/hidpi/dpi-awareness-context)
