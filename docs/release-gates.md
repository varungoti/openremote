# Release gates

Updated 19 November 2026. **Public release is blocked.** These gates apply to the
expanded Windows/Android product in [PRODUCT_PLAN.md](../PRODUCT_PLAN.md).
A prepared script, successful compilation or SDP exchange does not close a runtime
gate. Record the exact source revision, artifact hashes, environment and observed
result when closing a gate.

## Current engineering gates

| Gate | Required evidence | Current state |
| --- | --- | --- |
| E01 — Protected coordinator | Key-possession login, replay rejection, actor checks, permission narrowing, expiry and revoked-phone tests | Passed in Go/security integration tests |
| E02 — Durable PC identity | Real coordinator restart preserves PC ID and host bearer; no plaintext token in snapshot | Passed in independent HTTPS process test |
| E03 — Restart and storage correctness | Event cursor not reused after restart; no event published after failed persistence | Failing-then-passing regressions retained; race suite passed |
| E04 — Buildable native clients | Windows executable and Android test/release packages from recorded source inputs | Current build results in BUILD_STATUS.md; unsigned/test identities |
| E05 — Video encoder path | Production argument builder/access-unit assembler produces moving, decodable H.264 | Passed: 30 access units, 30 decoded frames, 30 distinct hashes |
| E06 — New session tools | Clipboard authorization/limits; file name, offsets, quotas, digest, cancel and cleanup tests | Unit suites passed; current counts in BUILD_STATUS.md; native tools behavior remains open |
| E07 — Dependency provenance | Locked graphs, exact artifact hashes and complete notices for shipped native components | Partial; unresolved native provenance/notice gaps are explicit |
| E08 — Packaging and upgrade | Compatible verified encoder, staged version, preserved identity and rollback on failure | Four actual activation/rollback cases pass on Linux PowerShell; Windows installation not run |
| E09 — Selected display contract | Immutable selector without new grants, strict catalog, input/capture geometry, fresh peer/renderer and stale-callback rejection | Implemented in 0.4; current test/build results in BUILD_STATUS.md; native mapping/cleanup remains open |
| E10 — Pilot record integrity | Candidate/evidence bytes, revision, required scenario observations and rejection of incomplete pass claims | 13 validator tests pass; no actual pilot executed |
| E12 — Code/PIN session authorization | Atomic one-use redemption, expiry/lockout/budgets, matching local host grant, strict client parsing and cancellation/late-result tests | Implemented in 0.6; exact executed results in BUILD_STATUS.md; physical first-use remains open |
| E13 — Receiving-PC browser lifecycle | Same-origin P-256 proof, fresh-video/display-bound input, stop cleanup, stale-result rejection and error redaction | Protocol plus real HTTP and actual-app VM tests pass; browser rendering/media remains unexecuted |
| E14 — Shared configuration provenance | Public-only origin profile in both clients, saved origin preserved, artifact rebuilt and pinned to profile bytes | Implemented with an empty profile; actual service not provisioned |
| E11 — Phone setup contract | Secret-free locator, raw URL validation, exact confirmation, origin-bound settings operations, bounded discovery and actionable checks | Implemented in 0.5; build/source evidence in BUILD_STATUS.md |

The additional [commercial release standard](commercial-release-standard.md) defines the required turnkey setup and usability targets. These are targets to validate, not achieved benchmarks.

## P0: physical connection and daily use

These gates are required even for a private pilot that people depend on.

| Gate | Acceptance | Current state |
| --- | --- | --- |
| P01 — Windows desktop | Capture real changing desktop pixels and inject intended input on an owned Windows PC | Not run: no Windows device session |
| P02 — Android session | Install on ARM64 phone, exercise Keystore, render real video, send input and end cleanly | Not run: no attached phone/emulator |
| P03 — Native interoperability | Independent controller receives moving H.264 and exact control messages from Rust peer | Blocked by workspace network-interface discovery policy; zero received media claimed |
| P04 — Browser interoperability fixture | Chromium decodes moving frames and verifies actual selected relay/input | Blocked before browser launch by workspace socket policy |
| P05 — Direct Internet path | Phone and PC on different networks; record selected candidate, frames and input | Not run |
| P06 — TURN UDP/TCP/TLS | Force each transport, including TLS/443 on suitable relay IP, and verify selected relay candidate | Not run; local fixture compiled and public configuration prepared |
| P07 — Renewal and loss | Session beyond relay credential expiry, Wi-Fi/mobile switch, 20-second recovery boundary, server and relay restarts | Source/test coverage partial; physical matrix open |
| P08 — Consent and revocation | Wrong phone denied, grants narrowed, revoke stops access, emergency disconnect always works | Coordinator tests pass; native UI matrix open |
| P09 — Clipboard and files | Explicit actions work across devices; hash matches; no completed file after cancellation/error | Unit tests passed; native integration open |
| P10 — Native installation | Clean install, existing identity upgrade, failed upgrade rollback, uninstall and re-enrollment | Not run on Windows/Android |
| P11 — Session cleanup | No held input, orphan capture process, partial file or usable ended-session token | Source/unit coverage partial; hardware fault matrix open |
| P12 — Phone first use | Native host copy/Stop, Android cold/warm intent, paste/cancel/confirm, rotation, server change, stale operation rejection and diagnostic failures | Not run; additional cases in phone-setup.md |

The network restriction is an execution blocker, not a waived test. Run the
prepared fixtures on an authorized runner with ordinary required capabilities;
do not change the restricted workspace's socket/interface controls to manufacture
a pass. A later synthetic fixture pass still does not replace real-device tests.

## P0: advanced first general release

The user's requested advanced product needs these implemented capabilities as
well. They are not reduced to optional polish because the preview compiles.

| Gate | Requirement | Implementation state |
| --- | --- | --- |
| A01 | Monitor selection, correct DPI/input mapping and display hot-plug recovery | 0.4 source implements GDI selection via new session, pinned geometry and observed-topology shutdown; physical mixed-DPI/portrait/hotplug validation remains open |
| A02 | Capability-tested hardware encoding, adaptive bitrate/resolution, measured 1080p60 and safe fallback | Planned; current software profile is up to 720p30 |
| A03 | PC audio, mute/volume, reconnect and A/V synchronization | Planned |
| A04 | Bidirectional/resumable file exchange with scoped host export and conflict handling | Planned; current transfer is bounded phone-to-PC upload only |
| A05 | Host settings/trust management UI and deliberate startup-at-sign-in | 0.5 visible PC/setup/Stop home implemented; settings/trust management and sign-in startup remain planned |
| A06 | Service/session-agent lifecycle and reboot/logon recovery | Planned; current host needs a logged-in desktop |
| A07 | Secure-desktop policy, reviewed privilege boundary and independent peer identity binding | Planned |
| A08 | Android biometric gate for unattended access and tested accessibility/phone/tablet ergonomics | Planned/partial |
| A09 | Signed installer, release APK/AAB identity, publisher verification and signed update rollback protection | Production signing and updater absent |
| A10 | Supported-system performance/reliability report, operational capacity and tested recovery | Not completed |

Wake-on-LAN, privacy screen, fleet/RBAC, invitations, recording and additional
platforms follow separate milestones in the product plan. They must not be
advertised as available before implementation and their own acceptance tests.

## P0: security and distribution

- Independently review authentication, first pairing, consent, revocation, protocol
  parsing, received files, service IPC and update trust before public release.
- Verify no clipboard/file/pixel/text/credential contents enter logs or support
  bundles. Test redaction, retention and deletion behavior.
- Test malicious JSON, duplicate fields, SDP/ICE payload limits, candidate floods,
  oversized DataChannel messages, file traversal/reparse races and rate limits.
- Apply all authorization checks at the receiving host. Phone UI visibility is
  not an authorization boundary.
- Complete exact WebRTC native source/build/notice provenance and remaining
  dependency notice gaps. Do not substitute a nearby upstream commit for the
  binary's actual source. Review exact FFmpeg materials before bundling a build.
- Bind a reviewed dependency inventory/SBOM to final signed binaries. Verify
  signatures, publisher identity, tampered update rejection and downgrade refusal.
- Exercise first enrollment, stolen/expired/revoked token cases, proof replay,
  origin changes, TLS certificate failures and coordinator persistence failure.
- Finish abuse controls for exposed deployments, including pairing/prompt flooding,
  authenticated lookup enumeration and capacity exhaustion.

## Required device and network matrix

Use [pilot-validation.md](pilot-validation.md) to create a candidate-bound record
with all 17 pilot scenarios initially `not_run`. Validation checks records and
bytes; it cannot attest execution or close unrelated general-release gates.

Record results for each supported configuration, with exact artifact hashes:

- Windows versions/editions advertised by the release; standard-user and elevated
  applications; mixed DPI, portrait displays and monitor hot-plug.
- ARM64 Android devices across supported OS ranges, including the intended personal
  phone; orientation, background/resume, memory pressure and 16 KB page devices.
- LAN, separate home broadband, mobile carrier NAT, IPv6/dual-stack, VPN, enterprise
  firewall and UDP-blocked networks; direct and every claimed relay fallback.
- 0/1/2/5% loss, 20/80/150/300 ms RTT, bandwidth limits and short/long disconnects.
- Two-hour sessions, multiple enrolled PCs, simultaneous requests, permission
  narrowing, denial, timeout, stale cursor, local stop, revocation and crash.
- Native clipboard limits and Unicode; empty/boundary/64 MiB files, duplicate names,
  wrong hash, cancel during transfer, full disk and process termination.
- Sleep/wake, lock/unlock, logoff/logon, reboot, encoder failure, relay failure,
  coordinator restart, storage error and interrupted installation/update.

## Release record and decision

The owner-facing release record must contain the version, source revision, signed
artifact identities, dependency inventory, supported systems, test outcomes,
measured limitations, operating instructions and rollback procedure. An explicit
unresolved list is mandatory. No P0 gate can be marked passed based on an assumption
or hidden fallback. BUILD_STATUS.md is evidence for this build; this document is
the completion criteria.
