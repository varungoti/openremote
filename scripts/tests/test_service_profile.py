import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("service_profile", Path(__file__).resolve().parents[1] / "configure_service_profile.py")
profile = importlib.util.module_from_spec(spec)
spec.loader.exec_module(profile)


class ServiceProfileTests(unittest.TestCase):
    def test_canonical_https_allows_real_dns_and_private_self_hosted_addresses(self):
        for given, expected in [("https://remote.company.tld/", "https://remote.company.tld"), ("https://REMOTE.company.tld:443", "https://remote.company.tld"),
            ("https://remote.company.tld:8443", "https://remote.company.tld:8443"), ("https://192.168.10.2", "https://192.168.10.2"), ("https://[2001:db8::1]:8443", "https://[2001:db8::1]:8443")]:
            self.assertEqual(profile.canonical_origin(given), expected)

    def test_rejects_credentials_repaired_origins_placeholders_and_encoder_secrets(self):
        for value in ["http://remote.company.tld", "https://user:key@host.tld", "https://host.tld/path", "https://host.tld/a/..", "https://host.tld?pin=123456", "https://host.tld#secret",
            "https://host.tld:0", "https://host.tld:65536", "https://host.tld:", "https://example.com", "https://remote.example.net", "https://host.invalid", "https://127.1", "https://2130706433", "https://0x7f000001",
            "https://host.tld\\evil", "https://hos%74.tld", "https://host.tld\n", "https://é.tld", "https://_host.tld", "https://host.tld//"]:
            with self.subTest(value=value), self.assertRaises(ValueError):
                profile.canonical_origin(value)

    def test_strict_schema_and_deliberate_origin_replacement(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "profile.json"
            initial = {"schemaVersion": 1, "serviceName": "OpenRemote", "apiOrigin": ""}
            path.write_text(json.dumps(initial))
            profile.configure(path, "https://remote.company.tld")
            self.assertEqual(profile.read_profile(path)["apiOrigin"], "https://remote.company.tld")
            previous = path.read_bytes()
            with self.assertRaisesRegex(ValueError, "different service"):
                profile.configure(path, "https://other.company.tld")
            self.assertEqual(path.read_bytes(), previous)
            profile.configure(path, "https://other.company.tld", replace=True)
            self.assertEqual(profile.read_profile(path)["apiOrigin"], "https://other.company.tld")
            for text in [json.dumps({**initial, "pin": "123456"}), json.dumps({**initial, "schemaVersion": True}),
                         '{"schemaVersion":1,"serviceName":"OpenRemote","apiOrigin":"","apiOrigin":"https://host.tld"}']:
                path.write_text(text)
                with self.assertRaises(ValueError):
                    profile.read_profile(path)


if __name__ == "__main__":
    unittest.main()
