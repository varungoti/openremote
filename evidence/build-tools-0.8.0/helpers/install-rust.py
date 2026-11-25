#!/usr/bin/env python3
"""Install the verified Rust component archives into a task-local prefix."""
import hashlib
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parent
REPO = ROOT.parent / "work/remote-desktop-platform"
SOURCE = json.loads((REPO / "evidence/windows-startup-0.5.1/toolchain-provenance.json").read_text())
STAGING = ROOT / "unpacked"
STAGING.mkdir(exist_ok=True)
PREFIX = ROOT / "rust-1.100.0"
for item in SOURCE["artifacts"]:
    if item["kind"] != "rust":
        continue
    archive = ROOT / "downloads" / item["url"].rsplit("/", 1)[1]
    if hashlib.sha256(archive.read_bytes()).hexdigest() != item["sha256"]:
        raise SystemExit(f"Hash mismatch: {archive.name}")
    subprocess.run(["tar", "-xJf", str(archive), "-C", str(STAGING)], check=True)
    directory = STAGING / archive.name.removesuffix(".tar.xz")
    subprocess.run(["sh", str(directory / "install.sh"), "--prefix=" + str(PREFIX), "--disable-ldconfig"], cwd=directory, check=True)
    print(f"INSTALLED {archive.name}", flush=True)
