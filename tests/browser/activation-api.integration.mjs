// Real Go coordinator + production browser protocol + independent Node WebCrypto.
// No browser, desktop pixels, native input, public server or TURN forwarding.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { generateKeyPairSync, webcrypto, createHash } from 'node:crypto';
import fs from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { Coordinator, APIError, pairingSession, serverInfo, sessionSnapshot } from '../../server/internal/webclient/assets/core.mjs';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const fixture = process.env.OPENREMOTE_PAIRING_FIXTURE || path.join(root, 'server/artifacts/activation-pairing-fixture');
const output = path.join(root, 'evidence/activation-tray-0.8.0/activation-api-integration.json');
const child = spawn(fixture, [], { stdio: ['ignore', 'pipe', 'pipe'] });
let stdout = '', processError, stage = 'fixture startup';
child.stdout.on('data', bytes => { stdout += bytes; });
child.stderr.on('data', () => {});
child.on('error', error => { processError = error.name; });
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const report = { startedAt: new Date().toISOString(), scope: 'Real isolated Go HTTP with production browser parsers and Node WebCrypto. No native desktop, browser pixels, input or public relay.', checks: [], ok: false };
try {
  const limit = Date.now() + 10000;
  while (!stdout.includes('\n')) {
    if (processError || child.exitCode !== null || Date.now() > limit) throw new Error('Fixture unavailable');
    await delay(20);
  }
  const settings = JSON.parse(stdout.split('\n')[0]); stdout = '';
  const api = new Coordinator(settings.origin);
  stage = 'activation discovery';
  const info = serverInfo(await api.request('/v1/info'));
  assert.equal(info.version, '0.8.0'); assert.equal(info.activationVersion, 1); assert.equal(info.maxActivationConnections, 4);
  const pair = generateKeyPairSync('ed25519');
  const publicKey = pair.publicKey.export({ format: 'der', type: 'spki' }).subarray(-32).toString('base64');
  const host = await api.request('/v1/devices/enroll', { method: 'POST', token: settings.enrollmentToken, body: { name: 'Activation test PC', platform: 'windows', publicKey } });
  settings.enrollmentToken = '';
  const hostPath = `/v1/hosts/${host.device.id}`, hostToken = host.deviceToken, pcId = host.device.displayId;
  assert.match(pcId, /^[0-9]{9}$/);
  const permissions = { viewScreen: true, controlInput: true, clipboard: false, fileTransfer: false, audio: false, power: false };
  const activation = await api.request(hostPath + '/pairing', { method: 'POST', token: hostToken, body: { mode: 'activation', pin: '003210', permissions } });
  assert.equal(activation.mode, 'activation'); assert.equal(activation.maxConnections, 4);
  report.checks.push('Permanent PC identity and additive activation discovery/create use actual coordinator routes');
  const peers = [];
  const names = ['Android · Test Phone', 'PC browser · Test Laptop', 'AI agent · Codex', 'AI agent · Reviewer'];
  stage = 'four authenticated PC-ID connections';
  for (const name of names) {
    const identity = await api.authenticate(undefined, webcrypto);
    const response = await api.request('/v1/pairings/redeem', { method: 'POST', token: identity.token, body: { code: pcId, pin: '003210', controllerName: name } });
    const session = pairingSession(response, identity.publicKey, pcId);
    assert.equal(session.mode, 'activation'); assert.equal(session.controllerName, name);
    peers.push({ identity, session });
  }
  assert.deepEqual(peers.map(p => p.session.permissions.controlInput), [true, false, false, false]);
  report.checks.push('Four independently proved browser/phone/agent identities connect using the same nine-digit PC ID and six-digit PIN; one input owner only');
  stage = 'capacity and authenticated roster';
  const current = await api.request(hostPath + '/sessions', { token: hostToken });
  assert.equal(current.sessions.length, 4);
  assert.deepEqual(current.sessions.map(s => s.controllerName).sort(), [...names].sort());
  assert(!JSON.stringify(current).includes('controllerToken'));
  await assert.rejects(api.request('/v1/pairings/redeem', { method: 'POST', token: peers[0].identity.token, body: { code: pcId, pin: '003210', controllerName: 'Fifth connection' } }), error => error instanceof APIError && error.status === 403);
  await assert.rejects(api.request(hostPath + '/sessions', { token: peers[0].identity.token }), error => error instanceof APIError && error.status === 401);
  report.checks.push('Pending sessions reserve the four-seat capacity; only the host can inspect its named roster and no controller bearer is exposed');
  stage = 'permission-preserving approval';
  for (const peer of peers) {
    await api.request(`/v1/sessions/${peer.session.id}/approve`, { method: 'POST', token: hostToken, body: { permissions: peer.session.permissions } });
    const snap = sessionSnapshot(await api.request(`/v1/sessions/${peer.session.id}`, { token: peer.session.controllerToken }), peer.session);
    assert.equal(snap.status, 'approved'); assert.equal(snap.permissions.controlInput, peer.session.permissions.controlInput);
  }
  assert.equal((await api.request(hostPath + '/pairing', { token: hostToken })).pairing.pairingId, activation.pairingId);
  report.checks.push('Host approvals preserve the input/view-only permissions and the activation remains available after redemption');
  stage = 'controller disconnect and next explicit connection';
  await api.request(`/v1/sessions/${peers[0].session.id}/end`, { method: 'POST', token: peers[0].session.controllerToken, body: {} });
  const replacementResponse = await api.request('/v1/pairings/redeem', { method: 'POST', token: peers[0].identity.token, body: { code: pcId, pin: '003210', controllerName: 'Reconnected laptop' } });
  const replacement = pairingSession(replacementResponse, peers[0].identity.publicKey, pcId);
  assert.equal(replacement.permissions.controlInput, true);
  for (const peer of peers.slice(1)) {
    const snapshot = sessionSnapshot(await api.request(`/v1/sessions/${peer.session.id}`, { token: peer.session.controllerToken }), peer.session);
    assert.equal(snapshot.permissions.controlInput, false);
  }
  report.checks.push('Ending the controller frees its slot for a new authorized connection; existing viewers never silently gain control');
  stage = 'deactivation revokes all remaining sessions';
  await api.request(hostPath + '/pairing', { method: 'DELETE', token: hostToken });
  for (const session of [...peers.slice(1).map(p => p.session), replacement]) {
    const snap = sessionSnapshot(await api.request(`/v1/sessions/${session.id}`, { token: session.controllerToken }), session);
    assert.equal(snap.status, 'ended');
  }
  assert.equal((await api.request(hostPath + '/sessions', { token: hostToken })).sessions.length, 0);
  await assert.rejects(api.request('/v1/pairings/redeem', { method: 'POST', token: peers[0].identity.token, body: { code: pcId, pin: '003210', controllerName: 'After deactivation' } }), error => error instanceof APIError && error.code === 'pairing_unavailable');
  report.checks.push('Deactivate ends pending and approved connections, clears the roster, and rejects later use of the same PC ID/PIN');
  report.ok = true;
} catch (error) {
  report.failure = { stage, type: error?.name || 'Error', ...(error instanceof APIError ? { status: error.status, code: error.code } : {}) };
  process.exitCode = 1;
} finally {
  child.kill('SIGTERM');
  await Promise.race([new Promise(resolve => child.once('exit', resolve)), delay(3000)]);
  if (child.exitCode === null) child.kill('SIGKILL');
  report.finishedAt = new Date().toISOString();
  report.fixtureSha256 = createHash('sha256').update(await fs.readFile(fixture)).digest('hex');
  await fs.mkdir(path.dirname(output), { recursive: true }); await fs.writeFile(output, JSON.stringify(report, null, 2) + '\n');
  console.log(JSON.stringify(report, null, 2));
}
