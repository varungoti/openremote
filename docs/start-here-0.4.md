# OpenRemote 0.4.0 — start here

18 November 2026. This is a **development/test build** for Windows x64 hosts and
an Android phone. It includes real native client code and compiled binaries, but
physical PC-to-phone operation and public Internet relay testing are still open.
It is not a signed production release.

## What this continuation adds

The phone can list a PC's monitors and choose **Switch and reconnect**. The host
pins capture and input to the chosen display; the new session waits for the host's
matching display identity and fresh rendered video. Layout changes end access
and require a refreshed connection. Cleanup, renderer-rotation and held-input
defects found in review were corrected. The full scope and remaining advanced
features are in `PRODUCT_PLAN.md`.

Existing features include saved PC numbers, verified phone identity, local
approval, explicit trusted unattended access, native video, touch/trackpad,
keyboard/text, text clipboard actions and phone-to-PC uploads.

## Files

| File | Purpose |
| --- | --- |
| `OpenRemote-0.4.0-Source.bundle` | Complete Git source history, implementation branch, installers, deployment files, tests and documentation |
| `OpenRemote-0.4.0-Android-ARM64-Test.apk` | Installable development-signed ARM64 Android test app |
| `OpenRemote-0.4.0-Windows-x64-Test.exe` | Unsigned Windows host; needs configured coordinator and a verified external FFmpeg |
| `OpenRemote-0.4.0-Coordinator.linux-amd64` | Linux coordinator executable; HTTPS/relay configuration is separate |
| `OpenRemote-0.4.0-Android-ARM64-Unsigned.apk` | Unsigned release variant for owner signing, not an installable signed release |
| `OpenRemote-0.4.0-Product-Plan.md` | Expanded requirements, priorities and acceptance criteria |
| `OpenRemote-0.4.0-Build-Status.md` | Exact successful checks, artifact hashes and remaining release gates |
| `OpenRemote-0.4.0-Pilot-Validation.md` | Procedure for the 17 physical-device/Internet cases |
| `OpenRemote-0.4.0-Pilot-Not-Run.json` | Candidate-bound initial record; all 17 cases are unexecuted |
| `OpenRemote-0.4.0-Verification.json` | Consolidated machine-readable build/source evidence |
| `OpenRemote-0.4.0-Checksums.sha256` | Download file integrity list |

The test APK has versionCode 4 and the same development signing certificate as
the 0.3 test APK. A physical upgrade still needs validation. Production signing
keys are not included.

## Recover the complete source

Install Git on the development computer, place the bundle in your working
folder, and run:

```sh
git clone --branch work/release-hardening OpenRemote-0.4.0-Source.bundle OpenRemote
cd OpenRemote
```

The original foundation is preserved as `main`. The reviewed 0.4 implementation
is captured in commit `53dccfc7ddcd8460bfbed43d9a363a08c58021ae`; later evidence-only
commits in the bundle record candidate custody and pilot status. The native
build-input hashes match the captured implementation. This is an unsigned
development handoff, not an independent reproducibility attestation.

The bundle does not embed the SDK/toolchain caches or generated native binaries.
The separate downloads above supply the principal compiled files. Platform build
instructions and pinned dependencies are in `server/README.md`,
`agent-windows/README.md` and `android/README.md`.

## Configure Internet connectivity once

OpenRemote needs three running parts: the Windows host on each PC, the Android
controller, and an owner-controlled HTTPS coordinator with TURN relay fallback.
No public coordinator or relay was deployed as part of this build. The apps cannot
connect merely by installing the two binaries without that deployment.

Use `docs/deployment.md` in the source for the complete Caddy/coordinator/coturn
setup. On the chosen Linux server, prepare a private configuration using your own
real domain names and public relay address:

```sh
python3 scripts/configure_deployment.py \
  --api-domain remote.your-domain.com \
  --turn-domain turn.your-domain.com \
  --turn-public-ip YOUR_PUBLIC_IPV4 \
  --email YOUR_CERTIFICATE_EMAIL
```

This generates private local configuration and refuses to overwrite it. It does
not provision a server, publish DNS or deploy containers. Follow the deployment
guide for certificates, exposed ports, persistent state and relay configuration.
TURN/TLS on port 443 needs a separate relay IP/host from the API's HTTPS listener.
Keep the enrollment secret on your PCs/server; it is not entered into the phone.

For a 0.3 upgrade, update the coordinator first, followed by the Windows host and
Android app. A 0.3 coordinator rejects selected-monitor requests. Ordinary
primary-monitor connections remain compatible with older hosts, but their UI
does not offer monitor selection.

## Install on each Windows PC

Use an owned Windows x64 PC with a logged-in user desktop. Obtain a standalone
Windows FFmpeg build containing `gdigrab`, `libx264` and `lavfi`, and verify its
publisher/build provenance and expected SHA-256. FFmpeg is not bundled here.

In the cloned source folder, copy the downloaded host into
`artifacts/windows/openremote-agent.exe`. From PowerShell run:

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

`-TestBuild` deliberately identifies the unsigned development executable.
The installer prompts locally for first enrollment with concealed input. Each PC
gets a permanent nine-digit number. Use meaningful names such as Office and Home.
Keep the visible host running; service startup, lock-screen/UAC access and reboot
recovery are not implemented.

Clipboard, file transfer and unattended access are off by default. To enable
clipboard/uploads explicitly, add `-AllowClipboard $true -AllowFileTransfer $true`
to the installation command. The phone must request them too, and the host can
narrow the grant. Unattended trust and revocation are documented in the Windows
installation guide; a fingerprint alone cannot replace the full phone public key.

## Connect from the phone

1. Install `OpenRemote-0.4.0-Android-ARM64-Test.apk` on the ARM64 Android test phone.
2. In app Settings enter the HTTPS coordinator origin used by the PCs.
3. Enter a PC's nine-digit number and choose the requested permissions.
4. Compare the phone fingerprint with the PC approval window and approve locally.
5. Wait for the desktop to render. Use touchpad/direct-touch and keyboard controls.
6. Open **Displays** to select another monitor. **Switch and reconnect** ends this
   session and requests a fresh one; the PC may ask for approval again.
7. Use **Clipboard & files** only for explicitly granted tools. Uploads are limited
   to 64 MiB per file and a 256 MiB session reservation budget. Completed files go
   to the host's `ReceivedFiles` folder and are never opened automatically.
8. Disconnect using either endpoint. A changed/unplugged display ends the session;
   reconnect to the primary display to refresh the catalog.

An unverified Windows teardown prevents further sessions until a local host
restart. The host reports that condition rather than silently reusing resources.
The current software video profile is up to 720p30. Exact mixed-DPI, portrait,
unplug and input behavior still requires the physical test matrix.

## Validate the actual devices and Internet path

The build checks passed: **58 Rust tests, 46 Android JVM tests, 19 Go test functions
plus subcases, nine HTTPS scenarios and 16 Python tests**. A fresh synthetic
encoder test produced 30 distinct decoded frames. These do not demonstrate a
Windows-to-phone session over the Internet.

Follow `docs/pilot-validation.md`. The bundled initial record names these exact
candidate binaries and begins with all 17 scenarios marked `not_run`. The validator
checks recorded files/observations and rejects an incomplete completion claim;
it cannot attest that someone performed a test.

For the supplied record, place binaries at these paths within the source checkout:

| Download | Path inside the checkout |
| --- | --- |
| Windows test executable | `artifacts/windows/openremote-agent.exe` |
| Android test APK | `artifacts/android/OpenRemote-0.4.0-arm64-test.apk` |
| Linux coordinator | `artifacts/controlplane-linux-amd64` |

Then validate the initial record without claiming completion:

```sh
python3 scripts/validate_pilot_evidence.py validate evidence/pilot-0.4-not-run.json \
  --root . --expected-revision 53dccfc7ddcd8460bfbed43d9a363a08c58021ae
```

Adding `--require-pass` must fail until all required observations and supporting
files are present. Test two Windows PCs and the phone on different networks,
including actual selected direct and TURN UDP/TCP/TLS443 paths. Use harmless test
documents and record the exact external FFmpeg hash and device/OS versions.

Production signing, full native provenance, actual devices/relay validation,
installation/upgrade, operations and the remaining advanced product features
still need completion before a release-ready claim is justified.
