#!/usr/bin/env python3
"""Fingerprint and validate OpenRemote private-pilot records; never attest execution."""
import argparse
from collections import Counter
from datetime import datetime, timezone
import hashlib
import json
import math
import os
from pathlib import Path, PurePosixPath
import re
import stat
import sys

MAX_JSON = 1_048_576
MAX_FILE = 1_073_741_824
HEX = re.compile(r"[0-9a-f]{64}")
REVISION = re.compile(r"[0-9a-f]{40}(?:[0-9a-f]{24})?")
STATUSES = {"not_run", "blocked", "failed", "passed"}
# These are physical-pilot observations, not unit-test names. No scenario is optional.
CASES = {
    "desktop_input": "Real changing Windows desktop, Android rendering, pointer, keyboard, Unicode and held-input release",
    "multiple_pcs": "Install and reconnect to two distinct owned Windows PCs by permanent number",
    "consent_permissions": "Local approval, denial, timeout and narrowed grants; view-only cannot inject input",
    "trust_revocation": "Explicit trusted-phone access; revoke denies stale trust and ends access",
    "internet_direct": "Different Internet networks with an actually selected non-relay candidate pair",
    "turn_udp": "Forced relay over UDP with observed selected relay candidate and working desktop/input",
    "turn_tcp": "Forced relay over TCP with observed selected relay candidate and working desktop/input",
    "turn_tls443": "Forced TLS relay on port 443 with selected relay evidence and working desktop/input",
    "renewal_handoff": "Beyond TURN credential lifetime plus Wi-Fi/cellular handoff; recovery or bounded clean termination",
    "coordinator_relay_restart": "Coordinator and relay restart; no silent stuck session or stale authorization",
    "clipboard_upload": "Explicit Unicode clipboard, file SHA-256, cancellation, no overwrite, Internet-zone metadata",
    "monitor_layout": "Two or more displays, negative virtual origin, portrait and mixed DPI with correct pointer mapping",
    "monitor_switch": "Selection uses a fresh session; old input is released and cannot reach the newly selected display",
    "monitor_unplug": "Unplug during held input and capture; input releases and session ends safely",
    "monitor_invalid": "Unknown/stale monitor ID never falls back; view-only selection cannot grant input",
    "lifecycle_cleanup": "Local emergency stop, phone rotation/background, encoder failure and disconnect leave no held input",
    "installation": "Clean install, preserved-identity upgrade, failed-upgrade rollback and uninstall retention choices",
}
ARTIFACT_ROLES = {"windows", "android", "coordinator"}
ENV_FIELDS = {"operator", "windowsPcs", "android", "coordinatorOs", "relayVersion", "networkDescription", "windowsFfmpegSha256"}
PC_FIELDS = {"id", "osVersion", "gpu", "monitorCount"}
ANDROID_FIELDS = {"model", "osVersion", "apiLevel", "pageSizeBytes"}
BASE_FIELDS = {"id", "status", "startedAtUtc", "finishedAtUtc", "pcIds", "observations", "measurements", "evidence"}
# Required typed facts for a claimed pass. Facts remain the operator's assertions.
FACTS = {
    "desktop_input": {"decodedFrames": "positive", "changingDesktopObserved": True, "matchedInputActions": "positive", "heldInputReleased": True},
    "multiple_pcs": {"pcNumbersStableAfterRestart": True, "reconnectBothPcs": True},
    "consent_permissions": {"approvalObserved": True, "denialObserved": True, "timeoutObserved": True, "viewOnlyInputDenied": True},
    "trust_revocation": {"trustedAccessObserved": True, "revokedAccessDenied": True, "activeAccessEnded": True},
    "internet_direct": {"differentInternetNetworks": True, "selectedCandidateType": "direct", "decodedFrames": "positive", "inputObserved": True},
    "turn_udp": {"differentInternetNetworks": True, "selectedCandidateType": "relay", "relayProtocol": "udp", "decodedFrames": "positive", "inputObserved": True},
    "turn_tcp": {"differentInternetNetworks": True, "selectedCandidateType": "relay", "relayProtocol": "tcp", "decodedFrames": "positive", "inputObserved": True},
    "turn_tls443": {"differentInternetNetworks": True, "selectedCandidateType": "relay", "relayProtocol": "tls", "relayPort": 443, "decodedFrames": "positive", "inputObserved": True},
    "renewal_handoff": {"durationSeconds": "positive", "turnCredentialLifetimeSeconds": "positive", "wifiCellularHandoffObserved": True, "noStaleInput": True, "boundedOutcome": True},
    "coordinator_relay_restart": {"coordinatorRestarted": True, "relayRestarted": True, "boundedOutcome": True, "endedCredentialsRejected": True},
    "clipboard_upload": {"clipboardExplicitOnly": True, "unicodeMatched": True, "sourceSha256": "digest", "receivedSha256": "digest", "sizeBytes": "nonnegative", "zoneIdentifierVerified": True, "cancelCleanupObserved": True, "noOverwriteObserved": True},
    "monitor_layout": {"displayCount": "two", "negativeOriginObserved": True, "portraitObserved": True, "mixedDpiObserved": True, "pointerTargetsMatched": True},
    "monitor_switch": {"freshSessionObserved": True, "oldInputReleased": True, "oldSessionInputRejected": True, "selectedDisplayObserved": True},
    "monitor_unplug": {"heldInputReleased": True, "captureStopped": True, "sessionEnded": True},
    "monitor_invalid": {"unknownIdRejected": True, "staleIdRejected": True, "noFallback": True, "viewOnlyInputDenied": True},
    "lifecycle_cleanup": {"emergencyStopObserved": True, "rotationObserved": True, "backgroundObserved": True, "encoderFailureObserved": True, "heldInputReleased": True, "captureStopped": True},
    "installation": {"cleanInstallObserved": True, "identityPreserved": True, "failedUpgradeRolledBack": True, "uninstallChoicesObserved": True},
}


class Invalid(ValueError):
    pass


def require(condition, message):
    if not condition:
        raise Invalid(message)


def exact(value, fields, where):
    require(isinstance(value, dict) and set(value) == set(fields), f"{where}: unexpected or missing fields")


def text(value, where, limit=4096, nonempty=False):
    require(isinstance(value, str) and len(value.encode("utf-8")) <= limit and "\x00" not in value,
            f"{where}: bounded UTF-8 text required")
    if nonempty:
        require(bool(value.strip()), f"{where}: value is required")


def integer(value, low, high, where):
    require(type(value) is int and low <= value <= high, f"{where}: integer outside allowed range")


def digest(value, where):
    require(isinstance(value, str) and HEX.fullmatch(value), f"{where}: lowercase SHA-256 required")


def timestamp(value, where):
    text(value, where, 40, True)
    require(re.fullmatch(r"[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}(?:\.[0-9]{1,6})?Z", value),
            f"{where}: full UTC timestamp ending Z required")
    try:
        return datetime.fromisoformat(value[:-1] + "+00:00")
    except ValueError as exc:
        raise Invalid(f"{where}: invalid timestamp") from exc


def duplicate_free(pairs):
    out = {}
    for key, value in pairs:
        require(key not in out, f"duplicate JSON field: {key}")
        out[key] = value
    return out


def finite_float(value):
    number = float(value)
    require(math.isfinite(number), "nonfinite JSON number")
    return number


def read_json(path):
    with Path(path).open("rb") as stream:
        data = stream.read(MAX_JSON + 1)
    require(len(data) <= MAX_JSON, "JSON report exceeds 1 MiB")
    try:
        return json.loads(data.decode("utf-8"), object_pairs_hook=duplicate_free,
                          parse_float=finite_float, parse_constant=lambda value: (_ for _ in ()).throw(Invalid("nonfinite JSON number")))
    except (UnicodeError, json.JSONDecodeError, RecursionError) as exc:
        raise Invalid("invalid UTF-8 JSON") from exc


def local_file(root, relative):
    text(relative, "file path", 1024, True)
    path = PurePosixPath(relative)
    require(not path.is_absolute() and relative == path.as_posix() and
            all(part not in {".", ".."} for part in path.parts) and "\\" not in relative and ":" not in relative,
            "file paths must be normalized relative paths without traversal")
    base = Path(root).resolve(strict=True)
    current = base
    for part in path.parts:
        current = current / part
        require(not current.is_symlink(), "symlink evidence/artifact paths are not accepted")
        # Windows directory junctions/reparse points also leave the owned bundle boundary.
        info = current.lstat()
        require(not getattr(info, "st_file_attributes", 0) & getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0x400),
                "reparse-point evidence/artifact paths are not accepted")
    resolved = current.resolve(strict=True)
    require(resolved.is_relative_to(base) and resolved.is_file(), "regular file inside the evidence root required")
    return resolved


def pin_file(root, relative):
    path = local_file(root, relative)
    before = path.stat()
    require(0 < before.st_size <= MAX_FILE, "evidence/artifact files must be 1 byte to 1 GiB")
    hasher = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            hasher.update(block)
    after = path.stat()
    require((before.st_size, before.st_mtime_ns, before.st_ino) == (after.st_size, after.st_mtime_ns, after.st_ino),
            "file changed while hashing")
    return {"path": relative, "sizeBytes": after.st_size, "sha256": hasher.hexdigest()}


def check_pin(root, pin, where):
    exact(pin, {"path", "sizeBytes", "sha256"}, where)
    integer(pin["sizeBytes"], 1, MAX_FILE, where + ".sizeBytes")
    digest(pin["sha256"], where + ".sha256")
    require(pin_file(root, pin["path"]) == pin, f"{where}: recorded bytes/hash do not match local file")


def template(root, version, revision, paths):
    require(REVISION.fullmatch(revision), "full lowercase Git source revision required")
    text(version, "version", 64, True)
    require(set(paths) == ARTIFACT_ROLES, "Windows, Android and coordinator artifacts required")
    return {
        "schemaVersion": 1, "purpose": "OpenRemote private pilot observations",
        "candidate": {"version": version, "sourceRevision": revision,
                      "artifacts": {role: pin_file(root, paths[role]) for role in sorted(paths)}},
        "environment": {"operator": "", "windowsPcs": [], "android": {"model": "", "osVersion": "", "apiLevel": 0, "pageSizeBytes": 0},
                        "coordinatorOs": "", "relayVersion": "", "networkDescription": "", "windowsFfmpegSha256": ""},
        "cases": [{"id": case, "status": "not_run", "startedAtUtc": "", "finishedAtUtc": "", "pcIds": [],
                   "observations": "", "measurements": {}, "evidence": []} for case in CASES],
    }


def check_environment(environment, executed):
    exact(environment, ENV_FIELDS, "environment")
    for field in ("operator", "coordinatorOs", "relayVersion", "networkDescription"):
        text(environment[field], "environment." + field, nonempty=executed)
    ffmpeg = environment["windowsFfmpegSha256"]
    if ffmpeg or executed:
        digest(ffmpeg, "windowsFfmpegSha256")
    exact(environment["android"], ANDROID_FIELDS, "android")
    for field in ("model", "osVersion"):
        text(environment["android"][field], "android." + field, 256, executed)
    integer(environment["android"]["apiLevel"], 26 if executed else 0, 1000, "android.apiLevel")
    page_size = environment["android"]["pageSizeBytes"]
    require(type(page_size) is int and page_size in ({4096, 16384, 65536} if executed else {0, 4096, 16384, 65536}),
            "android.pageSizeBytes: record the measured page size")
    pcs = environment["windowsPcs"]
    require(isinstance(pcs, list) and len(pcs) <= 32, "windowsPcs: bounded list required")
    pc_ids = set()
    for pc in pcs:
        exact(pc, PC_FIELDS, "windowsPcs item")
        for field in ("id", "osVersion", "gpu"):
            text(pc[field], "windowsPcs." + field, 256, True)
        require(pc["id"] not in pc_ids, "duplicate Windows PC label")
        pc_ids.add(pc["id"])
        integer(pc["monitorCount"], 1, 64, "windowsPcs.monitorCount")
    if executed:
        require(pc_ids, "at least one physical Windows PC must be identified")
    return pc_ids


def check_facts(case):
    case_id, facts = case["id"], case["measurements"]
    exact(facts, FACTS[case_id], case_id + ".measurements")
    for key, rule in FACTS[case_id].items():
        value = facts[key]
        where = case_id + "." + key
        if rule in ("positive", "nonnegative", "two"):
            integer(value, {"positive": 1, "nonnegative": 0, "two": 2}[rule], 2**53 - 1, where)
        elif rule == "digest":
            digest(value, where)
        elif rule == "direct":
            require(value in {"host", "srflx", "prflx"}, where + ": selected non-relay candidate required")
        else:
            require(type(value) is type(rule) and value == rule, where + ": required observation missing")
    if case_id == "renewal_handoff":
        require(facts["durationSeconds"] > facts["turnCredentialLifetimeSeconds"], "session must outlive TURN credential lifetime")
    if case_id == "clipboard_upload":
        require(facts["sourceSha256"] == facts["receivedSha256"], "uploaded file hashes differ")
    if case_id == "multiple_pcs":
        require(len(case["pcIds"]) >= 2, "multiple_pcs requires two distinct PC labels")


def validate(report, root, expected_revision=None, require_pass=False):
    exact(report, {"schemaVersion", "purpose", "candidate", "environment", "cases"}, "report")
    require(type(report["schemaVersion"]) is int and report["schemaVersion"] == 1, "unsupported schemaVersion")
    require(report["purpose"] == "OpenRemote private pilot observations", "wrong evidence purpose")
    candidate = report["candidate"]
    exact(candidate, {"version", "sourceRevision", "artifacts"}, "candidate")
    text(candidate["version"], "version", 64, True)
    require(isinstance(candidate["sourceRevision"], str) and REVISION.fullmatch(candidate["sourceRevision"]), "full Git source revision required")
    if expected_revision is not None:
        require(candidate["sourceRevision"] == expected_revision, "source revision differs from the expected candidate")
    if require_pass:
        require(expected_revision is not None, "--require-pass requires --expected-revision from the reviewed build")
    exact(candidate["artifacts"], ARTIFACT_ROLES, "artifacts")
    for role, pin in candidate["artifacts"].items():
        check_pin(root, pin, "artifact " + role)
    cases = report["cases"]
    require(isinstance(cases, list) and len(cases) == len(CASES), "exact required pilot scenario set is required")
    seen = set()
    for case in cases:
        exact(case, BASE_FIELDS, "case")
        require(isinstance(case["id"], str) and case["id"] in CASES and case["id"] not in seen, "unknown or duplicate case ID")
        require(isinstance(case["status"], str) and case["status"] in STATUSES, "unknown case status")
        seen.add(case["id"])
    pc_ids = check_environment(report["environment"], any(c["status"] in {"passed", "failed"} for c in cases))
    for case in cases:
        case_id, status = case["id"], case["status"]
        text(case["observations"], case_id + ".observations", nonempty=status != "not_run")
        require(isinstance(case["pcIds"], list) and len(case["pcIds"]) <= 32 and
                all(isinstance(p, str) and p in pc_ids for p in case["pcIds"]) and len(set(case["pcIds"])) == len(case["pcIds"]),
                case_id + ": PC labels must reference the environment without duplicates")
        require(isinstance(case["measurements"], dict) and len(case["measurements"]) <= 32, case_id + ": bounded measurements required")
        require(isinstance(case["evidence"], list) and len(case["evidence"]) <= 32, case_id + ": bounded evidence list required")
        if status == "not_run":
            require(case["startedAtUtc"] == case["finishedAtUtc"] == "" and not case["measurements"] and not case["evidence"],
                    case_id + ": not_run must not contain execution evidence")
        else:
            start = timestamp(case["startedAtUtc"], case_id + ".startedAtUtc")
            finish = timestamp(case["finishedAtUtc"], case_id + ".finishedAtUtc")
            require(finish >= start, case_id + ": finish precedes start")
            require(finish <= datetime.now(timezone.utc), case_id + ": future execution timestamp")
            require(case["evidence"], case_id + ": executed/blocked cases require an archived observation or log")
            for pin in case["evidence"]:
                check_pin(root, pin, case_id + " evidence")
            if status in {"passed", "failed"}:
                require(case["pcIds"], case_id + ": physical PC label required")
            if status == "passed":
                check_facts(case)
                if case_id == "monitor_layout":
                    declared = {p["id"]: p["monitorCount"] for p in report["environment"]["windowsPcs"]}
                    require(any(declared[p] >= case["measurements"]["displayCount"] for p in case["pcIds"]),
                            "monitor_layout count exceeds identified PC display counts")
    counts = Counter(c["status"] for c in cases)
    passed = counts["passed"] == len(CASES)
    require(not require_pass or passed, "pilot has failed, blocked or not-run scenarios")
    return {"recordValid": True, "reportedPilotComplete": passed, "releaseReady": False,
            "sourceRevision": candidate["sourceRevision"], "counts": {s: counts[s] for s in sorted(STATUSES)},
            "note": "Hashes and required recorded observations validated. This tool cannot attest execution, build provenance, signatures, or production readiness."}


def write_new(path, value):
    data = (json.dumps(value, indent=2, ensure_ascii=False, allow_nan=False) + "\n").encode("utf-8")
    require(len(data) <= MAX_JSON, "JSON exceeds report size limit")
    # Explicit target only, no directory creation, no overwrite, no deletion.
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd, "wb") as stream:
        stream.write(data)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    init = commands.add_parser("init", help="fingerprint candidate and create all-not-run report")
    init.add_argument("--root", type=Path, required=True)
    init.add_argument("--version", required=True)
    init.add_argument("--source-revision", required=True)
    for role in sorted(ARTIFACT_ROLES):
        init.add_argument("--" + role, required=True, help="normalized relative file path inside --root")
    init.add_argument("--output", type=Path, required=True)
    commands.add_parser("requirements", help="print scenario descriptions and required passing observations")
    pin = commands.add_parser("pin", help="print one local evidence file's SHA-256 record")
    pin.add_argument("--root", type=Path, required=True)
    pin.add_argument("path")
    check = commands.add_parser("validate", help="check record structure, candidate and evidence bytes")
    check.add_argument("report", type=Path)
    check.add_argument("--root", type=Path, required=True)
    check.add_argument("--expected-revision")
    check.add_argument("--require-pass", action="store_true")
    args = parser.parse_args()
    try:
        if args.command == "init":
            value = template(args.root, args.version, args.source_revision, {role: getattr(args, role) for role in ARTIFACT_ROLES})
            write_new(args.output, value)
            print(f"Created {args.output}: all {len(CASES)} physical pilot scenarios are not_run.")
        elif args.command == "requirements":
            print(json.dumps({case: {"description": CASES[case], "requiredMeasurements": FACTS[case]} for case in CASES}, indent=2))
        elif args.command == "pin":
            print(json.dumps(pin_file(args.root, args.path), indent=2))
        else:
            print(json.dumps(validate(read_json(args.report), args.root, args.expected_revision, args.require_pass), indent=2))
        return 0
    except (Invalid, OSError, UnicodeError, TypeError, OverflowError) as exc:
        print(f"Pilot evidence rejected: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
