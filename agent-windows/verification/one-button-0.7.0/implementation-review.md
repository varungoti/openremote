# One-button Windows 0.7.0 implementation review

This record covers source inspection and Linux-hosted Rust validation. It does
not establish execution of the Windows UI, DPAPI, Media Foundation, desktop
capture, native input, installer or a physical phone connection.

## Managed entry point and visible controls

- `main.rs` keeps the per-user instance lock and enters a persistent native owner
  window. Both ordinary launch and the retained `--configure` argument use it.
- There is exactly one native `BUTTON`: Connect remote desktop while idle or
  entering a PIN; Stop remote desktop while preparing or hosting. PIN entry is
  a masked six-character field with ASCII-digit validation and preserved leading
  zeroes. Enter or that same primary button submits it; no confirmation field.
- The normal window exposes no service/enrollment/encoder fields, settings,
  imported links, QR codes, copy/viewer buttons, permanent PC number or fingerprint.
  An unconfigured build produces the prescribed provider-contact message in the
  still-open window. The repository's public service profile remains empty.
- Status and code fields are updated only on the owning window thread. Native
  control calls occur outside mutable thread-local RefCell borrows, avoiding
  synchronous notification reentry with a borrowed panel.
- Operational errors remain visible and offer the same Connect action. Generation
  counters reject obsolete Ready/Finished events. The restart-required state is
  not cleared by Escape or a stale event.

## Automatic configuration and identity

- `managed_config` retains valid saved permissions and trust. A DPAPI-bound
  enrollment origin takes precedence. Without that binding only the provider's
  strict, canonical HTTPS profile can select a service; there is no manual setup
  fallback. Malformed existing configuration is not silently replaced.
- The bundled encoder module configures a pinned sibling ffmpeg.exe and the
  Media Foundation codec. The window never asks for its path or digest. Preflight
  verifies the file, GDI/codec availability and actual synthetic H.264 framing via
  the production argument builder. Process output and execution time are bounded.
- `register_or_load` verifies stored origin and public-key bindings before using
  a bearer. A healthy stored bearer works without requiring auto-registration to
  be enabled. Only a heartbeat 401 starts same-key recovery; other failures do
  not silently register again. No identity deletion or admin-secret fallback occurs.
- The challenge parser checks canonical base64, payload bounds, exact nine-line
  purpose/origin/ID/nonce/key/name/platform/expiry/trailing-newline structure,
  a canonical 32-byte nonce, and a live expiry no more than 90 seconds ahead.
  The backend's own TTL is 60 seconds. Pure Ed25519 signs the exact decoded bytes.
- Responses are bounded, redirects disabled, TLS required in the managed flow,
  and returned key/name/identifiers/token shape checked before DPAPI persistence.
  An interrupted registration response can be recovered on a later explicit
  Connect using a fresh proof and the preserved key. Active sessions do not rotate
  registration credentials. The server invalidates older outstanding challenges
  after successful recovery; the client never reuses an old proof automatically.
- The backend agent independently reviewed the signer, registration/recovery and
  origin-precedence source and reported no blocking protocol mismatch.

## Local grants, Stop, and cleanup

- The fresh pairing grant is installed locally before the Server ID event is
  sent to the UI. Its in-memory monotonic expiry and one-time consume gate remain
  authoritative. Pairing does not modify permanent trusted-controller settings.
- Simple grants include screen view and optional existing input permission only.
  Clipboard, files, audio and power stay false. Unknown historical grants are
  filtered before occupying the single-session slot; consuming still checks the
  exact grant, pending status, monitor restriction and permission ceiling atomically.
- Stop and window close first close the local grant and cancel the attempt. A
  canceled network request cannot install a late grant or revive a code on screen.
  After API establishment the attempt always makes a bounded revoke request.
- Encoder preflight owns its process cleanup: cancellation does not drop the
  reaping future. A failed or timed-out reap latches restart-required. Session
  teardown retains the same input release, encoder reap and peer-close invariants.
- The managed session entry point explicitly tells session.rs that its persistent
  owner window already provides Stop and the emergency hotkey. Non-managed callers
  retain the original separate notice. Untrusted ordinary requests are rejected
  before they can introduce an old fingerprint/approval dialog into the managed UI.
- The host checks local expiry and task completion both before each poll and on a
  periodic poll timer, so immediate coordinator responses cannot starve cleanup.
  Before returning to idle it waits for bounded teardown; panic or uncertain cleanup
  keeps retries closed until restart. Normal window close waits for that outcome.
- Initial independent protocol review found no registration mismatch. A further
  lifecycle review then found a real panic boundary: the wrapper caught an attempt
  panic without canceling the attempt token, so session/heartbeat tasks detached
  by unwinding could outlive the apparent failure. The pre-fix successful checks
  and binary identity are retained under `history/before-panic-cancellation-guard/`
  and that build was not packaged.
- The production supervisor now owns an RAII cancellation guard outside the
  panicking attempt task. It cancels that token before returning any result to
  the UI wrapper and also on owner-task abort/unwind. Only afterward can the
  wrapper publish Finished and disable/re-enable controls. The restart latch
  continues to block reuse after a panic. Two focused asynchronous regressions
  require detached child cancellation before panic failure is reported and
  require cancellation when the owner itself is aborted. An isolated negative
  control removes only that guard, using a distinct Cargo target directory;
  both focused tests failed as expected (exit 101), while all 99 tests pass with
  the production guard. The lifecycle reviewer confirmed by source inspection
  that cancellation precedes Finished and the confirmed blocker is closed.

## Verification records and limits

`final-checks.json` records exact commands, exit codes, durations and all 26 source
inputs, including both compiled JSON manifests. Source hashes are checked again
between commands. Initial Windows checking found three moved-module references
that still named the binary crate; the failure is retained in
`windows-clippy.initial.log`, and the references were corrected to `crate::`.
The subsequent intermediate Windows check passed. Final checks are recorded
separately against the frozen source and must be used for the packaged binary.

The Linux suite contains 99 tests, including new registration-proof validation,
one-button state/cancellation, promptless configuration/identity preservation,
old-event filtering, bundled encoder and Media Foundation argument coverage,
and the retained input, media, topology, tools and teardown regressions. The
pure state and process tests are meaningful failure-boundary checks; they do not
run Windows functions. Native work-area/DPI rendering, key navigation, accessibility,
startup recovery, capture, OS H.264 encoding and actual Stop during a live Windows
session remain outstanding release gates. A configured Internet coordinator/TURN
deployment and signing are also not supplied by this evidence.
