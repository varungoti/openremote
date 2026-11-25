# OpenRemote product and delivery plan

Updated 19 November 2026. The commercial release requirements in [commercial-release-standard.md](docs/commercial-release-standard.md) extend this plan and take precedence where a previous preview flow conflicts.

### One-button connection — 0.7.0

The current user requirement supersedes the older setup journeys: the Windows
home offers one Connect/Stop button, one six-digit PIN and a displayed six-digit
Server ID. Phone and browser entry have only Server ID/PIN and Connect. Service
address, enrollment/SSH key, fingerprint and encoder fields are removed. Automatic
registration and a bundled encoder replace those user tasks. The shared service
origin is a publisher build input, not a customer setting. No public deployment
has been supplied, so this explicitly labelled preview cannot connect a fresh
installation over the Internet. Empty-profile release builds are rejected by
default. Native device and Internet acceptance still precede commercial release.

### Code and PIN plus commercial usability — 0.6.0

The primary flow is now an owner-generated **six-digit connection code plus a
6–12 digit PIN**. Codes authorize one session, expire after ten minutes, and are
revocable. The running host must still hold its matching local authorization.
Android presents code/PIN first and moves nine-digit PC lookup into Advanced.
A browser viewer on the coordinator provides the receiving-PC flow, with an
explicit entry point from the host. Shared public build configuration supplies
the initial service origin without embedding enrollment or session secrets.

The user requires commercial parity and very easy use. This includes turnkey
runtime/enrollment packaging, Internet reliability, performance, signing,
updates and support—not just a connection form. These are concrete acceptance
requirements in the commercial release standard, with implemented, untested
and planned states distinguished. The current service profile is unconfigured;
no public coordinator or relay was provisioned. First host enrollment and the
separately verified encoder still require deployment work.

### Windows startup repair — 0.5.1

The reported immediate exit exposed a gap between the built host and a usable
first-run application. This repair adds a visible setup/progress screen,
recoverable startup errors, and an executable per-user installer. It covers
server/name/encoder/enrollment setup and preserves existing consent/trust policy.
Broader host settings and trust-management UI remain separate planned work.
See [the installer guide](docs/windows-installer-0.5.1.md) and current build status.
Native Windows installation and a real phone session remain required acceptance
gates; compilation and package extraction do not complete them.

## Product outcome

OpenRemote should let the owner install a visible host on several Windows PCs,
connect from Android or another PC using a six-digit code and the PIN chosen
on the host, and reach those desktops from a different Internet connection. The
permanent nine-digit PC number remains an advanced locator. The explicit Generate
code action grants one session; ordinary requests still require local approval. Explicitly trusted phones can use unattended access within the host's
configured permissions. The desktop must remain usable during ordinary network
changes, and both people must be able to stop access immediately.

The intended product includes the remote session, host onboarding, mobile controls,
Internet relay infrastructure, installers, updates, support diagnostics and the
operational process needed to maintain it. A connection animation or successful
SDP exchange is not an acceptance test for a remote desktop.

**Current stage: 0.6 development.** Real native components have been built and
checked against their recorded source inputs. Actual Windows-to-Android media, native input, public TURN fallback and
device installation still require execution on suitable devices/runners. The
workspace blocks the media test before packets flow. No release date or completed
production release is asserted.

## What changed from the previous plan

| Previous foundation | Revised requirement and implementation direction |
| --- | --- |
| Phone supplied a public key without proving ownership | One-use P-256 signature login, server-derived controller identity and replay rejection are implemented. Independently signed peer negotiation is a later security requirement. |
| Placeholder Windows WebRTC answer and media seam | A real Rust WebRTC peer, H.264 track and FFmpeg capture path are implemented and compiled. Transport must still pass moving-frame and input tests. |
| Short ID appeared sufficient to address a PC | The PC number only locates a host. Authentication, host consent and per-capability authorization are separate requirements. |
| Broad feature checklist with no completion evidence | Stable requirement IDs, acceptance criteria, dependency ordering and release gates below. |
| Missing native toolchains | Windows executable and Android APK builds now run with recorded toolchains and checksums. Native hardware behavior is a separate gate. |
| Permanent or unspecified relay credentials | Short-lived session-bound TURN credentials and a private coturn deployment configuration are implemented. Public relay validation is outstanding. |
| PostgreSQL/Redis listed but unused | The private preview uses one durable, locked coordinator snapshot. Multi-node database/queue design is required only before a supported clustered deployment. |
| Clipboard and file transfer left as placeholders | The 0.3 slice implements user-triggered text clipboard actions and bounded phone-to-PC file uploads with independent permissions. |
| Capture and pointer handling assumed the primary monitor | The 0.4 slice binds capture/input to one selected monitor, reconnects for changes, and ends on observed topology changes. Physical multi-monitor validation remains open. |
| Manual phone/server entry and console-only host setup | The 0.5 slice adds a visible host home, a secret-free setup link with explicit phone confirmation, and staged server/identity/PC diagnostics. Native first-use checks remain open. |
| Install/signing/update listed together | Installation, rollback, identity preservation, binary signing, signed updates and real upgrade tests have separate acceptance gates. |

## Product boundaries and operating modes

### Supported development target

Windows x64 host and Android 8+ native controller; Android ARM64 is the primary
physical-phone target. Android 16 KB page compatibility is checked in the APK
packaging, but must also run on an applicable device/emulator. Windows 11 x64 is
the first planned general-release host target. Any additional Windows edition or
version needs an explicit supported-system test entry before it is advertised.

The current host is a per-user process in an already logged-in desktop. The first
service-enabled version must add a small privileged broker and a separate user
session process, with authenticated IPC and a reviewed privilege boundary. It
must not simply run all network parsing and media processing as LocalSystem.

### Deployment modes

1. **Private owner deployment:** one coordinator and one or more relays controlled
   by the owner, encrypted backup, a small fleet of PCs and explicit phone trust.
   This is the current implementation direction.
2. **Managed private deployment:** supported installation, monitoring, updates,
   relay capacity and recovery procedures for a household or small team.
3. **Organization deployment:** accounts, passkeys, teams, RBAC, invitation and
   device enrollment policies, multi-region infrastructure and retention controls.
   This requires additional isolation and operational work; it is not implied by
   the current single-owner API.

No new cloud provider, paid infrastructure or unrelated repository is selected by
this plan. Publishing a PR requires a destination repository. Live Internet
validation needs an authorized public API/relay host, domain and device runners.

## User journeys

### Add a PC

Install the configured Windows application, open it and use **Connect remote
desktop**. Choose a six-digit PIN using the same primary action. Registration,
service discovery and bundled-encoder checks happen automatically, with plain
status and cancellable preparation. The window shows the six-digit Server ID and
its remaining invitation time. The action becomes **Stop remote desktop**.
There are no URL, enrollment key, fingerprint, encoder path, checksum, setup-link
or configuration buttons in this journey. Each additional PC follows the same flow.

A Server ID is one active invitation, expires after ten minutes and works once.
It is not the legacy permanent nine-digit machine identifier. A later product
iteration may offer remembered PCs and explicitly authorized unattended access,
but those must not complicate this primary flow or silently broaden PIN access.

### Connect from the phone

Enter **Server ID** and **Security PIN**, then choose **Connect remote desktop**.
The publisher's common service is embedded in the app. There is no Advanced form
or setup import. Preparation and failure use plain status messages; Cancel or Stop
replaces the current connection action. Connected status waits for the active
renderer's actual first frame. The browser viewer follows the same two-field flow.

### Use the desktop

Touchpad mode supports relative movement, click, drag, right click and two-finger
scroll; direct-touch mode maps the displayed desktop without letterbox errors.
Keyboard access includes text entry, Ctrl/Alt/Shift/Windows, function keys, Escape,
Tab and common shortcuts. The 0.4 monitor picker lists displays with dimensions
and primary/current indicators. **Switch and reconnect** creates a new authorized
session; it may require another PC approval. Input waits for the new renderer's
first frame and fresh decoded video. Observed display-layout changes end the
session so capture and input cannot silently diverge. The advanced release also
adds zoom/pan, expanded landscape/tablet layouts and external keyboards/mice.

Session tools expose only granted capabilities. Clipboard actions are explicit,
with a preview before copying retrieved text to the phone. The current upload
slice uses the Android document picker, progress, cancellation, integrity checks
and a dedicated PC receive folder. Later bidirectional transfers add a local
host-selected export scope, resumable chunks and conflict handling.

### End, recover or revoke

The PC keeps a visible session indicator and a local emergency disconnect. Every
end path releases remote-held buttons and modifiers, stops capture and removes
incomplete transfer data. Phone backgrounding ends the current preview session;
any later background-session option requires an explicit foreground service and
visible notification.

Network changes use bounded ICE recovery with fresh temporary relay credentials.
The UI disables control until fresh video arrives. Revoking a phone blocks new
sessions even if stale local trust metadata remains. Reboot recovery, secure
desktop and Wake-on-LAN are separate capabilities with clear prerequisites.

## Expanded feature scope

Status meanings: **implemented** means source exists; **built/tested** identifies
the specific verification in BUILD_STATUS.md; **next slice** means active work;
**planned** means no completion is claimed. All native behaviors retain their
physical-device test gates.

| ID | Capability | Scope and acceptance | Stage |
| --- | --- | --- | --- |
| C01 | Multiple PCs and permanent IDs | Enroll, name, save, select and reconnect to several PCs; IDs survive coordinator restart. | Implemented |
| C02 | Authenticated phone identity | Keystore key, challenge proof, replay rejection and exact identity on host approval. | Implemented |
| C03 | Consent and permission narrowing | View/control/clipboard/files granted independently; unsupported or disabled powers never granted. | Implemented |
| C04 | Trusted unattended sessions | Explicit owner setting and exact trusted phone; revocation wins over stale configuration. | Implemented; settings UI planned |
| C05 | Real video and input | Moving pixels, Unicode, shortcuts, drag and scroll work across physical devices. | Native code built; device gate open |
| C06 | Internet connection paths | Direct ICE, TURN UDP, TURN TCP and TURN TLS/443 selected and measured in independent tests. | Infrastructure prepared; live gate open |
| C07 | Network recovery | Wi-Fi/cellular handoff, temporary loss, credential renewal and coordinator restart have bounded outcomes. | Implemented recovery; device matrix open |
| C08 | Useful diagnostics | Actual decoded frames, selected candidate type, bitrate, RTT and useful error stage; no secret/pixel logging. | Basic session diagnostics; staged server/authentication/PC checks added in 0.5; relay flags describe configuration only |
| C09 | Phone setup import | Secret-free server/PC locator, full origin confirmation, no pre-confirmation network or trust grant, stale-operation rejection. | Implemented in 0.5; native intent/rotation/clipboard gate open |
| T01 | Explicit text clipboard | Phone-to-PC write and PC-to-phone preview/copy, 4 KiB UTF-8 cap, no background monitoring. | Implemented; build/unit tests pass |
| T02 | Phone-to-PC upload | Up to 64 MiB per file, bounded chunks, SHA-256, progress/cancel, no path traversal or overwrite. | Implemented; build/unit tests pass |
| T03 | Bidirectional file exchange | Host-selected export scope, explicit phone save location, integrity and no arbitrary path requests. | Planned |
| T04 | Resumable transfer queue | Pause/resume across reconnection, bounded storage, transfer journal, conflict policy and expiry. | Planned |
| D01 | Monitor selection | Enumerate up to 16 monitors; bind capture/input; change through a fresh session; observed unplug/layout changes end safely; reconnect refreshes selectors. | Implemented in 0.4 for GDI; native DPI/hotplug gate open |
| D02 | Hardware encoding | Capability-tested Media Foundation/NVENC/QuickSync/AMF path with software fallback. | Planned |
| D03 | Adaptive quality | Fit bitrate/resolution/frame rate to measured network conditions; owner quality controls. | Planned |
| D04 | PC audio | WASAPI loopback, Opus, mute/volume, reconnect and A/V sync; no microphone capture by default. | Planned |
| D05 | High-resolution mode | Measured 1080p60 on supported hardware; 1440p/4K as explicit supported profiles after benchmarks. | Planned |
| D06 | Advanced phone controls | Zoom/pan, orientation, tablet layout, external keyboard/mouse and accessibility support. | Partly implemented; expanded tests planned |
| H01 | Friendly host settings | PC number, origin, encoder health, trusted phones, permission toggles and logs in one visible UI. | 0.5 home shows PC/origin/setup link and Stop; permission/trust/encoder settings UI still planned |
| H02 | Startup at sign-in | Explicit owner switch, visible launch and correct identity; safe uninstall cleanup. | Planned |
| H03 | Service/session split | Reviewed broker IPC, per-session capture, least privilege, reconnect after reboot/logon. | Planned |
| H04 | Lock screen and UAC | Explicit policy, secure-desktop handling and Windows integrity testing; never bypass OS controls. | Planned after H03 |
| H05 | Wake-on-LAN and power | Separate permission; hardware/network support detection and an authorized always-on relay. | Planned |
| H06 | Privacy screen | Explicit local policy, recovery after crash and a working local escape path. | Planned |
| S01 | Secure storage | DPAPI host secrets, Keystore phone identity, no plaintext bearer in persisted coordinator state. | Implemented |
| S02 | Independent peer verification | Bind session, roles, ephemeral peer fingerprints and negotiation generations to identity signatures. | Planned |
| S03 | Phone biometric gate | Require local biometric/device authentication before unattended sessions or sensitive actions. | Planned |
| S04 | Revocation and history UI | Trusted-phone management, local and coordinator audit view, device/session revoke and recovery. | API/state partly implemented; UI planned |
| S05 | Signed distribution and updates | Authenticode, Android release identity, signed update metadata, rollback protection and staged rollout. | Build scaffolding; production keys/update agent absent |
| S06 | Dependency provenance | Exact locked dependencies, reproducible manifests, native component notices and vulnerability review. | Inventory implemented; remaining notice/source gaps |
| O01 | Private deployment | TLS, limited exposed ports, persistent state, expiring relay credentials and documented secrets. | Configuration implemented; runtime gate open |
| O02 | Operational diagnostics | Health checks, capacity/relay metrics, alerts, redacted support bundle and opt-in crash reporting. | Health/basic logs; expanded work planned |
| O03 | Backup and recovery | Encrypted backups, tested restore and version-compatible rollback; defined recovery objectives. | Runbook; restore drill open |
| O04 | Clustered operation | Transactional registry/session state, cross-node event delivery, rate limits and relay selection. | Planned before multi-node claim |
| F01 | Fleet and roles | Personal/team account, invitations, RBAC, device groups and policy enforcement. | Planned |
| F02 | Sharing and assistance | Expiring one-session invitations, visible helper identity and revocable permission scope. | Planned |
| F03 | Recording | Explicit consent, visible recording state, encryption, retention and export/delete. | Planned |
| X01 | More platforms | iOS controller, then macOS/Linux hosts with native permission flows. | Planned after Windows/Android release |

Remote terminals, unattended script execution, arbitrary port forwarding and
invisible capture are not part of the first general release. Adding such powers
would require a separate product decision, authorization model and test plan.

## Implementation milestones

### M0 — Replace the foundation with verifiable native components

Completed engineering work includes a real WebRTC host, Android native client,
cryptographic controller proof, durable PC enrollment, bounded protocol parsing,
local consent, expiring TURN credentials and recorded native build outputs.
Baseline security regressions were reproduced before their fixes. An actual
Windows-to-Android session remains the milestone's external validation gate.

### M1 — 0.3 useful session tools and recovery hardening

Engineering work completed in this build:

- Implement the shared tools protocol in [docs/session-tools.md](docs/session-tools.md).
- Add explicit clipboard and file-transfer request switches and host permission gates.
- Stream phone-selected file snapshots through acknowledged chunks, with integrity,
  cancellation, quotas and incomplete-file cleanup.
- Fix restart event-cursor reuse, heartbeat lease mismatch, stale request replay,
  and successful ICE-restart state reporting.
- Stage installer upgrades before switching the active version; preserve identity,
  require verified compatible FFmpeg and record truthful source provenance.
- Rebuild both clients and retain exact artifacts, tests and known blockers.

Exit evidence recorded: 42 Rust tests and 32 Android JVM tests pass, native builds
pass, both implementations were reviewed against the shared contract, and device
test instructions exist. This is still a development build; M2 is open.

### M2 — Physical-device and Internet pilot

Run the prepared independent media suites on an authorized runner with normal
network interface/socket access. Then use at least two Windows PCs and one ARM64
Android phone on different networks. Validate first enrollment, local approval,
unattended trust, revoked identity, display/input, clipboard/files, connection
recovery and installation/upgrade/uninstall.

Required relay cases are UDP, TCP and TLS, with the actual selected candidate
recorded. Include a session longer than the TURN credential lifetime, mobile
carrier NAT, an offline PC, a relay outage and a coordinator restart. Fix every
reproducible blocker before recruiting additional users.

Exit evidence: signed test reports with environment versions and artifact hashes,
no open loss-of-control/data-corruption defects, and explicit supported hardware.
Passing this phase permits a limited private pilot, not a broad performance claim.

### M3 — Advanced desktop experience

The 0.4 engineering slice implements monitor selection first because capture/input
geometry depend on it. It includes opaque topology-bound IDs, a strict catalog,
view-only-safe selection, negative-origin pointer mapping, a fresh-session switch,
bounded previous-session cleanup and renderer-generation checks. An explicit
selection is never displayed or controlled before the host confirms that display.
The host ends the session on observed topology change; it does not perform an
in-place capture switch. Native mixed-DPI, portrait and unplug/replug behavior
still must pass M2's physical-device tests.

A new [pilot evidence workflow](docs/pilot-validation.md) binds 17 unattempted
scenarios to exact candidate files. Its validator rejects incomplete/mismatched
records without claiming to attest the operator's observations or release readiness.

The 0.5 slice improves the first phone connection: origin-confirmed setup import,
public nonsecret protocol discovery, staged authentication/PC checks, actionable
errors and a visible PC home with local stop. It also serializes Android settings
work so an old server's result cannot overwrite a newer choice, and rejects
malformed/ambiguous relay URL configurations. The additional native setup and
diagnostic cases in [phone-setup.md](docs/phone-setup.md) are required before release;
they supplement the existing 17-case pilot and are not claimed as executed.

Add hardware encoder negotiation and measured adaptive quality next. Add PC audio,
bidirectional/resumable files, mobile ergonomics and the host settings UI. Each
capability is advertised only when the installed host actually supports it.

The first target performance profile is 1080p60 on declared supported hardware,
with graceful 720p30 fallback. Higher resolutions and codecs require separate
compatibility, licensing and performance evidence.

### M4 — Service lifecycle, identity and distribution

Build and review the service/session-agent split, startup/reboot behavior, explicit
secure-desktop policy, peer-signed negotiation, biometric phone gating and trusted
device management. Complete binary signing, the installer/repair/uninstaller and
signed updates with rollback protection. Finish native dependency notices and an
independent review of authentication, pairing, authorization and update flows.

### M5 — General release and operations

Run the full supported-system, impairment, install/update and recovery matrix.
Complete operating procedures, capacity limits, backup restoration and incident
diagnostics. The release record must name the exact source revision, binary
hashes, signed identities, passed environments and unresolved nonblocking issues.
Open P0 gates prohibit a production-ready label.

### M6 — Fleet and platform expansion

Add teams/RBAC, invitations, shared-PC policy, multi-region/cluster operation,
recording and additional platforms after the core Windows/Android experience has
measured reliability. These expand the trust and operational boundary and receive
their own release gates.

## Measurable acceptance targets

These are proposed engineering targets, not achieved benchmarks. Each report must
record the PC/GPU, phone, OS, build, resolution, network, relay path and measurement
method. Physical camera or timestamped test-fixture measurement is required for
glass-to-glass latency; API round-trip time alone is not that metric.

| Area | Acceptance target and test conditions |
| --- | --- |
| Connect | At least 95% of 100 attempts reach a decoded desktop within 10 seconds after approval on the supported network matrix; every failure has a bounded error state. |
| LAN interaction | 1080p60 target, p95 glass-to-glass latency under 150 ms on specified hardware with RTT at most 10 ms and no imposed loss. |
| Internet interaction | 720p30 fallback remains usable at 5 Mbps, RTT 80 ms and 1% imposed loss; measure latency and input delivery separately. |
| Recovery | Recover a supported Wi-Fi/mobile switch within 20 seconds, or end safely with an actionable error; no stuck input or duplicate active session. |
| Long session | Two-hour run including credential renewal, no stale authorization and bounded memory growth after warm-up. |
| Input | Reference text, Unicode, modifiers, all pointer buttons, drag and scroll match expected events; release-all on each teardown path. |
| Files | Exact SHA-256 for empty/boundary/64 MiB samples; cancellation/drop removes partials; bad offset/digest/path leaves no completed file. |
| Security | Unauthorized requests never reach pixels/input/tools; replay, revoked phones, permission escalation and expired credentials are rejected. |
| Install/update | Clean install and upgrade preserve PC identity; interrupted/failing upgrade restores a runnable previous version; uninstaller follows the chosen identity-retention policy. |
| Accessibility | Legible scalable text, accessible labels, screen-reader navigation, sufficient contrast and no essential function tied only to an unlabeled gesture. |
| Operations | Document and test recovery time/point targets against the chosen deployment; do not state availability SLAs before monitoring and capacity evidence exist. |

## Delivery discipline

Source changes belong in a reviewable branch/PR with the concrete problem, behavior
change and relevant evidence. The original supplied archive remains an immutable
baseline. Dependencies are locked; toolchains and native artifact hashes are
recorded. The prepared workflow runs manually on owner-controlled runners and
does not publish a release automatically.

Maintain three distinct records:

- [BUILD_STATUS.md](BUILD_STATUS.md): what this exact build actually passed, could
  not execute, and has not implemented.
- [docs/release-gates.md](docs/release-gates.md): release-blocking acceptance items.
- This plan: intended product scope, priorities and future milestones.

No feature moves to completed on the strength of a UI control, mocked response,
compilation alone or a locally encoded frame. A built artifact is useful evidence;
release readiness requires the behavior and operational gates as well.
