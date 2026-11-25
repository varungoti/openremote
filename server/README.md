# OpenRemote coordinator

This is a dependency-free Go HTTP coordinator for the Windows and Android clients.
It provides routing, authentication, authorization, signaling and audit metadata.
It does **not** carry desktop video or prove that a native video connection worked.

## Run

Use the verified Go 1.27.4 toolchain or a reviewed compatible security update.
`go.mod` requires Go 1.24 or newer for standard-library PBKDF2. Exact compiler,
test, source-input and artifact evidence is recorded in `BUILD_EVIDENCE.json`.

Version 0.6 adds one-use six-digit code/PIN pairing and an embedded browser
viewer at `/connect`. See [the shared contract](../docs/code-pin-contract.md).
The viewer and API use the same HTTPS origin; no cross-origin credential policy
or separate static host is needed. Pairing verifiers are ephemeral and never
enter the durable state snapshot. The default grant expires after ten minutes
and locks after five wrong PINs. Pairing creates a pending session: the running
Windows host must separately consume its owner-created local grant.

Version 0.7 adds automatic host registration through an origin-bound Ed25519
proof. The publisher configures the managed service once and distributes clients
bound to that service. The Windows user enters no server URL, enrollment secret
or relay configuration. Public registration remains disabled by default for
existing private deployments. See [the registration protocol](AUTOMATIC_REGISTRATION_PROTOCOL.md).

Version 0.8 adds owner-controlled active hosting. The permanent nine-digit PC ID
stays the same; a locally entered six-digit PIN enables a reusable activation for
up to 24 hours while the host runs. Up to four pending or live sessions may share
that activation. One session reserves input; additional sessions receive screen
viewing only, and existing viewers never acquire input automatically. Stop,
replacement, PIN lockout and expiry revoke the activation's sessions. Legacy
one-use invitations keep their previous wire format and behavior. See
[the activation protocol](ACTIVATION_PROTOCOL.md).

```sh
go test -race -cover ./...
go vet ./...
go build -trimpath -o controlplane ./cmd/controlplane
```

Production startup requires a server-side random administrator/enrollment secret
and either native TLS or an explicitly trusted reverse proxy. This is publisher
configuration, never a credential to distribute to customers. TLS certificates must cover the actual API
hostname. For local development only, set `OPENREMOTE_ALLOW_INSECURE=true`.
The default listener is loopback; Docker's image explicitly listens on all
container interfaces. Keep the backend port inaccessible outside its proxy.

| Variable | Meaning / default |
| --- | --- |
| `OPENREMOTE_LISTEN` | `127.0.0.1:8080` outside Docker |
| `OPENREMOTE_STATE_FILE` | `./data/controlplane.json`; Docker `/data/controlplane.json` |
| `OPENREMOTE_ENROLLMENT_TOKEN` | Required, at least 32 randomly generated characters |
| `OPENREMOTE_AUTOMATIC_HOST_REGISTRATION` | Exactly `true` enables possession-proved public host registration; disabled by default |
| `OPENREMOTE_PUBLIC_ORIGIN` | Required with automatic registration: canonical HTTPS API origin, without trailing slash; signed into every challenge |
| `OPENREMOTE_TLS_CERT`, `OPENREMOTE_TLS_KEY` | Native TLS PEM files, both required together |
| `OPENREMOTE_TRUSTED_PROXY_CIDRS` | Comma-separated exact proxy network ranges; no default trust |
| `OPENREMOTE_ALLOW_INSECURE` | Exactly `true` enables development HTTP |
| `OPENREMOTE_STUN_URLS` | Comma-separated `stun:` / `stuns:` URLs; no public default |
| `OPENREMOTE_TURN_URLS` | Comma-separated `turn:` / `turns:` URLs |
| `OPENREMOTE_TURN_SECRET` | coturn shared REST secret, minimum 32 characters |
| `OPENREMOTE_RELAY_ONLY` | Exactly `true` forces relay ICE policy; TURN must be configured |

The proxy must overwrite `X-Forwarded-Proto` and append the actual client address
to `X-Forwarded-For`. Only configured proxy peers can influence these headers;
untrusted clients cannot bypass the TLS requirement or per-IP limiter by adding
their own forwarding headers. Do not trust broad client-facing network ranges.

## Controller identity proof

Before sending a phone identity, the Android connection-check flow calls public
`GET /v1/info`. This reports service/protocol/setup versions, monitor selection
support and nonsecret relay configuration flags. It has the same HTTPS, storage
and rate-limit checks as the other routes, creates no login/session state and
returns no host or relay addresses/credentials. A successful response is not a
TURN allocation or desktop/media test. See [the setup contract](../docs/phone-setup.md).

The 0.5 coordinator rejects ambiguous or unsupported ICE URLs during startup:
lowercase schemes, ASCII DNS/IP host, optional port 1–65535, no path/credentials/
fragment/percent escapes, no STUN query, and at most one literal TURN transport
(`udp` or `tcp`). `turns:` uses TCP only in this implementation. Correct any
previous malformed configuration before upgrading. Valid existing deployment
examples are unchanged.

A public key string alone never authorizes a controller.

1. Generate an ECDSA P-256 key in Android Keystore. Export the public key as
   X.509 SubjectPublicKeyInfo DER, encoded with standard padded Base64.
2. `POST /v1/auth/challenge` with `{ "publicKey": "...", "algorithm": "p256" }`.
   The `201` response is `{ "challengeId", "challenge", "expiresAt" }`.
3. Base64-decode `challenge`. Sign **those raw bytes** with `SHA256withECDSA`.
   The signature is ASN.1 DER, not a 64-byte raw r/s concatenation.
4. `POST /v1/auth/verify` with `{ "challengeId", "signature": "<base64 DER>" }`.
   The `200` response contains `accessToken`, `expiresAt` and canonical `publicKey`.
5. Use `Authorization: Bearer <accessToken>` for PC lookup and session creation.

Challenges expire after 60 seconds and are consumed on the first verification
attempt, successful or failed. Login tokens expire after 10 minutes. Login again
when necessary; an already authorized session uses its own separate token.
The coordinator derives the controller key from this proof. A different key
supplied in the session request is rejected.

## Host registration and ownership

Managed clients use `POST /v1/hosts/register/challenge` and then
`POST /v1/hosts/register`, signing the exact purpose-, origin-, identity-, name-
and expiry-bound challenge with their existing Ed25519 private key. The public
key alone grants no ownership. Only a valid one-use proof allocates the device
record and its private bearer. The host stores its identity and bearer using
DPAPI and user-only filesystem access.

After an ambiguous registration response or a rejected stored bearer, a fresh
proof from the same identity can recover the same automatically registered
record. Recovery rotates the bearer and ends previous sessions and pairing
grants. A delayed proof made before that rotation is rejected. Legacy enrolled
records and revoked records cannot be recovered through this endpoint, and the
client must preserve its identity when recovery fails. The wire format, canonical
origin rules and bounded retry policy are in the registration protocol.

An operator can revoke a host with `POST /v1/admin/hosts/{opaque ID}/revoke`,
the server-side administrator bearer and an empty JSON object. This durably
invalidates host authentication and public recovery, ends its sessions and
pairing grants, and retains an identity tombstone. Revocation is idempotent;
this version has no unrevocation endpoint.

### Legacy administrator enrollment

`POST /v1/devices/enroll` requires the enrollment bearer and a body with `name`,
`platform: "windows"`, and `publicKey` (standard Base64 of 32 raw Ed25519 bytes).
It returns a persistent 9-digit display ID, opaque device ID and device bearer.
A PC ID is only a routing alias. Enrollment keys cannot be overwritten by a
second registration; keep the device credential on the host.

The enrollment secret remains available to an operator's existing private
administration tooling. It is never embedded in a distributed installer or phone,
and the primary host UI uses automatic registration. The per-device credential
owns that PC's heartbeat, request inbox, session approvals, trust registry and
audit access. Existing legacy host credentials keep working after an upgrade.

## Session API

| Endpoint | Authentication / behavior |
| --- | --- |
| `GET /v1/devices/{9-digit ID}` | Controller login bearer; online is calculated from heartbeat age |
| `POST /v1/hosts/{opaque ID}/heartbeat` | Host bearer; call every 15 seconds |
| `GET /v1/hosts/{opaque ID}/events?after=N` | Host bearer; up to 20-second long poll |
| `GET /v1/hosts/{opaque ID}/sessions` | Host bearer; current own sessions only, bounded metadata for single-instance startup orphan cleanup; never bearer tokens or signaling payloads |
| `GET/POST/DELETE /v1/hosts/{opaque ID}/pairing` | Host bearer; inspect, create or revoke an owner grant; optional POST `mode: "activation"` enables bounded reusable hosting |
| `POST /v1/pairings/redeem` | Controller login bearer; `code` is a permanent nine-digit PC ID for active hosting, or a legacy six-digit invitation; requires PIN and claimed controller name |
| `POST /v1/sessions` | Controller login bearer; body `displayId`, `controllerName`, `permissions`; optional same `controllerPublicKey` and host-issued `monitorId` |
| `GET /v1/sessions/{ID}` | Host bearer or returned `controllerToken` |
| `POST /v1/sessions/{ID}/approve` | Host only; optional `{ "permissions": { ... } }` may only narrow requested permissions |
| `POST /v1/sessions/{ID}/reject` | Host only, pending sessions only |
| `POST /v1/sessions/{ID}/signal` | Both session peers, only after approval |
| `GET /v1/sessions/{ID}/signals?after=N` | Both peers; only the other peer's signals are returned |
| `GET /v1/sessions/{ID}/ice` | Approved peers; short-lived signed TURN credentials and configured STUN |
| `POST /v1/sessions/{ID}/connected` | Peer reports real WebRTC connected state after offer/answer |
| `POST /v1/sessions/{ID}/end` | Either peer; idempotent, stops further signaling |
| `GET /v1/hosts/{ID}/audit` | Host bearer; latest 200 events for that host |
| `GET/PUT/DELETE /v1/hosts/{ID}/trusted` | Host bearer; persistent trusted keys, label, granted permissions |

Windows should always send an explicit subset to `/approve`. Empty approval
bodies retain the requested permissions for protocol compatibility. No request
may grant control without screen access. The host must enforce the actual
effective permissions at its input/clipboard/file handlers.

The activation mode adds optional `mode: "activation"` to its sessions and
request events. Its creation/inspection response also includes
`maxConnections: 4`. Discovery adds `activationVersion: 1` and
`maxActivationConnections: 4`. The server preserves the old single-session
approval behavior for legacy sessions and keeps legacy/trusted requests separate
from a currently enabled activation. The permanent PC ID is a public locator,
and a controller's name is a display claim; authentication proves possession of
the controller key rather than ownership of the claimed name.

`monitorId`, when supplied, must contain exactly 64 lowercase hexadecimal
characters. Omit it to request the primary monitor; null, an empty string and
other types are rejected. The selector is echoed unchanged to both authorized
session actors and the host's request event, and cannot be replaced at approval.
It grants no permission. The host rejects unknown/stale selectors against its
current topology, while the controller checks the selected catalog entry before
enabling input. See [display selection](../docs/display-selection.md). Upgrading
the coordinator before either 0.4 client is required for selected-monitor sessions.

State progression is `pending → approved → connecting → active → ended`.
Rejection and expiry are terminal. The initial offer belongs to the controller;
an answer must come from the other peer after an offer. SDP exchange changes the
state to `connecting`, **not** `active`. A connected report is still only a peer
report; production monitoring must validate actual video/control behavior.

Pending requests expire after 90 seconds. Approved sessions expire after 2 hours.
One live session per host is allowed; three pending host requests and five
simultaneous sessions per controller are the default limits. A host can reject
requests locally, and it may auto-approve only an explicitly pre-trusted key.
Server-side trust alone never auto-approves a request. Deleting a trusted key
terminates its live sessions, posts host disconnect events, and persists a
revocation tombstone. That key cannot request new sessions on the host, even if
its local trust file still contains the key. Only an explicit host-authenticated
`PUT /trusted` clears the tombstone. Do not upload local trust automatically on
startup, because doing so would undo an intentional revocation.

A signal body is `{ "kind": "offer|answer|ice|renegotiate", "payload": {...} }`.
SDP uses `{ "type": "offer|answer", "sdp": "v=0..." }`. ICE uses `candidate`,
`sdpMid`, `sdpMLineIndex`, and optional `usernameFragment`. A restart notification
uses `{ "iceRestart": true }`. Queue cursors must be nonnegative and cannot be
ahead of the server. Host event numbers remain monotonic across ordinary server
restarts; event payloads remain ephemeral. Retain the last returned `cursor`.
`cursor_reset_required` means the supplied cursor is ahead of the available
sequence (for example, after restoring an older backup). `cursor_expired` means
history was compacted or an older snapshot was migrated. Both errors return `resetAfter`;
resume at that cursor and revalidate each retained session before requesting consent.
Expired/missed sessions cannot be approved.

## Storage and operating limits

The durable backend is a **single Linux coordinator**, not a distributed database.
It atomically replaces and fsyncs a private `0600` JSON snapshot, including device
token hashes, explicit automatic-registration ownership, host revocation tombstones,
monotonic host event numbers, host-controlled trust entries, controller
revocations and bounded audit history. A kernel
file lock prevents two processes from writing the same state file. Closing or
crashing releases the lock automatically. Keep the directory private and mount
it as a persistent volume; back it up as confidential identity data.

Version 0.7 reads legacy snapshot version 1 as administrator-enrolled hosts and
writes snapshot version 2. It rejects duplicate raw identity keys and inconsistent
ownership metadata. Keep a pre-upgrade backup: older coordinators cannot read
version 2, and deleting its ownership/revocation metadata is not a safe downgrade.

Sessions, SDP, ICE, pending challenges and controller login tokens are intentionally
memory-only. Restart invalidates them. Device IDs, device token hashes, host
event numbers and trust survive restart; devices are offline until they heartbeat again. Audit history
retains the most recent 20,000 events. Export longer-term audit history before
rotation if required. This backend does not provide HA or tamper-proof archival.

Storage failure latches the coordinator into a `503` state. Fix storage and
restart; a failed audit write is never treated as a successful authenticated
mutation. Already-admitted polls also stop on the storage latch, so they cannot
publish an uncommitted host event number. Requests are capped at 300 KiB, SDP at 256 KiB, each session's signal
payloads at 1 MiB, total signaling at 32 MiB, and each queue at 512 entries. JSON
duplicate keys, unknown fields, trailing documents and excessive nesting are
rejected. Rate limits use 300 requests/minute/IP overall and 30/minute/IP for
lookup, login, registration, administration and new sessions. Automatic registration
also limits challenges to 10/minute/IP and 120/minute globally, verification
attempts to 10/minute/IP and 60/minute globally, and successful ownership proofs
to 6/minute/key. The challenge queue is capped at 1,000 entries with a maximum
60-second lifetime; the shared device registry is capped at 1,000 records.
Rate failures return `Retry-After: 60`. Large shared NAT deployments will need adjusted
limits after load testing. Response I/O is buffered outside the state lock so a
slow remote reader cannot block other hosts.

## Verification and remaining trust boundary

The automated suite verifies actual P-256 and Ed25519 signatures, purpose/origin
binding, copied-key rejection, replay and stale-proof rejection, recovery with
stable identity, revocation across restart, legacy ownership migration, session
states/roles, permission narrowing, expiry, presence, persistent restart, token
hashing, signed TURN credentials, malformed payloads, rate limiting, TLS policy,
concurrent access under Go's race detector, canceled polls, failed storage, and
exclusive writer locking. The independent project integration script runs the
binary over HTTPS and restarts the process.

The control plane is currently a trusted authorization component. WebRTC encrypts
peer traffic, including through TURN, but the native clients do not yet implement
independently signed SDP with out-of-band identity verification. Do not describe
this version as resistant to a malicious/co-opted coordinator. Native Windows and
Android tests, real TURN forwarding, NAT/firewall coverage, production load tests,
certificate/key operations and third-party security review remain release gates.
The provided PostgreSQL migration is an architecture draft; it is not the active
storage engine and is not run by this binary.
