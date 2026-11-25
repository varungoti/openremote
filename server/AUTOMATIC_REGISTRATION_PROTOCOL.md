# Automatic host registration, version 1 (OpenRemote 0.7)

This protocol removes enrollment-secret entry from the Windows host. It is enabled
only when the operator sets `OPENREMOTE_AUTOMATIC_HOST_REGISTRATION=true`; the
default remains disabled for existing private deployments. The publisher also
sets `OPENREMOTE_PUBLIC_ORIGIN` to the canonical HTTPS service origin; it is
configured once by the deployment generator, never entered in the host UI. The
server's existing administrator/enrollment secret remains server-side and is
never embedded in a client. This protocol does not supply a deployed service address.

The origin must be a canonical HTTPS origin with no trailing slash, explicit
default port, credentials, path, query, fragment or escaped spelling. The only
HTTP exception is an explicit loopback origin with `AllowInsecure` enabled for
development/tests; forwarded request headers cannot enable that exception.

## Challenge

`POST /v1/hosts/register/challenge`, without a bearer:

```json
{"publicKey":"standard padded base64 of 32 raw Ed25519 public-key bytes","name":"Office PC","platform":"windows"}
```

The key must use canonical standard Base64. The name is trimmed, 1–80 UTF-8 bytes,
and contains no control characters. Platform is exactly `windows`.

HTTP 201 returns `challengeId`, `challenge`, `expiresAt`, and `apiOrigin`. The challenge ID is
24 base64url characters without padding, derived from 18 random bytes. The nonce
is 32 random bytes. The challenge expires after 60 seconds. `challenge` is
standard padded Base64 of these **exact UTF-8 bytes**:

```text
OpenRemote.HostRegistration.v1\n<apiOrigin>\n<challengeId>\n<nonceBase64>\n<publicKeyBase64>\n<name>\nwindows\n<expiresAt>\n
```

Here `\n` means one byte `0x0A`, not a backslash and letter. There are eight
nonempty lines followed by a final newline; splitting at newline produces nine
items including the final empty item. `nonceBase64` is canonical standard padded
Base64 of the nonce. `expiresAt` is the exact response string in RFC3339 UTC form.
The API origin comes only from validated server configuration, never a request
Host or forwarded header. No BOM, additional spaces, carriage returns or other
bytes are present.

Before signing, the host bounds the decoded payload, checks this purpose, validates
the nonce, and verifies the exact API origin against its canonical bound/profile
origin and response `apiOrigin`, challenge ID, its own public key, requested name,
platform and response expiration. It never signs arbitrary server-provided bytes.

## Proof and recovery

Sign the decoded challenge bytes with **pure Ed25519**, without prehashing. Submit:

`POST /v1/hosts/register`

```json
{"challengeId":"the returned challenge ID","signature":"standard padded base64 of the raw 64-byte signature"}
```

The response is the existing enrollment shape: `{"device": {...}, "deviceToken":
"..."}`. A new registration returns HTTP 201. Successful identity recovery returns
HTTP 200. Both contain the stable opaque device ID, nine-digit routing ID, public
key, name and a fresh bearer token. The new six-digit pairing code remains a
separate owner-generated one-session authorization.

Each challenge is consumed once, including a failed signature attempt. A public
key alone never allocates or recovers a device. On an ambiguous network result,
the client keeps its private identity and requests a fresh challenge. A valid
new proof recovers the same record **only** when it was automatically registered
and has not been revoked. Recovery atomically rotates the bearer, invalidates the
previous token, ends its sessions, and revokes unused pairing codes. The signed
name updates that record. It creates no new unattended trust or pairing grant.

Every challenge also binds the current device/bearer generation privately on the
server. Successful registration or recovery makes any earlier outstanding proof
stale; it cannot overwrite the newly issued bearer. A stale proof receives the
same HTTP 401 as an invalid proof. The client can request a fresh challenge for a
bounded, serialized retry. Clients must not run concurrent recovery loops.

Existing administrator-enrolled devices remain usable with their existing bearer
but cannot be recovered through this public protocol. Revoked device identities
remain tombstoned and cannot be registered again, including through the legacy
enrollment endpoint. The client must never delete or replace its signing identity
to work around a rejection. Host startup may recover a rejected stored bearer by
proving possession of the same key at the same bound service origin; a running
session must not continuously rotate credentials.

## Errors and discovery

- HTTP 403 `automatic_host_registration_disabled`: feature is off.
- HTTP 401 `invalid_host_registration_proof`: bad, expired, consumed, stale or unknown proof.
- HTTP 403 `host_registration_unavailable`: proved identity is legacy or revoked.
- HTTP 429 `host_registration_rate_limited`: a source/global budget was exceeded.
- HTTP 503: challenge/device capacity or durable storage is unavailable.
- HTTP 400: malformed JSON, noncanonical public-key shape, forbidden query or invalid fields.

`GET /v1/info` advertises `automaticHostRegistrationVersion: 1` when enabled and
`0` when disabled; software version is `0.7.0`. TLS, strict JSON, no-store, source
address budgets and the durable storage-health latch apply to all routes.

Default additional budgets are 10 challenges/minute/IP, 120 challenges/minute
globally, 10 verification attempts/minute/IP, 60 verification attempts/minute
globally and 6 proved registrations/recoveries/minute/key. The existing shared
30 sensitive requests/minute/IP limit also applies. There can be at most 1,000
outstanding registration challenges and 1,000 total host records. Rate errors
include `Retry-After: 60`; these counters and challenges are ephemeral.

## Revocation and durable state

`POST /v1/admin/hosts/{opaque device ID}/revoke`, with the server's administrator
enrollment bearer and `{}` body, durably tombstones the host, ends sessions and
pairing codes, and rejects subsequent host bearer authentication and proof-based
recovery. The endpoint is idempotent. Administrator routes are not part of the
ordinary host or phone UI, and no administrator credential is shipped to clients.

Automatic-registration ownership and revoked-state flags are persisted with each
device in snapshot version 2. Legacy version 1 state is accepted only without
those privileges and is interpreted as administrator enrollment, never as
permission for public recovery. The next durable mutation writes version 2.
Keep a pre-upgrade backup; an older coordinator cannot consume the new snapshot,
and stripping the ownership or revocation fields is not a safe downgrade.
Challenges, raw bearer tokens, nonces and proofs are not persisted. A failed
durable write returns no credential and latches the service into HTTP 503 until
the storage fault is resolved and the coordinator restarts.
