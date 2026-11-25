"""Collect completed 0.8 engineering checks; never infer native/release success."""
import datetime
import hashlib
import json
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[2]
EV = Path(__file__).resolve().parent

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def read(path):
    return json.loads((ROOT / path).read_text())

def checked(mapping, base=ROOT):
    for name, expected in mapping.items():
        if isinstance(expected, dict):
            expected = expected['sha256']
        if digest(base / name) != expected:
            raise RuntimeError('Verified source changed: ' + name)
    return len(mapping)

windows = read('agent-windows/BUILD_EVIDENCE.json')
server = read('server/verification/activation-0.8.0/final-summary.json')
android = read('android/verification/activation-0.8.0/source-snapshot.json')
web = read('evidence/activation-tray-0.8.0/browser-lifecycle.json')
bridge = read('agent-bridge/verification/final-verification.json')
custody = {
    'windows': checked(windows['sourceInputs']['files'], ROOT / 'agent-windows'),
    'serverAndWeb': checked(server['sourceInputs']),
    'android': checked(android['files']),
    'browserLifecycle': checked(web['sourceInputs']),
    'agentBridge': checked(bridge['sourceInputsAfter'], ROOT / 'agent-bridge'),
}
counts = {}
for mode in ['unit', 'race']:
    items = []
    for line in (ROOT / f'server/verification/activation-0.8.0/final-{mode}.log').read_text().splitlines():
        try:
            row = json.loads(line)
        except ValueError:
            continue
        if row.get('Action') == 'fail':
            raise RuntimeError('Go check failed')
        if row.get('Action') == 'pass' and row.get('Test'):
            items.append(row['Test'])
    counts['go-' + mode] = {'topLevel': sum('/' not in name for name in items), 'includingSubcases': len(items)}
    assert counts['go-' + mode] == {'topLevel': 72, 'includingSubcases': 158}
rust = sum(map(int, re.findall(r'test result: ok\. (\d+) passed', (ROOT / 'agent-windows/verification/activation-0.8.0/tests.log').read_text())))
assert rust == 113
counts['rust'] = rust
for name, path, expected in [
    ('browserCore', EV / 'web-core.log', 17),
    ('browserLifecycle', EV / 'web-lifecycle.log', 20),
    ('mcp', ROOT / 'agent-bridge/verification/final-tests.log', 18),
]:
    # Node's TAP and human-readable reporters use different line prefixes.
    text = path.read_text()
    assert re.search(rf'^(?:#|ℹ) pass {expected}\s*$', text, re.MULTILINE)
    assert re.search(r'^(?:#|ℹ) fail 0\s*$', text, re.MULTILINE)
    counts[name] = expected
for name, log, expected in [('configuration', 'configuration-unit.log', 19), ('installer', 'installer-unit.log', 16)]:
    text = (EV / log).read_text()
    assert f'Ran {expected} tests' in text and '\nOK\n' in text
    counts[name] = expected
integrations = []
for name, expected in [('https-lifecycle.json', 12), ('https-registration.json', 8), ('web-api-integration.json', 6), ('activation-api-integration.json', 6)]:
    result = json.loads((EV / name).read_text())
    assert result['ok'] and len(result.get('checks', result.get('cases'))) == expected
    integrations.append({'record': name, 'groupedChecks': expected, 'status': 'passed'})
mobile = read('android/verification/activation-0.8.0/build-summary.json')
assert mobile['tests'] == {'tests': 90, 'failures': 0, 'errors': 0, 'skipped': 0}
counts['android'] = mobile['tests'] | {'suites': mobile['testSuites']}
assert mobile['buildExitCode'] == 0 and mobile['allInputsUnchangedDuringBuild']
assert mobile['releaseGuards']['defaultUnconfiguredRelease']['guardRejected']
assert mobile['releaseGuards']['injectedSigningWithPreviewException']['guardRejected']
artifacts = []
for path, sha, size, kind in [
    (ROOT / 'agent-windows' / windows['binary']['relativePath'], windows['binary']['sha256'], windows['binary']['sizeBytes'], 'unsigned engineering Windows host'),
    (ROOT / server['artifact']['path'], server['artifact']['sha256'], server['artifact']['size'], 'static coordinator'),
]:
    assert digest(path) == sha and path.stat().st_size == size
    artifacts.append({'path': str(path.relative_to(ROOT)), 'sha256': sha, 'sizeBytes': size, 'kind': kind})
for item in mobile['artifacts']:
    path = ROOT / item['path']
    assert digest(path) == item['sha256'] and path.stat().st_size == item['sizeBytes']
    artifacts.append(item)
assert read('evidence/activation-tray-0.8.0/installer-release-guard.json')['passedExpectedRejection']
record = {
    'recordedAtUtc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
    'version': '0.8.0',
    'sourceBaseRevision': '55d0dd83697c51df017ee2b1c790c8cab78c9808',
    'sourceBranch': subprocess.check_output(['git', 'branch', '--show-current'], cwd=ROOT, text=True).strip(),
    'engineeringChecks': 'passed within explicitly stated scope',
    'releaseReady': False,
    'serviceConfigured': False,
    'customerInstallerProduced': False,
    'sourceCustody': {'allPinnedInputsStillMatch': True, 'inputCounts': custody},
    'testCounts': counts,
    'integrations': integrations,
    'artifacts': artifacts,
    'componentRecords': [
        'agent-windows/BUILD_EVIDENCE.json',
        'agent-windows/verification/activation-0.8.0/final-checks.json',
        'server/BUILD_EVIDENCE.json',
        'server/verification/activation-0.8.0/final-summary.json',
        'android/verification/activation-0.8.0/build-summary.json',
        'agent-bridge/verification/final-verification.json',
        'evidence/build-tools-0.8.0/independent-review.json',
        'evidence/activation-tray-0.8.0/installer-release-guard.json',
        'evidence/activation-tray-0.8.0/encoder-custody.json',
    ],
    'notExecuted': [
        'Configured public coordinator/relay deployment',
        'Native Windows application or installer execution',
        'Physical Android install/update and native runtime',
        'Real browser page rendering or actual MCP screenshot/input',
        'Windows-to-phone/laptop/agent desktop media and input',
        'Public Internet/cellular/direct ICE/TURN UDP/TCP/TLS/443 sessions',
        'Production application signing and store publication',
    ],
    'browserRuntimeBlock': 'Normal sandboxed Chromium launch failed before page creation with socket EPERM; no bypass attempted',
    'openProductScope': ['Windows SCM/session broker and unattended reboot/UAC', 'Advanced commercial feature and performance milestones', 'Complete native/browser dependency notices and signed update operations'],
    'retainedFailures': [
        'server/verification/activation-0.8.0/before-contract.json',
        'evidence/activation-tray-0.8.0/web-core.initial.log',
        'evidence/activation-tray-0.8.0/web-lifecycle.initial.log',
        'evidence/activation-tray-0.8.0/installer-unit.initial.log',
        'android/verification/activation-0.8.0/initial-failure/',
        'evidence/build-tools-0.8.0/chromium-launch.json',
    ],
    'scope': 'Compilation, source review, unit/protocol/state and isolated API evidence only. Native device and public Internet release acceptance remain open.',
}
tracked = []
for folder in [EV, ROOT / 'agent-windows/verification/activation-0.8.0', ROOT / 'server/verification/activation-0.8.0', ROOT / 'android/verification/activation-0.8.0', ROOT / 'agent-bridge/verification', ROOT / 'evidence/build-tools-0.8.0']:
    tracked.extend(path for path in folder.rglob('*') if path.is_file() and path.name != 'verification.json')
record['evidenceFiles'] = {str(path.relative_to(ROOT)): digest(path) for path in sorted(set(tracked))}
inputs = [ROOT / 'scripts/build-windows-installer.py', ROOT / 'scripts/integration_test.py', ROOT / 'scripts/automatic_registration_test.py', ROOT / 'scripts/requirements-test.txt', *sorted((ROOT / 'scripts/tests').glob('*.py')), *sorted((ROOT / 'packaging/windows/installer/tests').glob('*.py')), *sorted((ROOT / 'tests/browser').glob('*.mjs'))]
record['rootVerificationInputs'] = {str(path.relative_to(ROOT)): digest(path) for path in inputs}
(EV / 'verification.json').write_text(json.dumps(record, indent=2) + '\n')
print(json.dumps({'testCounts': counts, 'sourceCustody': custody, 'artifactsVerified': len(artifacts), 'evidenceFiles': len(record['evidenceFiles']), 'releaseReady': False}, indent=2))
