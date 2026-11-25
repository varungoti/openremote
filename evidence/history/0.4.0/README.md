# OpenRemote — Windows host and Android controller

**Version 0.4.0 is a development build. Public release is blocked.** The repository
contains compiled native clients, a tested HTTPS coordinator, and explicit tests
for the media connection. It has not yet passed a real Windows-to-Android remote
desktop session or the Internet/NAT matrix. See [BUILD_STATUS.md](BUILD_STATUS.md).

The expanded scope and milestone acceptance criteria are in [PRODUCT_PLAN.md](PRODUCT_PLAN.md).

The product goal remains a phone app that connects to multiple enrolled PCs by
nine-digit PC number. Each PC must approve a phone locally or explicitly trust
its verified identity for unattended access. A PC number is an address, never a
password.

## Current implementation

| Component | Implemented behavior | Verification boundary |
| --- | --- | --- |
| Go coordinator | Persistent PC IDs, cryptographic phone login, approval, permission narrowing, presence, signaling, expiring TURN credentials, audit, persistent revocation | Unit/race tests and independent HTTPS process/restart integration passed |
| Windows host | Current-user DPAPI enrollment, native approval and disconnect window, real WebRTC peer, H.264 stream, selected-monitor capture and bound mouse/keyboard/Unicode input | Cross-compiled Windows executable; native OS behavior still needs Windows testing |
| Android | Server settings, saved PCs, Keystore identity, signed login, native WebRTC renderer, touch/trackpad and keyboard, network recovery | APK compilation/JVM tests; real device Keystore/video/input tests still required |
| Session tools | Explicit text clipboard actions and phone-to-PC file uploads; separate permissions, bounded chunks, SHA-256, progress/cancel and a scoped PC receive folder | Native source implemented; unit/build evidence and real-device boundaries in BUILD_STATUS.md |
| Display selection | Bounded monitor catalog, opaque IDs, fresh-session switch, negative-origin mapping, observed topology-change shutdown and first-render input gate | Contract/unit tests and builds; Windows/Android multi-monitor matrix remains open |
| Encoder path | FFmpeg capture process, bounded H.264 access-unit assembler, RTP samples | Synthetic encoding/assembly/decoding produced 30 distinct moving frames |
| Relay and hosting | Caddy HTTPS, private coordinator storage, coturn configuration, temporary credentials | Configuration and credential tests; no public deployment or real relay run |

The current video profile is software-encoded H.264 at up to 1280×720 / 30 FPS.
Optional FFmpeg DXGI capture is restricted to a single monitor. This release does
not yet implement audio, automatic clipboard synchronization, PC-to-phone or resumable
file transfer, in-place monitor switching, hardware encoder selection, adaptive 4K video, a Windows service,
lock-screen/UAC access, reboot recovery, signed updates, or iOS.

## Repository

- `server/`: dependency-free Go control plane; [API/configuration](server/README.md).
- `agent-windows/`: Rust host; [host setup/build guide](agent-windows/README.md).
- `android/`: native Kotlin/Compose controller; [Android build guide](android/README.md).
- `packaging/windows/`: current-user install/uninstall scripts with staged activation and tested file rollback logic.
- `docs/session-tools.md`: shared clipboard/upload wire contract and limits.
- `docs/display-selection.md`: selected-display session and catalog contract.
- `docs/pilot-validation.md`: 17 physical-device/Internet scenarios with candidate/evidence validation.
- `scripts/integration_test.py`: isolated HTTPS security and restart test.
- `tests/native_media/`: independent Go/Pion controller that must decode actual RTP video.
- `tests/browser/`: Chromium decoder/DataChannel verification.
- `infra/`: reverse proxy and coturn deployment configuration.
- `evidence/`: retained verification results, including blocked tests.
- `.github/workflows/verify.yml`: manually triggered verification on labeled owner-controlled runners; no deployment or release publishing.

## Deploy the private coordinator and relay

Follow [docs/deployment.md](docs/deployment.md). This needs a public server, DNS and
valid TLS certificates. No cloud account or server was provisioned by this build.

```sh
python3 scripts/configure_deployment.py \
  --api-domain remote.your-domain.com \
  --turn-domain turn.your-domain.com \
  --turn-public-ip YOUR_PUBLIC_IPV4 \
  --email YOUR_CERTIFICATE_EMAIL
```

The command writes private secrets locally and refuses to overwrite an existing
configuration. It does not deploy anything. Then start the coordinator through
Caddy and the relay on the chosen hosts after providing its TLS key/certificate.
For TURN/TLS on port 443, use a separate relay IP/host from the HTTPS API gateway.

## Install and connect

1. Configure and start the coordinator and relay.
2. On each Windows PC, install the host and a verified FFmpeg build, then enroll it
   using the private deployment enrollment token. The host displays its PC ID.
3. Install the Android test APK matching the phone architecture. The ARM64 build is
   for a 64-bit Android phone. A debug APK is for testing, not a signed store release.
4. In phone Settings enter the same HTTPS server origin. Enter the PC ID and request
   view-only or keyboard/mouse control.
5. Compare the phone identity fingerprint with the PC approval window and approve.
6. To use clipboard or file uploads, enable the corresponding local PC settings
   and request those permissions on the phone before connecting. Use **Clipboard
   & files** in the session. Text is limited to 4 KiB; uploads are limited to 64 MiB
   per file and 256 MiB per session, including cancelled reservations. Completed
   files go to `%LOCALAPPDATA%\OpenRemote\ReceivedFiles` and are never opened automatically.
7. For unattended access, copy the phone's public key from Android Settings to the
   PC's `trusted_controller_keys` and enable the local unattended flag. Keep the
   host running in the logged-in desktop session.
8. Open **Displays** during a session to select another monitor. **Switch and
   reconnect** ends the current session and requests the chosen display; approve
   again on the PC when required. If a monitor is unplugged or the layout changes,
   reconnect to the primary display to obtain a fresh list.

Upgrade the coordinator first, then the Windows host and Android app. A 0.3
coordinator rejects the new optional monitor selector. Primary-monitor sessions
remain compatible with an older host; explicitly selected sessions require its
display binding and never fall back silently.

See [the installation guide](packaging/windows/README.md) for exact commands.
The unsigned Windows development executable requires explicit `-TestBuild` when
using the installation script. Production installation requires Authenticode.
The scripts preserve existing identity on upgrade and do not install a hidden or
privileged service.

## Verify locally

Use the component build commands in their READMEs. The core checks are:

```sh
(cd server && go test -race -cover ./... && go vet ./...)
python3 -m unittest discover -s scripts/tests -v
python3 scripts/integration_test.py --binary artifacts/controlplane-linux-amd64
cargo test --locked --all-targets --manifest-path agent-windows/Cargo.toml
(cd android && ./gradlew testDebugUnitTest lintDebug assembleDebug assembleRelease)
pwsh -File packaging/windows/tests/Activation.Tests.ps1
```

The browser and native media suites are deliberately stricter than an SDP test:
they require decoded moving video, exact input delivery and actual selected relay
candidates. Their execution was blocked by this workspace's IPC/network-discovery
policy before any media could flow. They must run on a suitable authorized runner;
those results are not represented as passes.

[Release gates](docs/release-gates.md) list what remains before release approval.
The source is kept on `work/release-hardening`; a destination GitHub repository is
still required to publish a PR. No public deployment has been made.
