#!/usr/bin/env python3
"""Record only the intended empty-profile release guard failures."""
from datetime import datetime, timezone
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent
REPO = ROOT.parent.parent / "work/remote-desktop-platform"
EVIDENCE = REPO / "android/verification/activation-0.8.0"
snapshot = json.loads((EVIDENCE / "source-snapshot.json").read_text())
build = json.loads((EVIDENCE / "final-build.json").read_text())
if build["exitCode"] != 0 or not build["allInputsUnchanged"]:
    raise RuntimeError("Explicit preview build must have passed against frozen inputs")

def rejected(label, expected):
    record = json.loads((EVIDENCE / (label + ".json")).read_text())
    text = (EVIDENCE / record["log"]).read_text()
    passed = record["exitCode"] == 1 and record["allInputsUnchanged"] and expected in text
    if not passed:
        raise RuntimeError(f"{label} did not fail at its expected guard")
    return {
        "command": record["command"],
        "expectedExitCode": 1,
        "observedExitCode": record["exitCode"],
        "guardRejected": passed,
        "allInputsUnchanged": record["allInputsUnchanged"],
        "expectedMessage": expected,
        "log": record["log"],
    }

result = {
    "recordedAtUtc": datetime.now(timezone.utc).isoformat(),
    "sourceManifestSha256": snapshot["sourceManifestSha256"],
    "defaultUnconfiguredRelease": rejected(
        "unconfigured-release-rejection",
        "OpenRemote release packaging is blocked: the provider service profile is unconfigured.",
    ),
    "injectedSigningWithPreviewException": rejected(
        "unconfigured-signing-rejection",
        "An unconfigured OpenRemote preview must remain unsigned; release signing is not permitted with this exception.",
    ),
    "explicitUnsignedPreview": {
        "flag": "-PopenremoteUnconfiguredPreview=true",
        "guardAllowed": True,
        "configured": False,
        "customerRelease": False,
        "log": "final-build.log",
    },
}
(EVIDENCE / "release-guard-checks.json").write_text(json.dumps(result, indent=2) + "\n")
print(json.dumps({"defaultReleaseRejected": True, "injectedSigningRejected": True, "explicitUnsignedPreviewAllowed": True}))
