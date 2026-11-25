#!/usr/bin/env python3
"""Run pinned Gradle with the retained preview key and inherited network trust."""
import hashlib
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parent
REPO = ROOT.parent.parent / "work/remote-desktop-platform"
JAVA = ROOT / "jdk-21.0.13+9"
key = ROOT / "android-user/debug.keystore"
if not key.exists():
    raise SystemExit("Retained preview signing key is missing; refusing to create a replacement")
check = subprocess.run([str(JAVA / "bin/keytool"), "-exportcert", "-keystore", str(key), "-alias", "AndroidDebugKey", "-storepass", "android"], capture_output=True)
if check.returncode or hashlib.sha256(check.stdout).hexdigest() != "24c0441ceca6f73ea0e6de8a0958be6229cfcf2518328e8f7f9b71d98b980b28":
    raise SystemExit("Retained preview signing certificate does not match; refusing to build")
env = os.environ.copy()
env.update(JAVA_HOME=str(JAVA), ANDROID_HOME=str(ROOT / "sdk"), ANDROID_SDK_ROOT=str(ROOT / "sdk"), ANDROID_USER_HOME=str(ROOT / "android-user"), GRADLE_USER_HOME=str(ROOT / "gradle-user"))
env["PATH"] = str(JAVA / "bin") + os.pathsep + env["PATH"]
command = [str(ROOT / "gradle-8.13/bin/gradle"), "--no-daemon", "--max-workers=2", "--console=plain", "--init-script", str(ROOT / "local-pins.gradle"), "-Dorg.gradle.jvmargs=-Xmx1536m -Dfile.encoding=UTF-8", *sys.argv[1:]]
raise SystemExit(subprocess.run(command, cwd=REPO / "android", env=env).returncode)
