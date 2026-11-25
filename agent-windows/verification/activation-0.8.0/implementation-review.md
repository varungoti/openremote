# Windows 0.8.0 activation and PC identity review

This record covers source review and Linux-hosted Rust checks. The exact source
inputs for the current checks are in `final-source-inputs.json`; command exits,
durations and logs are in `final-checks.json`. Native Windows execution is not
established by cross compilation or by this review.

## User-visible failure and revised admission

The reported 0.7 failure is reproducible from the source configuration:
`config/service-profile.json` has an empty `apiOrigin`. The previous owner flow
requested a PIN before resolving that profile, so a user reached a provider
configuration error only after submitting a PIN. No real public deployment,
service binding or SSH access was supplied; replacing that with a sample or
fabricated origin would not make remote access work.

The 0.8 owner window resolves managed settings and the existing protected origin
before enabling activation. A missing profile produces the plain service/PC-ID
unavailable message on the initial screen and disables both actions. Keyboard
submission also checks admitted state and cannot enter PIN flow through the
disabled button. A configured build checks service capabilities and performs
protected registration before asking for a PIN. Network failure leaves a retry
state. No manual provider URL, global enrollment secret or encoder path is added
to the ordinary UI.

The service-issued nine-digit device `displayId` is now **Your PC ID**. It remains
on the screen while inactive, entering a PIN and hosting. Existing DPAPI metadata
can supply the previously registered ID while offline; a fresh installation has
no fabricated ID. The internal six-digit activation invitation code stays in the
local grant/service contract and is not mislabeled as the permanent PC ID.
The receiving phone and browser contracts accept the stable PC ID during the
current activation. The native **Connect to a PC** action opens only the checked
configured HTTPS origin's fixed `/connect` route, without credentials in its URL.

## Authorization and bounded resources

The normal host requests `mode: activation` and requires `maxConnections: 4`.
It validates the exact opaque grant, view/input permission ceiling and bounded
wall expiry, then enforces the lifetime using a monotonic clock. Local maximum
activation duration is 24 hours. Legacy no-mode, single-use, ten-minute grants
retain their consume behavior for internal compatibility callers.

Current activation sessions require the same mode and pairing ID, pending status
and a bounded supported permission subset. Ordinary/trusted-controller requests
cannot bypass the active PIN flow. Authorization consumes a session ID once,
not the whole activation. A 4096-entry per-activation replay ledger remains after
individual sessions end; exhaustion invalidates the grant. The local gate,
owner roster and task map all bound concurrent admission to four. The local gate
and roster independently permit at most one input session. Other sessions remain
view only; disconnecting a controller never upgrades an existing viewer.

Each session has its own input guard, cancellation child and WebRTC/capture
owner. Normal individual teardown removes that device without canceling the
activation or siblings. Unverified teardown sets the restart latch and cancels
the parent before waiting for the coordinator end response. A failed/uncertain
coordinator end acknowledgment cancels the activation with a fixed visible
reason rather than silently leaking server-side session capacity.

The current grant is refreshed through authenticated GET pairing, including
mode, ID, code, permissions and exact expiry. A missing/replaced/locked/expired
grant closes the local gate and cancels every child. Repeated inability to
refresh authorization also closes access. Stop and Quit close the gate and
cancel tokens synchronously; remote revocation is bounded and best effort.
Local access does not depend on that network request succeeding.

A legacy session left by an earlier process can block activation with
`409 host_busy`. Only that response permits one recovery before this process
owns any local session. The host lists at most 16 authenticated current sessions,
ends each through the existing idempotent owner endpoint, requires every end
acknowledgment and retries activation once. The cleanup budget is 15 seconds.
Cancellation before or during cleanup prevents retry. The per-user instance lock
is held throughout. A healthy bearer is not rotated and the PC identity is not
reset to escape this condition.

## Connection count and owner visibility

The roster initially records a device as Connecting. It becomes Connected only
after an accepted answer, a fresh sent video sample, healthy peer/ICE state and
an acknowledged coordinator `connected` action for the same answer generation.
A new offer explicitly clears answered readiness. A stale acknowledgment cannot
mark its replacement generation connected. Recovery or terminal reporter exit
clears the connected count before native teardown finishes. The production
readiness predicate has a focused regression for no answer, no media, missing
acknowledgment, stale generation, disconnected ICE and a missing fresh sample.

Device names are bounded, normalized descriptive text; control characters, line
breaks and directional formatting are removed. Tooltip truncation counts UTF-16
units and never splits a surrogate pair. The tooltip and window distinguish the
actual connected count from connecting/reconnecting entries and show which
session can control input.

The notification icon uses native `Shell_NotifyIconW`. The window hides only
after service admission, registration, encoder preflight, grant installation and
successful native icon addition. Icon failure leaves or restores the window.
Explorer's `TaskbarCreated` notification causes icon recreation. Left-click opens
the owner window; the menu provides Open, Deactivate and Quit. Closing or
minimizing an active window with a known tray icon hides it; closing without that
icon revokes and exits. Deactivation/failure restores the owner window. This is
an interactive signed-in desktop process, not an SCM or Session-0 Windows service.

Independent review found that a short/high-DPI client area could collapse the
device roster because its positions were fixed while the footer moved upward.
The correction uses a 520-logical-pixel minimum *content* extent, native vertical
scrolling, shared child-position offsets and keyboard focus reveal. It does not
force a window larger than the monitor work area. The reviewer confirmed that
thread-local PANEL borrows end before potentially reentrant native calls such as
SetScrollInfo, MoveWindow, ShowWindow and DestroyWindow.

## Cancellation and panic regression preservation

The existing owner-thread `RevokeOnExit` and `supervised_attempt` guards are
retained. The latter lives outside the potentially panicking attempt task, so
its token is canceled before a terminal UI result and also when the supervisor
is aborted. New session children have their own exit guards. The process panic
hook cancels the application token before any blocking native fatal alert.

The Linux regressions still require cancellation of detached children on attempt
panic and owner abortion. A new four-child test requires all live entries to
clear when the activation owner is aborted. Another test checks that completing
one child leaves its sibling and parent uncanceled. Historical isolated negative
control evidence for removal of the supervisor guard remains under the 0.7
verification directory and is not relabeled as a new 0.8 execution.

## Development checks and limits

Intermediate mechanical checks found an extra-argument lint after adding the
connection monitor, a large native event enum, and missing/correctly typed
Windows scrollbar imports/constants. The capture source is now derived from its
existing display selection, the admission payload is boxed, and the native
scrollbar declarations use their correct module and signed command type. These
intermediate results do not substitute for the final pinned checks.

The frozen final Rust suite contains 113 tests. Formatting, all-target Linux
tests, Linux Clippy and Windows-target Clippy passed with warnings denied before
the release build. `BUILD_EVIDENCE.json` records the completed release build and
independent PE inspection when available. Native UI, Explorer restart, keyboard
and screen-reader behavior, small-window/DPI rendering, DPAPI, screen capture,
Media Foundation, input injection, installer operation and physical phone/browser
Internet sessions remain unexecuted. The binary is unsigned. An empty service
profile is a distribution blocker; it is not evidence of a usable installer.

## Independent final read-only verdict

The second agent re-read the final teardown/count/orphan-cleanup delta and
reported no additional blocking finding. It confirmed the task-map limit before
reservation, immediate restart latch and parent cancellation on unverified
teardown, fixed visible failure on an unacknowledged end, fresh media and current
generation acknowledgment before counting a connection, owner-authenticated
bounded orphan cleanup, and the scrollbar correction. This is an independent
source review, not an executed native Windows test or a duplicate final build.
