#!/usr/bin/env python3
"""Restore saved, SHA-256-pinned OpenRemote build tools without global changes."""
from concurrent.futures import ThreadPoolExecutor, as_completed
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import subprocess
from urllib.parse import urlsplit

ROOT = Path(__file__).resolve().parent
REPO = ROOT.parent / "work/remote-desktop-platform"
EVIDENCE = REPO / "evidence/build-tools-0.8.0"
DOWNLOADS = ROOT / "downloads"
DOWNLOADS.mkdir(exist_ok=True)
old = json.loads((REPO / "evidence/windows-startup-0.5.1/toolchain-provenance.json").read_text())
items = [dict(x) for x in old["artifacts"] if "/7zip/" not in x["url"]]
seven = json.loads((REPO / "evidence/windows-startup-0.5.1/extractor-repair-provenance.json").read_text())
items.append({"url": seven["url"], "sha256": seven["sha256"], "bytes": seven["sizeBytes"], "kind": "7zip"})
go = json.loads((REPO / "server/verification/automatic-registration-0.7.0/go-toolchain-provenance.json").read_text())
items.append({"url": go["url"], "sha256": go["sha256"], "bytes": go["size"], "kind": "go"})

def digest(path):
    h = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            h.update(block)
    return h.hexdigest()

def download(item):
    destination = DOWNLOADS / Path(urlsplit(item["url"]).path).name
    if not destination.exists():
        temporary = destination.with_suffix(destination.suffix + ".partial")
        result = subprocess.run(["curl", "--fail", "--location", "--silent", "--show-error", "--max-time", "300", "--output", str(temporary), item["url"]], capture_output=True, text=True)
        if result.returncode:
            raise RuntimeError(f"{destination.name}: curl exit {result.returncode}: {result.stderr.strip()}")
        temporary.rename(destination)
    actual = digest(destination)
    if actual != item["sha256"]:
        raise RuntimeError(f"{destination.name}: SHA-256 mismatch")
    if destination.stat().st_size != item["bytes"]:
        raise RuntimeError(f"{destination.name}: size mismatch")
    print(f"VERIFIED {destination.name}: {item['bytes']} bytes", flush=True)
    return {**item, "path": str(destination), "actualSha256": actual, "verifiedAtUtc": datetime.now(timezone.utc).isoformat()}

records = []
errors = []
with ThreadPoolExecutor(max_workers=4) as pool:
    futures = {pool.submit(download, item): item for item in items}
    for future in as_completed(futures):
        try:
            records.append(future.result())
        except Exception as exc:
            errors.append(str(exc))
            print(f"FAILED {exc}", flush=True)

record = {"recordedAtUtc": datetime.now(timezone.utc).isoformat(), "sourceProvenance": ["evidence/windows-startup-0.5.1/toolchain-provenance.json", "evidence/windows-startup-0.5.1/extractor-repair-provenance.json", "server/verification/automatic-registration-0.7.0/go-toolchain-provenance.json"], "network": "Inherited proxy and CA settings retained; normal verified HTTPS; no policy changes", "artifacts": sorted(records, key=lambda item: item["url"]), "errors": errors}
(EVIDENCE / "restored-downloads.json").write_text(json.dumps(record, indent=2) + "\n")
if errors:
    raise SystemExit(1)
