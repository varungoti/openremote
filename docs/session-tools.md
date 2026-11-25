# Session tools protocol — v1

This contract extends the remote desktop with explicit text clipboard actions and
phone-to-PC file uploads. It does not implement automatic clipboard monitoring,
PC filesystem browsing, PC-to-phone downloads, resumable uploads, or execution of
received files. Those are separately tracked in the product plan.

## Authorization and transport

The Android session request must explicitly request `clipboard` and/or
`fileTransfer`. Both default to false. The PC must independently enable
`allow_clipboard` and/or `allow_file_transfer` in its local settings; both default
to false. Host approval displays the actual granted capabilities. A trusted
unattended controller still receives only the intersection of requested and
locally enabled permissions.

Android creates a reliable, ordered WebRTC DataChannel named `tools` before its
offer. It is separate from `control` so file data does not queue ahead of pointer
events. Only one channel with this label is accepted. The host advertises granted
`clipboard` and `fileTransfer` flags in its existing `capabilities` message on the
control channel. Capabilities do not replace server-approved permissions.

Every request and response is a UTF-8 JSON text frame of at most 16,384 bytes.
Binary messages, unknown operations/fields, unsupported versions, duplicate
fields, malformed IDs and out-of-range values fail validation. The receiver
limits requests to 128 per second. Invalid framing can close the tools channel;
an ordinary denied operation or file I/O failure returns a bounded error and must
not terminate an otherwise healthy desktop connection.

Every request contains `v:1`, `type`, and `requestId`, a fresh 32-character
lowercase hexadecimal identifier. File requests also contain a separate
`transferId` with the same format. Only one file upload may be active in a session.
The phone allows one outstanding tools request, with a 15-second response timeout.
A timed-out upload is cancelled rather than automatically replaying a chunk.

## Requests

| Type | Additional fields | Behavior |
| --- | --- | --- |
| `clipboard_read` | None | Read the current PC text clipboard once. |
| `clipboard_write` | `text` | Replace the PC text clipboard only after a user action. |
| `file_begin` | `transferId`, `name`, `size`, `sha256` | Start receiving one file into a private temporary file. |
| `file_chunk` | `transferId`, `offset`, `data` | Append standard padded Base64 data at exactly the next byte offset. |
| `file_finish` | `transferId` | Verify exact length and SHA-256, then make the received file visible. |
| `file_cancel` | `transferId` | Close and delete this transfer's incomplete file. |

Clipboard text is at most 4,096 UTF-8 bytes and contains no NUL. Empty text is
allowed. The serialized JSON must also fit the frame limit. The phone displays
received text before an explicit copy to its clipboard; it must not read or
overwrite the phone clipboard in the background. If the PC clipboard is too
large or contains no text, return an error instead of truncating silently.

Files are at most 67,108,864 bytes (64 MiB). The total accepted upload size per
session is at most 268,435,456 bytes (256 MiB), counting reservations for cancelled
or failed uploads. A chunk contains at most 8,192 decoded bytes. `size` and
`offset` are nonnegative JSON integers. `sha256` is exactly 64 lowercase hexadecimal
characters. Empty files are valid and use SHA-256 of empty bytes.

`name` is only a display basename: 1–120 UTF-8 bytes, no path separators, Windows
reserved device names, control characters, `<>:"/\\|?*`, or trailing dot/space.
The host never accepts a destination path from the phone. It saves to
`%LOCALAPPDATA%\OpenRemote\ReceivedFiles\<transferId>_<name>`, using exclusive
creation and no overwrite. A reparse-point/symlink receive directory is rejected.
The temporary file is deleted on cancellation, disconnect, idle expiry, length
mismatch, or digest mismatch. The idle timeout is 60 seconds. Received files are
never opened or executed automatically. Windows Internet-zone metadata is applied
before reporting success; unsupported filesystems must return an explicit error.

Android uses the system document picker and snapshots the selected file into a
bounded private cache file while computing its digest. It then streams acknowledged
chunks with progress/cancellation, deleting its cache on every terminal path. It
does not request broad device-storage access.

## Responses

Success:

```json
{"v":1,"type":"tool_result","requestId":"0123456789abcdef0123456789abcdef","ok":true,"result":{"text":"example"}}
```

Errors use the same envelope with `ok:false`, `error:{"code":"permission_denied",
"message":"Clipboard is not permitted for this session."}`. Messages contain no
secret, clipboard text, file content, or absolute local path.

Success result fields are fixed by request type:

| Request | `result` fields |
| --- | --- |
| `clipboard_read` | `text` |
| `clipboard_write` | Empty object |
| `file_begin` | `transferId`, `received:0`, `chunkBytes:8192` |
| `file_chunk` | `transferId`, `received` (cumulative confirmed bytes) |
| `file_finish` | `transferId`, `name` (saved basename), `size`, `sha256` |
| `file_cancel` | `transferId` |

The phone validates the response version, type, request ID and expected result
shape. It reports completion only after `file_finish` confirms the expected
length and digest. Pending operations fail promptly when the channel/session
closes. Neither client logs clipboard or file contents.

## Required verification

Protocol tests cover permission denial, malformed/duplicate JSON, traversal and
reserved names, oversized frames/chunks/files, wrong offsets, quota exhaustion,
digest mismatch, no overwrite, and cleanup after cancellation/drop. Clipboard
tests verify byte limits and explicit actions. Android tests cover strict result
validation, chunk acknowledgement and cancellation cleanup. Native Windows
clipboard, Internet-zone metadata, Android document-picker behavior, and transfer
over actual WebRTC remain device integration gates even when unit tests pass.
