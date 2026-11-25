# OpenRemote Windows host — 0.8.0 development candidate

OpenRemote shows a stable **Your PC ID** and an **Activate Connection** action.
After activation it runs in the notification area, with a live connected-device
count, a device list, and an explicit Deactivate/Quit menu. **Connect to a PC**
opens the receiving browser viewer on the configured service so another Windows
laptop can act as a client. This is an interactive background application in the
signed-in Windows desktop session, not an SCM service or a Session-0 host.

This source and its cross-compiled binary are a development candidate. Native
Windows UI, capture, input, installer, and actual Internet sessions remain release
gates. The exact source and build status are recorded in
[BUILD_EVIDENCE.json](BUILD_EVIDENCE.json).

## Everyday use

1. Install the provider-configured Windows installer and open **OpenRemote**.
   The app checks its configured service and registers this PC automatically.
   **Your PC ID** is the service-issued, stable nine-digit ID. It is displayed
   before a PIN is requested, stays visible after deactivation, and is recovered
   from the existing DPAPI registration when available while offline.
2. Select **Activate Connection**, choose a **six-digit security PIN** (leading
   zeroes are preserved), and select the same action again or press Enter.
   There is no PIN confirmation field. The app checks screen capture and creates
   a temporary activation bound to this process and your current permissions.
3. When service registration, capture preflight and activation all succeed,
   OpenRemote adds its notification icon and hides the window. It only hides
   after Windows confirms the icon was added. If the icon cannot be created or
   restored, the owner window remains visible.
4. On the configured phone app or receiving-PC browser viewer, enter **Your PC
   ID** and the PIN. The ID stays the same between activations; access is possible
   only during the current activation. Up to four devices can connect, including
   those still connecting. At most one session has input control; additional
   devices are view only and are never silently promoted to control.
5. Hover over the notification icon for the live connected count and bounded
   device names. Click it to open the owner window. The window shows connected,
   connecting and reconnecting devices separately, with control/view-only access.
   Names are descriptive labels supplied by the connecting device, not verified
   identity claims. An approved request alone does not count as connected.
   A native vertical scrollbar and keyboard focus reveal keep the full window
   accessible when the monitor work area is small or display scaling is high.
6. Select **Deactivate Connection**, use the tray menu, or press **Ctrl+Alt+F12**
   to revoke all access. **Quit OpenRemote** deactivates and exits. Closing or
   minimizing the window while an active tray icon exists keeps hosting in the
   tray; closing an inactive window exits. If the tray icon is unavailable,
   closing the active window deactivates and exits instead of hiding a host.

An activation lasts at most 24 hours and stops on deactivation, expiry,
revocation, loss of authorization, quitting, or a fatal local failure. A session
ending does not deactivate the host; other devices may reconnect with the same
PC ID and PIN while the activation remains valid. Individual service session
limits still apply. Restarting the app never restores the old local activation
or PIN. Activate again to allow new access.

**Connect to a PC** opens the configured HTTPS `/connect` viewer in the default
browser. No PIN, token, PC ID or credential is put in the browser URL. The user
enters the receiving PC's ID and PIN in that viewer. There is no manual server
address field or separate technical setup screen in the normal app.

## Unconfigured installers and service admission

This repository's public `config/service-profile.json` is intentionally empty
until an actual deployment is supplied. A fresh unconfigured build shows this
message immediately, with activation and the browser launcher disabled:

> This installer has no connection service configured. Your PC ID is unavailable.
> Ask the app provider for a configured installer.

The app does not ask for a PIN first, create a fake PC ID, invent a server, or
present local pairing preparation as successful Internet access. A working
phone-to-PC or laptop-to-PC deployment requires the same real HTTPS service in
all clients, the backend with activation support, and a reachable TURN relay.
This source change does not deploy or demonstrate that service.

Before any PIN entry, the host checks `/v1/info` for the OpenRemote protocol,
pairing version, activation version and four-session limit. It then registers
and checks the PC's protected identity. A network failure leaves a visible retry
action; an absent provider profile keeps activation disabled. Encoder preflight
runs after the owner supplies the PIN and before the app can report activation.

## Identity, permissions and bounded activation

The provider embeds only a public HTTPS origin. Neither the installer nor the
application contains a global enrollment secret. The PC creates its Ed25519
identity once and protects it with current-user Windows DPAPI. Its bearer token
is also DPAPI protected and bound to that key and exact service origin. Existing
origin bindings and permission settings remain authoritative; there is no
plaintext credential fallback or automatic origin migration.

If a previous host process left a legacy session behind, activation may return
`host_busy`. Only that specific response, before this process owns any local
session, permits one bounded recovery: list at most 16 authenticated host-owned
pending/live sessions, end each, require each acknowledgment, then retry
activation once. The per-user instance lock excludes another local host. A
canceled or unacknowledged cleanup never retries activation; the app stays
visible and inactive. It does not rotate a healthy bearer to force recovery.

The nine-digit PC ID is the registered device's `displayId`, not the six-digit
internal compatibility invitation code. Registration signs the exact bounded,
origin-bound challenge transcript. A valid saved bearer retains its existing
policy; a revoked bearer does not silently create another PC identity. See
[the automatic registration protocol](../server/AUTOMATIC_REGISTRATION_PROTOCOL.md).

The host requests the additive pairing mode `activation`; legacy six-digit,
ten-minute, single-use pairing remains supported by the internal API. The
activation response must state mode `activation` and `maxConnections: 4`.
Its opaque pairing ID, permissions and expiry are installed in an in-memory
local grant before the owner UI can become active. The host refreshes that exact
grant with the service and enforces a monotonic local maximum of 24 hours.

Every session needs the current activation ID, the correct mode and bounded
permissions. The local gate authorizes each session ID once, retains a bounded
4096-entry replay ledger per activation, limits native session owners to four,
and independently permits at most one input controller. Exhausting that ledger
closes the activation and requires a new activation. No ordinary trusted-device
session can bypass the active PIN flow. Existing trust settings are preserved
on disk for compatibility, but code/PIN access never adds permanent trust.

Clipboard, file transfer, audio and power are excluded from the activation
grant. The PIN exists only in memory and the HTTPS activation request. It is not
written to settings, logs, command arguments, links or the clipboard. The masked
field is cleared on submission or cancellation.

## Lifecycle and native resources

Each connecting or connected device owns a distinct bounded WebRTC/capture
session. Admission appears as **Connecting**. The count becomes **Connected**
only after the answer was accepted, peer and ICE state are healthy, a video
sample was sent, and the service acknowledged the `connected` transition.
Disconnect/recovery and terminal reporter exit clear that count. During recovery
a fresh sample and acknowledgment are required before it returns to Connected.

Ending one session removes its device entry and releases its capacity only after
teardown. The host stays active. All sessions are canceled together on
Deactivate, Quit, local expiry or service revocation. Native cleanup is bounded
and awaited; an unverified capture/input/peer teardown latches a restart-required
state. The owner window restores for the resulting status. Late asynchronous
results cannot reinstall a canceled grant or restore active state.

The owner-thread `RevokeOnExit` and `supervised_attempt` guards remain in place
for normal exit, task abortion and panics. The process panic hook also cancels
the shared application token before displaying any potentially blocking native
fatal alert. Windows Explorer's `TaskbarCreated` notification recreates the tray
icon; failure restores the owner window. Tooltip text is bounded to Windows'
127 UTF-16-unit payload without splitting surrogate pairs. Untrusted names have
line breaks, controls and bidi-formatting characters removed.

## Bundled capture and input

The installer supplies the existing pinned minimal LGPL FFmpeg 9.0.2 executable,
license and corresponding source. The runtime validates that sibling file
against the compiled public encoder manifest and uses Windows Media Foundation
`h264_mf`. It verifies the hash, GDI and H.264 components, and a short synthetic
encode before activation. Output and deadlines are bounded; cancellation waits
for confirmed encoder process reaping. See [encoder licensing](../licenses/ffmpeg/).

The host implements WebRTC H.264 screen transport, mouse movement/buttons,
wheel, supported keyboard keys and Unicode input. Activation captures the
primary monitor with the existing 1280×720 frame bound and up to 30 FPS.
Four viewers can therefore require four encoder processes; actual performance
and Windows Media Foundation behavior must be measured on target hardware.
A physical display-topology change stops the affected session and releases its
input rather than silently remapping another monitor.

Internal advanced capabilities and earlier protocol behavior are described in
[display-selection.md](../docs/display-selection.md) and
[session-tools.md](../docs/session-tools.md). The activation UI does not advertise
monitor switching, clipboard, uploads, audio, power, downloads or a terminal.

## Build and verification

The tested toolchain is **Rust 1.100.0** with the existing locked dependency graph.
No registry crates were added or upgraded for 0.8. The declared 1.89 minimum is
not the tested release matrix. For native Windows builds use the matching MSVC
build tools and Windows SDK:

```powershell
cargo fmt --check
cargo test --locked --all-targets
cargo clippy --locked --all-targets -- -D warnings
cargo build --locked --release --target x86_64-pc-windows-msvc
```

The development candidate is cross-compiled with `x86_64-pc-windows-gnullvm` and
LLVM-MinGW UCRT. Current check logs, exact source hashes, binary digest and PE
inspection are recorded under `verification/activation-0.8.0/`. Older evidence
is preserved as history and is not a claim about the current source.

Linux tests cover PIN/state transitions, service capability admission, identity
and origin preservation, activation generation/replay/capacity/expiry, device
lists and count changes, Unicode tooltip bounds, and retained cancellation,
input, media process, protocol and tools regressions. Cross-target checks compile
the Windows-specific code, but do not execute it. Native tray interaction,
Explorer recovery, DPI layout, keyboard accessibility, DPAPI, capture, Media
Foundation, native input, the installer and phone/laptop Internet sessions have
not been executed here. The binary remains unsigned and not release ready.
