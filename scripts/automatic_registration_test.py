#!/usr/bin/env python3
"""Real verified-HTTPS bootstrap -> PIN -> recovery/revocation integration.

Windows signing is represented by an independent Python Ed25519 implementation.
No desktop capture, Windows UI, Android device or Internet relay is executed.
"""
import argparse
import base64
import datetime as dt
import hashlib
import json
from pathlib import Path
import re
import tempfile

from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric import ec, ed25519
from integration_test import Harness, b64


def proof(harness, key, name="Automatic Windows PC"):
    public = b64(key.public_key().public_bytes(serialization.Encoding.Raw, serialization.PublicFormat.Raw))
    reply = harness.request('POST', '/v1/hosts/register/challenge',
                            {'publicKey': public, 'name': name, 'platform': 'windows'}, expect=201)
    assert set(reply) == {'challengeId', 'challenge', 'expiresAt', 'apiOrigin'}
    assert reply['apiOrigin'] == harness.base
    assert re.fullmatch(r'[A-Za-z0-9_-]{24}', reply['challengeId'])
    payload = base64.b64decode(reply['challenge'], validate=True)
    assert b64(payload) == reply['challenge'] and len(payload) <= 2048
    lines = payload.decode('utf-8').split('\n')
    assert len(lines) == 9 and lines[:3] == ['OpenRemote.HostRegistration.v1', harness.base, reply['challengeId']]
    nonce = base64.b64decode(lines[3], validate=True)
    assert len(nonce) == 32 and b64(nonce) == lines[3]
    assert lines[4:] == [public, name, 'windows', reply['expiresAt'], '']
    expires = dt.datetime.fromisoformat(reply['expiresAt'].replace('Z', '+00:00'))
    assert 0 < (expires - dt.datetime.now(dt.timezone.utc)).total_seconds() <= 61
    return {'challengeId': reply['challengeId'], 'signature': b64(key.sign(payload))}, payload


def run(binary):
    checks = []
    with tempfile.TemporaryDirectory(prefix='openremote-auto-registration-') as directory:
        h = Harness(binary, Path(directory), automatic_registration=True)
        try:
            h.start()
            info = h.request('GET', '/v1/info')
            assert info['version'] == '0.8.0' and info['automaticHostRegistrationVersion'] == 1
            assert h.enrollment not in json.dumps(info) and h.turn_secret not in json.dumps(info)
            checks.append('Verified HTTPS discovery advertises automatic registration without exposing the administrator secret')

            key = ed25519.Ed25519PrivateKey.generate()
            submitted, payload = proof(h, key)
            wrong = ed25519.Ed25519PrivateKey.generate()
            invalid = dict(submitted, signature=b64(wrong.sign(payload)))
            h.request('POST', '/v1/hosts/register', invalid, expect=401)
            h.request('POST', '/v1/hosts/register', submitted, expect=401)
            checks.append('Copied public key cannot register a host; failed signatures consume the challenge')

            submitted, _ = proof(h, key)
            host = h.request('POST', '/v1/hosts/register', submitted, expect=201)
            device, bearer = host['device'], host['deviceToken']
            heartbeat = f"/v1/hosts/{device['id']}/heartbeat"
            pairing = f"/v1/hosts/{device['id']}/pairing"
            h.request('POST', heartbeat, token=bearer)
            h.request('POST', '/v1/hosts/register', submitted, expect=401)
            checks.append('Origin-bound Ed25519 proof registers a host without an enrollment secret and rejects replay')

            phone_key = ec.generate_private_key(ec.SECP256R1())
            phone, public = h.authenticate(phone_key)
            permissions = {'viewScreen': True, 'controlInput': True, 'clipboard': False,
                           'fileTransfer': False, 'audio': False, 'power': False}
            pin = '003210'
            code = h.request('POST', pairing, {'pin': pin, 'permissions': permissions}, bearer, expect=201)
            connected = h.request('POST', '/v1/pairings/redeem',
                                  {'code': code['code'], 'pin': pin, 'controllerName': 'Automatic phone'}, phone, expect=201)
            session, controller = connected['session'], connected['controllerToken']
            assert session['hostDeviceId'] == device['id'] and session['controllerPublicKey'] == public
            assert session['permissions'] == permissions and session['pairingId'] == code['pairingId']
            path = '/v1/sessions/' + session['id']
            h.request('POST', path + '/approve', {'permissions': permissions}, bearer)
            h.request('GET', path + '/ice', token=controller)
            checks.append('Automatically registered host issues a Server ID; six-digit PIN redemption reaches owner approval and ICE configuration')

            submitted, _ = proof(h, key)
            recovered = h.request('POST', '/v1/hosts/register', submitted, expect=200)
            assert recovered['device']['id'] == device['id'] and recovered['deviceToken'] != bearer
            h.request('POST', heartbeat, token=bearer, expect=401)
            rotated = recovered['deviceToken']
            h.request('POST', heartbeat, token=rotated)
            assert h.request('GET', path, token=controller)['session']['status'] == 'ended'
            h.request('GET', path + '/ice', token=controller, expect=409)
            checks.append('Fresh identity proof recovers the same host, rotates its bearer and ends the prior session')

            unused = h.request('POST', pairing, {'pin': pin, 'permissions': permissions}, rotated, expect=201)
            submitted, _ = proof(h, key)
            recovered_again = h.request('POST', '/v1/hosts/register', submitted, expect=200)
            latest = recovered_again['deviceToken']
            assert h.request('GET', pairing, token=latest)['pairing'] is None
            h.request('POST', '/v1/pairings/redeem', {'code': unused['code'], 'pin': pin, 'controllerName': 'Old invitation'}, phone, expect=403)
            checks.append('Identity recovery invalidates unused connection invitations')

            h.stop(); h.start()
            h.request('POST', heartbeat, token=latest)
            h.request('POST', f"/v1/admin/hosts/{device['id']}/revoke", {}, latest, expect=401)
            h.request('POST', f"/v1/admin/hosts/{device['id']}/revoke", {}, h.enrollment)
            h.request('POST', heartbeat, token=latest, expect=401)
            submitted, _ = proof(h, key)
            h.request('POST', '/v1/hosts/register', submitted, expect=403)
            checks.append('Host identity survives real process restart; only the administrator can revoke it and proof cannot undo revocation')

            h.stop(); h.start()
            submitted, _ = proof(h, key)
            h.request('POST', '/v1/hosts/register', submitted, expect=403)
            raw = (h.folder / 'state.json').read_text()
            state = json.loads(raw)
            assert state['version'] == 2
            for secret in [bearer, rotated, latest, controller, phone, pin, submitted['signature']]:
                assert secret not in raw
            checks.append('Revocation survives a second process restart; durable state contains no raw bearer, PIN or proof')
        finally:
            h.stop()
    return {'ok': True, 'checks': checks, 'finishedAtUtc': dt.datetime.now(dt.timezone.utc).isoformat(),
            'binary': {'sha256': hashlib.sha256(binary.read_bytes()).hexdigest(), 'sizeBytes': binary.stat().st_size},
            'transport': 'HTTPS using a verified ephemeral test CA',
            'scope': 'Actual isolated coordinator process and independent Python Ed25519/P-256 clients. No native application, desktop pixels, remote input or public relay.'}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--report', type=Path, required=True)
    args = parser.parse_args()
    result = run(args.binary.resolve())
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2))
