# OpenRemote 0.5 setup and diagnostics — independent source review

## Decision and scope

The reviewed phone setup and connection-check slice has **no unresolved concrete
source finding identified by this focused review**. This is a development
candidate review, not production release approval, a native runtime result, or a
complete security audit. The component owners are finishing the required final
build/test runs separately; their current evidence belongs in `BUILD_STATUS.md`
and the consolidated verification record.

Review baseline: `2941212aac2738f1035857f1388070f13d6b3904` on
`work/release-hardening`. Review was performed on the changed working tree and
is pinned by the source hashes below, so a later Git commit does not substitute
for the reviewed bytes. The reviewer did not modify implementation code.

The requirements were the new `docs/phone-setup.md` contract and the existing
approval, permission, teardown and source-custody requirements: a phone setup
link is a locator only; it must not enroll, trust, authorize or auto-connect a
phone. A connection check can describe coordinator/authentication/PC presence and
advertised relay configuration; it must not claim decoded video or relay reachability.

## Findings resolved during implementation

| Finding | Observed consequence | Resolution reviewed |
| --- | --- | --- |
| Asynchronous settings initialization and unguarded settings writes | Cold-start intent review could be overwritten; old-origin lookup/settings work could update a later origin. | Initialization preserves staged setup, home actions require loaded settings, pending review blocks actions, mutations reserve a settings transaction synchronously, and lookup/check epochs reject stale completion. |
| Session creation saved the PC list outside the new settings transaction | Cancellation cannot interrupt an already executing synchronous file write; a later server save could overtake it. | All writes, including session saved-PC persistence, snapshot the endpoint/list and retain the transaction until blocking I/O finishes. A test cancels a genuinely blocked writer, rejects the second reservation, then verifies write order after release. |
| New cancellation UI also covered the period before a session POST returned | Cancelling only the home-request epoch left the session-generation finalizer able to update a later operation. | `cancelHomeRequest` now uses the common disconnect path, invalidating both ownership generations. |
| URL libraries normalize invalid raw origins | Pinned OkHttp accepted missing authority slashes and dot-segment paths, hiding the raw contract violation. | The link parser/generator check raw origin structure before canonicalization. Negative cases cover missing slashes, dot segments, controls, credentials and encoded ambiguity. |
| Generator/parser hostname subset mismatch | A permissive host URL parser could emit a locator rejected by Android's strict URI host parsing. | Windows exports only canonical ASCII DNS/IP names with valid label bounds; non-exportable legacy origins keep a visible host with link export disabled. |
| Go URI parser lowercases schemes while clients consume configured bytes | Uppercase TURN schemes could be advertised by discovery but rejected by Android ICE parsing. | Coordinator validation requires the original scheme prefix to match the supported canonical lowercase scheme. The failing pre-fix case is retained in `server/verification/ice-scheme-regression-before.log`. |
| A fixed logical window exceeded some short/high-scale work areas | Important controls could be outside the visible desktop. | Initial/DPI-changed bounds are clamped to the work area, a scrollable content viewport reveals focused fields, and Stop remains in a separate footer; geometry tests cover short and negative-origin work areas. |

The origin-normalization finding was independently reproduced using the pinned
OkHttp 4.12.0 and JDK 21 artifacts, with no network or native-device dependency:

```text
https:remote.example.com -> https://remote.example.com/ encodedPath=/
https://remote.example.com/a/.. -> https://remote.example.com/ encodedPath=/
https://remote.example.com/./ -> https://remote.example.com/ encodedPath=/
```

The strict raw-origin checks now reject these inputs. This probe demonstrated the
library behavior; it did not execute an Android Activity or a Windows host.

## Requirements and implementation checks

### Android

Reviewed the actual ingress and UI wiring, not only the parser tests. An
`ACTION_VIEW` intent or explicit paste stages a bounded locator for review;
normal `onCreate`/`onNewIntent` handling does not call authentication, lookup,
persistence or session creation. `singleTask` reuses the app's existing Activity
for normal warm-link delivery. The review shows the complete canonical server
origin and PC number and warns when a different server clears saved PCs.
Confirmation saves addresses and fills the number; lookup/check/connect remain
explicit actions. The setup path does not invoke the Settings dialog's identity
export code before confirmation.

The encrypted settings load preserves staged review. Session, busy and settings
transaction gates prevent an incoming link from replacing an in-use endpoint.
Cancellation invalidates lookup and session ownership; all three settings-write
sites use the same transaction primitive. The parser rejects malformed UTF-8,
duplicate decoded keys, unknown fields, credentials, path/query/fragment, wrong
versions, non-ASCII or oversized outer URLs and malformed PC numbers. Windows
canonical HTTPS/default-port, custom-port and bracketed-IPv6 vectors are covered.

The check first verifies a strict, bounded `/v1/info` response before disclosing
phone identity, then requests a fresh proof and optionally performs an
authenticated PC lookup. Lookup must echo the requested PC and use bounded
metadata. Relay flags are labelled information/configuration, offline/missing
relay are warnings, and neither creates a session nor requests relay credentials.
Redirects remain disabled and reports do not reflect arbitrary response text,
credentials, key material, SDP or private document contents. Clipboard reading
for setup happens only inside the explicit paste callback and consumes text
without resolving content URIs.

### Windows

The host creates the native home window before entering the event/heartbeat
loops. Startup errors or timeouts do not result in an invisible accepting host.
The dedicated window thread owns every HWND; the live root HWND is cleared on
`WM_NCDESTROY` and is never posted to by a late destructor on another thread.
Required controls, font, timer and message-loop errors are checked. Closing,
Stop, Escape, thread exit or unwind cancels the global token and its session
children. Shutdown waits for completion with a bound before joining. The existing
per-session consent and disconnect windows remain separate.

Reviewed synchronous Win32 reentrancy around layout and focus: state borrows are
released before calls that dispatch messages; initialization gates protect early
and teardown messages. The content body scrolls independently of the Stop footer.
Copy uses the owner-controlled read-only edit and reports a request/fallback,
without reading the clipboard back or claiming an unverified copy success.

### Coordinator and integration

The new route is read-only, shares HTTPS/storage/closed-state/rate/body controls,
rejects other methods and exposes only bounded version/feature/configuration
flags. It creates no challenge, identity or session and returns no relay endpoint
or credential. Startup and discovery use the same strict ICE URI parser. The
review covers DNS label bounds, bracketed IPv6, port ranges, credentials,
fragments, paths, whitespace, percent escapes, unknown/duplicate/empty transport
queries and the canonical scheme check. TLS flags mean the supported TLS/TCP
configuration subset, not an allocation or media result.

The independent HTTPS harness checks the exact discovery contract without
credentials or host details and still exercises the existing authentication,
approval, monitor-selection and restart behavior. The original negative discovery
and URI tests are retained in `server/verification/setup-regression-before.log`.

## Open runtime and release gates

The reviewer did not run the native Windows window or an Android Activity/device.
The source review and pure geometry/lifecycle/parser tests do not establish actual
DPI rendering, keyboard traversal, clipboard copying, Android task/intent delivery,
Keystore use, encrypted settings behavior or hardware video.
The phone runbook must exercise cold and warm links, rotation, process death,
link delivery while the SAF picker is active, and cancellation during settings I/O.
Windows testing must include short/high-DPI displays, resizing, multi-monitor
movement, Copy, all Stop paths and teardown while streaming.

The physical PC-to-phone direct/TURN matrix, real installation/upgrade, production
signatures and remaining provenance/operations gates remain required. No live
Internet coordinator/relay was deployed by this review. Prior blocked native-media
runs remain blocked evidence, not new successful tests. These are explicit limits
on the candidate's release claim, not newly invented approval requirements.

## Primary platform references

- Android task and intent lifecycle: https://developer.android.com/guide/components/activities/tasks-and-back-stack
- Android custom deep links and their unverified scheme routing: https://developer.android.com/training/app-links/create-deeplinks
- Win32 creator-thread window destruction: https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-destroywindow
- STUN URI syntax: https://www.rfc-editor.org/rfc/rfc7064
- TURN URI syntax: https://www.rfc-editor.org/rfc/rfc7065

## Reviewed source inputs

Reviewed files: **22**. Aggregate SHA-256: `5a73e070f82aed4a96343c81cb10b96e64ac5ad05b3aa7bdf0d78c6e93249d98`.

The aggregate is SHA-256 over the UTF-8 encoding of the map below serialized
with lexicographically sorted keys and compact JSON separators (`,`, `:`).

```json
{
  "agent-windows/Cargo.lock": "49b23f8f24090ee9b3d2201b6246a228e38e626cbe7234fa4a3f1a36a2ef1f4a",
  "agent-windows/Cargo.toml": "e9aa8dd1d9a4bc9a2a4a074c935a512095ae57a87ba4436605418d09faa7bd25",
  "agent-windows/src/host_home.rs": "400ba6c73759f7972b0bc290ef4f710183ef08f572d684089ab91cae8441eaba",
  "agent-windows/src/lib.rs": "e756e1520284c8face50fbcb14736e9b31fc143f3ad69ab1e6ca69271f0dccdc",
  "agent-windows/src/main.rs": "e3fff00ca1e61b20d4564d808e40b9ef8179594bdc28fd6be27d346a6b21f3d4",
  "android/app/build.gradle.kts": "d666f4fecde17635e6962129821cc1d6709bda141bbc22eda6c95cd24cfe5b99",
  "android/app/src/main/AndroidManifest.xml": "c4e445f1b7ded9a484eac92313491d63480033d951842e5dd4f7d241868f3760",
  "android/app/src/main/java/com/openremote/mobile/ApiClient.kt": "a79d536d778331f8224bc9cf136328fb75a7c503d5c79d5a584a69c087279d0a",
  "android/app/src/main/java/com/openremote/mobile/ConnectionChecks.kt": "e6327f59e3b9e0cebd7e6c4e2361a1a8f501065e3fc5ab4b7a0b14608e63bfdf",
  "android/app/src/main/java/com/openremote/mobile/ControllerIdentity.kt": "6993fa492614dd9ff1edfb516a3a346e566fbd95a5fece6456901d7b7cff57ec",
  "android/app/src/main/java/com/openremote/mobile/MainActivity.kt": "893ee250bcbeb748d45c902cfae0fc4e9402c4e95fdd8ece4b2e67cbbf2d1fdd",
  "android/app/src/main/java/com/openremote/mobile/Models.kt": "cbda4867e865b1b5cf75d2e1823131395a3d47915710d57ba72ffbd7136fdc3b",
  "android/app/src/main/java/com/openremote/mobile/OpenRemoteViewModel.kt": "2d55cf1b6fe144d0933398c46f5c0bc9913ddd0e130836d3fde3fc3009c92e28",
  "android/app/src/main/java/com/openremote/mobile/PhoneSetup.kt": "a3d18b92e2fc5c28e4cbc3087f94198eaef13217bdd954594a3a781f0f9e29f7",
  "android/app/src/main/java/com/openremote/mobile/SecureSettings.kt": "a3f1f2603e36973a5f250a0b84b9fed3bf6981650fdbbb65b8f04c3adbfeaef3",
  "android/app/src/test/java/com/openremote/mobile/ConnectionChecksTest.kt": "e9bbba66bd1614baf62e8f056d44ccfdaf2b8d0eddc9a8f7d6c9321744b2940c",
  "android/app/src/test/java/com/openremote/mobile/PhoneSetupTest.kt": "00990aa99a1afdf4b9c3a94ed804bb39f04eb669dabd2e15922f0ffa0587bbde",
  "scripts/integration_test.py": "2a509d5d266d6882f8e178469ce64fe38162b101dfb220c4afb29aadeaa79fae",
  "server/internal/app/app.go": "2140ac2d8c4480531b8af89c6fd30b882fbff8290cbea6a4d92983f6923b2008",
  "server/internal/app/ice.go": "a8b6f9162c5c93c0016e8699da4e4119cba501d9c1e35b732303adfab9d1c623",
  "server/internal/app/info.go": "5e3399784be01dd272d5cef768ddba057441059a00e8ee2dbb5ec1ce96f5d405",
  "server/internal/app/info_test.go": "dc3515a1e832106463db9ea04e2b712f9d6f9c8a88a98c0c89eed02d3a51e078"
}
```
