# Private deployment and operational runbook

This is the deployment procedure for the current **single-node Linux** coordinator.
It is a prepared runbook; the Docker/coturn runtime and public Internet path have
not been deployed or validated in this workspace.

## Required resources

- A Linux server with a public IPv4 address, Docker Engine and Compose v2.
- An API hostname pointing to the coordinator's public address.
- A TURN hostname pointing to the relay's public address.
- A valid TLS certificate/private key for the TURN hostname.
- A second public IP/host if TURN/TLS must listen on 443: Caddy already occupies 443
  on the API host. On one address use the TURN TLS port 5349; that does not promise
  connectivity from networks that only permit 443.

Do not expose the Go API's 8080 port directly. Only Caddy is published. The
coordinator trusts forwarded HTTPS/client addresses only from its configured
Caddy container address. Review the 172.30.80.0/29 subnet for collisions with your
own Docker/VPN networks before deployment, changing both the Compose addresses
and the exact trusted proxy CIDR together if needed.

## Configuration

Run `scripts/configure_deployment.py` with real domain names, a public IPv4 address
and the ACME contact email. It generates `.env`, `secrets/controlplane.env`, and
`secrets/turnserver.conf`. The private directory is 0700 and files 0600. It never
prints credentials or overwrites existing configuration.

The same randomly generated shared secret appears in the coordinator's private
TURN setting and the private coturn config. Phones receive only short-lived
HMAC credentials for approved sessions. The separate administrator/enrollment bearer
stays on the server and is never entered on the ordinary client or embedded in an
installer. The 0.7 generator enables `OPENREMOTE_AUTOMATIC_HOST_REGISTRATION=true`
and sets `OPENREMOTE_PUBLIC_ORIGIN` to the canonical HTTPS API origin. Fresh hosts
prove possession of an origin-bound Ed25519 key instead of receiving that secret.
This registration path is opt-in; see `server/AUTOMATIC_REGISTRATION_PROTOCOL.md`.

Before publishing clients, configure `config/service-profile.json` with this real,
validated origin using `scripts/configure_service_profile.py` and rebuild both
apps. The current checked-in profile is intentionally empty because no deployment
was supplied. Do not ask end users to enter infrastructure fields: a configured
build supplies them. Windows managed packaging checks live HTTPS discovery;
actual TURN/media paths still need the release validation below.

Place the relay certificate chain at `secrets/turn-cert.pem` and its key at
`secrets/turn-key.pem`. Use the same file owner recorded as `TURN_UID`/`TURN_GID`
and restrict the key to 0600. Coturn runs under that explicit account so it can
read the private mount; `/tmp` is writable for its PID/database. If moving the
relay configuration to another machine, update the UID/GID to that machine's
chosen dedicated account and verify file ownership. Do not make secrets
world-readable to address a mount permission error.

From the repository root on the API host:

```sh
docker compose --env-file .env config --quiet
docker compose --env-file .env up --build -d
```

On the relay host, with its private config/certificate/key and `.env` present:

```sh
docker compose --env-file .env -f infra/compose.turn.yml config --quiet
docker compose --env-file .env -f infra/compose.turn.yml up -d
```

Firewall rules must allow API TCP 80/443 (UDP 443 is optional for HTTP/3), relay
UDP/TCP 3478, relay TCP 5349 or the selected TLS 443 port, and UDP 49160–49259 for relay
allocations. The relay must be able to send UDP to public peer candidates.
Docker host networking is used only for coturn on Linux. No PostgreSQL or Redis
service is started because this version does not use them; the SQL file under
server/migrations is a design draft.

## Storage and recovery

The coordinator's `/data/controlplane.json` is an atomic, fsynced private snapshot.
It stores PC identity, hashed host tokens, controller trust/revocation and bounded
audit history. A kernel lock allows a single writer. Run one coordinator replica
on a local filesystem. Shared network filesystems/multi-replica writes are not a
supported deployment.

Back up the data volume and private deployment configuration to encrypted storage.
Test restoring them on an isolated instance before depending on the backup. The
PC numbers and host credentials survive a restart. Active sessions, SDP/ICE queues
and short-lived phone login tokens do not; clients reconnect and reauthenticate.
The host inbox has explicit cursor reset behavior after restart/compaction.

The host protects its identity and enrollment bearer with current-user Windows
DPAPI under `%LOCALAPPDATA%\OpenRemote`. Moving only these files to a different
Windows account does not transfer the ability to decrypt them. Upgrading the host
preserves them. If a PC is re-enrolled, record the new PC number and replace the
saved phone entry.

## Validation before using the deployment

Upgrade the coordinator before installing the 0.5 phone/host clients. Its public
`GET /v1/info` route supplies compatible setup discovery without revealing relay
credentials or host details. The phone's **Check connection** action verifies that
response, phone authentication and optional PC availability. The relay flags are
configuration indicators; the actual candidate and moving-video tests below
remain mandatory. Malformed/ambiguous ICE URI values now fail coordinator startup;
see `docs/phone-setup.md` for the supported syntax.

1. Verify API HTTPS with certificate checks enabled. Confirm raw 8080 is not public.
2. Enroll a Windows PC and connect a phone after local approval. Verify actual
   desktop movement, click, drag, scroll and keyboard text.
3. Force relay-only ICE at the coordinator and verify selected relay candidates in
   the clients' diagnostics; a successful TCP connection to 3478 alone is not a
   desktop test.
4. Test TURN/UDP, TURN/TCP, then TURN/TLS using real certificates. Record the selected
   relay transport and frames/input delivered for each.
5. Test different networks, carrier NAT, Wi-Fi↔mobile-data change, phone background,
   loss/recovery, revocation, local emergency disconnect, and a session longer
   than the 10-minute TURN credential lifetime.
6. Verify that host input stops and held buttons/keys are released on every end,
   rejected request, local close, app close and failed network recovery.
7. Restart the coordinator. Confirm PC identity persists and the old session
   cannot be reused.

Do not include screen contents, clipboard text, input text or bearer tokens in
operational logs. The current audit stream records authorization/session metadata,
not desktop recordings. Watch disk failures: the coordinator fails closed if it
cannot persist an authorization mutation.

## Rollback

Stop accepting new sessions, disconnect active sessions, back up the state volume,
then deploy a previously verified binary and its matching supported state format.
Do not run old and new coordinators against the same writable state concurrently.
Do not silently roll back into the original unauthenticated foundation code.

Client rollback must retain the same enrollment and signing identity only when
the older build supports the stored format. A new Android signing key cannot
update an existing installed app. No auto-update or rollback agent is implemented
in the current host.

## Primary sources

- [coturn configuration and REST credentials](https://github.com/coturn/coturn/blob/master/examples/etc/turnserver.conf)
- [coturn Docker 4.19.1-r0](https://github.com/coturn/coturn/releases/tag/docker/4.19.1-r0)
- [Caddy reverse proxy](https://caddyserver.com/docs/caddyfile/directives/reverse_proxy)
