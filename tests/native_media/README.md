# Native media interoperability verification

This executable is an independent **Go/Pion controller** for the Rust
`synthetic_peer` example. The Rust example uses the same `HostPeer`, H.264 access
unit assembler, RTP sender and validated DataChannel path as the Windows host.
Its source is synthetic moving video, and accepted input is logged rather than
injected into an operating system.

The controller does not reuse the Rust peer implementation. It uses pinned Pion
WebRTC `v4.2.26`, selected from the public Go module registry on 2026-11-18 and
recorded in `go.mod`/`go.sum`.

## What a successful run must demonstrate

For each direct, forced TURN/UDP and forced TURN/TCP case:

1. Create an offer with a receive-only H.264 video transceiver and an ordered,
   reliable control DataChannel. Relay cases offer **relay candidates only**.
2. Start the Rust synthetic peer with files containing the real SDP exchange.
3. Wait for real DTLS/WebRTC connection, an opened DataChannel, received video,
   and truthful host capabilities.
4. Send eight distinct pointer, keyboard, scroll, Unicode-text and release
   messages. Check the host's received event file exactly, including order.
5. Receive actual RTP; reconstruct H.264 Annex-B with Pion's H.264 depacketizer.
   End recording on a complete RTP frame boundary.
6. Read the **selected** ICE candidate pair. Relay tests require a relay local
   candidate and the requested `relayProtocol` (`udp` or `tcp`) in actual stats.
7. Decode the received file with ffmpeg to `framemd5`. Require at least ten
   decoded frames and ten distinct frame hashes, with no decoder error output.

A socket, connection, codec, relay, input or decoder failure produces nonzero
exit status. Missing evidence is never substituted with SDP/signaling success.
Every case is bounded to 50 seconds and cleans up its child processes.

## Run on a normal Linux development/CI machine

Prerequisites: Go 1.24 or newer (verified compile used 1.27.4), Rust dependencies
from the host lockfile, ffmpeg with H.264 encoding/decoding, and ordinary Linux
UDP/TCP plus network-interface/route discovery. No elevated networking setup or
public relay is required; the relay fixture uses loopback and short-lived
credentials and terminates automatically.

From the project root:

```sh
cargo build --manifest-path agent-windows/Cargo.toml --example synthetic_peer
go build -C tests/relay -o ../../artifacts/test-relay .
go build -C tests/native_media -o ../../artifacts/native-media-test .
./artifacts/native-media-test \
  --host ./agent-windows/target/debug/examples/synthetic_peer \
  --relay ./artifacts/test-relay \
  --ffmpeg /usr/bin/ffmpeg \
  --mode all \
  --output ./evidence/media-native.json
```

`--mode direct`, `--mode udp` and `--mode tcp` select individual reproductions.
`--capture 3s` controls received-video duration, between one and ten seconds.
The report includes frame/control counts, actual selected-pair stats, and paths
to per-case logs, SDP, H.264 and frame hashes. Files contain only synthetic test
content, but candidate information describes the machine's local interfaces.

## Current workspace outcome: BLOCKED

The program and dependency graph compiled successfully. The execution
workspace denied network-interface/route discovery before a WebRTC peer could
be created:

```text
failed to create network: route ip+net: netlinkrib: operation not permitted
```

The direct peer and the local TURN fixture both encountered this denial. The
recorded outcome in `evidence/media-native.json` is **blocked**, with zero
received packets, zero decoded frames and zero matched control events for all
three cases. This is an environment restriction, not evidence of an application
streaming failure or a successful remote-desktop session. No networking policy,
namespace, interface discovery or browser sandbox setting was changed to avoid
that restriction.

A successful execution on an authorized runner is still required. Even a pass
here proves only synthetic native interoperability. It does not validate Windows
GPU/desktop capture, actual OS input injection, Android hardware rendering,
public Internet CGNAT traversal, coturn deployment, TURN/TLS, audio, clipboard,
file transfer, reboot recovery, installers or application signing.
