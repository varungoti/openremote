# Owner-controlled verification

`verify.yml` runs only by manual `workflow_dispatch`. It has read-only repository
permission, disables persisted checkout credentials, and pins checkout to an
observed release commit (`evidence/checkout-action.json`). It neither deploys nor
publishes a release. Do not expose private runners to arbitrary fork code.

Prepare runners with the labels in the workflow:

| Label set | Required environment |
| --- | --- |
| `self-hosted, linux, x64, openremote` | Go 1.27.4, Node 24, C compiler for the race detector, Python 3.12 with venv/pip, network access to pinned test dependencies |
| `self-hosted, windows, x64, openremote` | Rust 1.100.0, MSVC build tools/Windows SDK, PowerShell 5.1+, Git; interactive native desktop tests remain an explicit device job |
| `self-hosted, linux, x64, openremote-android` | JDK 21, Android SDK 36/build tools 35, accepted SDK terms and ordinary Gradle cache/network access; project Gradle wrapper supplies 8.13 |
| `self-hosted, linux, x64, openremote-media` | Go/Rust as above, FFmpeg with libx264, regular UDP/TCP and network-interface discovery; generated test video only |

The runner service must support the runtime used by the pinned checkout release.
Provisioning and secrets are owner operations; this repository does not install
runners or assume credentials exist.

The native-media job requires actual decoded moving frames, exact input-event
receipt and selected relay candidates. It must fail when its runtime cannot
execute; it must not convert an environment denial into a pass or replace video
with an SDP assertion. It covers direct and local TURN UDP/TCP, not public
TURN/TLS or physical Windows/Android behavior. Test reports are generated in the
checkout; configure retention appropriate to the chosen repository when adding
artifact publication.

For native installation testing, run the activation logic tests first and then
the complete installer scenarios in `docs/release-gates.md` on a disposable owned
Windows test profile. Do not treat those four file-transaction tests as a full
installation test. A signing identity is deliberately not included in the workflow.

The control-plane job also runs the pilot-record validator's unit tests. Those
tests use synthetic records and do not execute the physical pilot. Use
`docs/pilot-validation.md` to initialize a candidate-bound record and collect the
17 actual Windows/Android/Internet observations. Keep it separate from build logs;
all-not-run or blocked records must fail the explicit `--require-pass` gate.

The code/PIN browser checks run the production protocol in Node WebCrypto
against a real loopback coordinator, and the actual app module in a VM with
DOM/RTC test doubles. The latter tests state ownership and input gating; they
are explicitly not rendered-browser or decoded-media verification. The separate
Chromium and physical-device gates still apply.
