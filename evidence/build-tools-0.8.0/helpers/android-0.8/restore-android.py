#!/usr/bin/env python3
"""Download and extract the Android tools using existing repository SHA-256 pins."""
from concurrent.futures import ThreadPoolExecutor, as_completed
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import zipfile

ROOT = Path(__file__).resolve().parent
REPO = ROOT.parent.parent / "work/remote-desktop-platform"
EVIDENCE = REPO / "evidence/build-tools-0.8.0"
DOWNLOADS = ROOT / "downloads"
DOWNLOADS.mkdir(exist_ok=True)
SDK = ROOT / "sdk"
items = json.loads((REPO / "android/verification/code-pin-0.6.0/toolchain-provenance.json").read_text())["packages"]
for item in items:
    item.pop("path", None)
for name, package in [("jdk-provenance.json", "jdk"), ("platform-tools-provenance.json", "platform-tools")]:
    item = json.loads((REPO / "android/verification/code-pin-0.6.0" / name).read_text())
    item["package"] = package
    items.append(item)

def digest(path, algorithm="sha256"):
    h = hashlib.new(algorithm)
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            h.update(block)
    return h.hexdigest()

def restore(item):
    archive = DOWNLOADS / item["url"].rsplit("/", 1)[1]
    if not archive.exists():
        partial = archive.with_suffix(archive.suffix + ".partial")
        proc = subprocess.run(["curl", "--fail", "--location", "--silent", "--show-error", "--max-time", "600", "--output", str(partial), item["url"]], capture_output=True, text=True)
        if proc.returncode:
            raise RuntimeError(f"{archive.name}: curl exited {proc.returncode}: {proc.stderr.strip()}")
        partial.rename(archive)
    actual = digest(archive)
    if actual != item["sha256"] or archive.stat().st_size != item["bytes"]:
        raise RuntimeError(f"{archive.name}: saved SHA-256/size pin does not match")
    if item.get("verificationAlgorithm") == "sha1" and digest(archive, "sha1") != item["expected"]:
        raise RuntimeError(f"{archive.name}: saved publisher SHA-1 also does not match")
    if "verifiedSha1" in item and digest(archive, "sha1") != item["verifiedSha1"]:
        raise RuntimeError(f"{archive.name}: saved publisher SHA-1 also does not match")
    print(f"VERIFIED {archive.name}: {item['bytes']} bytes", flush=True)
    package = item["package"]
    if package == "jdk":
        subprocess.run(["tar", "-xzf", str(archive), "-C", str(ROOT)], check=True)
        target = ROOT / "jdk-21.0.13+9"
    else:
        targets = {"platforms;android-36": SDK / "platforms/android-36", "build-tools;35.0.0": SDK / "build-tools/35.0.0", "cmdline-tools;latest": SDK / "cmdline-tools/latest", "platform-tools": SDK / "platform-tools", "gradle": ROOT / "gradle-8.13"}
        target = targets[package]
        staging = ROOT / ("unpack-" + package.replace(";", "-"))
        staging.mkdir(exist_ok=True)
        with zipfile.ZipFile(archive) as z:
            z.extractall(staging)
            for member in z.infolist():
                mode = member.external_attr >> 16
                if mode:
                    (staging / member.filename).chmod(mode & 0o777)
        directories = list(staging.iterdir())
        if len(directories) != 1 or not directories[0].is_dir():
            raise RuntimeError(f"{archive.name}: expected one top-level archive directory")
        target.parent.mkdir(parents=True, exist_ok=True)
        if target.exists():
            raise RuntimeError(f"{target}: refusing to replace an existing installation")
        shutil.move(str(directories[0]), target)
    print(f"INSTALLED {package}", flush=True)
    return {**item, "actualSha256": actual, "installedPath": str(target), "verifiedAtUtc": datetime.now(timezone.utc).isoformat()}

records, errors = [], []
with ThreadPoolExecutor(max_workers=4) as pool:
    futures = [pool.submit(restore, item) for item in items]
    for future in as_completed(futures):
        try:
            records.append(future.result())
        except Exception as exc:
            errors.append(str(exc))
            print(f"FAILED {exc}", flush=True)
record = {"recordedAtUtc": datetime.now(timezone.utc).isoformat(), "network": "Inherited proxy and CA settings retained; normal verified HTTPS; no policy changes", "artifacts": sorted(records, key=lambda item: item["package"]), "errors": errors}
(EVIDENCE / "android-restored-downloads.json").write_text(json.dumps(record, indent=2) + "\n")
if errors:
    raise SystemExit(1)
