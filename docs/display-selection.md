# Display selection — protocol v1, OpenRemote 0.4

Each session captures one selected Windows monitor. The host binds capture and
input to the same physical-pixel geometry for that session. Changing monitors
ends the old session and creates a new authorized session, peer connection and
renderer. This prevents buffered frames from one display from enabling input on
another. In-place switching is not implemented.

## Session request

`POST /v1/sessions` accepts an optional `monitorId`, exactly 64 lowercase
hexadecimal characters. Omit it to request the primary monitor. Empty strings,
null, wrong types and malformed IDs are invalid. `displayId` remains the nine-digit
PC number; it is distinct from `monitorId`.

The coordinator carries this immutable selector in the session response and the
host's `session.requested` event. It authenticates the controller and retains the
existing consent and permission checks. A selector grants no additional powers.
Only the host can resolve it against its current display topology. Unknown or
stale IDs end/reject the request; the host must never silently choose a different
display. Session selectors and catalogs are ephemeral, not a persistent display
inventory at the coordinator.

## Host catalog

After the reliable ordered `control` channel opens, a supporting host sends its
ordinary `capabilities` frame with `multiMonitor:true`, then one catalog:

```json
{"v":1,"type":"display_catalog","selectionMode":"reconnect","selectedMonitorId":"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef","monitors":[{"id":"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef","name":"Display 1","width":1920,"height":1080,"primary":true}]}
```

`multiMonitor` advertises this selection protocol, even when one monitor is
attached. The catalog has exactly the fields shown, 1–16 monitors, unique IDs,
exactly one primary monitor, and a selected ID present in the list. Each width
and height is an integer in 1–32,768. Names contain 1–80 UTF-8 bytes and no ISO
control characters. The host generates labels `Display 1` through `Display 16`;
hardware identifiers and local device paths are not sent. Frames are strict
UTF-8 JSON, bounded to 16,384 bytes, with duplicate or
unknown fields rejected.

IDs are SHA-256 derived opaque selectors bound to a random per-process nonce,
an observed-topology generation, the complete canonical display topology and the
individual monitor identity. An observed change or failed/incomplete topology
read rotates the generation; changing a layout and later restoring it cannot
revive a previously issued selector.
They remain usable across sessions only while that host process and topology
remain unchanged. The Android app keeps them only in the live session; it does
not persist a preferred token. Reconnecting to the primary monitor refreshes the
catalog after a host restart or topology change.

GDI capture supports the enumerated monitors. The current DXGI capture mode
retains its one-monitor restriction. A monitor catalog does not promise hardware
encoding, higher resolution, secure-desktop access or simultaneous displays.
Source monitor dimensions are additionally limited to 67,108,864 pixels. The
host's per-monitor DPI context does not set the DPI behavior of the separately
supplied FFmpeg process; mixed-DPI capture and cursor alignment must be verified
with the exact encoder binary before that configuration is supported.

## Safe switch and lifecycle

The phone offers **Switch and reconnect**, explains that the PC may request
approval again, and disables the action during clipboard/file operations or a
document-picker handoff. It retains the requested permissions without adding a
capability. It releases held input, closes the old renderer/peer, obtains an
acknowledged coordinator end, and then requests the selected monitor in a fresh
session. Old asynchronous callbacks cannot mutate the replacement session.

The host must finish cleanup of a terminal previous session before starting a
new one. A bounded wait may accommodate this handover; overlapping capture/input
owners are forbidden. If cleanup cannot finish, the new request fails explicitly.
The host requires confirmed encoder termination and attempts to release all
tracked virtual keys, Unicode input and mouse buttons. Failed releases remain
tracked for retry. Unverified teardown leaves the host unavailable for replacement
sessions until a local restart; returning from an asynchronous task alone is not
treated as proof that native resources were released.

For an explicitly selected monitor, the phone requires a valid catalog echoing
that exact ID before presenting video or enabling input or tools. A missing,
malformed or mismatched binding fails closed; even an older host that ignores
the new HTTP field cannot briefly show its primary display as the selected one.
Fresh decoded video and a frame rendered by the new renderer are required before
interactive control. Renderer recreation pauses input without cancelling an
already authorized upload while decoded video remains fresh. Older hosts remain usable for
ordinary primary-monitor sessions; they do not expose a monitor picker.

The host validates topology again after approval, before each input operation,
and at intervals of at most one second during capture. On an observed change it
releases held input and ends the session; it does not silently remap input or
capture. Enumeration and OS display changes are not atomic, so a change can occur
between validation and injection. The physical-device gate must characterize
this race and verify bounded shutdown; this implementation does not claim a
zero-frame/zero-event hardware guarantee.

## Acceptance and verification

Unit/contract tests cover strict catalog bounds, stale/unknown selectors,
duplicate IDs/fields, permission preservation, negative virtual origins, portrait
geometry, callback generations and first-frame gating. Coordinator tests prove
selectors survive authenticated session/event reads without granting control and
cannot be changed at approval. Tests also retain the omitted-selector legacy path.

Physical-device acceptance requires two monitors, negative-origin placement,
portrait and mixed-DPI layouts, a view-only session, repeated switches, a host
restart, unplug/replug during held input, and confirmation that the phone never
controls an unselected display. Native enumeration, GDI capture, SendInput and
renderer behavior remain unverified until those cases run on Windows/Android.

## Implementation references

- [Microsoft: EnumDisplayMonitors](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-enumdisplaymonitors)
- [Microsoft: SetThreadDpiAwarenessContext](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setthreaddpiawarenesscontext)
- [FFmpeg: gdigrab capture offsets](https://ffmpeg.org/ffmpeg-devices.html#gdigrab)
- [FFmpeg: current gdigrab implementation](https://github.com/FFmpeg/FFmpeg/blob/master/libavdevice/gdigrab.c)
- [WebRTC: renderer event API](https://webrtc.googlesource.com/src/+/refs/heads/main/sdk/android/api/org/webrtc/RendererCommon.java)

These references explain the APIs used. They do not establish which source
revision produced a separately supplied encoder or the packaged native WebRTC
library; binary provenance and physical-device results remain separate gates.
