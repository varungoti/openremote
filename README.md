# OpenRemote — PC ID, PIN and visible connections

**0.8.0 engineering build. Customer release is blocked by missing public service,
native acceptance tests and release signing. No 0.8 customer installer is issued.**

**Vercel update, 20 November 2026:** the `openremote` project was created in the
connected account and its production viewer/gateway deployment was submitted.
Vercel blocked it before building: **"integration deployments are not allowed in
production. Only CLI deployments are allowed."** No authenticated Vercel CLI
session is available here. The deployment is **not live**; its generated URL is
not a working service. See [the deployment record](evidence/vercel-deployment-0.8.0/deployment-attempt.json)
and [Vercel implementation/runbook](deploy/vercel/README.md).

The intended everyday flow is now:

1. Open OpenRemote on the Windows PC. Its permanent nine-digit **Your PC ID** is
   displayed after automatic registration with the configured OpenRemote service.
2. Select **Activate Connection**, enter a six-digit PIN, then select the same
   action again or press Enter. After confirmed activation, the host moves to the
   system tray.
3. On Android or another PC, enter that **PC ID** and **PIN**, then connect.
4. Hover over the host tray icon for the active count. Open it to inspect the
   named connection list or choose **Deactivate Connection** to stop access.

The host has one primary Activate/Deactivate action. Its secondary **Connect to
a PC** action opens the configured browser viewer for receiving a remote desktop
on a laptop. Clients have no server-address, SSH-key or enrollment-key form.
The public service origin is a publisher build input, shared by Windows, Android
and the local AI-agent bridge. The browser viewer uses its own service origin.

## Why the previous installer could not connect

The screenshot's “not connected to the OpenRemote service” message was accurate:
the 0.7 preview shipped with an empty service profile. A PIN cannot create the
missing Internet coordinator or relay. The Vercel deployment attempt is blocked
by the account's source policy, no coordinator/relay is hosted, and the profile
remains empty.

The host now checks service availability before accepting a PIN and attempts
automatic registration before displaying a usable PC ID. A previously enrolled
PC can display its protected saved ID while offline, but cannot activate access.
The installer builder now **unconditionally rejects an empty service profile**;
the previous `--unconfigured-preview` installer bypass has been removed.

## Connection and agent behavior

- The nine-digit PC ID is a durable locator. It survives a coordinator restart
  and same-key registration recovery. Deleting/recreating the host identity or
  changing service can produce a different ID.
- The chosen six-digit PIN enables a process-scoped activation for up to 24 hours.
  It is reusable while that activation remains available, with four pending/live
  sessions at most. Stop, replacement, expiry and PIN lockout revoke its sessions.
- One session can control input; additional sessions are view-only. An existing
  viewer does not silently gain control when the controller leaves. It can
  reconnect with current authorization to request the newly free input slot.
- Names identify claimed client labels, not independently verified people. The
  roster distinguishes pending/connecting, active and reconnecting states.
- The AI interface is a local **stdio MCP bridge**, documented in
  [agent-bridge/README.md](agent-bridge/README.md). It uses the same PIN admission,
  production browser/WebRTC path and limits. Its host label starts `AI agent ·`.
  Screenshot observation tokens, current video and input permission are required
  for mouse or keyboard actions. It exposes no arbitrary script or shell tool.
- Old six-digit one-use invitation APIs remain compatible. The primary UI now
  uses the permanent PC ID. Activation grants enable screen viewing and optional
  input, not legacy clipboard, file-transfer, audio or power capabilities.

The tray host runs in the logged-in Windows desktop. It is **not a Windows SCM
service**, and this build does not provide unattended restart, lock-screen or
UAC secure-desktop control. Android is the native mobile client; there is no iOS
package. The receiving-PC client is a browser viewer, not a native video viewer.

## Build, evidence and deployment

Read [BUILD_STATUS.md](BUILD_STATUS.md), [the installation/activation guide](docs/windows-installer-0.8.0.md),
[the revised product plan](PRODUCT_PLAN.md) and [the activation protocol](server/ACTIVATION_PROTOCOL.md).
Exact source pins, commands, retained failures and artifact hashes are indexed by
`evidence/activation-tray-0.8.0/verification.json`.

The public service must be deployed for this project using
[the deployment runbook](docs/deployment.md). The publisher then configures its
real canonical HTTPS origin and rebuilds all clients:

```sh
python3 scripts/configure_service_profile.py --api-origin https://YOUR-REAL-HOST
```

That command does not provision infrastructure. The profile contains no
administrator, PIN or TURN secret. Configured Windows packaging checks verified
HTTPS discovery, automatic registration, activation capacity and TLS relay
configuration; those checks do not prove that a real video session works.

Representative verification commands on a suitable runner:

```sh
(cd server && go test -race -cover ./... && go vet ./...)
python3 -m unittest discover -s scripts/tests -v
python3 -m unittest discover -s packaging/windows/installer/tests -v
cargo test --locked --all-targets --manifest-path agent-windows/Cargo.toml
node --test tests/browser/controller-core.test.mjs
node --experimental-vm-modules --test tests/browser/controller-lifecycle.test.mjs
go build -C server -o artifacts/activation-pairing-fixture ./testsupport/pairing_fixture
node tests/browser/controller-api.integration.mjs
node tests/browser/activation-api.integration.mjs
(cd agent-bridge && npm ci --ignore-scripts && npm test)
```

Android engineering builds retain the preview signing identity and include
strict dependency verification, JVM tests, lint and four ABI variants. They do
not become Internet-ready until rebuilt against the deployed service. No native
Windows/Android remote session or public TURN/cellular session has been executed
here. Those are release gates, along with signing and complete dependency notices.
