import importlib.util
import os
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('deployment', Path(__file__).resolve().parents[1] / 'configure_deployment.py')
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)

class ConfigTest(unittest.TestCase):
    def test_private_matching_credentials_and_no_overwrite(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            paths = m.configure(root, 'api.example.org', 'turn.example.org', '8.8.4.4', 'owner@example.org', 5349)
            settings = dict(line.split('=', 1) for line in paths[1].read_text().splitlines())
            compose_env = dict(line.split('=', 1) for line in paths[0].read_text().splitlines())
            self.assertEqual(int(compose_env['TURN_UID']), os.getuid())
            self.assertEqual(int(compose_env['TURN_GID']), os.getgid())
            relay = paths[2].read_text()
            self.assertGreaterEqual(len(settings['OPENREMOTE_ENROLLMENT_TOKEN']), 32)
            self.assertEqual(settings['OPENREMOTE_AUTOMATIC_HOST_REGISTRATION'], 'true')
            self.assertEqual(settings['OPENREMOTE_PUBLIC_ORIGIN'], 'https://api.example.org')
            self.assertIn('static-auth-secret=' + settings['OPENREMOTE_TURN_SECRET'], relay)
            self.assertIn('turns:turn.example.org:5349?transport=tcp', settings['OPENREMOTE_TURN_URLS'])
            if os.name != 'nt':
                for p in paths:
                    self.assertEqual(p.stat().st_mode & 0o777, 0o600)
            before = paths[1].read_bytes()
            with self.assertRaises(ValueError):
                m.configure(root, 'api.example.org', 'turn.example.org', '8.8.4.4', 'owner@example.org', 5349)
            self.assertEqual(before, paths[1].read_bytes())

    def test_refuses_private_relay_and_env_injection(self):
        with tempfile.TemporaryDirectory() as d:
            for address in ('10.0.0.1', '127.0.0.1', '100.64.1.1'):
                with self.assertRaises(ValueError):
                    m.configure(Path(d), 'api.example.org', 'turn.example.org', address, 'owner@example.org', 5349)
            with self.assertRaises(ValueError):
                m.configure(Path(d), 'api.example.org', 'turn.example.org', '8.8.4.4', 'owner@example.org\nEVIL=1', 5349)
            self.assertFalse((Path(d) / '.env').exists())

    def test_domain_input_has_no_protocol_or_directives(self):
        for bad in ('https://api.example.org', 'foo', 'foo.example', 'api.example.org\nallow-loopback-peers'):
            with self.assertRaises(Exception):
                m.hostname(bad)
        self.assertEqual(m.hostname('API.EXAMPLE.ORG'), 'api.example.org')

if __name__ == '__main__':
    unittest.main()
