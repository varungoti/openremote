# Code and PIN connection contract (0.6.0)

Status: implementation contract; execution evidence and deployment status are recorded separately.

## Owner flow

The running Windows host offers **Generate code**. The owner enters and confirms a 6–12 digit security PIN. Generation explicitly authorizes one screen-sharing session, optionally with input according to the host's local configuration. Clipboard, files, audio and power are not part of this grant. The host displays a six-digit code, expiry and **Revoke code**. Code and PIN are distinct strings and leading zeroes are significant. No PIN is stored in settings, links, audit records or logs.

The code is randomly allocated, unique among active codes on this coordinator, expires after ten minutes, and is consumed once. It is not a permanent globally unique computer identity. At most one code per host exists; replacement revokes the previous unused code. Five wrong PIN attempts invalidate a known code. Independent source-address, controller-identity, host-generation and coordinator-wide budgets limit guessing and expensive verification. Invalid, expired, consumed, revoked, locked or offline credentials return the same generic pairing failure.

## Shared service profile

`config/service-profile.json` has exactly `schemaVersion: 1`, `serviceName: "OpenRemote"`, and `apiOrigin` (empty for an unconfigured build, otherwise a strictly validated HTTPS origin). The host and Android builds read this same public file. It contains no enrollment key, host token, controller token, PIN or TURN secret. A configured profile supplies the first-run service origin; it never silently replaces a saved or enrolled origin. The desktop web controller uses its own HTTPS origin. A real deployed coordinator and TURN service are necessary for Internet use; no public endpoint is provided by this source tree. First host enrollment and the separately trusted encoder remain owner prerequisites.

## API additions

All endpoints retain existing TLS, strict JSON, bearer, storage-health and rate-limit rules. Codes and PINs are sent only in HTTPS JSON bodies. Controller authentication is the existing P-256 challenge/proof exchange. Pairing records are in memory only and disappear on coordinator restart.

### Create or replace a code

`POST /v1/hosts/{hostId}/pairing` with the host bearer:

```json
{"pin":"012345","permissions":{"viewScreen":true,"controlInput":true,"clipboard":false,"fileTransfer":false,"audio":false,"power":false}}
```

Success: HTTP 201:

```json
{"pairingId":"opaque-random-id","code":"012345","expiresAt":"RFC3339 timestamp","permissions":{"viewScreen":true,"controlInput":true,"clipboard":false,"fileTransfer":false,"audio":false,"power":false}}
```

The host retains this exact pairingId, expiration and permission ceiling **in memory before displaying the code**. Successful replacement clears the previous local grant. Neither the response nor the host status endpoint returns the PIN. Permission requests outside view/input are rejected.

`GET /v1/hosts/{hostId}/pairing` returns `{"pairing": null}` or `{"pairing": {"pairingId": "...", "code": "012345", "expiresAt": "...", "permissions": { ... }}}` for the current unused valid code, authenticated as that host. This supports visible status refresh only; it cannot recreate local auto-approval after process restart.

`DELETE /v1/hosts/{hostId}/pairing` revokes the current code and returns HTTP 200 `{"ok":true}`. The host clears its in-memory grant immediately on local revocation. Closing the host clears local authorization and attempts a bounded remote revocation. Revoking an unused code is separate from stopping a session already accepted; the existing visible session stop control remains authoritative.

### Redeem a code

`POST /v1/pairings/redeem` with an authenticated controller bearer:

```json
{"code":"012345","pin":"012345","controllerName":"My phone"}
```

Success: HTTP 201, the existing session-creation shape plus host display details:

```json
{"session":{"id":"...","status":"pending","hostDisplayId":"123456789","hostPublicKey":"...","hostName":"Office PC","pairingId":"opaque-random-id","permissions":{"viewScreen":true,"controlInput":true,"clipboard":false,"fileTransfer":false,"audio":false,"power":false}},"controllerToken":"..."}
```

`hostName` is a field on this response's `session` and subsequent session snapshots. Remaining Session fields keep their existing definitions. Pairing redemption creates the same bounded pending session and `session.requested` host event, with `pairingId` on the Session and event payload. It atomically consumes the code and observes host availability, controller revocations and all existing session capacity rules. A wrong guess never creates a session. Generic credential failure is HTTP 403 `{"error":"pairing_unavailable"}`; generic budget failure is HTTP 429 `{"error":"pairing_rate_limited"}`. Syntactically malformed JSON/fields may return 400 without disclosing code existence. Server capacity/storage errors retain 503. Existing non-pairing session requests cannot set pairingId.

### Host acceptance

The host auto-approves only a pending session whose nonempty pairingId matches its current unexpired, unused local grant, with a permission subset of that grant. It consumes the local grant before starting capture or approving the server session. Any unmatched pairing session is rejected, including after restart. This is a one-session local authorization created by the owner's Generate code action, not an unattended-trust entry. All ordinary session requests keep their current approval policy. Existing display/input binding, visible disconnect, held-key cleanup and teardown gates remain in force.

Monitor switching through a fresh session is not silently granted by a consumed code. Clients must label any later ordinary request as requiring fresh host approval. Pairing clients initially select the primary display and do not send monitorId during redemption.

## Discovery and desktop controller

`GET /v1/info` additionally advertises `pairingCodeVersion: 1` and `webController: true`; existing protocol and phone setup versions remain 1. Version is 0.6.0.

The coordinator serves a first-party desktop controller at `/connect` and static module/style files below `/connect/`. It has no third-party scripts, service worker, analytics or persistent credentials. A restrictive CSP, no-store, no-referrer and framing protections apply. It authenticates with a fresh browser P-256 key, redeems code/PIN, then uses the existing polling, ICE, SDP and control-data protocols. Media and input are enabled only after permission, peer connection, fresh decoded video and display identity gates. Disconnect, page hide and lost focus release held input; connection loss disables input and ends within a bounded interval.

## Required evidence

Meaningful regression cases cover uniqueness, leading zeroes, validation, expiry, replacement/revocation, wrong PIN lockout, budgets, concurrency/one-use, revoked controllers, offline hosts, permission ceilings, local host grant matching/consumption/restart, and client cancellation/late results. Browser and Android contracts must agree with actual coordinator handlers. Build/test evidence must identify exact final source and artifact hashes and separately state native Windows/Android and real Internet verification gaps.
