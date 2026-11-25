# OpenRemote Windows host — 0.6.0 development candidate

This is now a real WebRTC H.264 host, not the earlier simulated SDP foundation.
It must still pass Windows device testing, signing and the Internet network matrix
before public release.

The primary connection flow now uses an owner-generated six-digit code and a
chosen PIN. See [the 0.6 connection guide](../docs/windows-installer-0.6.0.md)
and [the shared pairing contract](../docs/code-pin-contract.md). The embedded
service profile is public and empty until an actual service is configured.

## What it implements

- A per-user Windows application with persistent nine-digit PC enrollment and
  one-use six-digit pairing codes for everyday connection.
- Masked PIN/confirmation, Generate/Revoke/Copy, expiry countdown and an in-memory
  local authorization gate. Unknown, expired, revoked or already-consumed grants
  cannot invoke unattended trust as a fallback.
- An explicit **Connect to another PC** button opens the configured `/connect`
  browser viewer. It supplies no PIN or credentials and never launches automatically.
- A persistent native host home window with the PC number, server origin,
  selectable phone setup link, explicit **Copy setup link** and **Stop host**.
  Closing the home window ends all remote access; it does not leave a hidden
  host running. The window must initialize successfully before the host accepts
  requests. Normal minimization is available while the host is in use.
- HTTPS control-plane requests with origin-bound credentials protected by
  **current-user DPAPI**. No plaintext fallback on other operating systems.
- Local native consent, controller fingerprint comparison, a 60-second request
  timeout and deny-on-close behavior.
- A persistent, topmost local session window with a **Disconnect now** button and
  **Ctrl+Alt+F12** emergency stop. Closing it revokes access.
- Explicitly configured unattended access only for an exact pre-trusted controller
  public key; the backend verifies controller key possession before forwarding it.
- Real SDP answers, ICE gathering, authenticated TURN configuration, H.264 RTP,
  and a reliable, ordered WebRTC control DataChannel.
- One selected monitor per session, captured through a separately supplied FFmpeg
  executable. GDI supports monitor selection, including negative desktop offsets;
  optional DXGI Desktop Duplication retains the exactly-one-monitor restriction.
- Software H.264 encoding at up to **1280×720 / 30 FPS**, constrained-baseline
  level 3.1. Portrait displays preserve aspect ratio inside the same frame bound.
- Mouse move, left/right/middle buttons, drag, wheel, whitelisted keyboard keys,
  modifiers and UTF-16 Unicode input. Held keys and buttons release on disconnect.
- Bounded ICE restart recovery, fresh TURN credentials per offer, bounded protocol
  messages, rate limits and cleanup of encoder processes.
- Explicit text clipboard read/write and phone-to-PC file uploads over a separate,
  reliable ordered `tools` DataChannel. Both require independent local enablement
  and a matching requested permission; both default to disabled.
- Visible consent lists the actual view, input, clipboard and upload grants.
  Unattended trust does not bypass any of those permission checks.
- A per-user exclusive instance lock, safe handling of retained session events,
  cleanup of orphan active sessions after a host-process restart, and connected
  status reporting after each answered, healthy ICE connection generation.
- A monitor catalog scoped to the approved session. Changing monitors ends the
  old session and starts a fresh peer/session, with approval repeated unless that
  exact phone already has unattended trust. In-place switching is not advertised.
- Capture/input share a pinned physical-pixel topology. Observed topology changes
  stop the session; stale monitor IDs never silently select the primary display.
- Encoder reaping and input release are verified before the local busy gate opens.
  Unverified teardown keeps the host closed to new sessions until restart.
- Audio, remote power, PC-to-phone downloads and remote terminal are not advertised.

## Setup on an owned Windows PC

1. Run `OpenRemote-0.6.0-Windows-x64-Setup.exe` and open **OpenRemote Host**.
   See [the installer guide](../docs/windows-installer-0.6.0.md). The advanced
   PowerShell installation workflow and source build remain available.
2. Obtain a trusted standalone Windows FFmpeg build containing `gdigrab`,
   `libx264` and `lavfi`. The installer does not bundle an encoder.
3. Complete the graphical setup form on first launch. Enter the deployed API's
   HTTPS origin, PC name, absolute encoder path and trusted expected SHA-256.
   Missing or incomplete setup stays visible instead of exiting immediately.
4. Enter the first-enrollment key in the masked field. It is not saved in
   `config.json` or included in the phone link. Existing credentials remain
   encrypted for this Windows user and bound to their original server.
5. Start from the form. Failed checks leave a visible retryable error. After
   successful startup the existing host home opens. Stop the host and use
   **OpenRemote Settings** (or `--configure`) to edit setup again. The process
   environment token remains supported for the existing scripted workflow.
6. If Android has no service address yet, import the host's **Copy setup link**
   once and confirm the displayed origin. A preconfigured build supplies it.
7. Enter and confirm a 6–12 digit PIN on the host, then choose **Generate code**.
   Enter the six digits and PIN on Android or the receiving-PC browser viewer.
   The local matching grant authorizes one view/input session without a second
   consent prompt. Ordinary advanced requests retain local approval/trust.

Phone setup accepts an ASCII HTTPS origin, including an explicit non-default
port or bracketed IPv6 host, with at most a trailing slash. Use an ASCII punycode
hostname for an internationalized domain. Credentials, paths, query strings,
fragments, whitespace, backslashes and percent-encoded authorities are rejected.
The final `openremote://connect` link is bounded to 2,048 bytes. A deliberately
configured loopback HTTP development host has no Android setup link. A legacy
runtime origin outside the link export subset also disables export while leaving
the visible, otherwise valid host configuration usable. The home window explains
that a standard HTTPS DNS/IP origin is needed for phone setup. A failed native
home-window startup still stops the host; link availability is separate.

The local copy button does not read clipboard content or enable remote clipboard
permissions. If Windows has the clipboard busy, select the link and use Ctrl+C.
The server/PC identity is public addressing metadata, so sharing the link does
not grant a recipient access. Compare the phone fingerprint locally before any
new approval. The new setup form covers server, name, enrollment and encoder
setup. Advanced permission and trusted-phone settings still use the local
configuration workflow; the installer does not enable them automatically.

The window fits the monitor work area and keeps **Stop host (Esc)** in a separate
footer. Smaller or highly scaled desktops can scroll its setup content. Tab focus
reveals an off-screen field, Enter activates the focused Copy/Stop button, and
Escape stops the host. Copying a link remains an explicit local action.

The home window owns its native controls on a separate UI thread. Any failed
startup, cancelled startup, message-loop error or UI thread exit signals the host
to stop remote access. Shutdown requests are handled on that owning thread and
waited for with a bounded join. This is local cancellation state, not a network
credential. Native copy, focus/keyboard navigation, high-DPI/monitor movement and
Stop/close during an actual session remain unexecuted Windows release gates.

Native UI references: Microsoft's [edit control behavior](https://learn.microsoft.com/en-us/windows/win32/controls/about-edit-controls),
[WM_COPY](https://learn.microsoft.com/en-us/windows/desktop/dataxchg/wm-copy),
[GetMessage](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getmessage),
[IsDialogMessageW](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-isdialogmessagew),
[SetScrollInfo](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setscrollinfo)
and [MONITORINFO work area](https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-monitorinfo).

Example configuration (substitute your hostname and verified hash):

```json
{
  "api_base_url": "https://remote.example.com",
  "device_name": "Office PC",
  "allow_unattended": false,
  "trusted_controller_keys": [],
  "allow_input": true,
  "allow_clipboard": false,
  "allow_file_transfer": false,
  "heartbeat_seconds": 15,
  "ffmpeg_path": "C:\\Program Files\\OpenRemote\\ffmpeg.exe",
  "ffmpeg_sha256": "REPLACE_WITH_VERIFIED_64_CHARACTER_SHA256",
  "capture_backend": "gdi",
  "fps": 30,
  "max_width": 1280,
  "bitrate_bps": 4000000,
  "allow_loopback_http": false
}
```

For unattended use, the owner must enable `allow_unattended` and add the exact
base64 P-256 SPKI public key exported in Android Settings to
`trusted_controller_keys`. The fingerprint is for comparison, not a substitute
for that full key. Each session still displays the local notice.

To use clipboard or uploads, enable `allow_clipboard` and/or
`allow_file_transfer` in the PC's local configuration and request the same
permission when connecting from Android. Text is read or replaced only in
response to explicit controller actions; no background clipboard sync runs.
The heartbeat interval must be 5–30 seconds so it stays below the coordinator's
45-second online lease. A device name must be 1–80 UTF-8 bytes, already trimmed,
without C0/DEL control characters, matching enrollment validation.

An enrollment is tied to the API origin and host key. The host refuses to send an
old bearer token to a newly configured server. Explicit re-enrollment is required
when changing servers or after credential revocation. Configuration from the
prototype's machine-wide ProgramData path is deliberately not silently migrated:
that prototype stored bearer credentials as plaintext.

## Monitor selection and lifecycle boundaries

The shared contract is [display-selection.md](../docs/display-selection.md).
The first connection omits `monitorId` and selects the primary display. Once
approved, the host sends a bounded catalog with generated names, physical width
and height, a primary flag, and opaque 64-character identifiers. Native device
names and desktop offsets stay local. At most 16 displays are supported, with a
maximum edge of 32,768 pixels and a maximum capture area of 64 Mi pixels per
monitor; unsupported or inconsistent topologies fail explicitly.

An identifier is valid only in the current host process and observed topology
epoch. A detected change, including an incomplete enumeration during hotplug,
rotates the epoch. Returning from arrangement A to B and back to A does not revive
old identifiers. The local selection is resolved before consent and rechecked
after consent. An unavailable explicit ID ends the request without fallback.
The selected display is named in both the consent prompt and persistent notice.

The host enters a per-monitor physical-pixel DPI context for enumeration. Capture
and pointer mapping use the same pinned display and virtual-desktop rectangles.
It checks topology before every input command and every 500 ms during the session.
A detected change releases remote-held inputs and stops capture. It does not
silently remap an old pointer position onto a newly arranged desktop. The owner
reconnects to refresh the catalog after unplug/replug, resolution changes or a
primary-monitor change. Changes that occur entirely between observations are not
claimed to be detected.

The separately supplied FFmpeg executable has its own process DPI behavior.
Mixed-DPI capture and cursor alignment must be validated with that exact binary
on real Windows displays. Rust geometry tests and a cross-build do not prove
that native behavior. The same release gate covers portrait displays, left/above
placements, display rotation and unplugging a screen during a drag.

On reconnect the host waits for normal prior-session cleanup before starting the
next one. Child task panics still run the cancellation/join path. Capture shutdown
requires a confirmed OS child wait, not only a kill request. Failed key, Unicode
or mouse-button releases remain tracked for retry; unconfirmed final release,
encoder reaping or peer shutdown latches a restart-required state. No subsequent
session is accepted by that running host. If native cleanup fails, the operator
must resolve the old encoder/input state before restarting; automatic recovery
from that exceptional state is not claimed.

Native implementation references: Microsoft's [EnumDisplayMonitors](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-enumdisplaymonitors),
[MONITORINFO](https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-monitorinfo),
[SetThreadDpiAwarenessContext](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setthreaddpiawarenesscontext),
and [MOUSEINPUT](https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-mouseinput),
plus the [FFmpeg gdigrab documentation](https://ffmpeg.org/ffmpeg-devices.html#gdigrab)
and [gdigrab implementation](https://github.com/FFmpeg/FFmpeg/blob/master/libavdevice/gdigrab.c).

## Clipboard and upload boundaries

The wire contract is [session-tools.md](../docs/session-tools.md). Clipboard
text is limited to 4,096 UTF-8 bytes without NUL; over-limit or unavailable text
returns an error instead of a truncated value. Every tools frame is at most
16,384 bytes. Duplicate fields, unknown fields/operations, noncanonical IDs,
binary frames and invalid types are rejected. The host accepts at most 128
requests per second, stores at most 65,536 fresh request IDs per tools channel,
and refuses replayed IDs. Blocking operations have a 10-second channel deadline
and a 3-second response-send deadline. A blocked native call may finish on its
worker afterward; revocation prevents a response or retaining an incomplete upload.

Uploads are limited to one active file, 64 MiB per file and 256 MiB of reserved
bytes per session. Cancelled and failed accepted uploads retain their quota
reservation. Each Base64 chunk is at most 8,192 decoded bytes and must match the
next confirmed offset. Completion requires both the exact length and SHA-256.

The only destination is
`%LOCALAPPDATA%\OpenRemote\ReceivedFiles\<transferId>_<name>`. The host rejects
path separators, Windows device names, controls, alternate-stream syntax,
trailing dots/spaces and overlong basenames. It rejects reparse/symlink receive
directories and keeps Windows directory handles that deny deletion and renaming
while receiving.
Temporary files have a protected owner/SYSTEM security descriptor and an
exclusive handle. The host applies and verifies `Zone.Identifier` with
Internet zone 3, then renames the same verified handle without replacement.
Unsupported metadata/filesystem operations fail explicitly. A file is never
automatically opened or executed.

Cancellation, disconnect, validation failure, 60-second idle expiry and normal
object drop remove incomplete files. A completion guard also rolls back a
renamed file if the session is revoked during native metadata I/O. Completed
files remain in ReceivedFiles; cancellation does not delete a previously
completed upload. An abrupt process crash or power loss can leave a private
`.part` file: crash recovery/cleanup is a remaining release gate, and this build
does not silently remove pre-existing files at startup.

## Build and check

Use Rust **1.100.0**, the tested toolchain, with the locked dependency graph.
The crate's declared minimum is 1.89; that lower bound is not a release-tested
matrix. On Windows install MSVC Build Tools and its Windows SDK first.

```powershell
cargo fmt --check
cargo test --locked --all-targets
cargo clippy --locked --all-targets -- -D warnings
cargo build --locked --release --target x86_64-pc-windows-msvc
```

Linux cross-builds can use Rust's `x86_64-pc-windows-gnullvm` target with a matching
LLVM-MinGW UCRT toolchain. GNU Rust expects GCC runtime libraries, so do not mix the
`x86_64-pc-windows-gnu` target with an LLVM-only linker and invent replacement
libraries. A successful cross-build verifies type/link correctness, not desktop
capture, consent rendering, DPI/input or DPAPI behavior on Windows.

## Synthetic media integration fixture

`examples/synthetic_peer.rs` shares the production peer and H.264 access-unit
assembler. It captures only generated test bars and records received control
messages; it cannot inject native input or access desktop pixels. The caller
writes a fully ICE-gathered offer and reads the fully ICE-gathered answer file:

```sh
cargo build --locked --example synthetic_peer
./target/debug/examples/synthetic_peer \
  --offer-file /tmp/offer.json --answer-file /tmp/answer.json \
  --events-file /tmp/new-events.jsonl --ffmpeg /usr/bin/ffmpeg --seconds 45
```

The events file must not already exist. The fixture exits after at most 60 seconds
of streaming and does not use backend credentials. The root integration harness is prepared to check decoded H.264 output and control
delivery, but its networked run is blocked in this workspace by an OS socket
policy; no successful WebRTC media transport is claimed here. Unit tests separately
exercise arbitrary Annex-B chunk boundaries, maximum frame lengths, protocol
versions, invalid keys/ranges, least privilege, trust configuration and the refusal
to fall back to plaintext credentials.

The 0.5 Linux suite passes 65 tests. Seven new tests cover strict setup links,
work-area bounds, keyboard-focus scrolling and access revocation on UI thread
exit/panic. The 58 retained tests cover session tools, monitor selection,
negative-origin mapping, topology epoch invalidation, strict control duplicate-key
rejection, reconnect cleanup, panic cleanup, actual OS child reaping, and failed
input-release retry coverage. The session-tools checks exercise strict tools framing, duplicate/replayed IDs,
clipboard byte/response limits, permission denial without side effects, real
temporary-file writes, offsets/digests/quotas, exclusive publication, symlink
rejection, cancellation/drop/idle cleanup, and revocation during a deliberately
blocked metadata operation. Native clipboard and Internet-zone operations are
replaced by explicit test adapters in those tests; they are not Windows runtime
verification. See `BUILD_EVIDENCE.json` for the exact executed checks and hashes.

### Executed encoder/decoder smoke test

The network-free fixture was executed successfully for 0.4 on Linux using the
installed FFmpeg. That evidence is carried forward explicitly in 0.5 because
the encoder, configuration and fixture source bytes are unchanged; it was not
rerun for this UI-only continuation. It generated 30 moving test frames through the **production** capture
argument builder, assembled the pipe's arbitrary chunks into complete H.264
access units, decoded those units with FFmpeg, and verified **30 decoded frames
with 30 distinct frame hashes**:

```sh
cargo run --locked --example capture_smoke -- \
  /usr/bin/ffmpeg /tmp/new-capture-smoke.h264
```

This proves the encoder/assembler/decoder path with synthetic input. It does not
prove native Windows screen capture, network transport, Android decoding or input
injection. The output path must be new so evidence is not overwritten.

## Explicit limitations / remaining release gates

- This process runs inside a logged-in user session. It is **not a Windows
  service**, cannot control the lock screen or UAC secure desktop, and cannot
  reconnect across logout/reboot until a service/session-agent design is built.
- No hardware encoder selection, AV1/HEVC, 4K, adaptive bitrate, audio,
  PC-to-phone downloads, automatic clipboard sync, remote terminal or in-place
  monitor switching is claimed. Monitor selection uses a fresh authorized session.
- Native clipboard ownership/delayed-rendering behavior, directory reparse and
  owner ACL behavior, alternate-stream sharing, handle rename/no-overwrite, and
  cleanup on Windows must pass real-device tests. Cross-compilation and Linux
  test adapters do not prove those operating-system behaviors.
- Abrupt-crash partial-file recovery is not implemented. Normal stop, cancellation,
  idle expiry and object-drop cleanup are implemented and unit-tested.
- DXGI is provided through FFmpeg, not through a newly implemented native D3D11
  engine. Mixed-DPI, multiple-monitor and GPU-reset behavior need native testing.
- The current control-plane authority is trusted to bind the authenticated phone
  identity to its SDP. A compromised coordinator is outside the current trust
  boundary; detached peer-signed SDP or an equivalent independently verified
  binding is a future security gate for an untrusted-server deployment.
- The FFmpeg binary's license and redistribution terms must be reviewed for the
  exact build before bundling it in a product installer. This repository currently
  supplies no third-party executable.
- Signing, independent security review, physical phone/PC tests and forced TURN
  UDP/TCP/TLS across real networks remain required. See the root BUILD_STATUS.md.

## Primary implementation references

- [webrtc-rs 0.17.3 maintenance release](https://github.com/webrtc-rs/webrtc/releases/tag/v0.17.3)
- [FFmpeg GDI capture](https://ffmpeg.org/ffmpeg-devices.html#gdigrab)
- [FFmpeg Desktop Duplication filter](https://ffmpeg.org/ffmpeg-filters.html#ddagrab)
- [Microsoft SendInput and UIPI restrictions](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput)
- [Microsoft CryptProtectData](https://learn.microsoft.com/en-us/windows/win32/api/dpapi/nf-dpapi-cryptprotectdata)
- [Microsoft OpenClipboard ownership](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-openclipboard)
- [Microsoft SetClipboardData memory ownership](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setclipboarddata)
- [Microsoft GetClipboardData handling](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getclipboarddata)
- [Microsoft file streams and sharing modes](https://learn.microsoft.com/en-us/windows/win32/fileio/file-streams)
- [Microsoft FILE_RENAME_INFO](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_rename_info)
- [Microsoft SetFileInformationByHandle](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-setfileinformationbyhandle)
- [Microsoft Windows filename rules](https://learn.microsoft.com/en-us/windows/win32/fileio/naming-a-file)
