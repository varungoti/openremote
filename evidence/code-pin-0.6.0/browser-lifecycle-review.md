# Independent browser lifecycle review

## Scope and result

The actual production `app.js` and `core.mjs` were evaluated as Node VM ES modules. Small explicit doubles supplied DOM events, HTTP responses, timers and RTC transitions. The controller source was not rewritten or replaced by a copy of its implementation in the test.

**Final result: 14 tests passed, 0 failed, 0 skipped.** No pinned input changed during the successful run. Node version and complete source hashes are recorded in `browser-lifecycle.json`; the executable output is in `browser-lifecycle.log`.

This is evidence for application lifecycle, request sequencing, UI state and input authorization decisions. It is **not** evidence for browser rendering, native browser focus/keyboard behavior, real decoded video, Windows capture/input, Android execution or an Internet/TURN connection. Chromium was unavailable in the parent execution environment because process startup hit an OS socket restriction; this review did not weaken or bypass that restriction.

## Defects reproduced and corrected

### A stale stop result could replace a newer connection result

Session A's asynchronous stop request could fail after a newer connection attempt B had already failed and returned to the home screen. The old callback checked only whether the application was idle. It could therefore replace B's useful connection error with A's old stop warning.

The regression executes that exact ordering in the production app module. It failed before the change. The parent owner fixed the application by capturing the sequence after stopping A and requiring that same sequence before rendering its delayed stop warning. The same regression now passes.

### Unsupported full screen caused an uncaught exception

The full-screen button called `requestFullscreen()` without checking that the browser supplied the method. On an unsupported surface, the synchronous TypeError escaped the event handler.

The regression removes that capability from the DOM fixture and invokes the production event handler. It failed before the change. The parent owner added availability checks, synchronous exception handling and Promise rejection handling for entry and exit. The regression now observes the actionable unavailable message and passes.

## Preserved failure map

| Evidence | Result | Interpretation |
|---|---|---|
| `browser-lifecycle-first.json` and `.log` | 11 passed, 3 failed | Two production defects above, plus one test timing error. |
| `browser-lifecycle-before-fixes.json` and `.log` | 12 passed, 2 failed | Corrected test timing; both production defects still reproduced on unchanged production inputs. |
| `browser-lifecycle.json` and `.log` | 14 passed, 0 failed | Same regressions passed after the parent's production fixes. |

The initial input-freshness assertion expected held-button release at 2600 ms after the last frame. The application blocks new input synchronously after 2500 ms and releases already held input on the following 250 ms maintenance tick. The corrected test checks new-input denial at 2600 ms and held-input release at 2750 ms. No production code was changed to satisfy that test correction.

## Behaviors exercised

- Cancellation during discovery prevents authentication and redemption.
- Cancellation during proof verification prevents sending the code or PIN.
- A one-use redemption already sent is not retried or aborted by ordinary cancellation; a known late session is explicitly ended without opening a desktop.
- A lost redemption response produces an uncertain-result instruction without exposing transport details.
- An escalated grant is ended before a peer is opened.
- Pending sessions cannot create a media peer; repeated stop is idempotent.
- Input requires the peer, open channel, allowed permission, valid primary-display catalog and fresh rendered-frame event together.
- Missing or wrong display identity cannot enable input.
- Page hide releases held input, closes the peer once and sends a keepalive stop request.
- Focus/visibility loss releases held input; hidden pages cannot inject new keys.
- Permission narrowing gates input; attempted restoration of a removed permission ends the session.
- A changed host identity ends the session without reusing the old peer.
- Late cleanup cannot replace a newer attempt's UI result.
- Missing full-screen support is handled without an uncaught exception.

## Source review observations

Host labels and status text use text content rather than HTML insertion. The first-party page has explicit field labels, keyboard focus targets, a live status region, a cancel action during setup and an always-visible session disconnect control. These are source observations only; layout, contrast, screen-reader behavior and actual browser keyboard interception remain unverified.

The reviewed code uses fixed first-party `UserMessage` errors for user-facing messages. Remote JSON parsing failures are converted to fixed text. The browser's primary-display gate now requires a catalog from the current host rather than accepting a missing identity. The parent-owned core tests separately cover malformed protocol objects and actual P-256 signature interoperability.

## Reproduction

From the repository root:

```sh
node --experimental-vm-modules --test tests/browser/controller-lifecycle.test.mjs
```

The test writes its own exact-input report to `evidence/code-pin-0.6.0/browser-lifecycle.json`. Set `OPENREMOTE_BROWSER_LIFECYCLE_EVIDENCE` to a new path to retain another run independently.
