#!/usr/bin/env python3
"""Package the verified Windows GUI host into the per-user NSIS installer.

No host binary is built or downloaded here. The binary hash, version and every
Rust input must match the supplied build-evidence record before packaging.
The emitted EXE is extracted independently with 7-Zip and every payload byte is
compared before it is published. This does not execute Windows installation.
"""
from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import tempfile
import tomllib
import urllib.request
from datetime import datetime, timezone


ROOT = Path(__file__).resolve().parents[1]
OWNED_PAYLOAD = (
    "openremote-agent.exe", "START-HERE.txt", "THIRD-PARTY-NOTICES.txt",
    "package-manifest.json", "ffmpeg.exe", "FFmpeg-9.0.2-source.tar.xz", "FFmpeg-BUILD.txt",
)
ENCODER_SOURCE_SHA256 = "8c3850283eb25fa026482078a04051e0be17347b09ef81a0849bec15a96e002e"
SYSTEM_DLLS = {
    "advapi32.dll", "bcrypt.dll", "bcryptprimitives.dll", "comctl32.dll",
    "comdlg32.dll", "crypt32.dll", "d3d11.dll", "dxgi.dll", "dwmapi.dll", "gdi32.dll", "imm32.dll",
    "iphlpapi.dll", "kernel32.dll", "ntdll.dll", "ole32.dll", "oleaut32.dll",
    "secur32.dll", "shell32.dll", "shlwapi.dll", "user32.dll", "userenv.dll",
    "uxtheme.dll", "version.dll", "winmm.dll", "ws2_32.dll", "mfplat.dll", "mfuuid.dll",
}


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            h.update(block)
    return h.hexdigest()


def inspect_pe(path: Path, *, required_subsystem: int = 2) -> dict:
    """Parse bounded PE headers without running the input executable."""
    data = path.read_bytes()
    if len(data) < 256 or data[:2] != b"MZ":
        raise ValueError("host is not a Windows PE executable")
    pe_offset = struct.unpack_from("<I", data, 0x3C)[0]
    if pe_offset > len(data) - 256 or data[pe_offset:pe_offset + 4] != b"PE\0\0":
        raise ValueError("invalid PE header offset/signature")
    machine, sections = struct.unpack_from("<HH", data, pe_offset + 4)
    characteristics = struct.unpack_from("<H", data, pe_offset + 22)[0]
    optional_size = struct.unpack_from("<H", data, pe_offset + 20)[0]
    optional = pe_offset + 24
    if optional_size < 96 or optional + optional_size > len(data):
        raise ValueError("truncated PE optional header")
    magic = struct.unpack_from("<H", data, optional)[0]
    subsystem = struct.unpack_from("<H", data, optional + 68)[0]
    result = {"machine": machine, "optionalMagic": magic, "subsystem": subsystem}
    if machine != 0x8664 or magic != 0x20B:
        raise ValueError("host must be an AMD64 PE32+ executable")
    if not characteristics & 0x2 or characteristics & 0x2000:
        raise ValueError("host must be an executable image, not a DLL")
    if subsystem != required_subsystem:
        if required_subsystem == 2:
            raise ValueError("host must use the Windows GUI subsystem; the old console host is not accepted")
        raise ValueError("bundled encoder must use the expected Windows console subsystem")
    table = optional + optional_size
    if not (1 <= sections <= 96) or table + sections * 40 > len(data):
        raise ValueError("invalid PE section table")
    section_ranges = []
    for index in range(sections):
        offset = table + index * 40
        virtual_size, rva, raw_size, raw_offset = struct.unpack_from("<IIII", data, offset + 8)
        if raw_offset + raw_size > len(data):
            raise ValueError("truncated PE section")
        section_ranges.append((rva, max(virtual_size, raw_size), raw_offset, raw_size))

    def from_rva(rva: int, length: int) -> int:
        for start, size, raw, raw_size in section_ranges:
            if start <= rva and rva + length <= start + size:
                relative = rva - start
                if relative + length > raw_size:
                    break
                return raw + relative
        raise ValueError("PE directory points outside file-backed sections")

    imports = []
    if optional_size >= 128:
        directory_count = struct.unpack_from("<I", data, optional + 108)[0]
        if directory_count > 1:
            import_rva, import_size = struct.unpack_from("<II", data, optional + 120)
            if import_rva:
                if not (20 <= import_size <= 1024 * 1024):
                    raise ValueError("invalid PE import-directory size")
                for index in range(min(import_size // 20, 4096)):
                    position = from_rva(import_rva + index * 20, 20)
                    descriptor = struct.unpack_from("<IIIII", data, position)
                    if not any(descriptor):
                        break
                    name_position = from_rva(descriptor[3], 1)
                    end = data.find(b"\0", name_position, min(name_position + 260, len(data)))
                    if end < 0:
                        raise ValueError("unterminated PE import DLL name")
                    name = data[name_position:end].decode("ascii").lower()
                    if any(character in name for character in "/\\:"):
                        raise ValueError("unexpected path in PE import DLL name")
                    imports.append(name)
                else:
                    raise ValueError("unterminated PE import table")
    result["importDlls"] = sorted(set(imports))
    return result


def inspect_setup_stub(path: Path) -> dict:
    """NSIS uses a native x86 GUI stub to install an AMD64 host on x64 PCs."""
    data = path.read_bytes()
    if len(data) < 256 or data[:2] != b"MZ":
        raise ValueError("setup/uninstaller is not a native PE executable")
    offset = struct.unpack_from("<I", data, 0x3C)[0]
    if offset > len(data) - 96 or data[offset:offset + 4] != b"PE\0\0":
        raise ValueError("setup/uninstaller has an invalid PE header")
    machine = struct.unpack_from("<H", data, offset + 4)[0]
    characteristics = struct.unpack_from("<H", data, offset + 22)[0]
    optional_size = struct.unpack_from("<H", data, offset + 20)[0]
    if optional_size < 72 or offset + 24 + optional_size > len(data):
        raise ValueError("setup/uninstaller has a truncated optional header")
    magic = struct.unpack_from("<H", data, offset + 24)[0]
    subsystem = struct.unpack_from("<H", data, offset + 24 + 68)[0]
    if (machine, magic) not in ((0x14C, 0x10B), (0x8664, 0x20B)) or subsystem != 2 or not characteristics & 0x2 or characteristics & 0x2000:
        raise ValueError("setup/uninstaller must be a native Windows GUI executable")
    return {"machine": machine, "optionalMagic": magic, "subsystem": subsystem}


def verify_host(host: Path, evidence: dict, root: Path = ROOT) -> dict:
    version = tomllib.loads((root / "agent-windows/Cargo.toml").read_text())["package"]["version"]
    if evidence.get("hostVersion") != version:
        raise ValueError("host evidence version does not match Cargo.toml")
    if version != "0.8.0":
        raise ValueError("this installer source targets host version 0.8.0")
    expected_hash = evidence.get("binary", {}).get("sha256", "")
    if sha256(host) != expected_hash:
        raise ValueError("host does not match the verified build's SHA-256")
    actual_inputs = {"Cargo.toml", "Cargo.lock", "../config/service-profile.json", "../config/encoder-profile.json"}
    agent = root / "agent-windows"
    for directory in ("src", "examples"):
        actual_inputs.update(path.relative_to(agent).as_posix() for path in (agent / directory).rglob("*.rs"))
    if (agent / "build.rs").exists():
        actual_inputs.add("build.rs")
    recorded_inputs = evidence.get("sourceInputs", {}).get("files", {})
    if actual_inputs != set(recorded_inputs):
        raise ValueError("host evidence does not cover the exact current Rust source-input set")
    for relative in sorted(actual_inputs):
        if sha256(agent / relative) != recorded_inputs[relative]:
            raise ValueError(f"host source changed after verification: {relative}")
    pe = inspect_pe(host)
    unexpected = [name for name in pe["importDlls"] if name not in SYSTEM_DLLS and not name.startswith("api-ms-win-")]
    if unexpected:
        raise ValueError(f"host needs an unbundled non-system DLL: {', '.join(unexpected)}")
    return {"version": version, "sha256": expected_hash, "sizeBytes": host.stat().st_size, "pe": pe}


def admit_service_profile(root: Path) -> dict:
    spec = importlib.util.spec_from_file_location("openremote_service_profile", Path(__file__).with_name("configure_service_profile.py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    profile = module.read_profile(root / "config/service-profile.json")
    if not profile["apiOrigin"]:
        raise ValueError("Installer blocked: the publisher must deploy and verify the OpenRemote service, then configure the shared profile. Unconfigured installers are not distributable.")
    return {"mode": "managed", "originConfigured": True, "apiOrigin": profile["apiOrigin"], "liveCapabilitiesChecked": False}


def check_live_service(service: dict) -> None:
    class NoRedirect(urllib.request.HTTPRedirectHandler):
        def redirect_request(self, req, fp, code, msg, headers, newurl):
            raise ValueError("Publisher service must answer directly; redirects are not accepted")

    request = urllib.request.Request(service["apiOrigin"] + "/v1/info", headers={"Accept": "application/json"})
    with urllib.request.build_opener(NoRedirect()).open(request, timeout=15) as response:
        raw = response.read(16385)
        if response.status != 200 or len(raw) > 16384:
            raise ValueError("Publisher service discovery is unavailable or too large")
    def unique_object(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                raise ValueError("Publisher discovery contains a duplicate field")
            result[key] = value
        return result

    info = json.loads(raw, object_pairs_hook=unique_object)
    if not isinstance(info, dict):
        raise ValueError("Publisher discovery must be an object")
    relay = info.get("relay")
    versions = [info.get(key) for key in ("protocolVersion", "automaticHostRegistrationVersion", "pairingCodeVersion", "activationVersion")]
    if not (info.get("service") == "openremote"
            and all(type(value) is int and value == 1 for value in versions)
            and type(info.get("maxActivationConnections")) is int and info["maxActivationConnections"] == 4
            and isinstance(relay, dict) and relay.get("configured") is True and relay.get("tls") is True):
        raise ValueError("Publisher service must enable automatic registration, PIN connections and TLS relay configuration")
    service.update(liveCapabilitiesChecked=True, checkedAtUtc=datetime.now(timezone.utc).isoformat(),
                   scope="Verified HTTPS discovery only; actual relay and native media still require release tests")


def verify_encoder(encoder: Path, evidence: dict, source: Path, root: Path = ROOT) -> dict:
    profile = json.loads((root / "config/encoder-profile.json").read_text())
    if set(profile) != {"schemaVersion", "fileName", "version", "encoder", "sha256", "sizeBytes"} or profile["schemaVersion"] != 1:
        raise ValueError("Invalid bundled encoder profile")
    if (profile["fileName"], profile["version"], profile["encoder"]) != ("ffmpeg.exe", "9.0.2", "h264_mf"):
        raise ValueError("Unsupported bundled encoder")
    actual = sha256(encoder)
    if actual != profile["sha256"] or encoder.stat().st_size != profile["sizeBytes"]:
        raise ValueError("Encoder bytes do not match the host's compiled runtime profile")
    if evidence.get("status") != "passed" or evidence.get("artifact", {}).get("sha256") != actual or evidence.get("artifact", {}).get("sizeBytes") != encoder.stat().st_size:
        raise ValueError("Encoder build evidence does not match its bytes")
    if evidence.get("license") != "LGPL-2.1-or-later" or evidence.get("externalCodecLibraries") != []:
        raise ValueError("Unexpected encoder redistribution configuration")
    if evidence.get("sourceSha256") != ENCODER_SOURCE_SHA256 or sha256(source) != ENCODER_SOURCE_SHA256:
        raise ValueError("Exact corresponding FFmpeg source archive is required")
    pe = inspect_pe(encoder, required_subsystem=3)
    unexpected = [name for name in pe["importDlls"] if name not in SYSTEM_DLLS and not name.startswith("api-ms-win-")]
    if unexpected:
        raise ValueError("Encoder requires an unbundled non-system DLL")
    return {"version": profile["version"], "codec": profile["encoder"], "sha256": actual,
            "sizeBytes": encoder.stat().st_size, "sourceSha256": ENCODER_SOURCE_SHA256, "pe": pe,
            "license": "LGPL-2.1-or-later", "nativeWindowsExecution": "not_run"}


def nsis_quote(value: str) -> str:
    if any(character in value for character in "\r\n\0"):
        raise ValueError("newline/NUL in an NSIS definition")
    return value.replace("$", "$$").replace('"', '$\\"')


def collect_notices(root: Path) -> tuple[bytes, dict[str, str]]:
    paths = [root / "THIRD_PARTY_NOTICES.md", root / "licenses/README.md"]
    paths.extend(sorted((root / "licenses/rust").rglob("*")))
    paths.extend(sorted((root / "licenses/rust-upstream").rglob("*")))
    paths.extend(sorted((root / "licenses/ffmpeg").rglob("*")))
    paths.append(root / "packaging/windows/installer/NSIS-3.11-copyright.txt")
    records = {}
    chunks = [
        b"OpenRemote Windows development installer: third-party notices\r\n"
        b"Original source texts follow without editorial changes.\r\n"
        b"The inventory's unresolved redistribution review is retained below.\r\n\r\n"
    ]
    for path in paths:
        if not path.is_file():
            continue
        relative = path.relative_to(root).as_posix()
        raw = path.read_bytes()
        # Keep exact bytes, including non-UTF-8 upstream notices. The manifest
        # identifies every original file; the combined output is not a new grant.
        records[relative] = hashlib.sha256(raw).hexdigest()
        chunks.append(f"\r\n===== {relative} =====\r\n".encode("utf-8"))
        chunks.append(raw)
        chunks.append(b"\r\n")
    if len(records) < 100:
        raise ValueError("Rust notice inventory is unexpectedly incomplete")
    return b"".join(chunks), records


def run(command: list[str], *, cwd: Path, log: Path, env: dict | None = None) -> None:
    with log.open("wb") as stream:
        result = subprocess.run(command, cwd=cwd, env=env, stdout=stream, stderr=subprocess.STDOUT)
    if result.returncode:
        raise RuntimeError(f"command exited {result.returncode}; see {log}")


def verify_extracted_payload(extracted: Path, payload: Path) -> dict:
    results = {}
    for name in OWNED_PAYLOAD:
        candidates = [path for path in extracted.rglob(name) if path.is_file()]
        if len(candidates) != 1:
            raise ValueError(f"installer extraction must contain exactly one {name}")
        actual = sha256(candidates[0])
        expected = sha256(payload / name)
        if actual != expected:
            raise ValueError(f"installer changed the payload bytes for {name}")
        results[name] = {"sha256": actual, "sizeBytes": candidates[0].stat().st_size}
    uninstallers = list(extracted.rglob("Uninstall.exe"))
    if len(uninstallers) != 1:
        raise ValueError("installer must include one native uninstaller")
    results["Uninstall.exe"] = {"sha256": sha256(uninstallers[0]), "sizeBytes": uninstallers[0].stat().st_size, "pe": inspect_setup_stub(uninstallers[0])}
    return results


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", required=True, type=Path)
    parser.add_argument("--host-evidence", required=True, type=Path)
    parser.add_argument("--encoder", required=True, type=Path)
    parser.add_argument("--encoder-evidence", required=True, type=Path)
    parser.add_argument("--encoder-source", required=True, type=Path)
    parser.add_argument("--makensis", default="makensis")
    parser.add_argument("--sevenzip", required=True)
    parser.add_argument("--guide", type=Path, default=ROOT / "docs/windows-installer-0.8.0.md")
    parser.add_argument("--output-dir", type=Path, default=ROOT / "artifacts/windows-installer")
    args = parser.parse_args(argv)
    args.output_dir = args.output_dir.resolve()
    if args.output_dir.exists() and any(args.output_dir.iterdir()):
        parser.error("--output-dir must be absent or empty; existing candidates/evidence are preserved")
    args.output_dir.mkdir(parents=True, exist_ok=True)
    record_path = args.output_dir / "installer-build-evidence.json"
    record = {
        "recordedAtUtc": datetime.now(timezone.utc).isoformat(),
        "status": "failed", "releaseReady": False,
        "nativeInstallation": {"status": "not_run", "reason": "packaging verification does not execute Windows"},
    }
    try:
        evidence = json.loads(args.host_evidence.read_text())
        host = verify_host(args.host.resolve(), evidence)
        service = admit_service_profile(ROOT)
        check_live_service(service)
        encoder_evidence = json.loads(args.encoder_evidence.read_text())
        encoder = verify_encoder(args.encoder.resolve(), encoder_evidence, args.encoder_source.resolve())
        record["encoder"] = encoder
        record["encoderBuildEvidenceSha256"] = sha256(args.encoder_evidence)
        record["serviceConfiguration"] = service
        guide = args.guide.read_bytes()
        notices, notice_sources = collect_notices(ROOT)
        installer_source = ROOT / "packaging/windows/installer/OpenRemote.nsi"
        record["host"] = host
        record["hostBuildEvidenceSha256"] = sha256(args.host_evidence)
        record["installerSourceSha256"] = sha256(installer_source)
        record["builderSourceSha256"] = sha256(Path(__file__))
        record["guideSha256"] = sha256(args.guide)
        record["noticeSources"] = notice_sources
        tool_paths = {}
        for name, executable in (("makensis", args.makensis), ("sevenzip", args.sevenzip)):
            resolved = shutil.which(executable)
            if resolved is None:
                raise ValueError(f"required tool was not found: {name}")
            tool_paths[name] = Path(resolved).resolve()
        option = "/" if os.name == "nt" else "-"
        nsis_version = subprocess.run([str(tool_paths["makensis"]), option + "VERSION"], capture_output=True, text=True, check=False)
        if nsis_version.returncode:
            raise ValueError("could not determine the NSIS compiler version")
        record["tools"] = {
            name: {"path": str(path), "sha256": sha256(path)}
            for name, path in tool_paths.items()
        }
        record["tools"]["makensis"]["version"] = (nsis_version.stdout + nsis_version.stderr).strip()
        version = host["version"]
        suffix = "Setup.exe"
        name = f"OpenRemote-{version}-Windows-x64-{suffix}"
        final = args.output_dir / name
        if final.exists():
            raise ValueError(f"refusing to overwrite existing installer: {final}; use a fresh output directory")
        with tempfile.TemporaryDirectory(prefix="openremote-installer-", dir=args.output_dir) as temporary:
            build = Path(temporary)
            payload = build / "payload"
            payload.mkdir()
            shutil.copyfile(args.host, payload / "openremote-agent.exe")
            shutil.copyfile(args.encoder, payload / "ffmpeg.exe")
            shutil.copyfile(args.encoder_source, payload / "FFmpeg-9.0.2-source.tar.xz")
            (payload / "FFmpeg-BUILD.txt").write_text(
                "OpenRemote includes the separate FFmpeg 9.0.2 program, licensed LGPL-2.1-or-later.\n"
                "Its exact, unmodified upstream source accompanies this installer in FFmpeg-9.0.2-source.tar.xz.\n"
                "FFmpeg is maintained by its respective contributors, not owned by OpenRemote.\n"
                "Build instructions: scripts/build-bundled-encoder.py in the accompanying OpenRemote source bundle.\n"
                "No GPL, nonfree or external codec libraries are enabled. Windows supplies Media Foundation.\n"
                "The LGPL permits modification/reverse engineering of this component; OpenRemote's integrity checks apply to its configured runtime.\n\n"
                + json.dumps(encoder_evidence, indent=2) + "\n")
            (payload / "START-HERE.txt").write_bytes(guide)
            (payload / "THIRD-PARTY-NOTICES.txt").write_bytes(notices)
            manifest = {
                "product": "OpenRemote", "version": version, "architecture": "x86_64",
                "developmentBuild": True, "authenticode": "unsigned", "releaseReady": False,
                "host": host, "hostBuildEvidenceSha256": record["hostBuildEvidenceSha256"],
                "encoder": encoder, "serviceConfiguration": service,
                "payload": {path.name: sha256(path) for path in sorted(payload.iterdir())},
                "noticeSources": notice_sources,
                "prerequisites": ["Windows 10 version 1703 (build 15063) or newer, or Windows 11, on an Intel/AMD 64-bit PC, with Media Foundation", "Publisher-configured OpenRemote Internet service"],
                "externalComponentsBundled": ["FFmpeg 9.0.2 executable, corresponding source and license notices"],
                "nativeInstallationTested": False,
            }
            (payload / "package-manifest.json").write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
            shutil.copyfile(installer_source, build / "OpenRemote.nsi")
            candidate = build / name
            definitions = {
                "VERSION": version, "HOST_SHA256": host["sha256"],
                "HOST_HASH_SHORT": host["sha256"][:16], "OUTPUT_FILE": str(candidate),
                "SERVICE_NOTE": "",
            }
            (build / "package-definitions.nsh").write_text("\n".join(f'!define {key} "{nsis_quote(value)}"' for key, value in definitions.items()) + "\n")
            run([str(tool_paths["makensis"]), option + "NOCONFIG", option + "WX", option + "V4", str(build / "OpenRemote.nsi")], cwd=build, log=args.output_dir / "makensis.log")
            extracted = build / "extracted"
            run([str(tool_paths["sevenzip"]), "x", "-y", f"-o{extracted}", str(candidate)], cwd=build, log=args.output_dir / "extraction.log")
            contents = verify_extracted_payload(extracted, payload)
            record["extractedPayload"] = contents
            record["installer"] = {"name": name, "sha256": sha256(candidate), "sizeBytes": candidate.stat().st_size, "authenticode": "unsigned", "pe": inspect_setup_stub(candidate)}
            record["logs"] = {name: sha256(args.output_dir / name) for name in ("makensis.log", "extraction.log")}
            os.replace(candidate, final)
            shutil.copyfile(payload / "package-manifest.json", args.output_dir / "package-manifest.json")
        record["status"] = "passed"
        record_path.write_text(json.dumps(record, indent=2, sort_keys=True) + "\n")
        print(json.dumps({"installer": str(final), "sha256": record["installer"]["sha256"], "evidence": str(record_path), "nativeInstallationTested": False}))
        return 0
    except (OSError, ValueError, RuntimeError, KeyError, UnicodeError) as error:
        record["error"] = str(error)
        record_path.write_text(json.dumps(record, indent=2, sort_keys=True) + "\n")
        print(str(error), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
