# OpenRemote 0.5.0 — install and connect from Android

**Windows update, 19 November 2026:** use
[the 0.5.1 Windows installer](windows-installer-0.5.1.md) for the repaired
graphical first-run flow. The older standalone Windows host described below
exits before showing a window when setup is incomplete. The Android and server
instructions below still apply to version 0.5.0; the Windows PowerShell route is
retained as an advanced workflow.

18 November 2026. **This is an installable Android development candidate, not a
production release.** The APK is development-signed. The Windows host is unsigned.
The actual Windows-to-phone session and public relay tests have not yet run.

You can install the phone APK to use the new setup and connection-check screens.
Remote control also requires a running Windows host and your own deployed HTTPS
coordinator/relay. Installing the two client files alone does not provide that
Internet service. No public coordinator has been provisioned for this project.

## Start with these files

| File | Use |
| --- | --- |
| `OpenRemote-0.5.0-Android-ARM64-Test.apk` | Install on the ARM64 Android test phone (Android 8 or newer) |
| `OpenRemote-0.5.0-Windows-x64-Test.exe` | Host for each owned Windows x64 PC; install/configure using the script below |
| `OpenRemote-0.5.0-Coordinator.linux-amd64` | Coordinator for the chosen Linux server; configure valid TLS and relay separately |
| `OpenRemote-0.5.0-Source.bundle` | Complete Git source/history, Windows installers, deployment files, tests and runbooks |
| `OpenRemote-0.5.0-Build-Status.md` | Exact artifact hashes, actual passed checks and remaining release gates |
| `OpenRemote-0.5.0-Verification.json` | Machine-readable source, artifact and verification custody |
| `OpenRemote-0.5.0-Checksums.sha256` | Download integrity checks |

The separate `Android-ARM64-Unsigned.apk` is an owner-signing input. Install the
**Test.apk** on the phone; the unsigned release variant is not the installable
test app. Do not share production signing keys or enrollment secrets in chat.

## 1. Install the phone app

Transfer `OpenRemote-0.5.0-Android-ARM64-Test.apk` to the Android phone and open it.
If Android asks, allow installation from that specific file source for the test,
then turn that allowance off after installation. Launch **OpenRemote** from the
phone's app list. This build has versionCode 5 and preserves the previous test
signing identity; the real-device upgrade flow is still an acceptance test.

The phone cannot display a remote desktop until the remaining setup below is
complete. There is no demo desktop presented as a live connection.

## 2. Recover the source and deployment scripts

On the development/PC setup machine, install Git, place the source bundle in your
working folder and run:

```sh
git clone --branch work/release-hardening OpenRemote-0.5.0-Source.bundle OpenRemote
cd OpenRemote
```

The original foundation is retained as `main`; the completed development branch
and `v0.5.0-dev` tag are included. The final verification record identifies the
captured implementation and handoff revisions. Toolchain caches and FFmpeg are
not embedded in the bundle. Component READMEs contain reproducible build commands
and exact dependency pins; source custody is not a reproducibility attestation.

## 3. Configure the Internet service once

Use `docs/deployment.md` for the owner-controlled HTTPS coordinator and coturn
relay. This requires an authorized public server, DNS and valid certificates.
No provider or paid service is automatically chosen. Prepare configuration on
the selected server with your actual values:

```sh
python3 scripts/configure_deployment.py \
  --api-domain remote.your-domain.com \
  --turn-domain turn.your-domain.com \
  --turn-public-ip YOUR_PUBLIC_IPV4 \
  --email YOUR_CERTIFICATE_EMAIL
```

This writes private local configuration and refuses to replace an existing one;
it does not deploy containers or publish DNS. Follow the deployment guide to run
the services. For TURN/TLS on port 443, the relay needs a separate IP/host from the
API listener on 443. Keep the enrollment secret on the server and PCs, never in
the phone or setup link. The phone and every enrolled PC must use the same API
origin. A PC number has meaning within that coordinator.

For an existing deployment, upgrade the coordinator first, then the PC hosts and
phone. The new **Check connection** flow requires `/v1/info` from version 0.5 or
later. The coordinator rejects malformed/ambiguous ICE URL configuration on
startup; valid examples in the deployment guide still apply.

## 4. Install the host on each PC

Use Windows x64 with a logged-in desktop. Obtain a trusted standalone Windows
FFmpeg executable containing `gdigrab`, `libx264` and `lavfi`, and establish its
expected SHA-256. It is deliberately supplied separately; this candidate does
not bundle an unverified encoder build.

Copy the host download into `artifacts/windows/openremote-agent.exe` inside the
source checkout. From PowerShell in that folder run:

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

`-TestBuild` is required for the unsigned development host. The installer prompts
locally for first enrollment with concealed input. It preserves the PC identity
on upgrade. Repeat on each PC with a useful name such as Office, Home or Workshop.
Keep the host's visible window open. The host does not run as a Windows service
and cannot access a lock screen/UAC desktop or reconnect across reboot/logoff.

Clipboard and file upload permissions are off by default on the PC. Add
`-AllowClipboard $true -AllowFileTransfer $true` only if you want those tools.
The phone must request them and the PC must approve them too. Unattended trust
still requires explicit local configuration and the full phone public key.

## 5. Connect from the phone

1. In the PC's host window, note the nine-digit PC number and full server origin.
   Use **Copy setup link**, then transfer that address to your phone by your own
   chosen method. The link contains only version, server origin and PC number.
2. Open OpenRemote on Android and import/paste the link. Review the full origin
   and PC number. Confirm **Save PC setup**. A different origin clears the prior
   server's saved PC list, as the confirmation explains. Cancel leaves settings
   unchanged. The imported PC is saved across app restarts with a neutral PC-number
   label until its authenticated name is available. This step does not connect
   or grant access.
3. Tap **Check connection**. The app checks server compatibility, phone identity
   and PC availability. The relay row reports configuration only; it cannot
   establish whether video will traverse the Internet.
4. Find/select the PC, choose view-only or keyboard/mouse and optional tools,
   then request the connection. Compare the phone fingerprint with the PC
   approval window and approve locally.
5. Wait for the real desktop to render. Use touchpad/direct touch, keyboard/text,
   **Displays** for monitor selection and **Clipboard & files** for granted tools.
6. Stop using the phone's disconnect control, the PC's session disconnect control,
   emergency hotkey, or **Stop host**. Closing the persistent host home also stops
   remote access; it does not hide the host in the background.

You can still enter the HTTPS origin manually in Settings and type the PC number.
If Android does not open a custom-scheme link or offers another app, use explicit
paste in OpenRemote. Native intent handling, clipboard copy and phone layout
still require testing on the actual devices.

## If a check fails

| Stage | Action |
| --- | --- |
| Server name or connection | Check DNS, phone Internet, final HTTPS origin, port and whether the coordinator is running |
| HTTPS certificate | Check phone date and the server's certificate/hostname; TLS verification stays enabled |
| Unsupported server information | Confirm the address; upgrade the coordinator to 0.5 or later |
| Phone identity | Check server policy and phone key availability; a setup link cannot override revoked access |
| Unknown PC | Check all nine digits and ensure PC/phone use the same coordinator |
| Offline PC | Wake the PC, sign in and start its host; check its outbound connection to the coordinator |
| No relay configured | Configure coturn and the coordinator's TURN URLs/secret; separate networks may need a relay |
| Checks pass, desktop fails | Record the actual failed session stage and run the physical/media pilot; discovery is not video proof |

## Release readiness

Automated checks and native builds are recorded in the build-status file. The
prepared pilot record starts with all 17 physical/Internet cases `not_run` and
must fail `--require-pass` until observations and exact candidate files exist.
The additional phone setup cases in `docs/phone-setup.md` are mandatory, including
Windows UI/copy/Stop and Android cold/warm starts, rotation, SAF picker overlap,
origin changes and diagnostic failures.

Production release also needs real Windows capture/input and Android rendering,
direct Internet and TURN UDP/TCP/TLS443 tests, installation/upgrade, publisher
signatures, complete native dependency provenance, a security review and tested
operations. Hardware/adaptive video, PC audio, bidirectional/resumable files,
service/reboot support and the other advanced features remain in PRODUCT_PLAN.md.
They are not represented as available in this candidate.
