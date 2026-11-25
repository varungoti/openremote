# Bundled Windows encoder — 0.7.0

OpenRemote bundles a separate FFmpeg 9.0.2 executable built from the exact,
unmodified upstream source archive. The build enables LGPL components only,
disables GPL/nonfree configuration and external codec libraries, and uses Windows
Media Foundation `h264_mf`. It does not bundle libx264.

- Upstream archive: https://ffmpeg.org/releases/ffmpeg-9.0.2.tar.xz
- Source SHA-256: `8c3850283eb25fa026482078a04051e0be17347b09ef81a0849bec15a96e002e`
- Source size: 12,040,788 bytes.
- Encoder SHA-256: `3cc67f5c4a38d19c2e76ae78b3ff89b398dd92ec31511fb40c25f6162c7a5e11`
- Encoder size: 3,288,064 bytes.
- Upstream signing key: `FCF986EA15E6E293A5644F10B4322F04D67658D8`.

`COPYING.LGPLv2.1` and `LICENSE.md` are the exact upstream texts. Additional
LLVM/MinGW runtime texts and their hashes are recorded in
`runtime-notice-provenance.json`. The installer includes the source archive as
`FFmpeg-9.0.2-source.tar.xz`, consolidated notices, and `FFmpeg-BUILD.txt` with the
complete configure/build evidence. The host launches the encoder as a separate
process and checks its exact hash before launch. FFmpeg is maintained by its own
contributors; OpenRemote does not claim ownership of it.

## Reproduce

Use the pinned LLVM-MinGW 20261104 UCRT x86_64 toolchain and source archive:

```sh
python3 scripts/build-bundled-encoder.py   --source-archive /absolute/path/ffmpeg-9.0.2.tar.xz   --toolchain-root /absolute/path/llvm-mingw-20261104-ucrt-ubuntu-22.04-x86_64   --output-dir /absolute/path/new-empty-build-directory   --jobs 4
```

The builder refuses a source hash mismatch and records exact compiler hashes,
configure flags, command exits and final artifact bytes. The checked-in evidence
is in `evidence/one-button-0.7.0/encoder/`. The build disables x86 assembly because
this runner has no NASM/YASM; no performance equivalence is claimed. Production
arguments select software Media Foundation, NV12, H.264 baseline, no B frames,
one-second keyframes, Annex B conversion, repeated keyframe headers and AUD
insertion for the existing access-unit reader. It is not a hardware-acceleration
benchmark or a Windows execution record.

The initial build correctly failed component admission because the configure
component is named `dump_extradata`, while its runtime filter is `dump_extra`.
The fresh corrected build passed; the initial logs remain in evidence/history.
The verified source signature and PE/import inspection are retained. Actual
Windows screen capture and Media Foundation encoding remain release tests.
