#!/usr/bin/env python3
"""Black-box HTTPS lifecycle/security test. Requires a built controlplane binary.

This starts an isolated process with an ephemeral state file and a test CA. It
uses Android-compatible P-256 SPKI/DER signatures and a Windows Ed25519 key.
It never uses a public or deployed server, real PCs, or production credentials.
"""
import argparse
import base64
import datetime as dt
import hashlib
import hmac
import ipaddress
import json
import os
from pathlib import Path
import secrets
import socket
import ssl
import subprocess
import tempfile
import time
import urllib.error
import urllib.request

from cryptography import x509
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import ec, ed25519
from cryptography.x509.oid import NameOID


def b64(data):
    return base64.b64encode(data).decode('ascii')


def certificate(folder):
    key = ec.generate_private_key(ec.SECP256R1())
    name = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, 'OpenRemote isolated integration test')])
    now = dt.datetime.now(dt.timezone.utc)
    cert = (x509.CertificateBuilder().subject_name(name).issuer_name(name)
            .public_key(key.public_key()).serial_number(x509.random_serial_number())
            .not_valid_before(now-dt.timedelta(minutes=2)).not_valid_after(now+dt.timedelta(hours=1))
            .add_extension(x509.SubjectAlternativeName([x509.DNSName('localhost'), x509.IPAddress(ipaddress.ip_address('127.0.0.1'))]), critical=False)
            .add_extension(x509.BasicConstraints(ca=True, path_length=None), critical=True)
            .sign(key, hashes.SHA256()))
    cert_path, key_path = folder/'server-cert.pem', folder/'server-key.pem'
    cert_path.write_bytes(cert.public_bytes(serialization.Encoding.PEM))
    key_path.write_bytes(key.private_bytes(serialization.Encoding.PEM, serialization.PrivateFormat.PKCS8, serialization.NoEncryption()))
    os.chmod(key_path, 0o600)
    return cert_path, key_path


class Harness:
    def __init__(self, binary, folder, automatic_registration=False):
        self.binary, self.folder = binary, folder
        self.automatic_registration = automatic_registration
        self.enrollment = secrets.token_urlsafe(48)
        self.turn_secret = secrets.token_hex(32)
        self.cert, self.key = certificate(folder)
        self.ssl = ssl.create_default_context(cafile=str(self.cert))
        self.proc = None
        self.log = None
        self.checks = []

    def start(self):
        with socket.socket() as sock:
            sock.bind(('127.0.0.1', 0))
            self.port = sock.getsockname()[1]
        self.base = f'https://127.0.0.1:{self.port}'
        env = dict(os.environ)
        for key in list(env):
            if key.startswith('OPENREMOTE_'):
                del env[key]
        env.update(OPENREMOTE_LISTEN=f'127.0.0.1:{self.port}',
                   OPENREMOTE_STATE_FILE=str(self.folder/'state.json'),
                   OPENREMOTE_ENROLLMENT_TOKEN=self.enrollment,
                   OPENREMOTE_AUTOMATIC_HOST_REGISTRATION='true' if self.automatic_registration else 'false',
                   OPENREMOTE_PUBLIC_ORIGIN=self.base if self.automatic_registration else '',
                   OPENREMOTE_TLS_CERT=str(self.cert), OPENREMOTE_TLS_KEY=str(self.key),
                   OPENREMOTE_TURN_URLS='turn:relay.example.org:3478?transport=udp,turns:relay.example.org:5349?transport=tcp',
                   OPENREMOTE_TURN_SECRET=self.turn_secret)
        self.log = (self.folder/'process.log').open('a', encoding='utf-8')
        self.proc = subprocess.Popen([str(self.binary)], env=env, stdout=self.log, stderr=self.log)
        until = time.monotonic()+15
        while time.monotonic()<until:
            if self.proc.poll() is not None:
                raise RuntimeError('Controlplane did not start: '+(self.folder/'process.log').read_text())
            try:
                self.request('GET', '/healthz', expect=200)
                return
            except (OSError, AssertionError):
                time.sleep(0.05)
        raise RuntimeError('Controlplane startup timeout')

    def stop(self):
        if self.proc:
            self.proc.terminate()
            try:
                self.proc.wait(timeout=8)
            except subprocess.TimeoutExpired:
                self.proc.kill()
                self.proc.wait()
            self.proc = None
        if self.log:
            self.log.close()
            self.log = None

    def request(self, method, path, body=None, token=None, expect=200):
        data = None if body is None else json.dumps(body).encode('utf-8')
        headers = {'Accept': 'application/json', 'Content-Type': 'application/json'}
        if token:
            headers['Authorization'] = 'Bearer '+token
        req = urllib.request.Request(self.base+path, data=data, headers=headers, method=method)
        try:
            response = urllib.request.urlopen(req, context=self.ssl, timeout=25)
        except urllib.error.HTTPError as error:
            response = error
        with response:
            raw = response.read()
            assert response.status==expect, f'{method} {path}: expected {expect}, got {response.status}: {raw[:400]!r}'
            assert response.headers.get('Cache-Control') == 'no-store', 'Sensitive response should not be cached'
            return json.loads(raw) if raw else {}

    def authenticate(self, key):
        public = b64(key.public_key().public_bytes(serialization.Encoding.DER, serialization.PublicFormat.SubjectPublicKeyInfo))
        challenge = self.request('POST', '/v1/auth/challenge', {'publicKey':public, 'algorithm':'p256'}, expect=201)
        proof = {'challengeId': challenge['challengeId'], 'signature': b64(key.sign(base64.b64decode(challenge['challenge']), ec.ECDSA(hashes.SHA256())))}
        auth = self.request('POST', '/v1/auth/verify', proof)
        self.request('POST', '/v1/auth/verify', proof, expect=401)
        return auth['accessToken'], public

    def run(self):
        self.start()
        info = self.request('GET', '/v1/info')
        assert info == {'service': 'openremote', 'protocolVersion': 1, 'version': '0.8.0',
                        'phoneSetupVersion': 1, 'monitorSelection': True,
                        'pairingCodeVersion': 1, 'webController': True,
                        'automaticHostRegistrationVersion': 1 if self.automatic_registration else 0,
                        'activationVersion': 1, 'maxActivationConnections': 4,
                        'relay': {'configured': True, 'udp': True, 'tcp': False, 'tls': True},
                        'relayOnly': False}, 'Unexpected setup-discovery contract'
        assert self.enrollment not in json.dumps(info) and self.turn_secret not in json.dumps(info)
        assert 'relay.example.org' not in json.dumps(info)
        self.request('POST', '/v1/info', expect=405)
        self.checks.append('Public HTTPS setup discovery, exact relay configuration flags and no credentials or host details')
        phone_key = ec.generate_private_key(ec.SECP256R1())
        owner, public = self.authenticate(phone_key)
        self.checks.append('P-256 SPKI authentication and single-use challenge replay rejection')
        # A copied public key with another private key must fail.
        challenge = self.request('POST','/v1/auth/challenge',{'publicKey':public,'algorithm':'p256'},expect=201)
        wrong = ec.generate_private_key(ec.SECP256R1()).sign(base64.b64decode(challenge['challenge']), ec.ECDSA(hashes.SHA256()))
        self.request('POST','/v1/auth/verify',{'challengeId':challenge['challengeId'],'signature':b64(wrong)},expect=401)
        self.checks.append('Copied public key does not authenticate an impostor')
        host_key = ed25519.Ed25519PrivateKey.generate()
        host_public = b64(host_key.public_key().public_bytes(serialization.Encoding.Raw, serialization.PublicFormat.Raw))
        enroll = {'name':'Integration Windows PC', 'platform':'windows', 'publicKey':host_public}
        self.request('POST','/v1/devices/enroll',enroll,expect=401)
        host = self.request('POST','/v1/devices/enroll',enroll,self.enrollment,expect=201)
        device, host_token = host['device'], host['deviceToken']
        self.request('GET','/v1/devices/'+device['displayId'],expect=401)
        self.request('POST',f"/v1/hosts/{device['id']}/heartbeat",token=host_token)
        self.request('GET','/v1/devices/'+device['displayId'],token=owner)
        self.checks.append('Protected enrollment, authenticated lookup, host heartbeat')
        request = {'displayId':device['displayId'], 'controllerName':'Integration Android Phone',
                   'permissions':{'viewScreen':True,'controlInput':True,'clipboard':True,'audio':True}}
        self.request('POST','/v1/sessions',request,expect=401)
        created = self.request('POST','/v1/sessions',request,owner,expect=201)
        session, token = created['session'], created['controllerToken']
        sid = session['id']; path = '/v1/sessions/'+sid
        assert session['controllerPublicKey']==public
        assert session['hostPublicKey']==host_public
        events = self.request('GET',f"/v1/hosts/{device['id']}/events?after=0",token=host_token)['events']
        assert any(event.get('sessionId')==sid for event in events)
        self.request('POST',path+'/signal',{'kind':'offer','payload':{'type':'offer','sdp':'v=0\r\n'}},token,expect=409)
        self.request('POST',path+'/approve',token=token,expect=403)
        self.request('POST',path+'/approve',{'permissions':{'viewScreen':True,'power':True}},host_token,expect=403)
        self.request('POST',path+'/approve',{'permissions':{'viewScreen':True,'controlInput':True}},host_token)
        status = self.request('GET',path,token=token)['session']
        assert status['status']=='approved' and status['permissions']['clipboard'] is False
        self.checks.append('Host consent, actor authorization, and permission narrowing')
        ice = self.request('GET',path+'/ice',token=token)
        relay = next(item for item in ice['iceServers'] if item.get('credential'))
        expected = b64(hmac.new(self.turn_secret.encode(),relay['username'].encode(),hashlib.sha1).digest())
        assert hmac.compare_digest(expected,relay['credential'])
        expiry = int(relay['username'].split(':',1)[0])
        assert 0 < expiry-time.time() <= 3600
        self.checks.append('Expiring TURN REST credential HMAC matches relay secret')
        # A wire-format signaling test, deliberately not called a media test.
        sdp = 'v=0\r\no=- 1 1 IN IP4 127.0.0.1\r\ns=-\r\nt=0 0\r\n'
        self.request('POST',path+'/signal',{'kind':'offer','payload':{'type':'offer','sdp':sdp}},token,expect=202)
        messages = self.request('GET',path+'/signals?after=0',token=host_token)['signals']
        assert messages[0]['kind']=='offer' and messages[0]['from']=='controller'
        self.request('POST',path+'/signal',{'kind':'answer','payload':{'type':'answer','sdp':sdp}},host_token,expect=202)
        messages = self.request('GET',path+'/signals?after=0',token=token)['signals']
        assert messages[0]['kind']=='answer' and messages[0]['from']=='host'
        self.request('POST',path+'/signal',{'kind':'ice','payload':{'candidate':'candidate:1 1 UDP 2130706431 127.0.0.1 5000 typ host','sdpMid':'0','sdpMLineIndex':0}},token,expect=202)
        self.checks.append('Approved bidirectional offer/answer and ICE routing')
        self.request('POST',path+'/end',token=token)
        self.request('POST',path+'/signal',{'kind':'ice','payload':{'candidate':''}},token,expect=409)
        self.request('GET',path+'/ice',token=token,expect=409)
        self.checks.append('Ended sessions cannot signal or acquire relay credentials')
        # A selector locates a monitor only; authorization remains view-only.
        monitor_id = '0123456789abcdef' * 4
        selected_request = {'displayId': device['displayId'], 'controllerName': 'Monitor test phone',
                            'monitorId': monitor_id, 'permissions': {'viewScreen': True}}
        for invalid in [None, '', 'A' * 64, 'a' * 63, 'a' * 65, 42]:
            self.request('POST', '/v1/sessions', dict(selected_request, monitorId=invalid), owner, expect=400)
        selected = self.request('POST', '/v1/sessions', selected_request, owner, expect=201)
        selected_path = '/v1/sessions/' + selected['session']['id']
        selected_token = selected['controllerToken']
        assert selected['session']['monitorId'] == monitor_id
        monitor_events = self.request('GET', f"/v1/hosts/{device['id']}/events?after=0", token=host_token)['events']
        assert any(event.get('sessionId') == selected['session']['id'] and
                   event.get('payload', {}).get('monitorId') == monitor_id for event in monitor_events)
        self.request('POST', selected_path + '/approve',
                     {'permissions': {'viewScreen': True}, 'monitorId': 'a' * 64}, host_token, expect=400)
        self.request('POST', selected_path + '/approve',
                     {'permissions': {'viewScreen': True, 'controlInput': True}}, host_token, expect=403)
        self.request('POST', selected_path + '/approve', {'permissions': {'viewScreen': True}}, host_token)
        selected_status = self.request('GET', selected_path, token=selected_token)['session']
        assert selected_status['monitorId'] == monitor_id and selected_status['permissions']['controlInput'] is False
        self.request('POST', selected_path + '/end', token=selected_token)
        self.checks.append('Immutable opaque monitor selector, strict validation, host event echo and view-only authorization')
        # The new pairing flow uses verified HTTPS and the same host approval
        # boundary; this harness never substitutes a handshake for real media.
        pairing_path = f"/v1/hosts/{device['id']}/pairing"
        pin = '001234567890'
        pairing_permissions = {'viewScreen': True, 'controlInput': True, 'clipboard': False,
                               'fileTransfer': False, 'audio': False, 'power': False}
        grant = self.request('POST', pairing_path, {'pin': pin, 'permissions': pairing_permissions}, host_token, expect=201)
        assert len(grant['code']) == 6 and grant['code'].isascii() and grant['code'].isdigit()
        assert 'pin' not in grant and grant['permissions'] == pairing_permissions
        self.request('POST', '/v1/pairings/redeem', {'code': grant['code'], 'pin': '009999', 'controllerName': 'Wrong PIN'}, owner, expect=403)
        paired = self.request('POST', '/v1/pairings/redeem', {'code': grant['code'], 'pin': pin, 'controllerName': 'Code phone'}, owner, expect=201)
        paired_session, paired_token = paired['session'], paired['controllerToken']
        paired_path = '/v1/sessions/' + paired_session['id']
        assert paired_session['status'] == 'pending' and paired_session['pairingId'] == grant['pairingId']
        assert paired_session['controllerPublicKey'] == public and paired_session['hostPublicKey'] == host_public
        assert paired_session['hostName'] == device['name'] and 'monitorId' not in paired_session
        self.request('POST', paired_path + '/approve', token=paired_token, expect=403)
        self.request('POST', paired_path + '/approve', {'permissions': pairing_permissions}, host_token)
        self.request('POST', '/v1/pairings/redeem', {'code': grant['code'], 'pin': pin, 'controllerName': 'Replay'}, owner, expect=403)
        self.request('POST', paired_path + '/end', token=paired_token)
        retained_grant = self.request('POST', pairing_path, {'pin': pin, 'permissions': pairing_permissions}, host_token, expect=201)
        self.checks.append('Verified HTTPS code/PIN redemption, wrong-PIN and replay denial, host-only approval and no extra grants')
        # Same persisted device ID and host credential must survive an actual process restart.
        self.stop(); self.start()
        owner2, _ = self.authenticate(phone_key)
        self.request('POST',f"/v1/hosts/{device['id']}/heartbeat",token=host_token)
        saved = self.request('GET','/v1/devices/'+device['displayId'],token=owner2)
        assert saved['name']=='Integration Windows PC'
        assert self.request('GET', pairing_path, token=host_token)['pairing'] is None
        self.request('POST', '/v1/pairings/redeem', {'code': retained_grant['code'], 'pin': pin, 'controllerName': 'Restart replay'}, owner2, expect=403)
        state = (self.folder/'state.json').read_text()
        assert host_token not in state and token not in state and paired_token not in state and pin not in state, 'Credential stored on disk'
        self.checks.append('Device identity survives process restart; bearer tokens absent from persisted JSON')
        self.checks.append('Unused pairing grant disappears on real process restart; PIN and pairing bearer absent from durable state')
        return {'ok':True, 'checks':self.checks, 'transport':'HTTPS with verified ephemeral test CA',
                'binary': {'sha256': hashlib.sha256(self.binary.read_bytes()).hexdigest(),
                           'size': self.binary.stat().st_size},
                'finishedAt': dt.datetime.now(dt.timezone.utc).isoformat(),
                'scope':'Control plane only. No desktop frames or input injected by this test.'}


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--binary',type=Path,required=True)
    p.add_argument('--report',type=Path)
    args=p.parse_args()
    with tempfile.TemporaryDirectory(prefix='openremote-integration-') as d:
        harness=Harness(args.binary.resolve(),Path(d))
        try:
            report=harness.run()
        finally:
            harness.stop()
    text=json.dumps(report,indent=2)+'\n'
    if args.report:
        args.report.parent.mkdir(parents=True,exist_ok=True)
        args.report.write_text(text)
    print(text)

if __name__=='__main__':
    main()
