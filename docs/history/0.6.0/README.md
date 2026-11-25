# OpenRemote — connect with a code and PIN

**0.6.0 development preview. Public/commercial release remains blocked.**

OpenRemote has a Windows host, a native Android controller, a receiving-PC
browser viewer, and an HTTPS coordinator. The primary flow is **Generate code
on the host → enter six digits and the chosen PIN → connect**. This update
preserves the visible Windows startup repair and executable installer.

Start with [the installation and connection guide](docs/windows-installer-0.6.0.md).
For exact checks and remaining limits, read [BUILD_STATUS.md](BUILD_STATUS.md).
The user's required commercial quality and ease of use are defined in
[the release standard](docs/commercial-release-standard.md) and [the product plan](PRODUCT_PLAN.md).

## What this update adds

- The Windows host accepts a chosen 6–12 digit PIN and produces a six-digit code
  with a ten-minute countdown, Copy and Revoke controls.
- Codes are unique among active codes on one coordinator, expire, lock after
  five wrong PINs, and open one session. They are not permanent PC numbers.
- The coordinator verifies a salted PIN, applies request/worker limits, and
  creates a pending session. The host independently matches and consumes its
  in-memory owner authorization before accepting it.
- Android makes code/PIN entry the primary flow. Advanced retains saved PCs,
  the older nine-digit lookup and explicit setup-link import.
- A browser viewer at the coordinator's `/connect` page lets another PC connect.
  The Windows home includes **Connect to another PC** to open that viewer.
- Windows and Android read one public service profile. Existing saved/enrolled
  origins take precedence. No enrollment key, PIN or relay secret is embedded.

**The supplied profile is unconfigured.** No public coordinator or TURN relay
was deployed for this project. A real deployment, first host enrollment and a
separately trusted FFmpeg executable remain prerequisites. This source supports
preconfigured builds; it does not invent or provide a running service.

## Install and connect

1. The deployment owner upgrades the coordinator to 0.6.0 and configures the
   real HTTPS and TURN services. See [deployment.md](docs/deployment.md).
2. Install `OpenRemote-0.6.0-Windows-x64-Setup.exe` on the host PC. Open
   **OpenRemote Host**. Existing valid settings are reused; a fresh installation
   shows the visible setup form.
3. On the host, enter and confirm a PIN, then choose **Generate code**.
4. On Android, open **OpenRemote Preview**, enter the code and PIN, and connect.
   An unconfigured preview needs its service origin or setup link saved once
   under Advanced. A configured build supplies that origin automatically.
5. On another PC, open the same service's `/connect` page, or use the Windows
   host's **Connect to another PC** button, then enter the code and PIN.
6. Keep the host running. Use Disconnect, the host session Stop control, or
   Stop host to end access.

The grant allows viewing and optionally keyboard/mouse input according to host
settings. It does not grant clipboard, files, audio, power or permanent unattended
trust. The previous explicit approval flow remains available for separately
configured capabilities. Android monitor switching requests fresh host approval;
a consumed code does not authorize a second session.

The ARM64 APK is a development preview for Android 8 or later. Its separate
`com.openremote.mobile.dev` package preserves an installed older preview whose
original debug signing key is unavailable. It is not an in-place update or a
production-signed store package. The release package remains `com.openremote.mobile`
and requires the owner's production signing identity.

## Implementation and evidence

| Component | Implemented | Evidence boundary |
| --- | --- | --- |
| Coordinator | Protected enrollment, P-256 proof, durable PC identity, presence, pending approval, permissions, signaling, TURN credentials, audit/revocation, ephemeral code/PIN | Go unit/race tests and real HTTP/HTTPS process tests; these do not transmit desktop pixels. |
| Windows host | DPAPI, visible setup/home/consent/stop, local one-use pairing, WebRTC H.264, display-bound capture/input | Rust tests, cross-link and PE inspection; real Win32/DPAPI/capture/input remains unverified here. |
| Android | Keystore proof, code/PIN, saved PCs, WebRTC renderer, touch/trackpad/keyboard, cancellation and recovery | JVM tests, lint and package builds; no physical phone session is claimed. |
| Browser viewer | Code/PIN, WebCrypto proof, WebRTC, primary-display/fresh-frame input gates, mouse/keyboard/text, full screen, disconnect | Protocol, actual-app VM lifecycle and real coordinator HTTP tests; Chromium could not launch under this workspace's socket policy. |
| Session tools | Explicit text clipboard and bounded phone-to-PC upload with separate grants | Existing unit/native source evidence; physical cross-device tools remain open. Code sessions leave these off. |
| Installer | Versioned per-user GUI installer, shortcuts, host lock, identity preservation and uninstaller | Admission tests, NSIS compilation and independent extraction; no Windows installation run. |
| Shared profile | Public service origin embedded into both clients with saved-origin precedence | Validation tests and exact input hashes. The delivered profile contains no live origin. |

## Deployment and preconfigured clients

A six-digit code cannot tell a client which arbitrary private service to contact.
Both clients must use the same deployed coordinator. The browser viewer uses its
own origin. After provisioning and validating the owner's service, set the input:

```sh
python3 scripts/configure_service_profile.py --api-origin https://YOUR-REAL-HOST
```

Rebuild both clients afterwards. Installer verification includes the profile's
exact bytes. The command only configures source inputs; it does not deploy,
enroll a PC or purchase infrastructure. There is no default public relay or
hidden use of another product's service. See the deployment guide for HTTPS,
TURN UDP/TCP/TLS and the separate relay IP/host needed for TLS on port 443.

## Verification

Core commands on a suitable authorized runner:

```sh
(cd server && go test -race -cover ./... && go vet ./...)
python3 -m unittest discover -s scripts/tests -v
python3 -m unittest discover -s packaging/windows/installer/tests -v
cargo test --locked --all-targets --manifest-path agent-windows/Cargo.toml
node --test tests/browser/controller-core.test.mjs
node --experimental-vm-modules --test tests/browser/controller-lifecycle.test.mjs
go build -C server -o artifacts/code-pin-pairing-fixture ./testsupport/pairing_fixture
node tests/browser/controller-api.integration.mjs
(cd android && ./gradlew --dependency-verification strict testDebugUnitTest lintDebug assembleDebug assembleRelease)
```

The lifecycle VM tests execute the actual app with explicit DOM/RTC doubles;
they do not render a browser or decode media. Separate browser/native media
fixtures require moving decoded video, exact input receipts and actual selected
relay candidates. Environment denials remain blocked, never passed. Physical
Windows/Android, real Internet/TURN, install/update and security gates still apply.

## Commercial release work

Video currently uses software H.264 up to 1280×720 at 30 fps. The host needs an
already logged-in desktop. Hardware/adaptive high-resolution video, audio,
bidirectional/resumable files, seamless monitor switching, service/logon/UAC/
reboot handling, production signing, signed updates, complete turnkey media
packaging, iOS and a native desktop receiving client remain open. The current
receiving-PC screen is explicitly a browser viewer.

The expanded product standard also requires accessible first use, trusted-device
management, measured performance, support diagnostics, complete distribution
provenance and operational recovery. These are release requirements, not implied
by compiling a preview. See [release gates](docs/release-gates.md),
[the commercial standard](docs/commercial-release-standard.md), and
[the code/PIN contract](docs/code-pin-contract.md).

The branch is `feat/code-pin-connect`; all foundation and repair history is
retained. A destination repository and actual OpenRemote deployment target are
still needed for publication/deployment. No unrelated repository or cloud project
has been used.
