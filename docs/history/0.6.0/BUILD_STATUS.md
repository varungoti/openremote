# OpenRemote 0.6.0 — code/PIN preview and commercial release gaps

19 November 2026. **This is a development preview. Commercial release is blocked.**

## User-facing changes

The Windows host now generates a six-digit, one-session connection code after
the owner chooses and confirms a 6–12 digit PIN. Android puts code and PIN on
its primary screen. Another PC can use the coordinator's browser viewer, opened
by the host's **Connect to another PC** button or directly at `/connect`.

A shared public service profile can supply the initial HTTPS origin to both
native clients. Existing saved/enrolled origins take precedence. **The delivered
profile is empty:** no public OpenRemote coordinator or relay has been deployed.
First host enrollment and a separately trusted compatible FFmpeg executable are
still needed. Turnkey provisioning and encoder packaging remain release work.

The user has required commercial parity and exceptional ease of use. The updated
[commercial release standard](docs/commercial-release-standard.md) makes setup,
reconnects, performance, advanced tools, updates and support explicit acceptance
requirements. It does not claim those requirements have already been achieved.

## Authorization behavior

Codes are cryptographically random, six decimal digits including leading zeroes,
and unique among active grants on a coordinator. They expire after ten minutes,
can be replaced/revoked, and are consumed atomically once. Five wrong PINs lock a
code. Salted PBKDF2-HMAC-SHA256 uses 600,000 iterations; PIN verifiers and pairing
records stay in memory and disappear on coordinator restart. The server also
applies proved-controller, source-address, host-generation and global budgets,
with two simultaneous expensive verifications.

Successful redemption creates a pending session, with the authenticated controller
identity and no expanded permissions. The host must still hold and consume the
matching unexpired local grant before it accepts. Closing/restarting/revoking the
host grant fails closed. Ordinary local approval and explicitly configured trust
remain unchanged. Code grants allow screen viewing and optionally input only;
they do not enable clipboard, files, audio, power or permanent unattended trust.

PIN fields clear before submission and on relevant cancellation/lifecycle paths.
One-use redemption is not blindly retried. A known late session is ended after
cancellation; a completely lost response cannot give the client a session token
it never received. Server pending/media timeouts and the visible host stop remain
necessary recovery mechanisms.

## Executed verification

The final aggregate report at `evidence/code-pin-0.6.0/verification.json` links
the component command records and records input/artifact hashes, counts,
failures and boundaries.

| Area | Executed result | What this establishes |
| --- | --- | --- |
| Windows host | 87 Rust tests, formatting, Linux/Windows Clippy with warnings denied and optimized cross-link passed | Compiled host logic and target linkage; not actual Win32 execution. |
| Windows binary inspection | AMD64 GUI executable, 23 system DLL imports; independent LLVM inspection | Expected architecture/subsystem and no new unbundled runtime DLL. |
| Windows installer | 11 admission/custody tests; NSIS warnings treated as errors; complete independent 7-Zip payload extraction passed | Package bytes match the verified host, guide, notices, manifest and generated native uninstaller. |
| Coordinator | 43 top-level tests / 107 including subcases passed in both normal and race-detector runs; vet and static build passed | Pairing/session/API state, races and compiled service. |
| Browser protocol | 15 tests passed, including independent P-256/ASN.1 verification, strict JSON, grant and display/input validation | Production protocol module executes correctly in Node; not a rendered browser. |
| Browser lifecycle | 14 tests passed against actual app modules with DOM/RTC/clock doubles | Cancellation/late results, no retry, input gates, stop and state ownership; not actual media. |
| Actual HTTP pairing | Six grouped real coordinator HTTP checks passed with production browser protocol and Node WebCrypto | Actual cross-language proof, wrong PIN, redemption, host event/approval, one-use replay, ICE access and stop/revoke contracts. |
| Python deployment/profile/pilot validation | 19 tests passed | Configuration and evidence validators; not a deployed service or pilot. |
| Android | 76 JVM tests passed; lint has 0 errors and 3 dependency-update suggestions; strict/offline debug and minified release builds passed for four ABIs, plus ARM64 signature and 16 KiB ZIP/ELF checks | Compiled app and JVM/HTTP tests; physical Android execution remains open. |
| Verified HTTPS and process restart | 12 grouped checks passed; report at `evidence/code-pin-0.6.0/https-controlplane.json` | Isolated coordinator lifecycle using a verified ephemeral test CA, including pairing and restart; no desktop pixels. |

### Failures retained and corrected

Independent browser review reproduced two actual lifecycle defects before their
fixes: a previous disconnect failure could overwrite a newer connection result,
and missing fullscreen APIs could throw uncaught exceptions. Their failing and
passing runs are preserved. Parser-error text was also restricted to controlled
messages, and pairing input now requires a primary-display catalog.

The PIN-check negative control intentionally removed verification in an isolated
copy; the wrong-PIN regression failed as required. Intermediate toolchain, metadata,
fixture and compile failures are retained in the component history rather than
removed from the record. No production verifier or test requirement was disabled
to achieve a pass.

The first final coordinator race run also exposed a test-clock problem: creating
twelve real PIN verifiers under contention took long enough for the fixture's
first host heartbeat to expire. Production correctly rejected that stale host.
Only the uniqueness fixture was given a fixed logical clock; the actual presence
and expiry tests remain. The focused regression and complete normal/race suites
then passed. Both failed and successful evidence are retained.

### Blocked and not-run checks

Chromium aborted before loading a page because the workspace denied its process
singleton socket and ptrace operations. The blocked result is retained in
`chromium-launch-blocked.json`. No restriction or browser flags were altered to
manufacture success. Browser visual layout, decoded video and real keyboard
behavior have not been verified here. Node VM state tests are not substitutes.

No physical Windows install/upgrade/uninstall, DPAPI/setup/PIN UI, capture,
SendInput, browser launch, monitor layout, or Windows-to-phone session ran.
No Android phone/emulator installation, Keystore/media/input, carrier network,
public TURN UDP/TCP/TLS/443, outage or real network handoff was executed.

## Distribution status

The Windows installer is unsigned. The Android test APK is **OpenRemote Preview**,
package `com.openremote.mobile.dev`, using a new development certificate. This
allows side-by-side installation with the older app whose debug signing key was
not present in the recovered source. It does not preserve that old app's identity
inside the new app or count as an in-place upgrade. The new preview key is retained
privately for future preview updates. Production release package identity remains
`com.openremote.mobile`; a production signing identity was not supplied.

The installer is for Intel/AMD Windows x64, minimum Windows 10 version 1703
(build 15063), or Windows 11. The phone artifact is ARM64 for Android 8 or later.
The browser viewer requires current WebRTC/H.264/WebCrypto/frame-callback support;
a browser compatibility matrix has not been executed. No iOS package is provided.

The preview preserves the earlier versioned per-user installation, current-host
lock, shortcut ownership and rollback checks. This does not establish physical
installer behavior, power-loss recovery or full upgrade safety.

## Open release requirements

- A real, authorized OpenRemote coordinator/relay deployment, origin profile,
  TLS/DNS, operational backups, monitoring and tested recovery.
- Turnkey compatible encoder packaging and scoped host enrollment provisioning.
- Physical Windows/Android/browser first-use, install, video, input, stop,
  multi-monitor, lock/unlock, failure and network matrices.
- Stable production signing identities, signed updates and rollback protection.
- Hardware/adaptive high-resolution video, audio, bidirectional/resumable files,
  polished trusted-device controls, service/logon/reboot/UAC policy, and iOS.
- Independent security review and complete distribution provenance/notices.
  Previously documented FFmpeg/native WebRTC and four Rust notice gaps remain;
  this feature update does not claim to close them.

See `docs/release-gates.md`, `docs/commercial-release-standard.md`, the installer
and connection guide, and the exact component evidence. All earlier foundation,
0.5 and 0.5.1 source/history remains in Git. No public service, paid infrastructure,
unrelated repository, PR publication or release announcement was created.
