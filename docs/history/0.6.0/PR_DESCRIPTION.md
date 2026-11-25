# Add one-session code/PIN connections across Windows, Android and the PC browser

Connecting previously required a server origin, nine-digit PC lookup and a
separate local approval. The requested everyday flow is an owner-generated
six-digit code plus the PIN chosen on the host, with configuration supplied by
the deployment and clear connection/stop controls.

This change adds bounded ephemeral pairing records, salted PIN verification,
wrong-PIN lockout and atomic single-use redemption. Redemption creates a pending
session; the Windows host automatically accepts only its own matching, unexpired,
unconsumed local grant. Existing consent/trust behavior and permission ceilings
remain in place for ordinary requests.

The native host gains Generate/Revoke/Copy/countdown controls and a button opening
the configured receiving-PC browser viewer. Android puts code/PIN first, moves
older PC lookup under Advanced, and handles canceled/uncertain one-use requests
without replay. Both clients consume one public service profile while preserving
saved origin bindings. The profile intentionally has no real endpoint yet.

The same-origin embedded browser viewer adds P-256 proof, WebRTC video,
primary-display/fresh-frame input gating, pointer/keyboard/text, full screen and
visible disconnect. Independent lifecycle review reproduced and fixed stale
cleanup overwriting a newer result and unsupported-fullscreen exceptions.

## Validation

Exact counts, commands, source hashes and artifacts are in BUILD_STATUS.md and
evidence/code-pin-0.6.0. Component tests, Go race checks, real HTTP/HTTPS contract
checks, native package builds and independent installer extraction are recorded
separately from unexecuted physical device/media tests. Failed intermediate checks
and corrected regressions are retained.

## Remaining release requirements

This is a development preview. Actual deployment and configured origin,
turnkey host enrollment/encoder packaging, Windows/phone/browser device sessions,
Internet/TURN matrix, stable production signing and remaining advanced capability
work are still required. Android Preview uses a separate package identity to
preserve the old installation whose debug signing key is unavailable. No public
service, unrelated repository, paid infrastructure or release publication was
created. The commercial usability standard is an acceptance plan, not a parity claim.
