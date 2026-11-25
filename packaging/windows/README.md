# Windows installation and upgrade

For the normal graphical setup route, use
**OpenRemote-0.6.0-Windows-x64-Setup.exe** and
[the installer guide](../../docs/windows-installer-0.6.0.md).
Executable installer sources and its reproducible packaging command are in
`installer/`. The sections below describe the existing advanced PowerShell route.

The 0.6.0 host is a development/test build. The included executable is unsigned;
Windows desktop behavior and native installation are still release gates. No
privileged service or automatic updater is installed.

## Prerequisites

- An owned Intel/AMD x64 PC running Windows 10 version 1703 (build 15063) or newer,
  or Windows 11, and a running HTTPS OpenRemote coordinator/relay deployment.
- A standalone Windows FFmpeg executable including `gdigrab`, `libx264` and the
  `lavfi` synthetic source used by installation preflight. A shared build requiring
  external codec DLLs is not supported by this installer.
- The expected FFmpeg SHA-256 from its trusted distribution or your own verified
  build. Do not establish trust by merely hashing an unknown downloaded file.
- The private coordinator enrollment token for first enrollment, entered locally.

FFmpeg is not bundled. The exact build's notices/source materials must be reviewed
before it is redistributed in a product installer. The host verifies its configured
hash each time it starts capture.

## Install a development build

From a PowerShell terminal in the checked-out repository, substitute your values:

```powershell
.\packaging\windows\Install-OpenRemote.ps1 `
  -AgentPath .\artifacts\windows\openremote-agent.exe `
  -FfmpegPath 'C:\YourVerifiedTools\ffmpeg.exe' `
  -ExpectedFfmpegSha256 'YOUR_VERIFIED_64_HEX_CHARACTER_HASH' `
  -ApiUrl 'https://remote.your-domain.com' `
  -DeviceName 'Office PC' `
  -TestBuild `
  -StartAfterInstall
```

`-StartAfterInstall` prompts for the enrollment token with concealed input and
restores any prior process environment value afterward. Subsequent host launches
use DPAPI-protected enrollment, so the token is not a permanent shortcut argument.
You may launch **OpenRemote Host** from the Start menu after installation.

The default permissions allow screen view and input subject to consent; clipboard,
file uploads and unattended access are off. To enable the tools deliberately on
this PC, add these arguments or use them during a later installation:

```powershell
-AllowClipboard $true -AllowFileTransfer $true
```

Explicit `$false` disables the corresponding local tool permission. Omitting these
arguments preserves existing settings. The phone must also request the capability
for each session, and the PC's approval displays the actual grant.

## Configuration and trust

Configuration and protected identity are under `%LOCALAPPDATA%\OpenRemote`.
The Windows user must keep the host running in the logged-in desktop. The current
host controls one selected monitor and respects Windows input-integrity boundaries.
It does not control the lock screen or UAC secure desktop.

To configure unattended access, copy the phone's full Base64 public key from its
Settings, add it to `trusted_controller_keys`, and enable `allow_unattended` locally.
The displayed fingerprint is for comparison and cannot replace the full key.
Restart the host after changing local configuration. A coordinator revocation
still blocks an old trusted key until explicit re-trust through its host API.
Server/name/encoder/enrollment setup has a graphical form in 0.6.0. The broader
permission/trusted-phone management GUI remains a planned feature.

Uploads finish in `%LOCALAPPDATA%\OpenRemote\ReceivedFiles`. Filenames have a
transfer-ID prefix to avoid overwrite. Windows Internet-zone metadata is applied;
files are not opened automatically. See `docs/session-tools.md` for exact limits.

## Upgrade behavior

Disconnect and quit the host first. The installer verifies the existing API
origin, effective name, supplied executable hash and host signature policy before
activation. It stages both executables, runs a relocated FFmpeg capability/encode
probe, and uses a hash-specific version directory under
`%LOCALAPPDATA%\Programs\OpenRemote\releases`.

Configuration and the Start-menu shortcut switch only after staging succeeds.
An exclusive installation lock and a check for intervening local edits prevent
concurrent preparations from silently replacing changed configuration. Caught
activation failures restore the previous configuration and shortcut. Existing
identity files are not replaced, and previous binary versions remain available.

The four actual file-activation/rollback tests pass under Linux PowerShell 7.7.
That does not validate Windows COM, Authenticode, DLL loading, DPAPI or Windows
filesystem behavior. Abrupt power loss between the two activation switches and
complete repair/rollback remain physical Windows release tests.

## Uninstall

```powershell
.\packaging\windows\Uninstall-OpenRemote.ps1
```

This removes installed program versions and the Start-menu entry, preserving
local identity/configuration and received files. The coordinator enrollment is not
automatically revoked. The explicit `-RemoveLocalIdentity` option deletes host credentials and configuration
while preserving received files. Deleting received files requires the separate
`-RemoveReceivedFiles` option.

## Build and signing

`./scripts/build-windows.ps1` runs formatting, Rust tests, Clippy and a Windows
release build. It records Git revision, dirty status, an exact source-content
manifest, target, binary digest and signature result. A signed build requires a
clean committed tree and a configured signing identity. Output is staged and
published as a complete binary/manifest/source set after signing verification.
No key, release certificate or public distribution identity is included.
