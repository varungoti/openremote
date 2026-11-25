#!/usr/bin/env python3
"""Verify the restored cross linker and installer tooling without Windows execution."""
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parent
REPO = ROOT.parent / "work/remote-desktop-platform"
EVIDENCE = REPO / "evidence/build-tools-0.8.0"
SMOKE = ROOT / "smoke"
SMOKE.mkdir(exist_ok=True)
records = []

def run(label, command):
    result = subprocess.run(command, capture_output=True, text=True, cwd=SMOKE)
    log = EVIDENCE / (label + ".log")
    log.write_text(result.stdout + result.stderr)
    records.append({"name": label, "command": command, "exitCode": result.returncode, "log": log.relative_to(REPO).as_posix()})
    if result.returncode:
        raise RuntimeError(f"{label} exited {result.returncode}; see {log}")
    return result.stdout + result.stderr

versions = {}
for label, command in [
    ("rustc", ["rustc", "--version"]),
    ("cargo", ["cargo", "--version"]),
    ("rustfmt", ["rustfmt", "--version"]),
    ("clippy", ["cargo", "clippy", "--version"]),
    ("clang", ["x86_64-w64-mingw32-clang", "--version"]),
    ("nsis", ["makensis", "-VERSION"]),
    ("go", ["go", "version"]),
]:
    versions[label] = run("version-" + label, command).strip()
versions["7zip"] = run("version-7zip", ["7zz", "i"]).split("Formats:")[0].strip()

(SMOKE / "cross-link.rs").write_text('fn main() { println!("OpenRemote pinned cross-link smoke"); }\n')
run("windows-link-smoke", ["rustc", "--edition=2021", "--target", "x86_64-pc-windows-gnullvm", "-C", "linker=" + os.environ["CARGO_TARGET_X86_64_PC_WINDOWS_GNULLVM_LINKER"], str(SMOKE / "cross-link.rs"), "-o", str(SMOKE / "cross-link.exe")])
headers = run("windows-link-pe", ["llvm-readobj", "--file-headers", str(SMOKE / "cross-link.exe")])
if "IMAGE_FILE_MACHINE_AMD64" not in headers:
    raise RuntimeError("Cross-link smoke is not AMD64")

(SMOKE / "toolchain-smoke.nsi").write_text('Unicode true\nName "OpenRemote toolchain smoke"\nOutFile "toolchain-smoke.exe"\nInstallDir "$TEMP\\OpenRemoteToolchainSmoke"\nRequestExecutionLevel user\nSection\n  SetOutPath "$INSTDIR"\n  WriteUninstaller "$INSTDIR\\Uninstall.exe"\nSectionEnd\nSection "Uninstall"\n  Delete "$INSTDIR\\Uninstall.exe"\n  RMDir "$INSTDIR"\nSectionEnd\n')
run("nsis-compile-smoke", ["makensis", "-NOCONFIG", "-WX", str(SMOKE / "toolchain-smoke.nsi")])
run("nsis-extract-smoke", ["7zz", "x", "-y", "-o" + str(SMOKE / "extracted"), str(SMOKE / "toolchain-smoke.exe")])
uninstallers = list((SMOKE / "extracted").rglob("Uninstall.exe"))
if len(uninstallers) != 1:
    raise RuntimeError("7-Zip did not extract the generated uninstaller")
uninstaller_headers = run("nsis-uninstaller-pe", ["llvm-readobj", "--file-headers", str(uninstallers[0])])
if "IMAGE_SUBSYSTEM_WINDOWS_GUI" not in uninstaller_headers:
    raise RuntimeError("Generated NSIS uninstaller is not a Windows GUI PE")

record = {"recordedAtUtc": datetime.now(timezone.utc).isoformat(), "versions": versions, "checks": records, "windowsTarget": "x86_64-pc-windows-gnullvm", "windowsCrossLink": "passed", "nsisCompileWithWarningsAsErrors": "passed", "sevenZipGeneratedUninstallerExtraction": "passed", "nativeWindowsExecution": "not_run", "environmentScript": str(ROOT / "env.sh"), "scope": "Task-local compiler, linker, package compiler and extractor only. No application build or native Windows execution is claimed by this record."}
(EVIDENCE / "toolchain-verification.json").write_text(json.dumps(record, indent=2) + "\n")
print(json.dumps({"versions": versions, "windowsCrossLink": "passed", "nsisCompileAndUninstallerExtraction": "passed"}, indent=2))
