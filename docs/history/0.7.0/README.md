# OpenRemote — one button, Server ID and PIN

**0.7.0 unconfigured development preview. Commercial release remains blocked.**

The Windows home has one primary action: **Connect remote desktop**. Choose a
six-digit PIN and the host prepares itself, registers automatically and shows a
six-digit Server ID. Android and the PC browser viewer ask only for **Server ID**
and **Security PIN**, followed by **Connect remote desktop**. The same action
becomes Stop/Cancel when appropriate.

There is no ordinary server-address, enrollment-key, fingerprint, encoder-path or
checksum form. The installer bundles its pinned Windows encoder. The publisher
configures the shared service once in the build; no administrator credential is
shipped to clients. The former Settings shortcut and Android setup-link/import
and Advanced paths are removed from the public flow.

**No public OpenRemote coordinator/relay has been provisioned for this project.**
The delivered shared profile is empty. A fresh preview install therefore shows
service unavailable; the phone disables Connect. These artifacts cannot yet
provide a fresh Internet session. Default end-user packaging rejects that empty
profile, with an explicit unconfigured-development exception used for these files.

Start with [the installation guide](docs/windows-installer-0.7.0.md),
[BUILD_STATUS.md](BUILD_STATUS.md) and [the product plan](PRODUCT_PLAN.md).

## What changed

- One persistent Windows window, one six-digit masked PIN field and one primary
  button. Preparation is cancellable; Stop/close clears authorization and ends
  access. Unconfirmed cleanup latches a restart requirement.
- Automatic enrollment through short-lived, origin-bound Ed25519 proof. Existing
  valid identity stays bound to its original service. Fresh proof can recover
  eligible same-key credentials; revoked or legacy registrations cannot be taken
  over through the public registration path.
- Android and browser home pages contain just Server ID/PIN and one current
  connection action. The Android preview retains its 0.6 signing identity.
- A minimal FFmpeg 9.0.2 Windows executable uses the OS Media Foundation H.264
  encoder. Hash verification, compatible framing arguments, corresponding source
  and exact notices are packaged. No external encoder setup is required.
- The deployment generator enables automatic registration with a canonical
  public origin; administrator and TURN secrets stay in private server files.

## Connection authorization

Server IDs are six-digit, one-session invitations, unique among active invitations
on the service. They expire after ten minutes and lock after five wrong PINs.
The host independently consumes its matching in-memory owner grant before
accepting a session. PINs are not persisted. Screen viewing and optionally input
are granted; clipboard, files, audio, power and permanent unattended trust are not.
New clients require exactly six ASCII PIN digits. Legacy server APIs retain
compatibility for previously supported 6–12 digit PINs.

Keep the host open in the interactive Windows desktop. Closing it stops access.
A consumed invitation cannot be reused for another session or monitor. The PC
receiving client is the service's `/connect` browser page, not a native Windows
viewer. No iOS package is included.

## Publisher deployment

Deploy and validate the project's HTTPS coordinator and TURN service using
[the runbook](docs/deployment.md). The automatic registration contract is in
[server/AUTOMATIC_REGISTRATION_PROTOCOL.md](server/AUTOMATIC_REGISTRATION_PROTOCOL.md).
Then set the shared public build input:

```sh
python3 scripts/configure_service_profile.py --api-origin https://YOUR-REAL-HOST
```

Rebuild both native apps after configuration. This command does not provision a
server. The profile contains no enrollment/admin secret, session PIN or relay
secret. Existing enrolled origins remain authoritative and are not silently
migrated. The browser uses its own origin. Configured Windows packaging verifies
live HTTPS discovery, automatic-registration and relay capabilities before
packaging; actual relay/media still require separate tests.

## Verification

The aggregate record is `evidence/one-button-0.7.0/verification.json`. It links
source hashes, commands, artifacts, retained failures and unexecuted checks.
Current component results and scope are in [BUILD_STATUS.md](BUILD_STATUS.md).

On a suitable authorized runner:

```sh
(cd server && go test -race -cover ./... && go vet ./...)
python3 -m unittest discover -s scripts/tests -v
python3 -m unittest discover -s packaging/windows/installer/tests -v
cargo test --locked --all-targets --manifest-path agent-windows/Cargo.toml
node --test tests/browser/controller-core.test.mjs
node --experimental-vm-modules --test tests/browser/controller-lifecycle.test.mjs
go build -C server -o artifacts/one-button-pairing-fixture ./testsupport/pairing_fixture
node tests/browser/controller-api.integration.mjs
```

Android default release packaging requires the configured service. The explicit
`-PopenremoteUnconfiguredPreview=true` exception permits unsigned development
inspection only and rejects injected production signing. Browser VM tests do not
render a browser; API tests do not capture pixels or inject input. Native Windows,
physical Android, public Internet/relay and production signing remain unverified.
