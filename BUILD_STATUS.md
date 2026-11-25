# OpenRemote 0.8.0 — build and release status

20 November 2026. **Engineering verification passed. Commercial release remains
blocked. No 0.8 customer installer has been issued.**

## Vercel deployment follow-up

The connected Vercel team now has an `openremote` project configured for the
Node 24 viewer/gateway implementation. The production attempt
`dpl_Dmf2psGm2bR33KLaWSUdgGdYNR6s` is **BLOCKED**, before any build events.
Vercel's exact reason is: **"integration deployments are not allowed in
production. Only CLI deployments are allowed."** There is no authenticated
Vercel CLI session in this workspace. The deployment-source policy was not
changed. The assigned URL must not be presented as a live application.

The implemented gateway serves the real browser controller, checks discovery
before accepting credentials, and requires fixed HTTPS upstream configuration
plus an authenticated gateway/client-address boundary. The Go addition preserves
independent TLS, host/controller/PIN and permission checks while keeping client
rate limits separate behind the gateway. Discovery capability checks also apply
to native and direct-agent clients without changing the discovery schema.

The Vercel-specific evidence is under `evidence/vercel-deployment-0.8.0/`.
Forty gateway/viewer tests passed. The six new Go gateway regression tests,
`go vet ./...` and the full `go test -race ./...` suite also passed. The retained
before-fix test demonstrates the shared-egress HTTP429 failure. A freshly
compiled Go process and the actual
Node gateway passed ten grouped runtime checks over 66 certificate-verified
HTTPS connections, including public-origin-bound registration, P-256 controller
authentication, PIN activation, SDP signaling, stop and independent rate limits.
These checks use synthetic platform IP headers and do not establish public
hosting, native video/input, phone behavior or TURN forwarding.

The coordinator and TURN relay still need durable hosting outside this HTTP
gateway. `config/service-profile.json` remains empty. No new configured Windows
installer or Android release is claimed. The older 0.8 verification below remains
historical evidence for its pinned source; it does not cover the new Go changes.

## What changed

The Windows host now checks the configured service and registers its identity
before PIN entry. It displays permanent nine-digit **Your PC ID**, replacing the
old temporary invitation as the main locator. Select **Activate Connection**,
enter a six-digit PIN and confirm with the same action or Enter. Confirmed
activation moves the host to its native tray icon. The tooltip shows the active
count; opening the icon shows claimed device names, connection states and
input/view-only permission. Deactivate and Quit stop access. Closing an active
window hides it only when the tray icon exists; otherwise it revokes and exits.

Android and the receiving-PC browser viewer use PC ID/PIN only. Android sends a
bounded manufacturer/model name, while the browser labels its platform. The
Windows **Connect to a PC** entry opens the configured viewer. No server address,
SSH key, enrollment key or encoder setting is exposed in the ordinary flow.

The local stdio MCP bridge adds named AI-agent sessions through the production
browser/WebRTC controller. Its tools are connect, status, screenshot, click,
move, scroll, key, type_text and disconnect. Input requires current permission,
current video and an unused screenshot observation token. Tokens expire after
15 seconds and are tied to the connection/display/dimensions. Cancellation,
timeout, inactivity and client EOF close local access; unacknowledged remote end
is reported without claiming success. No real third-party agent client has been
connected to a physical desktop in this environment.

## Why the reported error still requires deployment

The user's screenshot came from the 0.7 unconfigured preview. Its service profile
had an empty public origin. This is an infrastructure absence, not an incorrect
PIN. Vercel access is now available, but its deployment-source policy blocked
publication and no public coordinator/relay is hosted. The profile is still empty
and the engineering apps therefore cannot establish a fresh Internet session.

The new startup path reports that problem before asking for a PIN. A saved PC
ID can be shown offline; no ID is fabricated for a fresh unregistered host. The
Windows installer builder now rejects an empty profile **without a preview
bypass**. The actual CLI was checked against the new verified host and rejected
packaging with the expected message. No installer executable was produced.

## Authorization and concurrent connections

The permanent PC ID survives coordinator restart and same-key recovery while
the current PIN activation is ephemeral. Activation lasts up to 24 hours and
allows four pending/live sessions. One reserves input; others are view-only.
Existing viewers do not gain input silently when the controller leaves. Stop,
replacement, expiry, lockout and identity recovery revoke the appropriate
activation and sessions. Five wrong PIN guesses lock the grant. Request limits,
bounded KDF work and independent local host checks remain in place.

Legacy one-use six-digit codes retain their old protocol contract. Ordinary
legacy/trusted requests cannot bypass active PIN authorization. Activation
permits view and optional input only. Legacy clipboard/upload code is not
exposed by this activation flow. Display names are claims, not verified people.

The roster counts a peer only after an accepted answer, fresh host video,
healthy peer/ICE state and a matching coordinator acknowledgment. Pending and
recovering peers remain separately visible. This is a conservative host-side
readiness measure; it does not prove that a physical client decoded a frame.

Unverified native teardown cancels the activation and requires restart. An
unacknowledged service end stops access visibly but allows another activation
attempt. Upgrade recovery for a prior legacy orphan is host-authenticated,
bounded and attempted only after the specific busy response, with no healthy
credential rotation or identity reset.

## Executed verification

Exact inputs, commands, retained failures, limitations and output hashes are
indexed in `evidence/activation-tray-0.8.0/verification.json`.

| Area | Result | What it establishes |
| --- | --- | --- |
| Windows host | 113 Rust tests; format, Linux and Windows-target Clippy passed; optimized AMD64 GUI PE linked and independently inspected | Application logic and cross-build; not native Windows execution |
| Coordinator | 72 top-level tests / 158 including subcases passed in normal and race runs; vet and static Linux build passed | Activation, identity, capacity, input reservation, revocation, persistence and concurrency |
| Verified-HTTPS integration | 12 lifecycle/security and 8 automatic-registration/recovery checks passed | Actual isolated compiled server, independent cryptographic clients and process restarts; no media |
| Browser modules | 17 core and 20 lifecycle tests passed | Production JS with explicit DOM/RTC/canvas doubles; no rendered page or real image |
| Real Go HTTP/browser protocol | 6 legacy and 6 activation grouped checks passed | Actual routes and WebCrypto identities, permanent ID, four peers, one input owner, fifth denial and Stop |
| MCP bridge | 18 tests, clean locked dependency install and syntax checks passed | Real stdio protocol exchanges/EOF plus mocked browser lifecycle, redaction and observation bounds |
| Configuration | 19 Python tests passed | Profile, deployment and evidence validation logic |
| Installer | 16 admission tests and the actual empty-profile CLI rejection passed | Source/binary custody and prevention of another unconfigured installer |
| Android | 90 JVM tests across 14 suites, 0 failures/skips; lint 0 errors/0 warnings; strict offline debug and minified unsigned release builds for four ABIs | Compile/package and JVM behavior; not a phone session |
| Android artifacts | All 8 package/version/label and ZIP 16 KiB alignment checks passed; retained preview signatures and unsigned release state verified; 8 relevant native libraries passed ELF alignment checks | Artifact structure and identities, not install/update on a device |
| Encoder | Recovered unchanged FFmpeg/source matched the pinned build record, checksums and PE/import rules | Custody of the prior verified cross-build; no new encoder or native run claimed |

Independent source reviews of Windows lifecycle, coordinator admission and MCP
permissions/cleanup found no further blocking issue after the documented fixes.
Those reviews do not substitute for native or Internet tests.

## Built engineering artifacts

| Artifact | SHA-256 |
| --- | --- |
| Windows x64 host 0.8.0, unsigned, 9,660,928 bytes | `0635293e5b48adc0b2ad95bec144a00f69d3bab5d486c3f6109bbf3f8f8af5c4` |
| Android ARM64 preview 0.8.0 / code 8, 24,417,783 bytes | `4ac99f04f81b3b2cd20e33b5801bf6f3a85174e31f983767279c253e1684503a` |
| Linux amd64 coordinator, 7,479,456 bytes | `331d5a570e25a8b92eca7cc8108167df0c2e9845027689d449d09bb7b651f71d` |

The Android preview retains `com.openremote.mobile.dev` and certificate SHA-256
`24c0441ceca6f73ea0e6de8a0958be6229cfcf2518328e8f7f9b71d98b980b28`.
It is eligible for an ordinary 0.6/0.7 preview update by package/version/signature,
but that update has not been executed on a phone. Production variants remain
unsigned and unconfigured. No iOS package or app-store release is included.

## Failures retained and release gates still open

The original coordinator rejected activation mode, captured before the change.
Initial browser/Android recovery-text assertions reflected the old temporary-ID
flow; they were updated to the intended PC-ID behavior. The browser lifecycle
fixture also exposed an event-listener promise wait, corrected by preserving
immediate UI disconnect while the agent API can wait for cleanup. Independent
Windows review found a short/high-DPI roster overlap; scrolling/focus reveal
corrected it. Initial failures and final source pins are retained.

Chromium launch failed before page execution because the workspace denied its
required socket operation. Its sandbox was not disabled to bypass that limit.
No native Windows tray/DPAPI/Media Foundation/SendInput, physical Android
Compose/Keystore/media, rendered browser, actual MCP screenshot/input or complete
Windows-to-phone session ran. Public cellular/NAT/TURN UDP/TCP/TLS/443, outages,
installer execution and upgrade/rollback/uninstall remain unverified.

The Windows host is a logged-in interactive tray application, **not an SCM
service**. Automatic reboot/logon recovery and lock/UAC secure-desktop control
remain planned. Production signing, complete dependency notice/provenance gates,
updates and operations acceptance are unfinished. The current scope is not
commercial feature parity with AnyDesk or Phaze.

The next release-critical input is this project's hosting target/domain and
deployment access. After deploying and verifying that service, rebuild the
clients with its public origin, execute the native/Internet acceptance matrix,
resolve notices and sign the final packages. End users will still enter only
PC ID and PIN; infrastructure setup belongs to the publisher.
