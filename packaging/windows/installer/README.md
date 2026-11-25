# Windows setup executable

This is the graphical, per-user installer for the OpenRemote **0.6.0 Windows
host**. Android and the coordinator are also version 0.6.0. The installer embeds the
verified host, a setup guide, the available third-party notice texts and a
package manifest. It does not download or embed FFmpeg or deploy a coordinator.

The generated file is `OpenRemote-0.6.0-Windows-x64-Setup.exe`. It is an unsigned
development installer. Compiling and inspecting it does not establish a
successful Windows install, upgrade, uninstall or remote desktop session.

## Build

First finish the Windows host build and update `agent-windows/BUILD_EVIDENCE.json`
with the actual `hostVersion`, `binary.sha256` and `sourceInputs.files`. The
source-input map uses paths relative to `agent-windows` and covers `Cargo.toml`,
`Cargo.lock`, every Rust file under `src` and `examples`, `build.rs` if one
exists, and `../config/service-profile.json` (embedded by the host). The builder rejects an incomplete or stale map.

Use Python 3.11+, NSIS 3.11 and **7-Zip 26.02 or later**. The current verification
used 7-Zip 26.06. 7-Zip 25.01 has a known parser failure for modern NSIS command
streams: it labels the image `BadCmd=13` and omits the generated uninstaller.
The complete extraction check must pass; do not remove that gate to accommodate
an old extractor.

```sh
python3 scripts/build-windows-installer.py \
  --host agent-windows/target/x86_64-pc-windows-gnullvm/release/openremote-agent.exe \
  --host-evidence agent-windows/BUILD_EVIDENCE.json \
  --makensis /absolute/path/to/makensis \
  --sevenzip /absolute/path/to/7zz \
  --output-dir artifacts/windows-installer-candidate
```

The output directory must be absent or empty. Existing candidates and their
evidence are never overwritten by a rerun. On Windows, use the corresponding
MSVC host output and the Windows executables for Python, MakeNSIS and 7-Zip.
Keep `NSISDIR`/`NSISCONFDIR` set when using a relocated NSIS installation.

Before compiling, the script verifies the source version and every recorded
source hash, the host artifact hash, AMD64 PE32+ architecture, GUI subsystem and
the absence of unbundled non-system DLL imports. A console-subsystem 0.5.0 host
is deliberately rejected. NSIS warnings are fatal.

After compiling, 7-Zip independently extracts the installer. All four payload
files must have exactly the staged bytes and exactly one generated native GUI
uninstaller must be present. Both setup and uninstaller PE headers are checked.
Only after those checks does the candidate move to its final output filename.

`installer-build-evidence.json` records the host/build/installer/source/tool
hashes, payload hashes, notice sources, compiler/extraction logs and the native
execution limitation. `package-manifest.json`, the two logs and the actual
installer are also written to the output directory.

## Installed behavior

- Windows 10 version 1703 (build 15063) or newer, or Windows 11, on Intel/AMD x64
  is required. Earlier Windows 10 builds are rejected before host launch because
  the host directly imports `SetProcessDpiAwarenessContext` and uses per-monitor
  DPI awareness v2. The NSIS bootstrap UI is a native
  x86 executable; the embedded host is AMD64. Windows on ARM and 32-bit Windows
  are rejected by the installer.
- Files are staged in a fresh version directory under
  `%LOCALAPPDATA%\Programs\OpenRemote\releases` before activation. No previous
  host executable is overwritten. Previous release directories are retained.
- The installer holds the same exclusive `host-instance.lock` as the host.
  A running host or another active installation produces a visible error.
  It never kills an existing process.
- Start menu entries launch the host normally or with `--configure`. A desktop
  shortcut is optional. An edited/foreign desktop shortcut is not silently
  overwritten. Positively owned desktop shortcuts are removed if that option is
  cleared on upgrade. Shortcut deletion checks the saved target and arguments.
- Caught activation failures restore saved shortcut bytes and uninstall values.
  If rollback itself fails, the installer reports it and retains both release
  payloads. Power-loss recovery is a native acceptance test, not a claimed
  transaction across the Windows registry and filesystems.
- Apps/Installed apps receives a per-user uninstall entry. Uninstall removes
  only its exact owned payload filenames. It does not use recursive deletion.
  An older release's uninstaller cannot remove a newer active registration.
- `%LOCALAPPDATA%\OpenRemote` is kept: settings, identity, enrollment, logs,
  received files and the lock file. Server enrollment is not automatically
  revoked. Unknown files and earlier release directories are retained.
- No Windows service, sign-in startup task, firewall rule or hidden background
  host is installed. Graphical installation is required; `/S` is not supported.

## Verification and required Windows checks

Run the admission/custody tests with:

```sh
python3 -m unittest discover -s packaging/windows/installer/tests -v
```

They exercise stale/missing source pins, wrong binary hashes, console or DLL
hosts, wrong architecture, malformed images, an unbundled runtime DLL and
modified/duplicated extraction results. They do not emulate Win32 operations.

On a disposable Windows account, validate a first install with an empty state
directory, cancel and restart setup, install while the host is open, upgrade an
existing enrollment, deselect an owned desktop shortcut, preserve a user-edited
shortcut, inject file/registry activation failures, and uninstall while keeping
identity and received files. Test ordinary and high-DPI desktop layouts and
non-ASCII Windows user profile paths. Confirm the first-run host window stays
visible when settings, FFmpeg or server access are missing. These checks require
actual Windows execution and remain separate from packaging evidence.

## Primary references

- [NSIS scripting reference](https://nsis.sourceforge.io/Docs/Chapter4.html)
- [NSIS Modern UI 2](https://nsis.sourceforge.io/Docs/Modern%20UI%202/Readme.html)
- [7-Zip issue 225: NSIS extraction](https://github.com/ip7z/7zip/issues/225)
- [Microsoft IShellLinkW::GetPath](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ishelllinkw-getpath)
- [Microsoft SetProcessDpiAwarenessContext requirements](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setprocessdpiawarenesscontext)
- [Microsoft DPI awareness contexts](https://learn.microsoft.com/en-us/windows/win32/hidpi/dpi-awareness-context)

`NSIS-3.11-copyright.txt` is copied unchanged from the exact Debian NSIS 3.11-1
package used for the build. The available Rust notices and current project
notice inventory are embedded unchanged; this package does not invent missing
license grants or declare the public-redistribution review complete.
