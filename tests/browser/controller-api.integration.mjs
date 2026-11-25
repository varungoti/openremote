// Production browser protocol module + real production coordinator HTTP routes.
// This test has no browser, Windows capture, local input, or public relay.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { generateKeyPairSync, webcrypto } from 'node:crypto';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { Coordinator, APIError, pairingSession, serverInfo, sessionSnapshot } from '../../server/internal/webclient/assets/core.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const fixture = process.env.OPENREMOTE_PAIRING_FIXTURE || path.join(root, 'server/artifacts/activation-pairing-fixture');
const output = process.env.OPENREMOTE_PAIRING_API_EVIDENCE || path.join(root, 'evidence/activation-tray-0.8.0/web-api-integration.json');
const child = spawn(fixture, [], { stdio: ['ignore', 'pipe', 'pipe'] });
let raw = '', stderr = '';
child.stdout.on('data', b => { raw += b; });
child.stderr.on('data', b => { stderr += b; });
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
let fixtureError;
child.on('error', error => { fixtureError = error; });
const report = { scope: 'Browser protocol module in Node WebCrypto against real Go coordinator HTTP; no browser/media/native desktop.',
  startedAt: new Date().toISOString(), cases: [], ok: false };
try {
  const deadline = Date.now() + 10_000;
  while (!raw.includes('\n')) {
    if (fixtureError || child.exitCode !== null || Date.now() > deadline) throw new Error('Fixture did not start: ' + (fixtureError?.message || stderr));
    await delay(25);
  }
  const settings = JSON.parse(raw.split('\n')[0]); raw = '';
  const api = new Coordinator(settings.origin);
  const info = serverInfo(await api.request('/v1/info'));
  assert.equal(info.version, '0.8.0');
  const page = await fetch(settings.origin + '/connect');
  assert.equal(page.status, 200); assert.match(page.headers.get('content-security-policy'), /default-src 'none'/);
  assert.match(await page.text(), /PC ID/);
  report.cases.push('real discovery and embedded browser assets');
  const pair = generateKeyPairSync('ed25519');
  const hostPublicKey = pair.publicKey.export({ format: 'der', type: 'spki' }).subarray(-32).toString('base64');
  const host = await api.request('/v1/devices/enroll', { method: 'POST', token: settings.enrollmentToken,
    body: { name: 'Integration PC', platform: 'windows', publicKey: hostPublicKey } });
  settings.enrollmentToken = '';
  const token = host.deviceToken, hostPath = `/v1/hosts/${host.device.id}`;
  const perms = { viewScreen: true, controlInput: true, clipboard: false, fileTransfer: false, audio: false, power: false };
  const generate = pin => api.request(hostPath + '/pairing', { method: 'POST', token, body: { pin, permissions: perms } });
  let grant = await generate('001234');
  assert.match(grant.code, /^[0-9]{6}$/); assert(!Object.hasOwn(grant, 'pin'));
  const publicClient = await api.authenticate(undefined, webcrypto);
  report.cases.push('real browser P-256 challenge signature accepted by Go');
  await assert.rejects(api.request('/v1/pairings/redeem', { method: 'POST', token: publicClient.token,
    body: { code: grant.code, pin: '009999', controllerName: 'Integration browser' } }),
  e => e instanceof APIError && e.status === 403 && e.code === 'pairing_unavailable');
  report.cases.push('incorrect PIN rejected without session');
  const response = await api.request('/v1/pairings/redeem', { method: 'POST', token: publicClient.token,
    body: { code: grant.code, pin: '001234', controllerName: 'Integration browser' } });
  const session = pairingSession(response, publicClient.publicKey);
  assert.equal(session.hostName, 'Integration PC'); assert.equal(session.hostPublicKey, hostPublicKey); assert.equal(session.pairingId, grant.pairingId);
  const events = await api.request(hostPath + '/events?after=0', { token });
  assert(events.events.some(e => e.type === 'session.requested' && e.sessionId === session.id && e.payload.pairingId === grant.pairingId));
  await api.request(`/v1/sessions/${session.id}/approve`, { method: 'POST', token, body: { permissions: perms } });
  const snap = sessionSnapshot(await api.request(`/v1/sessions/${session.id}`, { token: session.controllerToken }), session);
  assert.equal(snap.status, 'approved');
  assert.equal((await api.request(hostPath + '/pairing', { token })).pairing, null);
  report.cases.push('one-use redemption, identity-bound host event and host approval');
  await assert.rejects(api.request('/v1/pairings/redeem', { method: 'POST', token: publicClient.token,
    body: { code: grant.code, pin: '001234', controllerName: 'Replay' } }), e => e instanceof APIError && e.code === 'pairing_unavailable');
  const ice = await api.request(`/v1/sessions/${session.id}/ice`, { token: session.controllerToken });
  assert(Array.isArray(ice.iceServers)); assert.equal(ice.iceTransportPolicy, 'all');
  await api.request(`/v1/sessions/${session.id}/end`, { method: 'POST', token: session.controllerToken, body: {} });
  const ended = sessionSnapshot(await api.request(`/v1/sessions/${session.id}`, { token: session.controllerToken }), session);
  assert.equal(ended.status, 'ended');
  report.cases.push('consumed-code replay rejected; session ICE and stop work');
  grant = await generate('654321');
  await api.request(hostPath + '/pairing', { method: 'DELETE', token });
  await assert.rejects(api.request('/v1/pairings/redeem', { method: 'POST', token: publicClient.token,
    body: { code: grant.code, pin: '654321', controllerName: 'Revoked code' } }), e => e instanceof APIError && e.code === 'pairing_unavailable');
  report.cases.push('host revocation prevents later redemption');
  report.ok = true;
} catch (error) {
  // Fixed safe metadata: never include fixture secrets, request/response bodies or SDP.
  report.error = error instanceof APIError ? { type: 'APIError', status: error.status, code: error.code }
    : { type: error?.name || 'Error', message: String(error?.message || 'Failed').replace(/Bearer\s+\S+/g, '[redacted]').slice(0, 800) };
  process.exitCode = 1;
} finally {
  child.kill('SIGTERM');
  await Promise.race([new Promise(resolve => child.once('exit', resolve)), delay(3000)]);
  if (child.exitCode === null) child.kill('SIGKILL');
  report.finishedAt = new Date().toISOString();
  await fs.mkdir(path.dirname(output), { recursive: true });
  await fs.writeFile(output, JSON.stringify(report, null, 2) + '\n');
  console.log(JSON.stringify(report, null, 2));
}
