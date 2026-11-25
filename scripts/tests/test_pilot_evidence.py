import copy
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('pilot_evidence', Path(__file__).resolve().parents[1] / 'validate_pilot_evidence.py')
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)

REVISION = 'a' * 40


class PilotEvidenceTests(unittest.TestCase):
    """Synthetic report fixtures test validation; they are not application evidence."""

    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        for name in ('host.exe', 'phone.apk', 'server', 'observations.txt'):
            (self.root / name).write_bytes(('synthetic unittest fixture: ' + name).encode())
        self.report = m.template(self.root, 'test-only', REVISION,
                                 {'windows': 'host.exe', 'android': 'phone.apk', 'coordinator': 'server'})

    def environment(self):
        self.report['environment'] = {
            'operator': 'Synthetic unittest fixture, no devices used',
            'windowsPcs': [
                {'id': 'fixture-one', 'osVersion': 'Windows test fixture', 'gpu': 'fixture GPU', 'monitorCount': 2},
                {'id': 'fixture-two', 'osVersion': 'Windows test fixture', 'gpu': 'fixture GPU', 'monitorCount': 1},
            ],
            'android': {'model': 'fixture', 'osVersion': 'fixture', 'apiLevel': 36, 'pageSizeBytes': 16384},
            'coordinatorOs': 'fixture Linux', 'relayVersion': 'fixture relay',
            'networkDescription': 'Synthetic local test data only', 'windowsFfmpegSha256': 'b' * 64,
        }

    def complete_case(self, case_id, status='passed'):
        self.environment()
        case = next(case for case in self.report['cases'] if case['id'] == case_id)
        case.update(status=status, startedAtUtc='2025-01-01T00:00:00Z', finishedAtUtc='2025-01-01T00:20:00Z',
                    pcIds=['fixture-one'], observations='Synthetic validator fixture; no app behavior was executed.',
                    evidence=[m.pin_file(self.root, 'observations.txt')])
        if status == 'passed':
            rules = m.FACTS[case_id]
            case['measurements'] = {key: {'positive': 30, 'nonnegative': 0, 'two': 2, 'digest': 'c' * 64, 'direct': 'srflx'}.get(rule, rule)
                                    for key, rule in rules.items()}
            if case_id == 'renewal_handoff':
                case['measurements'].update(durationSeconds=1200, turnCredentialLifetimeSeconds=600)
            if case_id == 'multiple_pcs':
                case['pcIds'].append('fixture-two')
        return case

    def test_initial_record_has_no_execution_claims_and_does_not_pass_gate(self):
        result = m.validate(self.report, self.root, REVISION)
        self.assertEqual(result['counts']['not_run'], 17)
        self.assertFalse(result['reportedPilotComplete'])
        self.assertFalse(result['releaseReady'])
        with self.assertRaisesRegex(m.Invalid, 'not-run'):
            m.validate(self.report, self.root, REVISION, True)

    def test_candidate_change_or_wrong_revision_is_rejected(self):
        with self.assertRaisesRegex(m.Invalid, 'source revision'):
            m.validate(self.report, self.root, 'd' * 40)
        (self.root / 'host.exe').write_bytes(b'replaced candidate')
        with self.assertRaisesRegex(m.Invalid, 'hash do not match'):
            m.validate(self.report, self.root, REVISION)

    def test_duplicate_and_nonfinite_json_and_oversize_are_rejected(self):
        path = self.root / 'report.json'
        for value in ('{"a":1,"a":2}', '{"nested":{"x":1,"x":2}}', '{"n":NaN}', '{"n":Infinity}', '{"n":1e999}', '[' * 2000):
            with self.subTest(value=value[:40]):
                path.write_text(value)
                with self.assertRaises(m.Invalid):
                    m.read_json(path)
        path.write_bytes(b' ' * (m.MAX_JSON + 1))
        with self.assertRaisesRegex(m.Invalid, '1 MiB'):
            m.read_json(path)

    def test_paths_must_stay_inside_root_without_links(self):
        for path in ('../host.exe', '/etc/hosts', 'x/../host.exe', './host.exe', 'x//host.exe', 'C:\\host.exe'):
            with self.subTest(path=path), self.assertRaises(m.Invalid):
                m.pin_file(self.root, path)
        try:
            (self.root / 'linked.exe').symlink_to(self.root / 'host.exe')
        except OSError:
            self.skipTest('Symlink creation unavailable on this test platform')
        with self.assertRaisesRegex(m.Invalid, 'symlink'):
            m.pin_file(self.root, 'linked.exe')
        (self.root / 'directory-link').symlink_to(self.root, target_is_directory=True)
        with self.assertRaisesRegex(m.Invalid, 'symlink'):
            m.pin_file(self.root, 'directory-link/host.exe')

    def test_required_scenarios_unique_statuses_and_schema_are_enforced(self):
        invalid = [
            lambda r: r.update(releaseReady=True),
            lambda r: r['cases'].pop(),
            lambda r: r['cases'][1].update(id=r['cases'][0]['id']),
            lambda r: r['cases'][0].update(status='skipped'),
            lambda r: r.update(schemaVersion=True),
            lambda r: r['cases'][0].update(measurements={'fabricated': True}),
        ]
        for edit in invalid:
            report = copy.deepcopy(self.report)
            edit(report)
            with self.subTest(edit=edit), self.assertRaises(m.Invalid):
                m.validate(report, self.root)

    def test_claimed_pass_requires_physical_environment_and_archived_observation(self):
        case = self.complete_case('desktop_input')
        original = copy.deepcopy(self.report)
        edits = [
            lambda r: r['environment'].update(windowsFfmpegSha256=''),
            lambda r: r['environment']['android'].update(pageSizeBytes=0),
            lambda r: r['environment']['windowsPcs'][0].update(osVersion=''),
            lambda r: r['cases'][0].update(evidence=[]),
            lambda r: r['cases'][0].update(observations=''),
            lambda r: r['cases'][0].update(startedAtUtc='2025-01-02T00:00:00Z'),
            lambda r: r['cases'][0].update(finishedAtUtc='2099-01-01T00:00:00Z'),
            lambda r: r['cases'][0].update(startedAtUtc='2025-01-01Z'),
            lambda r: r['cases'][0].update(pcIds=[]),
        ]
        self.assertTrue(m.validate(original, self.root)['recordValid'])
        for edit in edits:
            report = copy.deepcopy(original)
            edit(report)
            with self.subTest(edit=edit), self.assertRaises(m.Invalid):
                m.validate(report, self.root)
        case['measurements']['decodedFrames'] = True
        with self.assertRaisesRegex(m.Invalid, 'integer'):
            m.validate(self.report, self.root)

    def test_relay_claim_needs_selected_candidate_and_exact_transport(self):
        case = self.complete_case('turn_tcp')
        self.assertTrue(m.validate(self.report, self.root)['recordValid'])
        case['measurements']['selectedCandidateType'] = 'srflx'
        with self.assertRaisesRegex(m.Invalid, 'observation missing'):
            m.validate(self.report, self.root)
        case['measurements']['selectedCandidateType'] = 'relay'
        case['measurements']['relayProtocol'] = 'udp'
        with self.assertRaisesRegex(m.Invalid, 'observation missing'):
            m.validate(self.report, self.root)
        case['measurements']['relayProtocol'] = 'tcp'
        case['measurements']['decodedFrames'] = 0
        with self.assertRaises(m.Invalid):
            m.validate(self.report, self.root)

    def test_renewal_must_outlast_credentials_and_upload_hash_must_match(self):
        case = self.complete_case('renewal_handoff')
        case['measurements']['durationSeconds'] = 600
        with self.assertRaisesRegex(m.Invalid, 'outlive'):
            m.validate(self.report, self.root)
        case['measurements']['durationSeconds'] = 1200
        upload = self.complete_case('clipboard_upload')
        upload['measurements']['receivedSha256'] = 'd' * 64
        with self.assertRaisesRegex(m.Invalid, 'hashes differ'):
            m.validate(self.report, self.root)

    def test_actual_evidence_bytes_are_rechecked(self):
        self.complete_case('monitor_unplug')
        (self.root / 'observations.txt').write_text('changed observation')
        with self.assertRaisesRegex(m.Invalid, 'hash do not match'):
            m.validate(self.report, self.root)

    def test_monitor_and_two_pc_claims_require_matching_environment(self):
        case = self.complete_case('monitor_layout')
        case['measurements']['displayCount'] = 3
        with self.assertRaisesRegex(m.Invalid, 'display counts'):
            m.validate(self.report, self.root)
        case['measurements']['displayCount'] = 2
        case = self.complete_case('multiple_pcs')
        case['pcIds'] = ['fixture-one']
        with self.assertRaisesRegex(m.Invalid, 'two distinct'):
            m.validate(self.report, self.root)

    def test_failed_and_blocked_cases_preserve_failure_and_do_not_pass_gate(self):
        for status in ('failed', 'blocked'):
            self.complete_case('installation', status)
            result = m.validate(self.report, self.root, REVISION)
            self.assertEqual(result['counts'][status], 1)
            self.assertFalse(result['reportedPilotComplete'])
            with self.assertRaisesRegex(m.Invalid, 'not-run'):
                m.validate(self.report, self.root, REVISION, True)

    def test_complete_synthetic_fixture_only_marks_record_complete_not_release(self):
        for case_id in m.CASES:
            self.complete_case(case_id)
        with self.assertRaisesRegex(m.Invalid, 'expected-revision'):
            m.validate(self.report, self.root, require_pass=True)
        result = m.validate(self.report, self.root, REVISION, True)
        self.assertTrue(result['reportedPilotComplete'])
        self.assertFalse(result['releaseReady'])
        self.assertIn('cannot attest execution', result['note'])

    def test_new_report_never_overwrites_existing_file(self):
        path = self.root / 'report.json'
        m.write_new(path, self.report)
        before = path.read_bytes()
        with self.assertRaises(FileExistsError):
            m.write_new(path, {'replacement': True})
        self.assertEqual(path.read_bytes(), before)
        if os.name != 'nt':
            self.assertEqual(path.stat().st_mode & 0o777, 0o600)
        self.assertEqual(m.read_json(path), self.report)


if __name__ == '__main__':
    unittest.main()
