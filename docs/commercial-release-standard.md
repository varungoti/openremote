# OpenRemote commercial release standard

Updated 20 November 2026. This is an acceptance plan, not a claim of achieved parity.

## Product requirement

The user requires the ease and capability expected from commercial remote desktop
software. The release must cover installation, first connection, everyday use,
recovery, updates and support. A successful build or a tested authorization API
does not establish that the complete product meets that requirement.

The everyday flow is **install → open → Your PC ID → Activate Connection and PIN
→ enter PC ID/PIN on the receiving device → connect**. The same primary Activate
action reveals and then submits the PIN. The host stays visibly available in the
tray, with named peers and an active count. For repeatedly accessing one's own PCs, a separately authorized,
revocable remembered-device flow must remove repeated pairing without weakening
identity checks. An ephemeral active-PIN grant and permanent unattended access are
different authorizations and must be explained in ordinary language.

## Reference products and scope

The official AnyDesk feature catalog lists remote control, unattended access,
file management, remote printing, Wake-on-LAN, privacy mode, allowlists and
two-factor authentication. TeamViewer's official feature pages describe desktop
and mobile access, unattended access, remote printing and multi-monitor support.
Those pages establish useful capability categories; they do not establish this
project's measured performance, implementation or release quality.

Earlier commercial-category sources reviewed 19 November 2026:

- [AnyDesk features](https://anydesk.com/en/features)
- [TeamViewer Remote features](https://www.teamviewer.com/en-in/products/remote/features/)
- [TeamViewer remote access and work](https://www.teamviewer.com/en/products/remote/use-cases/remote-work/)
- [RustDesk self-host deployment model](https://rustdesk.com/docs/en/self-host/)

The user additionally supplied [Phaze](https://phaze.app/) on 20 November 2026.
Its [AI page](https://phaze.app/phaze-for-ai/) describes an experimental MCP
interface; this informs the independent OpenRemote stdio agent bridge and its
explicit permission and owner-visibility requirements. No third-party client
compatibility is claimed without an executed test.

No proprietary code, paid account, third-party production relay or another
project's deployment is used by OpenRemote. The current implementation remains
its own Windows/Android/WebRTC coordinator architecture. A different engine or
license strategy would require an explicit technical and distribution decision.

## Usability contract

| Journey | Required experience | 0.8 implementation / remaining work |
| --- | --- | --- |
| Install host | One ordinary installer, recognizable publisher, compatible runtime supplied | Source and pinned encoder exist; empty-profile packaging now unconditionally rejected; configured/native/signing gates open |
| First run | No URL, port, hash, enrollment token, config file or terminal | Service admission and automatic identity registration before PIN; public deployment missing |
| Share access | Permanent PC ID, chosen six-digit PIN, single primary Activate/Deactivate action | Implemented; 24-hour process activation, four peers and one input owner; native acceptance open |
| Connect phone | PC ID/PIN, clear progress, cancel and stop | Android implementation and builds tested; physical phone/network gate open |
| Connect another PC | Configured viewer, PC ID/PIN, real desktop | Same-origin browser and native launcher implemented; real browser/media gate open |
| See connections | Visible tray status, active count, names and state | Implemented with conservative host readiness; native tray/DPI/Explorer tests open |
| Connect AI agent | Explicit owner authorization, named session, scoped screenshot/input, immediate revoke | Local MCP bridge tested with real stdio and mock browser; actual agent/desktop compatibility open |
| Use favorite PCs | Names, availability, search and authorized remembered access | No saved-PC screen in current public flow; account/ownership/trust management remains planned |
| Recover | Explain unavailable/denied/network states, preserve identity, no blind redemption replay | Bounded API/native state logic implemented; public outage/handoff/reboot matrix open |
| Stop | Release input, end capture and authorization, report uncertainty | Cancellation/revocation implemented; native failure acceptance open |
| Update | Signed update, identity preserved, rollback, no forced reinstall | Versioned installer source/custody exists; signed updater/native upgrade gate open |

Technical setup belongs to a deliberate owner/admin deployment flow. Advanced
options must be behind Advanced/Settings, with safe defaults and specific recovery
instructions. Ordinary users must never be told to install compilers, edit JSON,
copy enrollment secrets between clients or disable operating-system protection.

## Capability milestones and acceptance

| Milestone | Required outcome | Exit evidence |
| --- | --- | --- |
| A — Visible activated connection | Permanent PC ID/PIN across Windows, Android, PC viewer and named agents, four-peer bound, one input controller, no duplicate request replay | Current component regressions and build evidence plus physical Windows/Android/browser session matrix; the latter is still open. |
| B — Turnkey private deployment | Real HTTPS coordinator and TURN UDP/TCP/TLS, preconfigured clients, automatic possession-proved enrollment, compatible packaged media runtime | A novice completes first use without technical fields; owner backup/restore and key rotation runbook executed. |
| C — Reliable daily work | Adaptive hardware video with software fallback, measured 1080p quality, display switching, clipboard and bidirectional resumable files, audio | Measured decode, input, quality, resource use and recovery across supported machines/networks; integrity and cancellation tests for tools. |
| D — Unattended access | Visible trusted-device management, biometric/passkey or equivalent extra owner protection, revocation, sign-in start and reboot recovery | Reviewed service/session-agent boundary, UAC/secure-desktop policy, real lock/logon/reboot tests; no network parser running broadly privileged. |
| E — Commercial distribution | Stable signing identities, signed installers/updates, complete dependency provenance/notices, supported versions and support diagnostics | Clean install, update, rollback, uninstall and certificate-rotation tests on physical systems; independent security review. |
| F — Expanded parity | iOS controller, native desktop viewer, remote printing, privacy screen, Wake-on-LAN, optional recording, organizational management | Separate platform/privacy/permission acceptance for each feature before advertising it. |

The 0.8 activation grant deliberately enables screen viewing and configured input only.
It does not imply file, clipboard, audio, power or permanent device trust. Adding
those capabilities requires a clear grant/consent design and its own testing.

## Measurable targets to validate

These are proposed product targets, not measurements or guarantees:

- At least four of five representative first-time users complete their first
  connection in two minutes after installation, without coaching or technical fields.
- For online, awake, supported hosts on the approved network matrix, at least
  99% of 200 attempts connect successfully; p95 time to usable video is at most
  ten seconds. Direct and forced relay results are reported separately.
- At 1080p on the documented reference machines and 100 ms network RTT, p95
  input-to-visible-change latency is at most 250 ms at the chosen quality target.
  High-resolution/60 fps modes must report actual decoded frames and resources.
- A normal workday is supported with explicit session authorization renewal,
  successful relay credential renewal and bounded memory use. The current
  coordinator's two-hour session TTL is not an eight-hour acceptance pass.
- Network transitions pause input immediately when video is stale, then recover
  or end with an actionable message within the documented bound. No stuck keys,
  orphan encoders or usable ended-session tokens are acceptable.
- All supported layouts pass keyboard/touch, screen reader, large-text,
  small-screen, landscape, high-DPI and multiple-monitor checks.
- Support exports contain versions, operation codes and selected transport only;
  no PIN, enrollment key, token, clipboard contents, file contents or screen pixels
  are collected without an explicit separate user action.

## Release evidence and dependencies

Every release records source commit, toolchain and dependency inputs, compiled
artifact hashes, signing identity, passed/failed/blocked checks, and device/network
observations. A generated screenshot, simulated state test or synthetic video
cannot be described as a physical Windows-to-phone session.

The current blockers needing outside resources are the real OpenRemote service
origin and authorized host/relay deployment, Windows and Android device runs,
production signing credentials/distribution identity, and reviewed encoder/native
dependency distribution materials. Those prerequisites are visible in the release
status. They are not bypassed or renamed as completed features.
