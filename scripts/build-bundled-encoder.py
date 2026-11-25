#!/usr/bin/env python3
"""Build the minimal Windows capture executable from the pinned FFmpeg source.

No executable is downloaded. The product runs this separate LGPL program over
an H.264 pipe; it does not link FFmpeg libraries into the proprietary host.
This cross-build does not execute Windows Media Foundation or desktop capture.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import time

VERSION = "9.0.2"
SOURCE_SHA256 = "8c3850283eb25fa026482078a04051e0be17347b09ef81a0849bec15a96e002e"


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def build(archive: Path, toolchain: Path, output: Path, jobs: int) -> dict:
    if digest(archive) != SOURCE_SHA256:
        raise ValueError("FFmpeg source does not match the signature-verified release pin")
    if output.exists() and any(output.iterdir()):
        raise ValueError("Use a new output directory; existing evidence is never overwritten")
    output.mkdir(parents=True, exist_ok=True)
    source_root = output / "source"
    source_root.mkdir()
    with tarfile.open(archive, "r:xz") as source:
        source.extractall(source_root, filter="data")
    source = source_root / f"ffmpeg-{VERSION}"
    if (source / "RELEASE").read_text().strip() != VERSION:
        raise ValueError("Unexpected FFmpeg release contents")
    work = output / "build"
    work.mkdir()
    prefix = toolchain / "bin"
    tools = {name: prefix / binary for name, binary in {
        "cc": "x86_64-w64-mingw32-clang", "ar": "llvm-ar",
        "nm": "llvm-nm", "ranlib": "llvm-ranlib", "strip": "llvm-strip",
    }.items()}
    for executable in tools.values():
        if not executable.is_file():
            raise ValueError(f"Missing cross-build tool: {executable}")
    flags = [
        "--enable-cross-compile", "--target-os=mingw32", "--arch=x86_64",
        *[f"--{name}={executable}" for name, executable in tools.items()],
        "--disable-autodetect", "--disable-everything", "--disable-network",
        "--disable-doc", "--disable-debug", "--disable-ffplay", "--disable-ffprobe",
        "--disable-x86asm", "--disable-shared", "--enable-static",
        "--disable-pthreads", "--enable-w32threads", "--enable-ffmpeg",
        "--enable-mediafoundation", "--enable-d3d11va",
        "--enable-indev=gdigrab,lavfi", "--enable-decoder=bmp,rawvideo",
        "--enable-encoder=h264_mf", "--enable-parser=h264",
        "--enable-filter=testsrc2,scale,format,null,ddagrab,hwdownload",
        "--enable-bsf=h264_metadata,h264_mp4toannexb,dump_extradata",
        "--enable-muxer=h264,null", "--enable-protocol=pipe",
        "--extra-cflags=-O2 -D_WIN32_WINNT=0x0A00",
        "--extra-ldflags=-static",
    ]
    record = {
        "version": VERSION, "sourceSha256": SOURCE_SHA256,
        "sourceUrl": f"https://ffmpeg.org/releases/ffmpeg-{VERSION}.tar.xz",
        "license": "LGPL-2.1-or-later", "externalCodecLibraries": [],
        "encoder": "Windows Media Foundation h264_mf", "hardwareEncodingForced": False,
        "assemblyOptimization": "x86 assembler disabled; encoder runs through the Windows OS",
        "configureFlags": flags, "toolSha256": {name: digest(path) for name, path in tools.items()},
        "startedAtUtc": datetime.now(timezone.utc).isoformat(), "steps": [],
        "windowsExecution": "not_run", "status": "failed",
    }
    env = dict(os.environ)
    env["PATH"] = str(prefix) + os.pathsep + env.get("PATH", "")
    env["SOURCE_DATE_EPOCH"] = "1789699562"
    try:
        for name, command in [
            ("configure", [str(source / "configure"), *flags]),
            ("make", ["make", f"-j{jobs}", "ffmpeg.exe"]),
        ]:
            started = time.monotonic()
            with (output / f"{name}.log").open("wb") as stream:
                result = subprocess.run(command, cwd=work, env=env, stdout=stream, stderr=subprocess.STDOUT)
            record["steps"].append({"name": name, "command": command, "exitCode": result.returncode,
                                    "durationSeconds": round(time.monotonic() - started, 3)})
            if result.returncode:
                raise RuntimeError(f"{name} failed; see {output / (name + '.log')}")
        config = (work / "config.h").read_text()
        for required in ["#define CONFIG_GPL 0", "#define CONFIG_NONFREE 0",
                         "#define CONFIG_NETWORK 0", "#define CONFIG_MEDIAFOUNDATION 1"]:
            if required not in config:
                raise ValueError(f"Required build policy absent: {required}")
        components = (work / "config_components.h").read_text()
        for required in ["CONFIG_H264_MF_ENCODER", "CONFIG_GDIGRAB_INDEV", "CONFIG_LAVFI_INDEV",
                         "CONFIG_H264_METADATA_BSF", "CONFIG_H264_MP4TOANNEXB_BSF", "CONFIG_DUMP_EXTRADATA_BSF"]:
            # FFmpeg calls the dump_extra registration DUMP_EXTRADATA in some versions.
            if required == "CONFIG_DUMP_EXTRADATA_BSF":
                if "#define CONFIG_DUMP_EXTRADATA_BSF 1" not in components and "#define CONFIG_DUMP_EXTRA_BSF 1" not in components:
                    raise ValueError("Required extra-data repetition filter was not built")
            elif f"#define {required} 1" not in components:
                raise ValueError(f"Required component not built: {required}")
        executable = output / "ffmpeg.exe"
        shutil.copy2(work / "ffmpeg.exe", executable)
        record["artifact"] = {"file": executable.name, "sha256": digest(executable), "sizeBytes": executable.stat().st_size}
        record["status"] = "passed"
        for filename in ["config.h", "config_components.h", "ffbuild/config.mak"]:
            shutil.copy2(work / filename, output / Path(filename).name)
        for filename in ["COPYING.LGPLv2.1", "LICENSE.md", "README.md"]:
            shutil.copy2(source / filename, output / ("FFmpeg-" + filename))
        return record
    finally:
        record["finishedAtUtc"] = datetime.now(timezone.utc).isoformat()
        (output / "build-evidence.json").write_text(json.dumps(record, indent=2) + "\n")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-archive", type=Path, required=True)
    parser.add_argument("--toolchain-root", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--jobs", type=int, default=4)
    args = parser.parse_args()
    if not 1 <= args.jobs <= 16:
        parser.error("Use 1–16 build jobs")
    result = build(args.source_archive.resolve(), args.toolchain_root.resolve(), args.output_dir.resolve(), args.jobs)
    print(json.dumps({"status": result["status"], "artifact": result["artifact"]}))


if __name__ == "__main__":
    main()
