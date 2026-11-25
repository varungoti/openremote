import test from 'node:test';
import assert from 'node:assert/strict';
import { createPublicKey, randomBytes, verify, webcrypto } from 'node:crypto';
import { APIError, Coordinator, DisplayGate, base64, boundedName, friendlyError, normalizedPoint,
  p256SignatureDER, pairingSession, serverInfo, sessionSnapshot, strictJson, validCode, validPin } from '../../server/internal/webclient/assets/core.mjs';

const perms = { viewScreen: true, controlInput: true, clipboard: false, fileTransfer: false, audio: false, power: false };
const hostKey = base64(new Uint8Array(32).fill(7));
const session = { id: 's'.repeat(24), status: 'pending', pairingId: 'p'.repeat(24), hostDisplayId: '123456789',
  hostName: 'Office PC', hostPublicKey: hostKey, controllerPublicKey: 'proof-key', permissions: perms,
  expiresAt: new Date(Date.now() + 90_000).toISOString() };
const envelope = () => ({ session: { ...session, permissions: { ...perms } }, controllerToken: 't'.repeat(43) });
const caps = { v: 1, type: 'capabilities', text: true, clipboard: false, fileTransfer: false, multiMonitor: true, audio: false, power: false };
const catalog = { v: 1, type: 'display_catalog', selectionMode: 'reconnect', selectedMonitorId: 'a'.repeat(64),
  monitors: [{ id: 'a'.repeat(64), name: 'Primary display', width: 1920, height: 1080, primary: true }] };

test('code and PIN retain leading zeroes and reject formatting tricks or non-ASCII digits', () => {
  assert.equal(validCode('000001'), true); assert.equal(validPin('000001'), true);
  assert.equal(validCode('000123456'), true); assert.equal(validPin('000123456'), false);
  assert.equal(validPin('123456789012'), false);
  for (const v of [123456, '12345', '1234567', '123 456', '123456\n', '１２３４５６', '١٢٣٤٥٦', '123\u0000456']) assert.equal(validCode(v), false, String(v));
  for (const v of [null, '12345', '1234567', '1234567890123', '123456 ', 'abc123', '123456\n']) assert.equal(validPin(v), false);
});
test('permanent PC identity requires activation and stays bound during refresh', () => {
  const e = envelope(); e.session.mode = 'activation';
  const initial = pairingSession(e, 'proof-key', '123456789');
  assert.equal(initial.hostDisplayId, '123456789');
  assert.throws(() => pairingSession(e, 'proof-key', '999999999'));
  assert.throws(() => pairingSession(envelope(), 'proof-key', '123456789'));
  for (const mode of [null, '', 'other']) {
    const changed = envelope(); changed.session.mode = mode;
    assert.throws(() => pairingSession(changed, 'proof-key', '123456789'));
  }
  assert.throws(() => sessionSnapshot(envelope(), initial));
  const changed = envelope(); changed.session.mode = 'activation'; changed.session.hostDisplayId = '999999999';
  assert.throws(() => sessionSnapshot(changed, initial));
});
test('activation discovery accepts only an explicit supported integer capability and capacity', () => {
  const info = { service: 'openremote', protocolVersion: 1, pairingCodeVersion: 1, webController: true, activationVersion: 1, maxActivationConnections: 4 };
  assert.equal(serverInfo(info).activationVersion, 1);
  for (const patch of [{ activationVersion: true }, { activationVersion: null }, { activationVersion: 2 },
    { maxActivationConnections: '4' }, { maxActivationConnections: 0 }, { maxActivationConnections: null }])
    assert.throws(() => serverInfo({ ...info, ...patch }));
});
test('names are byte bounded and exclude control characters', () => {
  assert(boundedName('Home PC')); assert(boundedName('తెలుగు PC'));
  for (const v of ['', ' PC', 'PC\n', 'PC\u0085', 'x'.repeat(81), '🖥️'.repeat(20)]) assert.equal(boundedName(v), false);
});
test('strict JSON rejects duplicate authorization keys including escaped aliases and nested objects', () => {
  for (const v of ['{"pin":"1","pin":"2"}', '{"controlInput":false,"control\\u0049nput":true}', '{"a":[{"x":1,"x":2}]}', '{"a":1} false', '[1,]', '{"a":1,}', '[[[', '{"x":"unterminated}', '{"x":NaN}'])
    assert.throws(() => strictJson(v), v);
  assert.deepEqual(strictJson('{"x":[true,false,null,0,-2.25e3,"\\\"test"],"a":{}}'), { x: [true, false, null, 0, -2250, '"test'], a: {} });
  assert.throws(() => strictJson('"' + 'x'.repeat(40) + '"', 20));
  assert.throws(() => strictJson('['.repeat(34) + '0' + ']'.repeat(34)));
});
test('WebCrypto signatures are converted to actual Go-compatible ASN.1, verified independently', async () => {
  const pair = await webcrypto.subtle.generateKey({ name: 'ECDSA', namedCurve: 'P-256' }, false, ['sign', 'verify']);
  const pub = createPublicKey({ key: Buffer.from(await webcrypto.subtle.exportKey('spki', pair.publicKey)), type: 'spki', format: 'der' });
  for (let i = 0; i < 12; i++) {
    const challenge = randomBytes(32);
    const raw = await webcrypto.subtle.sign({ name: 'ECDSA', hash: 'SHA-256' }, pair.privateKey, challenge);
    assert(verify('sha256', challenge, pub, p256SignatureDER(raw)));
    assert(!verify('sha256', randomBytes(32), pub, p256SignatureDER(raw)));
  }
  assert.throws(() => p256SignatureDER(new Uint8Array(63)));
});
test('DER integer zero trimming and high-bit padding are canonical', () => {
  const raw = new Uint8Array(64); raw[31] = 1; raw[32] = 0x80;
  const actual = p256SignatureDER(raw);
  assert.deepEqual([...actual.slice(0, 8)], [0x30, 38, 2, 1, 1, 2, 33, 0]);
  assert.equal(actual.length, 40);
});
test('redemption requires exact verified controller and a least-privilege primary-display grant', () => {
  const result = pairingSession(envelope(), 'proof-key'); assert.equal(result.hostName, 'Office PC');
  for (const mutate of [
    e => { e.session.controllerPublicKey = 'attacker'; }, e => { e.session.permissions.fileTransfer = true; },
    e => { e.session.permissions.controlInput = 1; }, e => { e.session.permissions.viewScreen = false; },
    e => { e.session.monitorId = 'a'.repeat(64); }, e => { e.session.pairingId = ''; },
    e => { e.session.hostPublicKey = 'bad'; }, e => { e.session.status = 'approved'; },
    e => { e.session.hostName = '<valid plain text>'.repeat(9); }, e => { e.controllerToken = 'x\r\ny'; },
  ]) { const e = envelope(); mutate(e); assert.throws(() => pairingSession(e, 'proof-key')); }
});
test('session refresh cannot switch device, key, pairing or broaden permissions', () => {
  const initial = pairingSession(envelope(), 'proof-key');
  const next = envelope(); next.session.status = 'active'; next.session.permissions.controlInput = false;
  assert.equal(sessionSnapshot(next, initial).permissions.controlInput, false);
  for (const key of ['id', 'hostPublicKey', 'hostDisplayId', 'controllerPublicKey', 'pairingId']) {
    const e = envelope(); e.session[key] = 'changed'; assert.throws(() => sessionSnapshot(e, initial));
  }
  const e = envelope(); e.session.permissions.audio = true; assert.throws(() => sessionSnapshot(e, initial));
});
test('input requires peer, channel, permissions, fresh frame AND immutable display binding', () => {
  const gate = new DisplayGate(); gate.permission = gate.transport = gate.channel = true; gate.lastFrame = 1000;
  assert.equal(gate.ready(1001), false); gate.receive(JSON.stringify(caps)); assert.equal(gate.ready(1001), false);
  gate.receive(JSON.stringify(catalog)); assert.equal(gate.ready(1001), true);
  assert.equal(gate.ready(3501), false); assert.equal(gate.ready(1001, false), false); assert.equal(gate.ready(999), false);
  gate.channel = false; assert.equal(gate.ready(1001), false); gate.channel = true;
  gate.permission = false; assert.equal(gate.ready(1001), false); gate.permission = true;
  gate.revoke(); assert.equal(gate.ready(1001), false);
});
test('display gate rejects changed topology, wrong primary, duplicate monitors and capabilities', () => {
  assert.throws(() => new DisplayGate().receive(JSON.stringify({ ...caps, multiMonitor: false })));
  const gate = new DisplayGate(); gate.receive(JSON.stringify(caps)); gate.receive(JSON.stringify(catalog));
  assert.doesNotThrow(() => gate.receive(JSON.stringify(catalog)));
  const altered = structuredClone(catalog); altered.monitors[0].width = 1280;
  assert.throws(() => gate.receive(JSON.stringify(altered)));
  assert.throws(() => gate.receive(JSON.stringify({ ...caps, text: false })));
  for (const mutate of [x => { x.monitors.push({ ...x.monitors[0] }); }, x => { x.monitors[0].primary = false; },
    x => { x.selectedMonitorId = 'b'.repeat(64); }, x => { x.monitors[0].width = -5; }]) {
    const x = structuredClone(catalog); mutate(x);
    const next = new DisplayGate(); next.receive(JSON.stringify(caps)); assert.throws(() => next.receive(JSON.stringify(x)));
  }
});
test('pointer coordinates respect actual video letterboxing and never inject into black bars', () => {
  const rect = { left: 10, top: 20, width: 400, height: 400 };
  assert.deepEqual(normalizedPoint(210, 220, rect, 1920, 1080), { x: .5, y: .5 });
  assert.equal(normalizedPoint(210, 30, rect, 1920, 1080), null);
  assert.equal(normalizedPoint(NaN, 220, rect, 1920, 1080), null);
  assert.equal(normalizedPoint(210, 220, rect, 0, 0), null);
  assert.equal(normalizedPoint(411, 220, rect, 1920, 1080), null);
});
test('service capability checks fail before consuming a code on a legacy coordinator', () => {
  assert.throws(() => serverInfo({ service: 'openremote', protocolVersion: 1 }));
  assert.throws(() => serverInfo({ service: 'other', protocolVersion: 1, pairingCodeVersion: 1, webController: true }));
  assert(serverInfo({ service: 'openremote', protocolVersion: 1, pairingCodeVersion: 1, webController: true }));
});
test('API only sends credential bodies to the same origin, never redirects or ambient cookies', async () => {
  const calls = [];
  const api = new Coordinator('https://remote.company.tld', async (url, init) => {
    calls.push({ url, ...init }); return new Response('{"ok":true}', { headers: { 'Content-Type': 'application/json' } });
  });
  assert.deepEqual(await api.request('/v1/pairings/redeem', { method: 'POST', token: 'private', body: { code: '000001', pin: '987654' } }), { ok: true });
  assert.equal(calls[0].url, 'https://remote.company.tld/v1/pairings/redeem');
  assert.equal(calls[0].redirect, 'error'); assert.equal(calls[0].credentials, 'omit'); assert.equal(calls[0].referrerPolicy, 'no-referrer');
  assert.equal(calls[0].headers.Authorization, 'Bearer private');
  await assert.rejects(api.request('//attacker.tld/v1/auth'));
  for (const origin of ['http://remote.company.tld', 'https://remote.company.tld/path', 'https://user:password@remote.company.tld']) assert.throws(() => new Coordinator(origin));
});
test('API bounds streamed responses and does not retry a one-use POST', async () => {
  let calls = 0, canceled = false;
  const api = new Coordinator('https://remote.company.tld', async () => {
    calls++; return new Response(new ReadableStream({ start(controller) { controller.enqueue(new Uint8Array(1_048_577)); }, cancel() { canceled = true; } }),
      { headers: { 'Content-Type': 'application/json' } });
  });
  await assert.rejects(api.request('/v1/pairings/redeem', { method: 'POST', body: { code: '000001', pin: '987654' } }));
  assert.equal(calls, 1); assert.equal(canceled, true);
});
test('API distinguishes safe error codes from arbitrary remote text', async () => {
  const api = new Coordinator('https://remote.company.tld', async () => new Response('{"error":"<script>private detail</script>"}', {
    status: 403, headers: { 'Content-Type': 'application/json' } }));
  await assert.rejects(api.request('/v1/info'), error => error instanceof APIError && error.code === 'http_error');
  assert.match(friendlyError(new APIError(403, 'pairing_unavailable')), /PC ID and PIN/);
  assert.doesNotMatch(friendlyError(new Error('Bearer supersecret')), /supersecret/);
  try { strictJson('{"controllerToken":"SYNTHETIC_PRIVATE_TOKEN", broken}'); }
  catch (error) { assert.doesNotMatch(friendlyError(error), /SYNTHETIC_PRIVATE_TOKEN|controllerToken/); }
  assert.match(friendlyError(new Error('fetch failed'), true), /connection list/);
});
test('actual P-256 proof exchange uses a non-exportable signing key and verifies independently', async () => {
  let publicKey, nonce, verified = false;
  const api = new Coordinator('https://remote.company.tld', async (url, init) => {
    const request = JSON.parse(init.body); let body;
    if (url.endsWith('/challenge')) {
      assert.equal(request.algorithm, 'p256'); publicKey = request.publicKey; nonce = randomBytes(32);
      body = { challengeId: 'c'.repeat(24), challenge: nonce.toString('base64'), expiresAt: new Date(Date.now() + 60_000).toISOString() };
    } else {
      assert.equal(request.challengeId, 'c'.repeat(24));
      verified = verify('sha256', nonce, createPublicKey({ key: Buffer.from(publicKey, 'base64'), format: 'der', type: 'spki' }), Buffer.from(request.signature, 'base64'));
      assert(verified); body = { accessToken: 't'.repeat(43), publicKey, expiresAt: new Date(Date.now() + 60_000).toISOString() };
    }
    return new Response(JSON.stringify(body), { headers: { 'Content-Type': 'application/json' } });
  });
  const result = await api.authenticate(undefined, webcrypto); assert(verified); assert.equal(result.publicKey, publicKey);
});
