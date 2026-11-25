# Physical Windows/Android pilot record

This runbook turns the open device and Internet gates into a repeatable record.
It is **prepared, not executed**. Run it on owned test PCs and a physical Android
phone using the exact candidate binaries. The development workspace has not
provided those devices or a public deployment.

The helper [validate_pilot_evidence.py](../scripts/validate_pilot_evidence.py)
fingerprints candidate files, creates an all-`not_run` report and checks recorded
observations against required scenarios. It never installs an application, starts
a session, changes a firewall, contacts a PC, collects private desktop contents,
or publishes the record. A valid record is not an attestation that its claims are
true. Independent review and the remaining [release gates](release-gates.md) are
still required; the helper always returns `releaseReady:false`.

## Prepare the candidate and devices

Use at least two owned Windows PCs and one ARM64 Android phone. At least one PC
needs two displays for the display tests. Use a disposable, standard-user Windows
test profile and harmless test documents. Have a person at the PC who can approve
and stop access. Record OS builds, GPU, monitor count, phone model/Android API/page
size, coordinator OS, coturn version and the network arrangement. Keep identifying
labels such as `office-test-pc`; a report does not need a real PC number or public
IP address.

Compare each installed binary's SHA-256 against the reviewed build record. The
record must distinguish the signed production identity from any debug/unsigned
test build. Record the exact Windows FFmpeg executable hash and confirm it matches
the configured, independently trusted hash. The helper checks the format of this
FFmpeg observation; it does not have access to the installed Windows executable.
Native FFmpeg behavior and publisher/source provenance remain separate gates.

Use a normal HTTPS coordinator and coturn deployment with valid certificates and
session-bound credentials. The phone and PC must use genuinely different Internet
connections for the direct and relay cases. A loopback fixture or two devices on
the same Wi-Fi does not close those cases. For TLS/443, follow
[deployment.md](deployment.md) so the API and relay ports do not conflict.

Do not change the restricted development workspace's networking controls to run
this matrix. The separate synthetic [native-media fixture](../tests/native_media/README.md)
remains useful on an authorized runner, but its report does not substitute for
Windows capture, Android rendering, native input, clipboard or file operations.

## Create the all-not-run record

The helper needs Python 3.9+ and only the standard library. Paths passed for
artifacts and evidence are normalized forward-slash paths inside `--root`.
Absolute paths, `..`, symlinks and Windows reparse points are rejected. It hashes
files in place and does not copy or redistribute FFmpeg or any candidate binary.
Keep the files and their containing directories under the operator's control while
validating; the helper is not a hostile-filesystem sandbox.

From the source checkout, after creating an empty `pilot` directory:

```sh
python3 scripts/validate_pilot_evidence.py init \
  --root . \
  --version 0.5.0 \
  --source-revision FULL_REVIEWED_GIT_REVISION \
  --windows artifacts/windows/openremote-agent.exe \
  --android android/app/build/outputs/apk/debug/app-arm64-v8a-debug.apk \
  --coordinator artifacts/controlplane-linux-amd64 \
  --output pilot/observations.json
```

Substitute the full revision from the build manifest, not a shortened hash or a
later documentation commit. Keep that manifest with the candidate. The helper
records the revision supplied by the operator and the actual file bytes; it does
not rebuild the binaries or prove that a declared revision produced them. The
reviewed build/source-input evidence provides that link.

Creation refuses to overwrite an existing report. All 17 scenarios begin as
`not_run`, with no timestamps, measurements or attachments. Preserve the original
candidate fields while filling in observations. The report and any supporting
files are private local records until the owner chooses to share them.

## Execute and preserve observations

Populate `environment` with the measured device details before recording a pass
or failure. `windowsPcs[].id` supplies local labels used in each case's `pcIds`.
`windowsFfmpegSha256` is exactly 64 lowercase hexadecimal characters.

For each attempt, record UTC start/finish times ending in `Z`, the PC labels,
what actually happened, and one of `passed`, `failed` or `blocked`. A blocked case
records the exact denial or missing prerequisite and remains an open gate. Leave
an unattempted case as `not_run`; it must not contain fabricated measurements or
execution timestamps. If an attempt fails, retain that record and supporting
files when collecting a later retest instead of rewriting history as a pass.

Archive an original observation, diagnostic excerpt or harmless test-screen
photo/video for each attempted case. Prefer test text and synthetic images;
exclude credentials, SDP secrets, clipboard contents, private documents and
unnecessary addresses. The phone intentionally protects its remote screen from
ordinary screenshots; do not disable that protection merely to collect evidence.
An operator's written observation is permitted and must be identified as such.
It is weaker evidence than instrumented measurements and needs human review.

Fingerprint each archived file using:

```sh
python3 scripts/validate_pilot_evidence.py pin --root . pilot/case-observation.txt
```

Copy the returned `path`, `sizeBytes`, and `sha256` object into that case's
`evidence` array. No attachment contents are printed. Files are limited to 1 GiB
and JSON reports to 1 MiB. Edited/redacted attachments receive new hashes; retain
the original privately if it is needed to substantiate the observation.

Print the exact required measurement keys for a passing case:

```sh
python3 scripts/validate_pilot_evidence.py requirements
```

In that output, `positive`, `nonnegative`, and `two` mean integer values of at
least 1, 0, and 2 respectively. `digest` means a lowercase SHA-256. The `direct`
rule accepts the selected non-relay candidate type `host`, `srflx`, or `prflx`.
Boolean `true` means the operator actually observed the required result. Do not
copy the rule descriptions into the measurements as if they were observations.

## Required scenarios

| ID | Procedure and necessary observation |
| --- | --- |
| `desktop_input` | Display a changing harmless desktop; record actual decoded-frame count and successful pointer, keyboard, Unicode and release actions. Verify the intended Windows application changed, not only that a DataChannel message arrived. |
| `multiple_pcs` | Enroll both PCs, save their permanent numbers, connect to each and restart the coordinator. Confirm numbers remain stable and both PCs can be reached afterward. Use two distinct `pcIds`. |
| `consent_permissions` | Accept one request, deny another and allow one to time out. Narrow a request to view-only. Attempt input and verify it is denied at the host. |
| `trust_revocation` | Deliberately trust the test phone locally, verify unattended access, then revoke it. Verify active access ends and stale local trust does not authorize a new session. |
| `internet_direct` | Put phone and PC on separate Internet connections. Record actual selected non-relay candidate type, decoded video and observed native input. Candidate availability alone is insufficient. |
| `turn_udp` | Force relay use with UDP transport. Record the actual selected relay candidate and `relayProtocol:udp`, decoded video and input. |
| `turn_tcp` | Force relay use with TCP transport. Record the actual selected relay candidate and `relayProtocol:tcp`, decoded video and input. |
| `turn_tls443` | Force the TLS relay endpoint on port 443. Correlate selected relay-candidate stats with the actual TLS transport and relay-side connection evidence. Record `relayProtocol:tls`, port 443, decoded video and input; a configured `turns:` URL alone is insufficient. |
| `renewal_handoff` | Maintain a session longer than its observed TURN credential lifetime, then change between Wi-Fi and cellular. Record both durations. Verify recovery or the documented bounded termination without stale input. |
| `coordinator_relay_restart` | Restart each service separately during a test session. Verify a bounded recovery/end outcome and that ended credentials cannot regain access. Preserve both attempts in the attachment. |
| `clipboard_upload` | Exercise explicit Unicode clipboard write/read/preview/copy; upload a harmless file and compare source/received SHA-256 and length. On Windows inspect Internet-zone metadata. Cancel an upload and verify partial cleanup; repeat a filename and verify no overwrite. |
| `monitor_layout` | Test at least two displays, including one left/above the primary (negative virtual origin), portrait orientation and mixed DPI. Point at harmless targets near each edge/corner and verify exact target selection. Record display count and each layout. |
| `monitor_switch` | Select another monitor using **Switch and reconnect**. Verify the old session/renderer ends, held input releases, a fresh session displays the requested monitor, and old-session input cannot affect it. Repeat in both directions. |
| `monitor_unplug` | While a remote key/button is held and capture is active, unplug the selected monitor. Verify release of held input, stopped capture and ended session. Replug and obtain a new primary session/catalog before another selection. |
| `monitor_invalid` | Submit an unknown selector and a previously valid selector after topology change or host restart. Both must reject/end without primary fallback. Select a monitor in a view-only session and confirm input remains denied. |
| `lifecycle_cleanup` | Exercise local emergency stop, phone rotation, ordinary backgrounding, encoder failure and disconnect. Verify no held input and no orphan capture process. Document the system-document-picker exception separately. |
| `installation` | Perform clean install, upgrade while preserving PC identity, deliberately failed upgrade with rollback, and uninstall with each supported retention choice. Do this in a disposable owned profile; preserve failure/rollback observations. |

For a claimed pass, every required measurement key must be present with its
expected type/value. Upload source and received digests must match. Renewal
session duration must exceed the credential lifetime. The monitor count must fit
at least one identified test PC. A selected relay candidate, working frames and
input must be observed together; successful HTTPS, ICE gathering or SDP exchange
cannot substitute for the desktop.

The clipboard/file test is deliberately a bounded pilot scenario. Boundary-sized
files, disk-full behavior, abrupt crashes, reparse races and native security review
still remain in the full release matrix. Likewise this record does not close audio,
hardware encoding, secure desktop, signing, updater or other unimplemented gates.

## Validate and decide

Inspect a partial report without converting open cases into passes:

```sh
python3 scripts/validate_pilot_evidence.py validate pilot/observations.json \
  --root . --expected-revision FULL_REVIEWED_GIT_REVISION
```

Require all pilot scenarios to be recorded as passed:

```sh
python3 scripts/validate_pilot_evidence.py validate pilot/observations.json \
  --root . --expected-revision FULL_REVIEWED_GIT_REVISION --require-pass
```

The second command exits nonzero for any failed, blocked or not-run scenario.
It also rejects malformed/duplicate JSON, missing cases, wrong artifact/evidence
bytes, unsafe paths, missing observations, future/inverted timestamps, absent
device identity and contradictory measurement fields. The expected revision must
come from the separately reviewed candidate manifest.

`reportedPilotComplete:true` means the supplied report has all required passing
observations and matching files. It does **not** independently verify those
observations, the build/source link, signatures, publisher trust or performance.
The complete synthetic report used by unit tests is confined to temporary test
directories and explicitly labeled as a validator fixture. No synthetic fixture
is shipped as proof of a successful remote-desktop session.
