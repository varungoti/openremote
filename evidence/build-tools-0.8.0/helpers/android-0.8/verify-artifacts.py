#!/usr/bin/env python3
"""Verify every Android split, its retained signature, and frozen build inputs."""
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import struct
import subprocess
import xml.etree.ElementTree as ET
import zipfile

ROOT = Path(__file__).resolve().parent
REPO = ROOT.parent.parent / "work/remote-desktop-platform"
EVIDENCE = REPO / "android/verification/activation-0.8.0"
TOOLS = ROOT / "sdk/build-tools/35.0.0"
CERT = "24c0441ceca6f73ea0e6de8a0958be6229cfcf2518328e8f7f9b71d98b980b28"
env = os.environ.copy()
env["JAVA_HOME"] = str(ROOT / "jdk-21.0.13+9")
env["PATH"] = env["JAVA_HOME"] + "/bin:" + env["PATH"]

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def command(name, args):
    result = subprocess.run(args, capture_output=True, text=True, env=env)
    (EVIDENCE / (name + ".txt")).write_text(result.stdout + result.stderr)
    return result

artifacts, alignment, signatures, native = [], [], [], []
for variant in ["debug", "release"]:
    for abi in ["arm64-v8a", "armeabi-v7a", "x86_64", "x86"]:
        suffix = "debug" if variant == "debug" else "release-unsigned"
        apk = REPO / f"android/app/build/outputs/apk/{variant}/app-{abi}-{suffix}.apk"
        if not apk.is_file():
            raise RuntimeError(f"Missing split {apk.name}")
        relative = apk.relative_to(REPO).as_posix()
        badging = command(apk.stem + "-package", [str(TOOLS / "aapt"), "dump", "badging", str(apk)])
        if badging.returncode:
            raise RuntimeError(f"Could not inspect {apk.name}")
        package = "com.openremote.mobile.dev" if variant == "debug" else "com.openremote.mobile"
        label = "OpenRemote Preview" if variant == "debug" else "OpenRemote"
        for expected in [f"package: name='{package}'", "versionCode='8'", "versionName='0.8.0'", "sdkVersion:'26'", "targetSdkVersion:'36'", f"application-label:'{label}'", f"native-code: '{abi}'"]:
            if expected not in badging.stdout:
                raise RuntimeError(f"Unexpected APK identity in {apk.name}: {expected}")
        sig = command(apk.stem + "-signature", [str(TOOLS / "apksigner"), "verify", "--verbose", "--print-certs", str(apk)])
        signature_text = sig.stdout + sig.stderr
        match = re.search(r"Signer #1 certificate SHA-256 digest:\s*([0-9a-fA-F]+)", signature_text)
        certificate = match.group(1).lower() if match else None
        if variant == "debug" and (sig.returncode != 0 or certificate != CERT):
            raise RuntimeError(f"Preview signature mismatch: {apk.name}")
        with zipfile.ZipFile(apk) as archive:
            signed_entries = [name for name in archive.namelist() if re.match(r"META-INF/[^/]+\.(RSA|DSA|EC)$", name, re.I)]
            if variant == "release" and (sig.returncode == 0 or certificate is not None or signed_entries or b"APK Sig Block 42" in apk.read_bytes()):
                raise RuntimeError(f"Release preview was signed: {apk.name}")
            if abi in ["arm64-v8a", "x86_64"]:
                for name in archive.namelist():
                    if not name.startswith(f"lib/{abi}/") or not name.endswith(".so"):
                        continue
                    data = archive.read(name)
                    if data[:6] != b"\x7fELF\x02\x01":
                        raise RuntimeError(f"Not a little-endian ELF64: {name}")
                    machine = struct.unpack_from("<H", data, 18)[0]
                    offset = struct.unpack_from("<Q", data, 32)[0]
                    entry_size, count = struct.unpack_from("<HH", data, 54)
                    segments = []
                    for index in range(count):
                        values = struct.unpack_from("<IIQQQQQQ", data, offset + index * entry_size)
                        if values[0] == 1:
                            segments.append({"offset": values[2], "vaddr": values[3], "alignment": values[7], "congruent16KiB": values[2] % 16384 == values[3] % 16384})
                    passed = bool(segments) and all(segment["alignment"] >= 16384 and segment["congruent16KiB"] for segment in segments)
                    if not passed:
                        raise RuntimeError(f"Native library alignment failed: {name}")
                    native.append({"apk": relative, "library": name, "sha256": hashlib.sha256(data).hexdigest(), "machine": machine, "loadSegments": segments, "passes16KiB": passed})
        check = command(apk.stem + "-alignment", [str(TOOLS / "zipalign"), "-c", "-P", "16", "4", str(apk)])
        if check.returncode:
            raise RuntimeError(f"ZIP alignment failed: {apk.name}")
        alignment.append({"path": relative, "command": ["zipalign", "-c", "-P", "16", "4", relative], "exitCode": check.returncode})
        signatures.append({"path": relative, "exitCode": sig.returncode, "validPreviewSignature": variant == "debug", "certificateSha256": certificate, "unsignedRelease": variant == "release"})
        artifacts.append({"path": relative, "filename": apk.name, "variant": variant, "abi": abi, "package": package, "label": label, "versionCode": 8, "versionName": "0.8.0", "sha256": digest(apk), "sizeBytes": apk.stat().st_size})
        if abi == "arm64-v8a":
            manifest = command("apk-" + variant + "-manifest", [str(TOOLS / "aapt"), "dump", "xmltree", str(apk), "AndroidManifest.xml"])
            if manifest.returncode or "android.intent.action.VIEW" in manifest.stdout or "android.intent.category.BROWSABLE" in manifest.stdout:
                raise RuntimeError(f"Unexpected exported setup link in {apk.name}")
            if "android.intent.action.MAIN" not in manifest.stdout or "android.intent.category.LAUNCHER" not in manifest.stdout:
                raise RuntimeError(f"Launcher missing in {apk.name}")

tests = {"tests": 0, "failures": 0, "errors": 0, "skipped": 0}
suites = []
junit = EVIDENCE / "junit"
junit.mkdir(exist_ok=True)
for path in sorted((REPO / "android/app/build/test-results/testDebugUnitTest").glob("TEST-*.xml")):
    suite = ET.parse(path).getroot()
    values = {key: int(suite.attrib.get(key, "0")) for key in tests}
    for key, value in values.items():
        tests[key] += value
    suites.append({"name": suite.attrib["name"], **values})
    shutil.copyfile(path, junit / path.name)
if not suites or tests["failures"] or tests["errors"]:
    raise RuntimeError("Android unit tests are missing or failed")
activation = [suite for suite in suites if suite["name"].endswith("ActivationConnectionTest")]
if len(activation) != 1 or activation[0]["tests"] != 7:
    raise RuntimeError("Expected seven frozen activation contract tests")
lint_source = REPO / "android/app/build/reports/lint-results-debug.xml"
issues = ET.parse(lint_source).getroot().findall("issue")
lint = {"errors": sum(issue.attrib.get("severity") in ["Error", "Fatal"] for issue in issues), "warnings": sum(issue.attrib.get("severity") == "Warning" for issue in issues), "issues": [{key: issue.attrib.get(key) for key in ["id", "severity", "message"]} for issue in issues]}
if lint["errors"]:
    raise RuntimeError("Android lint errors found")
shutil.copyfile(REPO / "android/app/build/reports/lint-results-debug.txt", EVIDENCE / "lint-results-debug.txt")
snapshot = json.loads((EVIDENCE / "source-snapshot.json").read_text())
if any(digest(REPO / path) != expected for path, expected in snapshot["files"].items()):
    raise RuntimeError("Frozen source changed after build")
build = json.loads((EVIDENCE / "final-build.json").read_text())
if build["exitCode"] != 0 or not build["allInputsUnchanged"]:
    raise RuntimeError("Final build is not a successful frozen-input build")
guards = json.loads((EVIDENCE / "release-guard-checks.json").read_text())
if not guards["defaultUnconfiguredRelease"]["guardRejected"] or not guards["injectedSigningWithPreviewException"]["guardRejected"] or not guards["explicitUnsignedPreview"]["guardAllowed"]:
    raise RuntimeError("Required unconfigured release guards were not verified")
mapping = REPO / "android/app/build/outputs/mapping/release/mapping.txt"
if not mapping.is_file() or mapping.stat().st_size == 0:
    raise RuntimeError("Minified release R8 mapping is missing")
summary = {"recordedAtUtc": datetime.now(timezone.utc).isoformat(), "version": "0.8.0", "versionCode": 8, "status": "build-verified unconfigured development preview; native Android and Internet sessions untested", "baseGitCommit": snapshot["sourceBaseRevision"], "sourceManifestSha256": snapshot["sourceManifestSha256"], "sourceInputs": snapshot["inputs"], "allInputsUnchangedDuringBuild": build["allInputsUnchanged"], "buildExitCode": build["exitCode"], "buildElapsedSeconds": build["elapsedSeconds"], "tests": tests, "testSuites": len(suites), "activationTests": activation[0]["tests"], "lint": lint, "previewPackage": "com.openremote.mobile.dev", "releasePackage": "com.openremote.mobile", "serviceProfileConfigured": False, "unconfiguredPreviewBuildException": True, "releaseSigning": False, "releaseGuards": guards, "r8Mapping": {"path": str(mapping.relative_to(REPO)), "sha256": digest(mapping), "sizeBytes": mapping.stat().st_size}, "debugCertificate": {"sha256": CERT, "alias": "AndroidDebugKey", "production": False, "reusesRetained06And07PreviewKey": True}, "artifacts": artifacts, "executed": ["Full JVM unit test suite", "Debug plus minified unsigned preview release builds for four ABIs", "Debug lint", "Default empty-profile release rejection", "Signing-injection rejection for empty-profile preview exception", "All four preview APK signature checks and all four unsigned release checks", "All eight APK ZIP 16KiB alignment checks", "ARM64 and x86_64 ELF PT_LOAD alignment and offset/address congruence", "All compiled APK package/version/label and ARM64 launcher-only manifest inspection"], "notExecuted": ["Physical Android installation or in-place update", "Android Activity/Compose and Keystore on a device or emulator", "Windows-to-Android video/input", "Real Internet, carrier NAT or TURN relay session", "Production signing or store publication"], "evidence": "android/verification/activation-0.8.0"}
for name, value in [("build-summary.json", summary), ("artifacts.json", artifacts), ("apk-alignment.json", alignment), ("apk-signatures.json", signatures), ("native-alignment.json", native), ("tests.json", {"totals": tests, "suites": suites})]:
    (EVIDENCE / name).write_text(json.dumps(value, indent=2) + "\n")
(EVIDENCE / "artifacts.sha256").write_text("\n".join(item["sha256"] + "  " + item["path"] for item in artifacts) + "\n")
print(json.dumps({"tests": tests, "activationTests": activation[0]["tests"], "lint": lint, "apks": len(artifacts), "retainedCertificate": CERT, "nativeLibrariesChecked": len(native), "sourceInputsUnchanged": True}, indent=2))
