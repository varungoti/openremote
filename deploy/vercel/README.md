# OpenRemote on Vercel

This project publishes the actual OpenRemote browser controller and a stateless
HTTPS gateway. It does not replace the durable Go coordinator or the coturn
Internet relay. A deployed viewer is not proof of a working remote session.

## Current deployment status

The `openremote` project exists in `varubs-projects`. Production attempt
`dpl_Dmf2psGm2bR33KLaWSUdgGdYNR6s` was blocked before build by this exact policy:
**"integration deployments are not allowed in production. Only CLI deployments
are allowed."** No authenticated CLI session is available in the workspace.
The source policy remains unchanged. The assigned
`https://openremote-varubs-projects.vercel.app` address is not a live application.
The retained receipt is `evidence/vercel-deployment-0.8.0/deployment-attempt.json`.

Resolve publication using an authenticated CLI under the allowed policy, or an
explicitly authorized project-level change allowing the integration deployment.
Rechecking the same blocked integration request without that change is not a fix.

## Build and verify

Use Node 24 from the repository root:

```sh
cd deploy/vercel
npm ci --ignore-scripts --no-audit --no-fund
npm run build
npm test
```

The build copies the production controller, protocol decoder and CSS unchanged
from `server/internal/webclient/assets`. It adds a readiness gate to the HTML.
Generated assets stay in the Function bundle; the static `public` directory is
empty. Every viewer request passes explicit URL/method checks, no-store and the
same CSP as the coordinator viewer. No query-based credential import exists.

Configure the Vercel project with root directory `deploy/vercel`, Node 24,
framework Other, and source files outside the root directory enabled. The
checked-in `vercel.json` contains the build, Function and routing configuration.
Keep preview/deployment URLs protected and expose the stable production alias
for the customer app. Do not point installed clients at an expiring preview URL.

## Default published behavior

Without both publisher settings below, recognized API routes respond with
JSON HTTP503 `service_not_configured`. The page performs discovery before
accepting a PC ID or PIN and leaves those fields disabled when unavailable.
The primary button retries discovery only; no identity, authentication or PIN
redemption occurs on that path. No fake PC ID is generated.

`/deployment-status` reports gateway configuration separately from discovery.
It never reports verified remote access. `/healthz` is an upstream health
request and remains503 when no coordinator is configured.

## Connect the durable coordinator

These are **publisher deployment settings**, never fields in the customer UI.
Set them only after the intended coordinator and relay have been deployed and
verified:

| Vercel setting | Value |
| --- | --- |
| `OPENREMOTE_COORDINATOR_ORIGIN` | Fixed canonical HTTPS origin of the durable coordinator, without a trailing slash, path, query, credentials or fragment |
| `OPENREMOTE_GATEWAY_TOKEN` | Publisher-generated random secret, 32–512 printable non-space ASCII characters; identical to the coordinator setting |

The coordinator must use this branch's authenticated gateway support. Set its
`OPENREMOTE_GATEWAY_TOKEN` to the same secret and `OPENREMOTE_PUBLIC_ORIGIN` to
the **stable client-facing Vercel origin**, not the upstream origin. Registration
proofs bind to that public origin. Configure automatic host registration, real
TURN URLs/credentials, persistent protected state, TLS and the exact trusted
immediate proxy if a TLS terminator is used. Never trust all proxy addresses.

The Function reads the platform-owned `x-vercel-forwarded-for` only when running
on Vercel, normalizes a single IP literal and authenticates that address to the
coordinator with the gateway secret over verified TLS. The Go coordinator must
verify that proof before assigning rate-limit buckets. It still independently
requires host/controller authentication, the PIN, permissions and session
admission. Customer-supplied forwarding headers and gateway headers are never
passed through. A missing verified client address fails closed.

The gateway preserves raw JSON and authorization bytes. Only supported public
protocol paths are forwarded. Administrative enrollment/revocation and arbitrary
proxy destinations are not exposed. It never retries a PIN or other mutation,
follows redirects, forwards cookies, or logs credentials/request bodies.

Successful `/v1/info` replies pass the production strict JSON/protocol decoder
and require activation 1 / capacity 4, automatic registration 1 and declared TLS TURN
configuration. This policy applies to native, browser and direct agent discovery
without adding fields to the Android discovery schema. It checks configuration;
it does not establish relay reachability or desktop media delivery.

The upstream deadline is 28 seconds around the coordinator's 20 second polling
window. Requests are bounded at 307,200 bytes and responses at 4 MiB. Do not lengthen
the coordinator poll beyond this contract without changing the clients and
gateway together. Responses are never CDN cached.

## Release boundary

The coordinator still owns live auth, PIN grants, sessions, signaling and input
reservations in one process. Its JSON snapshot is not a distributed transaction
store. Running that executable in independently scaled Functions would split
those authorities. Keep it on a durable single host until a deliberate shared
state migration is implemented and tested. The TURN relay also needs inbound
network listeners and a relay port range outside the HTTP gateway.

After infrastructure is active, verify signed automatic registration against the
public origin, separate-client rate limits, forced TURN over mobile data,
desktop video/input, reconnection, tray roster/count and revocation. Then bind
`config/service-profile.json`, rebuild and sign Windows/Android releases, and
test installs/updates on the actual devices. An empty native profile remains a
hard installer release failure; a live website does not remove that gate.

Current platform references reviewed on 20 November 2026:

- https://vercel.com/docs/functions/runtimes/node-js
- https://vercel.com/docs/headers/request-headers
- https://vercel.com/docs/daemons/limits
- https://vercel.com/docs/functions/runtimes/container

The new Vercel container/WebSocket/Daemon features were considered. Daemons do
not accept inbound HTTP/TCP or persistent local storage and cannot be rewrite
targets; they are not a direct home for this coordinator or coturn.
