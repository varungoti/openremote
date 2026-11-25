# Protocol v1

The implemented HTTP authentication, status codes and request/response formats
are defined in [server/README.md](../server/README.md). Clients use HTTPS and
reject redirects to prevent credential movement across origins.

## Authentication

Controller P-256 SPKI public keys and ASN.1 DER signatures use standard padded
Base64. The signature covers the raw 32 decoded random bytes returned by the
one-use challenge endpoint. The coordinator's verified identity supplies the
controller key on every session request. A copied public-key string cannot log in.
Host enrollment uses a separate private deployment token; subsequent host calls
use that PC's opaque protected bearer. Session tokens are separate from controller
login tokens.

## Session state

`pending → approved → connecting → active → ended`

Rejection, expiry and revocation are terminal. Host approval may narrow requested
permissions but never add them. `connecting` is a valid live negotiation state;
clients must neither start duplicate peers nor disconnect solely because they
observe it. `active` is a peer connection report; actual decoded-frame evidence is
still required before presenting a usable desktop.

## Signaling

Only approved session members may exchange signaling. The controller creates the
initial offer after adding a receive-only video transceiver and a reliable ordered
`control` DataChannel. Offers/answers are JSON `{type,sdp}`; ICE uses
`candidate`, `sdpMid`, `sdpMLineIndex` and optional `usernameFragment`.

Candidates arriving before a remote description are bounded and queued. Cursors
advance only after a message is processed. Host inbox cursor errors include
`resetAfter`; revalidated pending requests can then be recovered after compaction
or a coordinator restart. A network change may trigger a new authenticated ICE
offer with refreshed temporary relay credentials.

## Reliable ordered `control` DataChannel

UTF-8 JSON, protocol `v:1`, maximum 16 KiB. Unknown message types/fields and invalid
ranges are rejected. Current receiver rate limit is 400 messages/second; Android
bounds buffered input to 256 KiB rather than queuing stale input indefinitely.

| Type | Fields and meaning |
| --- | --- |
| `pointer` | `action:move/down/up`, normalized `x/y` in [0,1], `buttons` bitmask left 1/right 2/middle 4 |
| `scroll` | Finite bounded `dx/dy` wheel deltas |
| `key` | Whitelisted DOM-style `code`, `down` boolean, `modifiers` mask Ctrl 1/Alt 2/Shift 4/Meta 8 |
| `text` | Nonempty UTF-8 `text`, maximum 4,096 bytes, no NUL; translated to UTF-16 input on Windows |
| `release_all` | Releases inputs held by this session; permitted even after input permission is removed |

Coordinates map to the selected monitor and account for letterboxing in the
controller. The host pins the same physical-pixel monitor and virtual-desktop
geometry for capture and input. Layout changes end the session. The 0.4 monitor
picker uses a fresh session/peer/renderer for each switch, as specified in
[display-selection.md](display-selection.md).

The host sends a `capabilities` message when the channel opens. It advertises
only supported granted behavior: text input, explicit clipboard and phone-to-PC
file upload. Clipboard and file flags require both local enablement and the
approved session grant. A supporting 0.4 host sets `multiMonitor:true` and sends a
bounded `display_catalog` immediately after capabilities. Audio and power remain
false. The client must also honor the server-approved permissions; capabilities
alone do not authorize a feature.

## Explicit clipboard and uploads

The separate reliable ordered `tools` channel uses the strict versioned contract
in [session-tools.md](session-tools.md). It carries user-triggered text operations
and bounded, acknowledged file chunks. It does not share the pointer/input queue.
Ordinary tools failures close or fail tools without terminating a healthy desktop.
The phone requires both host capability and the final permission grant.

## Native media

H.264 Annex-B from FFmpeg is assembled across arbitrary stdout chunk boundaries
into complete access units before `TrackLocalStaticSample.write_sample`.
Constrained-baseline level 3.1 and up to 1280×720/30 FPS are the current profile.
RTP/DTLS/SRTP negotiation is implemented in the native libraries. A passed SDP
exchange or a locally encoded frame is not claimed as a transported/decoded
remote desktop session.
