# OpenRemote Windows host — 0.7.0 development candidate

The Windows application now has one everyday Connect/Stop flow. It opens a
persistent owner window; setup no longer asks the person using the PC for a
server address, enrollment key, encoder path, or hash. This source and its
cross-compiled binary are a development candidate, pending native Windows and
real-device Internet testing and signing. The exact build status is recorded in
[BUILD_EVIDENCE.json](BUILD_EVIDENCE.json).

## Everyday use

1. Install the provider-configured Windows installer and open **OpenRemote**.
2. Click **Connect remote desktop**. Choose a **six-digit security PIN**, including
   leading zeroes if desired, and press Enter. There is no confirmation field.
3. The app checks its bundled encoder, connects to its configured service,
   registers this PC automatically when necessary, and displays a six-digit
   **Server ID**. Enter that Server ID and PIN on the configured phone app or
   receiving-PC browser viewer.
4. Keep the window open while sharing. The primary button becomes **Stop remote
   desktop** during preparation and hosting. Stop, closing the window, or
   **Ctrl+Alt+F12** cancels access and releases remote-held inputs.
5. A Server ID authorizes one session and expires after the service's bounded
   lifetime (currently ten minutes). The countdown is visible before connection.
   After expiry or session end, click Connect again for a fresh Server ID and PIN.

The same primary button can submit the PIN for users who prefer clicking to
pressing Enter. The old `--configure` shortcut opens this same window. There is
no separate technical setup screen, settings shortcut, or trust prompt in the
managed flow. Existing locally configured trusted controllers keep their exact
local permission policy, but the normal interface does not add new trust.
The Server ID is temporary addressing information; it is not a permanent device
identifier and is not sufficient without the PIN.

This repository's public `config/service-profile.json` is intentionally empty
until an actual OpenRemote deployment is supplied. Such a build remains visible
and says:

> This app build is not connected to the OpenRemote service. Contact the app provider for a configured installer.

There is no user-facing field to work around an unconfigured distribution. A
working phone-to-PC deployment requires the same real HTTPS service configured
into the host and mobile distributions, with its backend and TURN relay running.
This source change does not deploy that service or demonstrate Internet reachability.

## Automatic registration and preserved identity

The provider embeds only a public HTTPS origin. Neither the installer nor the
application contains a global enrollment secret. The PC creates its Ed25519
identity once and protects it with current-user Windows DPAPI. Its bearer token
is also DPAPI-protected and bound to that key and exact service origin. The app
never falls back to plaintext credential storage. Existing origin bindings and
locally configured permission/trust settings remain authoritative.

For a fresh registration, the host obtains a short-lived challenge, verifies its
purpose, service origin, own key, exact PC name, platform, challenge identifier,
nonce and expiry, then signs the exact bounded UTF-8 payload using pure Ed25519.
The service consumes the challenge once and returns the PC's own bearer token.
See [the automatic registration protocol](../server/AUTOMATIC_REGISTRATION_PROTOCOL.md).

A still-valid saved token works even if automatic registration has since been
disabled. A saved-token 401 during connection preparation attempts recovery using
a fresh challenge and the same preserved signing identity. The service permits
that only for its own non-revoked, automatically registered device record; it
rotates the bearer and ends earlier sessions and codes. Legacy or revoked records
cannot use this recovery path. No active-session recovery, key reset, origin
migration, or administrator enrollment fallback is performed.

Network and preparation failures stay visible in the same window. Retry requires
an explicit Connect action. A canceled or uncertain registration response may be
recovered by a fresh same-key proof on the next attempt; it never causes the host
to invent a second identity. Raw backend responses, tokens, PINs, and command
arguments are not written to the fixed-code startup diagnostic log.

## Local authorization and Stop behavior

A successful code response is installed into an in-memory, expiring local grant
before the Server ID can be displayed. Only an exact, pending pairing ID with a
subset of the approved view/input permissions can consume it, once. Unknown,
expired, revoked, superseded or already-consumed codes fail closed. A process
restart loses that local grant. Historical events do not reserve the active
session slot or consume a newly generated grant.

The PIN is kept in memory only and sent in the HTTPS creation request. It is
never put in settings, logs, command arguments, links or clipboard. The masked
field is cleared when the request starts or is canceled. Clipboard, file
transfer, audio and power are false for the simple pairing grant; permanent
unattended trust is never created by knowing a code and PIN.

Stop and window close invalidate local grants synchronously, cancel preparation
or the current session, and make a bounded request to revoke the server code.
Late asynchronous results cannot redisplay or reinstall a canceled grant. The
window waits for bounded cleanup before offering Connect again; unverified
encoder, input or peer teardown latches a restart-required state. The persistent
owner window replaces the old separate session-stop dialog only for this entry
point. Internal callers without an owner window retain the separate notice.

The normal window contains one native button, a masked PIN field when entering
it, a selectable Server ID when hosting, and status text. It does not show the
coordinator origin, permanent PC number, fingerprints, imported links, encoder
settings or a second settings/viewer control. DPI scaling and desktop work-area
fitting are implemented. Native rendering, small-screen behavior, keyboard
navigation and accessibility still require validation on Windows.

## Bundled capture and remote control

The installer supplies a pinned minimal LGPL FFmpeg 9.0.2 executable alongside
the host, its license and corresponding source. The managed runtime validates
that sibling file against the compiled public encoder manifest and uses Windows
Media Foundation `h264_mf`. Users do not select or download an encoder. The
encoder's actual Windows behavior remains a release gate; compiling it is not a
capture/codec runtime test. See [encoder licensing](../licenses/ffmpeg/).

Before registration the app verifies the encoder hash, required GDI and H.264
components, and a synthetic encode using the production argument builder and
H.264 access-unit parser. Child output and deadlines are bounded. Stop waits for
confirmed process reaping; uncertain reaping prevents another attempt in that
running app. Media Foundation requires a supported Windows installation and
available OS media components.

The host implements real WebRTC H.264 screen transport, mouse buttons/movement,
wheel, supported keyboard keys and Unicode input. Capture is one primary monitor
for the simple pairing flow, with a 1280×720 frame bound and up to 30 FPS. Pointer
mapping uses the same pinned physical display topology as capture; topology
changes stop the session and release input instead of remapping silently.
The existing native monitor selection and separately permissioned tools protocol
remain internal advanced capabilities, described in
[display-selection.md](../docs/display-selection.md) and
[session-tools.md](../docs/session-tools.md). The simple UI does not advertise
monitor switching, clipboard, uploads, audio, power, downloads or a terminal.

## Build and verification

The tested Rust toolchain is **1.100.0** with the locked dependency graph. The
crate declares a 1.89 minimum but that lower version is not the tested release
matrix. Native Windows builds need the appropriate MSVC tools and Windows SDK:

```powershell
cargo fmt --check
cargo test --locked --all-targets
cargo clippy --locked --all-targets -- -D warnings
cargo build --locked --release --target x86_64-pc-windows-msvc
```

The supplied development candidate was cross-compiled with
`x86_64-pc-windows-gnullvm` and LLVM-MinGW UCRT. Format, Linux tests, Linux Clippy,
Windows-target Clippy, final linking, exact source inputs and independent PE
inspection are recorded under `verification/one-button-0.7.0/`. Older evidence is
retained as history and must not be read as current source verification.

Tests cover the single-action state transitions, exact PIN handling, generation
and cancellation guards, managed profile/identity preservation, transcript
binding and expiry, one-use local grants, permission boundaries, and retained
media, input, teardown and tools regressions. Pure/process tests run on Linux;
DPAPI, Windows UI, screen capture, Media Foundation encoding, native input and
the actual installer have not been executed here. The Windows executable is
unsigned. No phone-to-Windows Internet session is claimed by this evidence.

`examples/synthetic_peer.rs` retains a synthetic-only integration fixture sharing
production WebRTC, capability/catalog emission and H.264 framing. It generates
test bars and logs controller input without injecting keys or capturing a real
desktop. `examples/capture_smoke.rs` exercises the same framing path with the
legacy Linux encoder configuration. Their historical successful process tests
are not evidence that the new Windows Media Foundation encoder has run.
