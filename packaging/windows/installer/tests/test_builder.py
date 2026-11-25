"""Installer admission/custody checks. These do not simulate Windows UI APIs."""
import importlib.util
import json
from unittest import mock
from pathlib import Path
import struct
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[4]
SPEC = importlib.util.spec_from_file_location("installer_builder", ROOT / "scripts/build-windows-installer.py")
builder = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(builder)


def make_pe(*, subsystem=2, machine=0x8664, characteristics=0x22):
    image = bytearray(1024)
    image[:2] = b"MZ"
    struct.pack_into("<I", image, 0x3C, 0x80)
    image[0x80:0x84] = b"PE\0\0"
    struct.pack_into("<HH", image, 0x84, machine, 1)
    struct.pack_into("<HH", image, 0x94, 240, characteristics)
    optional = 0x98
    struct.pack_into("<H", image, optional, 0x20B)
    struct.pack_into("<H", image, optional + 68, subsystem)
    struct.pack_into("<I", image, optional + 108, 16)
    section = optional + 240
    image[section:section + 8] = b".text\0\0\0"
    struct.pack_into("<IIII", image, section + 8, 512, 0x1000, 512, 512)
    return bytes(image)


class PackageAdmissionTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.agent = self.root / "agent-windows"
        (self.agent / "src").mkdir(parents=True)
        (self.agent / "Cargo.toml").write_text('[package]\nversion = "0.8.0"\n')
        (self.agent / "Cargo.lock").write_text("fixture lock\n")
        (self.agent / "src/main.rs").write_text("fn main() {}\n")
        (self.root / "config").mkdir()
        (self.root / "config/service-profile.json").write_text('{"schemaVersion":1,"serviceName":"OpenRemote","apiOrigin":""}\n')
        (self.root / "config/encoder-profile.json").write_text('{"schemaVersion":1,"fileName":"ffmpeg.exe","version":"9.0.2","encoder":"h264_mf","sha256":"","sizeBytes":0}\n')
        self.host = self.root / "host.exe"
        self.host.write_bytes(make_pe())
        self.evidence = {
            "hostVersion": "0.8.0", "binary": {"sha256": builder.sha256(self.host)},
            "sourceInputs": {"files": {relative: builder.sha256(self.agent / relative) for relative in ("Cargo.toml", "Cargo.lock", "src/main.rs", "../config/service-profile.json", "../config/encoder-profile.json")}},
        }

    def tearDown(self):
        self.temporary.cleanup()

    def test_accepts_matching_gui_amd64_source_and_bytes(self):
        result = builder.verify_host(self.host, self.evidence, self.root)
        self.assertEqual(result["version"], "0.8.0")
        self.assertEqual(result["pe"]["subsystem"], 2)

    def test_console_flash_candidate_is_rejected_even_with_matching_hash(self):
        self.host.write_bytes(make_pe(subsystem=3))
        self.evidence["binary"]["sha256"] = builder.sha256(self.host)
        with self.assertRaisesRegex(ValueError, "GUI subsystem"):
            builder.verify_host(self.host, self.evidence, self.root)

    def test_wrong_architecture_and_dll_are_not_installable_hosts(self):
        for image in (make_pe(machine=0x14C), make_pe(characteristics=0x2022)):
            self.host.write_bytes(image)
            with self.assertRaises(ValueError):
                builder.inspect_pe(self.host)

    def test_changed_binary_is_rejected_before_packaging(self):
        self.host.write_bytes(self.host.read_bytes() + b"changed")
        with self.assertRaisesRegex(ValueError, "SHA-256"):
            builder.verify_host(self.host, self.evidence, self.root)

    def test_changed_or_new_rust_input_invalidates_build_evidence(self):
        (self.agent / "src/main.rs").write_text("fn main() { panic!(); }\n")
        with self.assertRaisesRegex(ValueError, "source changed"):
            builder.verify_host(self.host, self.evidence, self.root)
        (self.agent / "src/main.rs").write_text("fn main() {}\n")
        (self.agent / "src/first_run.rs").write_text("// new build input\n")
        with self.assertRaisesRegex(ValueError, "source-input set"):
            builder.verify_host(self.host, self.evidence, self.root)

    def test_service_profile_is_a_required_binary_input(self):
        (self.root / "config/service-profile.json").write_text('{"schemaVersion":1,"serviceName":"OpenRemote","apiOrigin":"https://changed.company.tld"}\n')
        with self.assertRaisesRegex(ValueError, "source changed"):
            builder.verify_host(self.host, self.evidence, self.root)

    def test_encoder_profile_is_a_required_binary_input(self):
        (self.root / "config/encoder-profile.json").write_text('{"changed":true}\n')
        with self.assertRaisesRegex(ValueError, "source changed"):
            builder.verify_host(self.host, self.evidence, self.root)

    def test_unconfigured_installer_is_never_distributable(self):
        with self.assertRaisesRegex(ValueError, "Installer blocked"):
            builder.admit_service_profile(self.root)
        # The previous escape flag cannot bypass the release guard anymore.
        with self.assertRaises(TypeError):
            builder.admit_service_profile(self.root, True)

    def test_live_discovery_rejects_ambiguous_or_wrongly_typed_capabilities(self):
        valid = {"service": "openremote", "protocolVersion": 1, "automaticHostRegistrationVersion": 1,
                 "pairingCodeVersion": 1, "activationVersion": 1, "maxActivationConnections": 4, "relay": {"configured": True, "tls": True}}
        bodies = [b'[]', b'{"service":"openremote","service":"other"}',
                  json.dumps(dict(valid, protocolVersion=True)).encode(),
                  json.dumps(dict(valid, relay=[])).encode(),
                  json.dumps(dict(valid, activationVersion=0)).encode(),
                  json.dumps(dict(valid, maxActivationConnections=True)).encode()]
        for raw in bodies:
            with self.subTest(raw=raw):
                response = mock.MagicMock()
                response.__enter__.return_value = response
                response.status = 200
                response.read.return_value = raw
                opener = mock.Mock()
                opener.open.return_value = response
                with mock.patch.object(builder.urllib.request, "build_opener", return_value=opener):
                    with self.assertRaises(ValueError):
                        builder.check_live_service({"apiOrigin": "https://service.company.tld"})

    def test_configured_profile_has_no_credentials_or_manual_setup_fallback(self):
        path = self.root / "config/service-profile.json"
        path.write_text(json.dumps({"schemaVersion":1,"serviceName":"OpenRemote","apiOrigin":"https://remote.company.dev"}))
        result = builder.admit_service_profile(self.root)
        self.assertTrue(result["originConfigured"])
        self.assertEqual(result["apiOrigin"], "https://remote.company.dev")
        path.write_text(json.dumps({"schemaVersion":1,"serviceName":"OpenRemote","apiOrigin":"https://user:secret@remote.company.dev"}))
        with self.assertRaises(ValueError):
            builder.admit_service_profile(self.root)

    def test_bundled_encoder_requires_matching_bytes_and_corresponding_source(self):
        encoder = self.root / "ffmpeg.exe"
        encoder.write_bytes(make_pe(subsystem=3))
        source = self.root / "source.tar.xz"
        source.write_bytes(b"isolated source fixture")
        source_hash = builder.sha256(source)
        actual = builder.sha256(encoder)
        profile = {"schemaVersion":1,"fileName":"ffmpeg.exe","version":"9.0.2","encoder":"h264_mf","sha256":actual,"sizeBytes":encoder.stat().st_size}
        (self.root / "config/encoder-profile.json").write_text(json.dumps(profile))
        evidence = {"status":"passed","artifact":{"sha256":actual,"sizeBytes":encoder.stat().st_size},"license":"LGPL-2.1-or-later","externalCodecLibraries":[],"sourceSha256":source_hash}
        with mock.patch.object(builder, "ENCODER_SOURCE_SHA256", source_hash):
            self.assertEqual(builder.verify_encoder(encoder, evidence, source, self.root)["pe"]["subsystem"], 3)
            source.write_bytes(b"different source")
            with self.assertRaisesRegex(ValueError, "corresponding"):
                builder.verify_encoder(encoder, evidence, source, self.root)
            source.write_bytes(b"isolated source fixture")
            changed = bytearray(encoder.read_bytes()); changed[-1] ^= 1
            encoder.write_bytes(changed)
            with self.assertRaisesRegex(ValueError, "compiled runtime profile"):
                builder.verify_encoder(encoder, evidence, source, self.root)

    def test_stale_version_is_rejected(self):
        self.evidence["hostVersion"] = "0.5.0"
        with self.assertRaisesRegex(ValueError, "version"):
            builder.verify_host(self.host, self.evidence, self.root)

    def test_missing_non_system_runtime_dll_is_rejected(self):
        image = bytearray(make_pe())
        struct.pack_into("<II", image, 0x98 + 120, 0x1000, 40)
        struct.pack_into("<IIIII", image, 512, 0, 0, 0, 0x1040, 0)
        name = b"not-shipped-runtime.dll\0"
        image[576:576 + len(name)] = name
        self.host.write_bytes(image)
        self.evidence["binary"]["sha256"] = builder.sha256(self.host)
        with self.assertRaisesRegex(ValueError, "unbundled non-system DLL"):
            builder.verify_host(self.host, self.evidence, self.root)

    def test_truncated_malformed_pe_is_rejected(self):
        for image in (b"MZ", make_pe()[:530], b"bad" * 500):
            self.host.write_bytes(image)
            with self.assertRaises(ValueError):
                builder.inspect_pe(self.host)

    def test_independent_extraction_rejects_changed_or_duplicated_payload(self):
        payload = self.root / "payload"
        extracted = self.root / "extracted"
        payload.mkdir(); extracted.mkdir()
        for name in builder.OWNED_PAYLOAD:
            content = b"MZdummy" if name.endswith(".exe") else b"known fixture bytes"
            (payload / name).write_bytes(content)
            (extracted / name).write_bytes(content)
        (extracted / "Uninstall.exe").write_bytes(make_pe())
        self.assertEqual(len(builder.verify_extracted_payload(extracted, payload)), 8)
        (extracted / "START-HERE.txt").write_text("substituted")
        with self.assertRaisesRegex(ValueError, "changed the payload bytes"):
            builder.verify_extracted_payload(extracted, payload)
        (extracted / "START-HERE.txt").write_bytes((payload / "START-HERE.txt").read_bytes())
        (extracted / "nested").mkdir()
        (extracted / "nested/openremote-agent.exe").write_bytes(b"MZduplicate")
        with self.assertRaisesRegex(ValueError, "exactly one"):
            builder.verify_extracted_payload(extracted, payload)

    def test_uninstaller_must_be_an_actual_gui_pe_image(self):
        self.host.write_bytes(b"MZnot an executable")
        with self.assertRaises(ValueError):
            builder.inspect_setup_stub(self.host)
        self.host.write_bytes(make_pe(subsystem=3))
        with self.assertRaises(ValueError):
            builder.inspect_setup_stub(self.host)


if __name__ == "__main__":
    unittest.main()
