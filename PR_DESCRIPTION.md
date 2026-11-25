# Permanent PC ID, visible activation and agent access

The previous preview requested a PIN before revealing that no service was
configured, and displayed a short-lived invitation instead of a durable PC ID.
That made the user's phone/laptop workflow unclear and could never connect a
fresh install over the Internet. The host also lacked the requested tray roster
and named concurrent sessions.

This change makes startup admit the service and register identity before PIN,
displays permanent Your PC ID, and adds Activate/Deactivate with confirmed
tray hosting, names, states and active counts. Android and the browser accept
PC ID/PIN and preserve activation identity and permissions. The coordinator
supports a reusable 24-hour activation with four pending/live peers and exactly
one input reservation. Stop/lockout/replacement/expiry revoke the grant and its
sessions. Legacy one-use invitation contracts remain isolated and compatible.

A local stdio MCP bridge uses the production browser controller for authorized
agent screenshots and input. Fresh one-use observation tokens, current media,
permission checks, cancellation and sanitized errors bound each operation.
The Windows installer no longer permits an unconfigured-preview bypass.

## Validation

- Windows: 113 tests, format, Linux/Windows Clippy, optimized GUI cross-build and PE custody.
- Server: 72 top-level / 158 including subcases in normal and race runs, vet and static build.
- Android: 90 JVM tests, lint 0 errors/warnings, strict/offline four-ABI debug and minified unsigned release builds, artifact and guard checks.
- Browser: 17 core + 20 VM lifecycle tests; real coordinator HTTP 6 legacy + 6 activation checks.
- Verified HTTPS: 12 lifecycle/security + 8 registration/recovery checks against the compiled binary.
- MCP: 18 tests including real stdio protocol and EOF; browser interactions use explicit mocks.
- Configuration: 19 tests. Installer: 16 tests and actual empty-profile packaging rejection.

Exact source hashes, outputs, retained initial failures and independent reviews
are indexed in `evidence/activation-tray-0.8.0/verification.json`.

## Release limitations

The public profile is empty and no OpenRemote service is deployed for this
project. No customer installer was produced. Native Windows/Android operation,
real browser rendering, complete phone/laptop/agent desktop sessions and public
TURN/cellular connectivity remain unverified. The tray host is an interactive
application, not a Windows service or unattended reboot/UAC solution. Production
signing, complete dependency notices and advanced commercial scope remain open.

This description is prepared for review; no external pull request or deployment
has been created because no repository/deployment target was supplied.
