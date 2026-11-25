#!/usr/bin/env python3
"""Restore Android dependency bytes using checked-in strict SHA-256 pins."""
from concurrent.futures import ThreadPoolExecutor, as_completed
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import xml.etree.ElementTree as ET
from urllib.parse import urljoin, urlsplit

ROOT = Path(__file__).resolve().parent
REPO = ROOT.parent.parent / "work/remote-desktop-platform"
EVIDENCE = REPO / "evidence/build-tools-0.8.0"
MAVEN = ROOT / "maven"
CACHE = ROOT / "gradle-user/caches/modules-2/files-2.1"
metadata = REPO / "android/gradle/verification-metadata.xml"
old = json.loads((REPO / "android/verification/code-pin-0.6.0/dependency-prefetch-initial.json").read_text())["results"]
urls = {(item["group"], item["name"], item["version"], item["file"]): item["url"] for item in old if "url" in item and "error" not in item}
for item in json.loads((REPO / "android/verification/code-pin-0.6.0/additional-metadata-pins.json").read_text()):
    urls[(item["group"], item["module"], item["version"], item["artifact"])] = item["url"]

namespace = {"v": "https://schema.gradle.org/dependency-verification"}
items = []
for component in ET.parse(metadata).findall(".//v:component", namespace):
    group, name, version = (component.attrib[key] for key in ["group", "name", "version"])
    for artifact in component.findall("v:artifact", namespace):
        file = artifact.attrib["name"]
        checksums = [child.attrib["value"] for child in artifact.findall("v:sha256", namespace)]
        if not checksums:
            raise RuntimeError(f"No SHA-256 pin for {group}:{name}:{version}:{file}")
        key = (group, name, version, file)
        relative = "/".join([group.replace(".", "/"), name, version, file])
        base = "https://dl.google.com/dl/android/maven2/" if group.startswith(("androidx.", "com.android", "com.google.testing.platform")) else "https://repo.maven.apache.org/maven2/"
        url = urls.get(key, base + relative)
        # Gradle metadata names such as "runtime.aar" are aliases. The pinned
        # .module file gives their real, versioned repository URL.
        module_name = name + "-" + version + ".module"
        module_path = MAVEN / group.replace(".", "/") / name / version / module_name
        module_pin = component.find("v:artifact[@name='" + module_name + "']", namespace)
        if file != module_name and module_path.exists() and module_pin is not None:
            expected_module = [child.attrib["value"] for child in module_pin.findall("v:sha256", namespace)]
            if hashlib.sha256(module_path.read_bytes()).hexdigest() not in expected_module:
                raise RuntimeError("Module metadata has not passed its strict pin")
            module = json.loads(module_path.read_text())
            matches = {entry["url"] for variant in module.get("variants", []) for entry in variant.get("files", []) if entry.get("name") == file}
            if len(matches) > 1:
                raise RuntimeError("Multiple different URLs for the same artifact alias")
            if matches:
                module_relative = "/".join([group.replace(".", "/"), name, version, module_name])
                module_url = urls.get((group, name, version, module_name), base + module_relative)
                artifact_url = next(iter(matches))
                url = urljoin(module_url, artifact_url)
                relative = str((Path(module_relative).parent / artifact_url))
        items.append({"group": group, "name": name, "version": version, "file": file, "checksums": checksums, "url": url, "relative": relative})

aliases_only = "--aliases-only" in sys.argv[1:]
if aliases_only:
    items = [item for item in items if Path(item["relative"]).name != item["file"]]

def restore(item):
    path = (MAVEN / item["relative"]).resolve()
    if not path.is_relative_to(MAVEN.resolve()):
        raise RuntimeError("Dependency metadata path leaves its task-local repository")
    path.parent.mkdir(parents=True, exist_ok=True)
    if not path.exists():
        temporary = path.with_suffix(path.suffix + ".partial")
        proc = subprocess.run(["curl", "--fail", "--location", "--silent", "--show-error", "--max-time", "300", "--output", str(temporary), item["url"]], capture_output=True, text=True)
        if proc.returncode:
            raise RuntimeError(f"{item['group']}:{item['file']}: curl {proc.returncode}: {proc.stderr.strip()}")
        temporary.rename(path)
    data = path.read_bytes()
    checksum = hashlib.sha256(data).hexdigest()
    if checksum not in item["checksums"]:
        raise RuntimeError(f"{item['group']}:{item['file']}: strict SHA-256 pin mismatch")
    cache_path = CACHE / item["group"] / item["name"] / item["version"] / hashlib.sha1(data).hexdigest() / item["file"]
    cache_path.parent.mkdir(parents=True, exist_ok=True)
    if not cache_path.exists():
        shutil.copyfile(path, cache_path)
    return {"group": item["group"], "name": item["name"], "version": item["version"], "file": item["file"], "url": item["url"], "sha256": checksum, "bytes": len(data), "cachePath": str(cache_path)}

records, errors = [], []
with ThreadPoolExecutor(max_workers=8) as pool:
    futures = [pool.submit(restore, item) for item in items]
    for future in as_completed(futures):
        try:
            records.append(future.result())
            if len(records) % 25 == 0:
                print(f"VERIFIED {len(records)}/{len(items)} dependency artifacts", flush=True)
        except Exception as exc:
            errors.append(str(exc))
            print(f"FAILED {exc}", flush=True)
result = {"recordedAtUtc": datetime.now(timezone.utc).isoformat(), "verificationMetadataSha256": hashlib.sha256(metadata.read_bytes()).hexdigest(), "expectedArtifacts": len(items), "verifiedArtifacts": len(records), "totalBytes": sum(item["bytes"] for item in records), "strictPinsChanged": False, "network": "Inherited proxy and CA settings retained; normal verified HTTPS; no policy changes", "artifacts": sorted(records, key=lambda item: (item["group"], item["name"], item["version"], item["file"])), "errors": errors}
record_name = "android-dependency-aliases.json" if aliases_only else "android-dependency-restore.json"
(EVIDENCE / record_name).write_text(json.dumps(result, indent=2) + "\n")
print(f"FINISHED {len(records)}/{len(items)} verified; {len(errors)} errors", flush=True)
if errors:
    raise SystemExit(1)
