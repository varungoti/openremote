# OpenRemote 0.4 monitor and lifecycle review

18 November 2026. **Source findings below are resolved; native release gates remain open.**
This is an independent read-only review of the Windows/Android monitor-selection
slice against [display-selection.md](../docs/display-selection.md). It is not a
physical-device test or a complete security audit.

## Scope and source identity

The reviewed working tree was based on Git commit `612a5bd5d62ee5fc8d06be37be783833bb8f19d3`. The implementation
files were frozen after the final formatter; their exact hashes are recorded
below because a base commit does not identify uncommitted changes. Documentation,
build manifests and verification outputs may be assembled afterward. Any later
change to these implementation hashes needs corresponding review.

The Android final source-input manifest inspected in this review has SHA-256
`b3a56713276d984fa93d0bd1e09ef6075c7f4689f6bfa65345b11638d7563a27`. Its test summary records 46 JVM tests and explicitly states
that device tests were not executed. The host's final build evidence is recorded
separately in `agent-windows/BUILD_EVIDENCE.json` and `agent-windows/verification/`;
this review does not substitute for its compilation and final-source checks.

## Findings and final disposition

| Finding | Final source behavior | Evidence boundary |
| --- | --- | --- |
| Child-task panic or capture abort could bypass teardown and reopen the single-session gate | Join failures are collected as values; cleanup runs unconditionally; capture is awaited and FFmpeg requires a confirmed bounded process wait. Unverified cleanup produces a typed error that keeps the host unavailable until restart. | Static review; synthetic panic/delayed-cleanup tests and a real Linux child-process reap test. No Windows capture process was exercised. |
| Failed native key/button release discarded the state needed for retry | Failed virtual-key, Unicode key-up and button releases remain tracked. Final teardown retries and retains the restart latch if release cannot be confirmed. | Static review plus injected-failure state tests. Windows SendInput/UIPI behavior remains unexecuted. |
| Input callback could pass cancellation before waiting for the teardown mutex | Cancellation is checked by `apply_control_locked` after acquiring the same guard used for final input release. Teardown and delayed injection therefore have a defined order. | Static call-site review and a synchronized thread regression. No operating-system input was injected. |
| A completed task plus a stale nonterminal status could be mistaken for failed cleanup | Handover distinguishes StillActive, Finished, TimedOut and Failed; only an actual failed join produces that failure classification. | Static review and dedicated lifecycle regression; this last correction still uses the final host verification run as its execution record. |
| A legacy host could show primary-display pixels despite an explicit different selector | Android withholds video publication until a matching capability/catalog binding. Explicit selection cannot fall back to a legacy host. | Static cross-client contract review; strict parser/readiness and API tests. Actual renderer behavior remains a device gate. |
| Rotation after a long session could immediately exhaust an old render deadline | Active renderer disposal starts a fresh wait; stale renderer tokens cannot enable input or alter the current deadline. First render clears the wait. | Static review and clock-driven Android readiness tests. |
| Renderer recreation during the two-minute document-picker handoff conflicted with the render timeout | The explicit picker handoff pauses only renderer waiting and new interaction. The independent picker deadline remains; selected-file transfer waits for the current renderer after return. | Static review and clock-driven state tests. Actual SAF, rotation and hardware decoding remain device gates. |
| Server monitor-binding violations were initially treated like transient status-fetch errors | Malformed/missing/changed selector echoes fail immediately through the fatal IllegalArgumentException path. Transport failures retain their bounded retries. | Static ViewModel/API review and API contract tests. |

The coordinator preserves an omitted selector for legacy primary sessions, carries
explicit selectors unchanged through authenticated session/event reads, rejects
malformed selectors and cannot add input permission during approval. Monitor IDs
remain separate from the permanent PC number. Changing displays creates a fresh
session and peer, retains the original requested permissions and waits for the
old session's end acknowledgement. Tool/picker activity disables switching.

The host binds capture and pointer mapping to one immutable physical layout.
Native enumeration uses a synchronous per-thread DPI context. The observed
topology epoch invalidates old selectors after a detected change, including a
return to a previous layout. Catalog fields and limits match Android's strict
parser. These are source-level conclusions; enumeration/injection are not atomic,
and an unobserved hardware change between checks is not ruled out by this review.

## Verification interpretation

The reviewer inspected the source, relevant regression tests, host test log and
Android final test/source-input records. Client owners execute the final build
suites and preserve their logs. The review did not rerun blocked network-media
fixtures or change workspace networking controls. The pilot-record helper has
13 passing validator tests using temporary synthetic records; those records are
not evidence of a successful remote-desktop session.

No unresolved concrete source finding from this review remains after the listed
corrections. This does not close native installation, media, security or release
gates. Use [pilot-validation.md](../docs/pilot-validation.md) for the 17 explicitly
unattempted physical/Internet scenarios and [release-gates.md](../docs/release-gates.md)
for the complete product requirements.

Required remaining evidence includes real changing Windows pixels on Android;
correct pointer/keyboard behavior on negative-origin, portrait and mixed-DPI
layouts; repeated fresh-session switches; unplug/replug with held input; native
release failure and emergency stop; real clipboard/file metadata and cancellation;
SAF/rotation/background behavior; installation/upgrade rollback; and actual public
TURN UDP/TCP/TLS selection and network handoff. The separately supplied Windows
FFmpeg build must be tested on those layouts. Host DPI setup alone does not prove
that external encoder's behavior. Production signing and dependency provenance
remain separate open gates.

## Reviewed implementation fingerprints

SHA-256 of the following sorted manifest text, including its final newline:
`f84088aef886b7a57d166ed9c50d8a47cc57d4372db29628a3964c01b6b1eeee`.

```text
f2c47c4dfa86d6ec09a17c9429d7c628e304912406c44925a2d3ca7ce4c81caa  agent-windows/src/api.rs
540bc6f43f00ec5a041351fb88426016093e80f13446ae6067872f4f0f8ed629  agent-windows/src/displays.rs
5a552bb001a4a074afc6727699c8cb40d564f7079d53dacb2f94eb73548a1f41  agent-windows/src/input.rs
5112c205207dc77078ca79eb3ab7906069286b07a25c720a6f683ca3430ef734  agent-windows/src/lib.rs
e9da79b28e495d75258177180168c1d72313f2545bb90961f32f09b1f5b50e9b  agent-windows/src/main.rs
dae8e76edeb07d43c7853dbbd3522af854a0df6fa1590e09748299127ba3028d  agent-windows/src/media.rs
78b7f9c57b36ddfcf8cbf1a573af5bbdc576c29506c448c391b0116549c3a596  agent-windows/src/protocol.rs
a5ff8c1d37fd0756d18baa78c111672488119fa3855cc091587c4dcd7e9e49b3  agent-windows/src/session.rs
b07a9f6d5a0f02062edfb8ffbd0d4fb7b42e5cbaba39b7eb126c2f5ac712ee96  agent-windows/src/ui.rs
5aaeaf7ead525ba91ca1a18468a3c1fc511f7944e5cea99b9353f3023b26753e  android/app/src/main/java/com/openremote/mobile/ApiClient.kt
ad657a1fb30d2d191256375c787419bbc8d1e4f7538db987ea8cf1601148ba91  android/app/src/main/java/com/openremote/mobile/DisplayProtocol.kt
c8046d4de85743ba20285a67f7251402a726211b5ea4b766b6d74d3228f7583d  android/app/src/main/java/com/openremote/mobile/MainActivity.kt
49db8de16806cc21fc5012689e05be5fc183ce019ed93248815656a6383b24b5  android/app/src/main/java/com/openremote/mobile/Models.kt
371899ce5b5831614c634c3a77b0c23a682b8b87dfb657436fbe3e22592b055d  android/app/src/main/java/com/openremote/mobile/OpenRemoteViewModel.kt
b65d3302ae7437771e37c050969c65bbe60f4aa4d4cbddbb70297a7274db0967  android/app/src/main/java/com/openremote/mobile/RemoteDisplay.kt
88176ee013d2a151b76ace97b720d564f4e738f5c70a97a5c7fbe4a8ddb851d9  android/app/src/main/java/com/openremote/mobile/RemotePeer.kt
c358204428cb78bd79e493613ce97debf4e1defaa48bcc7d8a4cf32235ccd297  server/internal/app/app.go
9a97bda685779cf26e7e750d5274a64430ad03529dcab5caf42e4868c55282ee  server/internal/app/sessions.go
```
