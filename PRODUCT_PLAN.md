# OpenRemote product and release plan — 0.8

Updated 20 November 2026. This plan supersedes the older temporary-ID and manual
setup journeys. Earlier versions are retained under `docs/history/0.7.0/`.
The engineering build does not establish a commercial release.

## Product goal

Install OpenRemote on several owned Windows PCs. Each PC shows a permanent PC ID.
Its owner chooses a six-digit PIN and activates access with one primary action.
Android, a receiving PC and an authorized AI agent connect using that PC ID/PIN.
The host remains visibly available in the system tray, identifies all connection
labels and states, and lets the owner stop access. No ordinary client is asked
for a coordinator address, SSH key, enrollment credential or encoder path.

The supplied references guide usability and agent interoperability:

- [AnyDesk](https://anydesk.com/en) and its [connection guide](https://support.anydesk.com/docs/connect-to-a-remote-client)
  establish the useful pattern of a visible own-device ID and a separate remote
  address entry. OpenRemote keeps its own design and nine-digit ID contract.
- [Phaze](https://phaze.app/) and [Phaze for AI](https://phaze.app/phaze-for-ai/)
  inform the shared people/agent workflow and MCP interface. Phaze describes its
  MCP feature as experimental. OpenRemote is an independent implementation; no
  compatibility, affiliation or integration with Phaze itself is claimed.

These references do not establish feature parity, latency, security certification
or market readiness for OpenRemote. Such claims require measured acceptance.

## Primary journeys and implementation status

| Journey | 0.8 implementation | Remaining acceptance |
| --- | --- | --- |
| First Windows launch | Resolve publisher profile and protected identity; verify discovery; auto-register; display permanent nine-digit Your PC ID before PIN | Real configured HTTPS service, native first install and DPAPI behavior |
| Activate | Exactly six PIN digits; server and local activation grant; encoder preflight; confirmed activation before tray hide | Native PIN entry, encoder initialization and actual moving desktop |
| Phone connect | PC ID/PIN only, capability admission before PIN disclosure, actual Android model label, permission preservation | Physical Android UI/Keystore/decoder/input and device rotation |
| Laptop connect | Native launcher opens fixed service `/connect`; PC ID/PIN browser viewer | Real Chrome/Edge rendering, keyboard/pointer/media and supported-browser matrix |
| Connection visibility | Bounded roster, claimed names, connecting/active/recovering states, input/view-only labels, tray active count | Real Windows tray/Explorer restart, high DPI, keyboard accessibility and count timing |
| Multiple peers | Four pending/live sessions maximum; one input controller and view-only followers | Four physical peers, resource use, independent teardown and meaningful performance |
| Stop | Local grant cancellation and session task shutdown; server revocation; uncertain cleanup remains visible | Native capture/input termination, stuck-key recovery and outage timing |
| AI access | Stdio MCP tools using production browser/WebRTC; named AI label; screenshot-bound one-action input; cancellation and timeouts | Sandboxed Chromium with real desktop video and independent MCP clients |
| Installation | Per-user NSIS source, protected identity preservation, pinned encoder/source, strict empty-profile rejection | Configured package compilation, signing, native install/upgrade/rollback/uninstall |
| Internet | HTTPS coordinator, authenticated signaling and short-lived TURN credentials; deployment generator/runbooks | Project-specific deployment plus direct, TURN UDP/TCP/TLS and cellular sessions |

Activation expires after at most 24 hours and is not silently restored after a
process or coordinator restart. PC identity is durable; the current activation
is intentionally ephemeral. Five wrong PIN guesses lock the activation and stop
its sessions. Legacy one-use six-digit invitations remain protocol-compatible
but are no longer the primary PC ID. Names are claims and must not be presented
as verified people. A view-only connection never silently acquires input rights.

## Release dependency order

### 1. Restore a real deployable service

The authorized Vercel project and viewer/gateway source are prepared. The first
production attempt was blocked by a policy allowing CLI deployments only; the
connected integration route cannot publish under that policy. Complete an
authenticated CLI deployment or obtain an explicit project-level policy change
before resubmitting. No live website is claimed. The exact attempt is recorded in
`evidence/vercel-deployment-0.8.0/deployment-attempt.json`.

Use Vercel for the browser viewer and public HTTPS gateway. Provide durable
hosting for the current stateful coordinator and raw-network TURN relay; the
current single-process authority cannot be split across independently scaled
Functions. The new authenticated client-address boundary prevents Vercel egress
from collapsing every client into the same rate-limit bucket. Configure the
coordinator's public origin to the stable Vercel alias and its gateway token to
the publisher secret, then prove that binding with real registration.

Provide this project's coordinator/relay hosting target and access. Provision the
coordinator with private durable state, its enrollment/admin secret and verified
HTTPS. Provision a TURN relay with bounded short-lived credentials, TLS and
sufficient bandwidth. Publish monitoring and backup/recovery procedures.

Verify actual service behavior before setting `config/service-profile.json`.
Embed only the public HTTPS origin in Windows, Android and agent bridge builds.
Never embed administrator or relay secrets. Existing enrolled origins are not
silently migrated. The shared profile is currently empty and no functioning
public service is hosted; this is the direct cause of the user's screenshot.

**Exit evidence:** domain/TLS checks, successful automatic identity registration,
real relay allocation and media, credential-expiry tests, restart/restore tests,
monitoring records and reviewed operating cost/limits. A `/healthz` response or
configured-relay flag alone is insufficient.

### 2. Complete the first native path

Build a configured Windows package and install it on a clean supported Windows
PC. Confirm first launch, permanent ID, PIN activation, media capture and tray
visibility. Install the corresponding Android build on a physical phone and
connect using a different Internet connection. Repeat from a laptop browser.
Check moving frames and injected mouse/keyboard actions, not just signaling.

**Exit evidence:** version/hash-pinned native artifacts, install records, consented
screen recordings or explicit test observations, actual ICE path and connection
timings, confirmed local and remote stop, upgrade preserving the PC ID.

### 3. Validate simultaneous people and agents

Connect one input controller plus three viewers with distinct names. Verify
count transitions, cancellation while pending, independent disconnect, no input
promotion, fifth-session denial, PIN lockout and activation expiry. Run the MCP
bridge with a real sandboxed browser and two independent agent clients. Test
fresh screenshot capture, bounded actions, stale-token denial, client EOF,
timeout and owner-stop behavior. Screen text is untrusted application content;
the agent client must retain its own authorization and prompt-injection controls.

**Exit evidence:** real transport, screenshots and input on the target PC, correct
host roster, no leaked input state, bounded CPU/memory/network consumption and
MCP compatibility records. VM doubles and stdio protocol tests are not these
acceptance tests. Large MCP image buffers also need client-specific validation.

### 4. Package and operate a supported release

Resolve all exact-binary dependency notice/provenance gaps. Sign Windows and
Android with production identities; protect keys outside source. Test clean
install, existing preview upgrade, interrupted upgrade, rollback, uninstall and
revocation. Establish signed updates with rollback protection, supported-version
policy, crash reporting without secrets/pixels, service health monitoring and
an incident/rollback runbook.

**Exit evidence:** signed artifacts and checksums, verified release identity,
complete notices/SBOM, green acceptance matrix, reproducible source/build records,
operations ownership and an explicitly approved release decision.

## Native and Internet acceptance matrix

All rows remain open unless a separate execution record names real devices,
versions, artifact hashes, observed behavior and the tester. Engineering tests
cover logic but do not mark these rows passed.

| ID | Required scenario and pass criterion |
| --- | --- |
| R01 | Clean Windows install: no console exit; usable window; correct publisher binding and durable PC ID |
| R02 | Empty/offline/wrong-capability service: PIN/activation blocked with actionable visible state; no fake ID |
| R03 | Activate: real encoder starts; grant confirmed; tray icon exists before window hides |
| R04 | Tray: hover count, roster, Open/Deactivate/Quit, Explorer restart and icon-loss recovery |
| R05 | Layout: short windows, 125–200% DPI, keyboard focus, screen reader labels and long Unicode names |
| R06 | Android: PC ID/PIN, leading zeroes, wrong PIN, readable progress, decoded moving frames and view-only handling |
| R07 | Desktop input: pointer mapping, click, drag, scroll, Unicode, shortcuts and release on disconnect |
| R08 | Laptop browser: Chrome/Edge keyboard, pointer, fullscreen, resize, focus loss and visible disconnect |
| R09 | Four peers: names/counts, exactly one input owner, fifth denied, owner exit and viewer reconnect |
| R10 | Consent/revocation: Stop, Quit, lockout, grant replacement/expiry and administrator revoke stop access |
| R11 | Direct ICE, TURN UDP, TURN TCP and TURN TLS/443 each carry real desktop traffic |
| R12 | Wi-Fi/cellular handoff, temporary outage, relay expiry and coordinator restart produce bounded visible outcomes |
| R13 | Multi-monitor: negative origins, mixed DPI, display change/reconnect and unplug stop input safely |
| R14 | Agent: real screenshot, valid action, stale/replayed token denied, view-only denied, cancellation/EOF cleanup |
| R15 | Upgrade: 0.7 identity and Android preview data preserved; orphan legacy session cleanup bounded |
| R16 | Installation rollback/uninstall: owned files/shortcuts only; local data policy correct; no orphan active host |
| R17 | Performance: record startup/connect latency, frame rate, input latency, CPU/GPU, memory and relay bandwidth across supported hardware |
| R18 | Operations: backup/restore, service restart, certificate renewal, monitoring and signed update rollback |

## Advanced scope beyond the current activation slice

These are explicit future work, not hidden behind “release-ready” terminology:

- **Windows service and session broker:** least-privilege SCM broker, authenticated
  IPC, per-user interactive capture, safe session switching, explicit unattended
  policy and reboot recovery. Secure desktop/UAC is a separate review and device
  gate. The current tray host requires a logged-in desktop.
- **Trusted device accounts:** saved PCs, revocable scoped trust, stronger
  unattended credentials, account MFA, per-device permissions and ownership
  transfer. A six-digit PIN is not a replacement for account authentication.
- **Performance:** adaptive bitrate/resolution/frame rate, hardware capability
  probing and fallback, measured multi-stream resource limits, audio and A/V sync.
- **Additional sharing:** explicit clipboard and bidirectional resumable file
  transfer, conflict policy and independent permission prompts. Legacy internal
  clipboard/upload code is not granted by the current PIN activation flow.
- **Agent administration:** scoped short-lived credentials, owner-approved agent
  identities, per-tool policy, audit and enterprise provisioning. The current
  bridge uses explicit owner PIN access; it is not an autonomous privileged agent.
- **Platform breadth:** supported iOS client, native receiving-PC packaging if
  required, tested accessibility, localization and consistent tablet controls.
- **Fleet and operations:** authenticated device directory, roles, durable audit,
  multi-node data model when needed, relay capacity planning, updates and support.

Scope expansion must preserve the simple front page. Administrative and advanced
features belong behind intentional owner/admin actions. They must not reintroduce
server, enrollment or SSH fields into ordinary connect/activate flows.

## Current decision

Ship this work as a source/evidence checkpoint only. Preserve the built engineering
artifacts without presenting them as a working configured release. Do not issue
another unconfigured installer. The next release-critical dependency is the
OpenRemote hosting target and public domain, followed by native device acceptance.
