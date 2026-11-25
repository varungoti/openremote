# Owner-controlled active hosting, version 0.8

Active hosting is a process-scoped, owner-enabled grant for the existing
Windows host. The host retains its permanent nine-digit `device.displayId`
across registration recovery and coordinator restart. This PC ID is a public
locator. A controller still needs the current owner-entered six-digit PIN,
possession-proved controller authentication, and approval by the running host's
matching local activation.

## Discovery and owner activation

`GET /v1/info` reports `version: "0.8.0"`, `activationVersion: 1` and
`maxActivationConnections: 4` alongside the existing discovery fields. A host
must require the supported activation version and capacity before offering this
flow. The publisher must update strict discovery/session decoders before using
the additive protocol. Discovery does not establish relay or media health.

The existing host registration challenge and proof endpoints are unchanged.
Registration recovery keeps the same permanent PC ID, rotates its host bearer,
and revokes all authorization issued to the previous host process.

An authenticated owner creates an activation with:

```http
POST /v1/hosts/{opaque-host-id}/pairing
Authorization: Bearer {host-bearer}
Content-Type: application/json

{
  "mode": "activation",
  "pin": "001234",
  "permissions": { "viewScreen": true, "controlInput": true }
}
```

The PIN must contain exactly six ASCII digits. Only screen viewing and optional
input control can be granted. Omitted permission booleans are false. The owner
may instead create a view-only activation. Explicit null, empty, misspelled or
unsupported `mode` values are rejected, preventing an accidental downgrade to a
one-use invitation.

The `201` response includes the existing `pairingId`, `code`, `expiresAt` and
`permissions` fields plus `mode: "activation"` and `maxConnections: 4`. The
six-digit `code` remains an internal compatibility invitation; the primary UI
shows the permanent nine-digit `device.displayId` as the PC ID. Creating a new
activation does not change that permanent ID.

The activation expires 24 hours after creation. The verifier, PIN, grant and
sessions never enter durable snapshots. The owner stores the returned grant ID
and expiry only in the running process; a fresh process must invalidate any old
server grant before offering activation. Recovering the host credential already
invalidates the previous generation. The owner must not silently restart a grant
or reuse a saved PIN after process or coordinator restart.

`GET /v1/hosts/{opaque-host-id}/pairing`, authenticated with the host bearer,
returns `{ "pairing": <the same activation object> }` while the activation is
current, or `{ "pairing": null }` after revocation, lockout or expiry. The local
host must compare its grant ID, mode and expiry against this response and stop
accepting when they do not match or coordinator authorization cannot be
confirmed. A recreated grant must never authorize a queued request from an old
generation.

## Controller redemption

The controller first completes the existing P-256 challenge/proof login. It
then calls the unchanged redemption endpoint:

```http
POST /v1/pairings/redeem
Authorization: Bearer {controller-login-bearer}
Content-Type: application/json

{
  "code": "123456789",
  "pin": "001234",
  "controllerName": "Work laptop"
}
```

`code` accepts exactly nine ASCII digits for a permanent PC ID, or exactly six
ASCII digits for an existing invitation. A nine-digit ID selects only that
host's currently enabled activation; it never selects a legacy one-use grant.
Unknown IDs, stopped hosts, expired/locked grants, wrong PINs and unavailable
admission use the same `403 { "error": "pairing_unavailable" }` response.
Unknown or ineligible IDs still perform the dummy PBKDF2 calculation. Existing
per-controller, per-IP and global rate limits and the two-worker KDF bound are
shared by both ID forms. Global coordinator capacity and failed storage retain
their existing `503` behavior; saturated rate/worker limits return `429`.

The PIN uses the existing salted PBKDF2-HMAC-SHA-256 verifier with 600,000
iterations. Five wrong guesses lock the entire activation for all controllers,
including guesses made through either ID form. Successful redemptions do not
reset the failure count. No PIN, verifier or code is sent in a host event, audit
event or session record.

A successful redemption returns the existing `session` and `controllerToken`
response, and queues the existing `session.requested` host event. Both session
and event payload additionally contain `mode: "activation"`, the current
`pairingId`, and the actual granted `permissions`. Authentication binds the
request to `controllerPublicKey`. `controllerName` is a bounded display claim,
not proof of the person's or agent's identity; clients must sanitize it for
their display context.

The coordinator leaves the session `pending`. Only the owner process holding
the matching local activation may approve it. Possessing a server-side grant
record alone never bypasses this local check. The legacy `controllerVerified`
event flag refers to proof of key possession, not verification of the name.

## Capacity and permissions

The coordinator admits at most four non-ended activation sessions per host,
including requests still pending approval or preparing media. Admission and
input reservation occur under one state lock. If input was owner-enabled, the
first current session reserves input. Additional sessions receive
`controlInput: false` and screen viewing only. Approval can narrow permissions
but cannot widen them.

Pending input reservations count, so racing requests and out-of-order approvals
cannot create two input controllers. Ending, rejecting or expiring a session
releases its reservation. A later new session may reserve a released input slot.
Existing spectators keep their original permissions and never gain input when
the controller leaves. The Windows process must independently enforce the same
capacity, replay protection and input restriction at its native handlers.

Legacy and active-hosting sessions do not mix. Enabling activation while any
legacy pending or live session exists returns `409 host_busy`. Legacy/trusted
session requests are blocked while an activation is enabled or has outstanding
sessions. Outside active hosting, the historical limits and single-live-session
approval semantics are unchanged.

The existing pending timeout and normal session timeout still apply, and neither
may extend a child session past its activation's 24-hour expiry. The default
pending timeout is 90 seconds and the normal session timeout is two hours.

## Stop, replacement and media state

`DELETE /v1/hosts/{opaque-host-id}/pairing` is owner-authenticated and idempotent.
It invalidates creations currently hashing outside the state lock, removes the
current grant, and ends all of the host's activation sessions, including pending,
approved, connecting and connected sessions. It still cleans up activation
sessions if the grant was already removed. Replacing an activation also ends the
previous generation's sessions. PIN lockout ends that generation's sessions, and
expiry bounds every child's authorization. Host credential recovery and
administrator revocation invalidate the entire previous host process.

Session termination clears its signaling queue and emits the existing
`session.ended` event. The local host must close each affected media task and
release native input resources. `POST /v1/sessions/{id}/end` remains available
to either peer, allowing a disconnect or failed preparation to release a slot.
The coordinator cannot directly observe a peer-to-peer media connection.

`POST /v1/sessions/{id}/connected` retains its existing offer/answer ordering
requirements. A pending or approved server record must not count as a connected
device in the tray UI. That count and the displayed names must reflect the local
host's actual media readiness, clear during reconnect, and disappear on teardown.
For an upgrade from a previous host process, owner-authenticated
`GET /v1/hosts/{id}/sessions` returns `{ "sessions": [...] }` containing only
the host's current non-ended session metadata, sorted by creation time and ID.
It excludes bearer secrets and signaling payloads. The normal three/four-session
invariants keep this small; an unexpected set larger than 16 is rejected with
`409 session_list_capacity` rather than truncated. After acquiring its local
single-instance lock, and only when no local session tasks exist, a new host
that receives `409 host_busy` may inspect and explicitly reject/end these old
process sessions, confirm completion, and retry activation once. Inspection
itself never approves or ends a session. No permanent unattended-access
entitlement is introduced by this protocol.

## Legacy compatibility

Omitting `mode` from owner creation preserves the exact legacy response fields,
six-to-twelve-digit PIN rule, ten-minute default grant expiry, one-use redemption
and single-live-session behavior. Legacy sessions and request events omit
`mode`; legacy pairing responses omit both `mode` and `maxConnections`.
Revoking an already consumed legacy invitation preserves its historical session
semantics. Existing automatic registration, host ownership and controller
revocation checks continue to apply.
