# Windows code and PIN increment

This record describes source review and cross compilation. It is not evidence of running the Windows UI, installer, DPAPI, desktop capture or input on a Windows device.

## Behavior

- The host home window starts with PIN, confirm PIN, Generate code, Revoke code, Copy code and a six-digit code/countdown. The existing service address and previous PC-number connection method remain below that flow in the scrollable window.
- An explicit **Connect to another PC** button opens the configured HTTPS server's `/connect` browser viewer using Windows ShellExecuteW. It never opens automatically and passes no query, PIN, credentials or command-line arguments. A failed OS launch shows a fixed error plus the validated public viewer address. This is a browser viewer entry point; in this version the installed host must finish its existing setup before the home window is available.
- A PIN must be exactly 6–12 ASCII digits. Leading zeroes are preserved. Both entries must match, and the native password edits are cleared before an HTTP request begins. No PIN is included in a setup link, diagnostic log or saved configuration.
- The creation response is bounded to 4096 bytes and strictly decoded. Code, grant ID, expiry and permission ceiling are validated before the local grant is installed. The code is shown only after installation. A monotonic deadline caps local authorization at ten minutes even if the wall clock subsequently changes.
- Only the generated screen/view and optional input permission are granted. Clipboard, files, audio and power remain off. This does not create an unattended trusted-controller entry.
- A pending pairing request must match the exact unused local grant ID, have no monitor selector and stay within its permission ceiling. The local grant is consumed atomically before capture setup and server approval. Unknown, restarted, expired, revoked or reused pairing grants are rejected before the ordinary trusted-controller policy is considered.
- The session identity, pairing ID, monitor selector and permissions are checked again before approval. Existing single-session, display binding, visible stop, input cleanup and teardown behavior remains in force.
- Server code status may disappear because redemption has created a pending event. Such a status response hides the code but does not erase the local authorization before that matching event arrives. Its existing monotonic expiry still applies.
- Revoke clears local access first. It increments a generation so a delayed creation response cannot reauthorize. Generation remains serialized until bounded remote cleanup finishes. Closing the owner window closes the local gate, cancels work and attempts bounded coordinator cleanup. A network failure never restores local permission.
- Async results carry their generation. An older status result cannot alter the newer operation. The Generate button and handlers observe busy/restart-required state; ordinary and pairing requests retain server/host concurrency gates.
- New native control reads/writes occur outside thread-local RefCell borrows, avoiding synchronous edit/focus notification re-entry while a mutable borrow is held.

## Public service profile

The host embeds `config/service-profile.json`. Exactly schemaVersion, serviceName and apiOrigin are accepted; duplicate keys, unknown fields, incompatible versions, credentialed/path-bearing URLs and sample origins are rejected. Empty apiOrigin is an explicitly unconfigured build. Only a missing settings file with no encrypted enrollment binding receives a profile origin. Existing saved settings and enrollment always take precedence. Enrollment authorization and a separately verified FFmpeg remain prerequisites; no public Internet service is bundled or deployed by this change.

## Validation

The final source map, commands and logs are in this directory. The eleven new unit regressions cover PIN confirmation, leading zeroes, exact once-only grants, restart/unknown grants, cancellation and late responses, expiry, permissions, simultaneous local redemption, view-only ceilings, first-run profile precedence/schema, and the browser viewer's fixed HTTPS origin/path with rejection of credentialed, executable and query-bearing inputs. The retained initial check records a Clippy helper-argument warning, corrected by grouping related services; it was not a runtime failure. Final formatting and native/Windows-target Clippy use warnings as errors.

The synthetic peer was extended to emit the production capabilities and primary-display catalog for an explicitly synthetic 640×360 pattern. It still logs input rather than injecting it and grants no clipboard/file operations. Its metadata helper returns only a catalog and does not expose fabricated capture/input selection. Browser end-to-end execution, if performed, is recorded separately by the integration harness.

Native Windows UI/reentrancy/browser launch, install/upgrade/uninstall, real desktop capture/input, physical Android behavior, real Internet direct/relay paths and production signing remain outside this execution evidence.
