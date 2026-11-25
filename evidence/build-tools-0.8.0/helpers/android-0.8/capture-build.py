#!/usr/bin/env python3
"""Capture a requested Gradle invocation against the frozen Android source."""
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parent
REPO = ROOT.parent.parent / "work/remote-desktop-platform"
EVIDENCE = REPO / "android/verification/activation-0.8.0"
snapshot = json.loads((EVIDENCE / "source-snapshot.json").read_text())
label = sys.argv[1]
command = [sys.executable, str(ROOT / "run-build.py"), *sys.argv[2:]]

def check():
    return [{"path": path, "expected": expected, "actual": hashlib.sha256((REPO / path).read_bytes()).hexdigest()} for path, expected in snapshot["files"].items()]

before = check()
if any(item["actual"] != item["expected"] for item in before):
    raise SystemExit("Frozen Android inputs changed before build; root must review and refreeze")
started = datetime.now(timezone.utc).isoformat()
clock = time.monotonic()
with (EVIDENCE / (label + ".log")).open("w") as log:
    result = subprocess.run(command, cwd=REPO / "android", stdout=log, stderr=subprocess.STDOUT)
after = check()
record = {"startedAtUtc": started, "completedAtUtc": datetime.now(timezone.utc).isoformat(), "command": command, "exitCode": result.returncode, "elapsedSeconds": round(time.monotonic() - clock, 3), "sourceManifestSha256": snapshot["sourceManifestSha256"], "inputs": snapshot["inputs"], "allInputsUnchanged": all(item["actual"] == item["expected"] for item in after), "before": before, "after": after, "log": label + ".log"}
(EVIDENCE / (label + ".json")).write_text(json.dumps(record, indent=2) + "\n")
print(json.dumps({key: value for key, value in record.items() if key not in ["before", "after"]}, indent=2))
if not record["allInputsUnchanged"]:
    raise SystemExit("Frozen Android inputs changed during build; evidence does not prove the final source")
raise SystemExit(result.returncode)
