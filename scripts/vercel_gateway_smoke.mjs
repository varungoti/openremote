// Optional runtime integration: actual stateless Node gateway -> actual Go
// coordinator over verified HTTPS, isolated on loopback. This is not a desktop,
// mobile, Vercel platform, or TURN connectivity test.
// Run: node scripts/vercel_gateway_smoke.mjs --go /path/to/go
import assert from 'node:assert/strict';
import { spawn, execFile } from 'node:child_process';
import { createHash, generateKeyPairSync, randomBytes, sign, webcrypto } from 'node:crypto';
import fs from 'node:fs/promises';
import https from 'node:https';
import net from 'node:net';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';
import { createGateway, vercelClientIP } from '../deploy/vercel/lib/gateway.mjs';
import { Coordinator, pairingSession, serverInfo } from '../server/internal/webclient/assets/core.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const execute = promisify(execFile);
const args = process.argv.slice(2);
const options = new Map();
for (let i = 0; i < args.length; i += 2) {
  if (!['--go', '--python', '--report'].includes(args[i]) || !args[i + 1] || options.has(args[i])) {
    throw new Error('Usage: node scripts/vercel_gateway_smoke.mjs [--go /path/to/go] [--python /path/to/python] [--report path.json]');
  }
  options.set(args[i], args[i + 1]);
}
const go = options.get('--go') || process.env.OPENREMOTE_GO || 'go';
const python = options.get('--python') || process.env.OPENREMOTE_PYTHON || 'python3';
const output = path.resolve(root, options.get('--report') || 'evidence/vercel-deployment-0.8.0/gateway-runtime.json');
const publicOrigin = 'https://viewer.openremote.invalid';
const coordinatorHost = 'coordinator.openremote.invalid';
const scratch = await fs.mkdtemp(path.join(os.tmpdir(), 'openremote-gateway-runtime-'));
const binary = path.join(scratch, 'controlplane');
const report = { ok: false, startedAtUtc: new Date().toISOString(),
  command: 'node scripts/vercel_gateway_smoke.mjs --go <Go executable>',
  scope: 'Actual Node gateway function, actual isolated Go coordinator process, production browser protocol decoder, independent Ed25519/P-256 signatures, and verified HTTPS with an ephemeral test CA. Platform client-IP headers are synthetic fixtures. No public deployment, native app, desktop pixels, remote input, or TURN forwarding.',
  checks: [], environment: { node: process.version } };
let stage = 'source snapshot', child, childError, transport, sourcePaths, before;
let connectionCount = 0;
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
const digest = bytes => createHash('sha256').update(bytes).digest('hex');

async function filesUnder(directory) {
  const entries = await fs.readdir(directory, { withFileTypes: true });
  const nested = await Promise.all(entries.map(entry => entry.isDirectory()
    ? filesUnder(path.join(directory, entry.name)) : [path.join(directory, entry.name)]));
  return nested.flat();
}
async function sourceHashes() {
  return Object.fromEntries(await Promise.all(sourcePaths.map(async file =>
    [path.relative(root, file), digest(await fs.readFile(file))])));
}
async function freePort() {
  const reservation = net.createServer();
  await new Promise((resolve, reject) => { reservation.once('error', reject); reservation.listen(0, '127.0.0.1', resolve); });
  const port = reservation.address().port;
  await new Promise((resolve, reject) => reservation.close(error => error ? reject(error) : resolve()));
  return port;
}
async function stopChild() {
  if (!child || child.exitCode !== null || child.signalCode !== null) return;
  await new Promise(resolve => {
    const timer = setTimeout(() => { child.kill('SIGKILL'); }, 5000);
    child.once('exit', () => { clearTimeout(timer); resolve(); });
    child.kill('SIGTERM');
  });
}

// Reuse the repository HTTPS fixture's certificate generator, extending its SAN
// for a test-only logical upstream name. TLS still verifies the real hostname;
// a local lookup hook replaces DNS, never certificate verification.
const certificateProgram = `
import sys
from pathlib import Path
sys.path.insert(0, sys.argv[3])
from integration_test import certificate
from cryptography import x509
from cryptography.hazmat.primitives import hashes, serialization
folder = Path(sys.argv[1])
cert_path, key_path = certificate(folder)
original = x509.load_pem_x509_certificate(cert_path.read_bytes())
key = serialization.load_pem_private_key(key_path.read_bytes(), password=None)
builder = (x509.CertificateBuilder().subject_name(original.subject).issuer_name(original.issuer)
    .public_key(original.public_key()).serial_number(original.serial_number)
    .not_valid_before(original.not_valid_before_utc).not_valid_after(original.not_valid_after_utc))
for extension in original.extensions:
    value = extension.value
    if isinstance(value, x509.SubjectAlternativeName):
        value = x509.SubjectAlternativeName(list(value) + [x509.DNSName(sys.argv[2])])
    builder = builder.add_extension(value, extension.critical)
cert_path.write_bytes(builder.sign(key, hashes.SHA256()).public_bytes(serialization.Encoding.PEM))
`;

try {
  sourcePaths = [...await filesUnder(path.join(root, 'server/internal')),
    ...await filesUnder(path.join(root, 'server/cmd/controlplane')),
    ...['server/go.mod', 'deploy/vercel/lib/gateway.mjs', 'deploy/vercel/api/gateway.mjs',
      'scripts/integration_test.py', 'scripts/vercel_gateway_smoke.mjs'].map(file => path.join(root, file))]
    .filter(file => !file.endsWith('_test.go')).sort();
  before = await sourceHashes();
  report.gitRevision = (await execute('git', ['rev-parse', 'HEAD'], { cwd: root })).stdout.trim();
  stage = 'Go compiler and runtime build';
  report.environment.go = (await execute(go, ['version'], { timeout: 20_000 })).stdout.trim();
  await execute(go, ['build', '-trimpath', '-o', binary, './cmd/controlplane'], {
    cwd: path.join(root, 'server'), timeout: 120_000, maxBuffer: 256 * 1024,
    env: { ...process.env, GOTOOLCHAIN: 'local' },
  });
  report.binary = { sha256: digest(await fs.readFile(binary)), sizeBytes: (await fs.stat(binary)).size };
  stage = 'ephemeral verified TLS certificate';
  await execute(python, ['-c', certificateProgram, scratch, coordinatorHost, path.join(root, 'scripts')], {
    timeout: 20_000, maxBuffer: 32 * 1024,
  });
  const ca = await fs.readFile(path.join(scratch, 'server-cert.pem'));
  const port = await freePort();
  const coordinatorOrigin = `https://${coordinatorHost}:${port}`;
  const gatewayToken = randomBytes(48).toString('base64url');
  const cleanEnvironment = Object.fromEntries(Object.entries(process.env).filter(([name]) => !name.startsWith('OPENREMOTE_')));
  stage = 'real Go coordinator startup';
  child = spawn(binary, [], { cwd: scratch, stdio: ['ignore', 'ignore', 'ignore'], env: {
    ...cleanEnvironment,
    OPENREMOTE_LISTEN: `127.0.0.1:${port}`,
    OPENREMOTE_STATE_FILE: path.join(scratch, 'state.json'),
    OPENREMOTE_ENROLLMENT_TOKEN: randomBytes(48).toString('base64url'),
    OPENREMOTE_GATEWAY_TOKEN: gatewayToken,
    OPENREMOTE_AUTOMATIC_HOST_REGISTRATION: 'true',
    OPENREMOTE_PUBLIC_ORIGIN: publicOrigin,
    OPENREMOTE_TLS_CERT: path.join(scratch, 'server-cert.pem'),
    OPENREMOTE_TLS_KEY: path.join(scratch, 'server-key.pem'),
    OPENREMOTE_TURN_URLS: 'turn:relay.openremote.invalid:3478?transport=udp,turns:relay.openremote.invalid:5349?transport=tcp',
    OPENREMOTE_TURN_SECRET: randomBytes(48).toString('base64url'),
    OPENREMOTE_RELAY_ONLY: 'true',
  } });
  child.once('error', error => { childError = error; });
  transport = new https.Agent({ keepAlive: false, ca, rejectUnauthorized: true,
    lookup(hostname, options, callback) {
      if (hostname !== coordinatorHost) { callback(new Error('Unexpected fixture upstream hostname.')); return; }
      if (options.all) callback(null, [{ address: '127.0.0.1', family: 4 }]);
      else callback(null, '127.0.0.1', 4);
    } });

  // Use the real HTTPS socket transport. Only DNS is fixture-controlled; both
  // the certificate chain and the logical upstream hostname must verify.
  const wire = [];
  async function upstreamFetch(url, options = {}) {
    assert.equal(new URL(url).origin, coordinatorOrigin);
    const entry = { url, method: options.method || 'GET', headers: new Headers(options.headers),
      requestBody: Buffer.from(options.body || []), responseBody: null };
    wire.push(entry);
    return new Promise((resolve, reject) => {
      const request = https.request(url, { agent: transport, method: entry.method,
        headers: Object.fromEntries(entry.headers), signal: options.signal }, response => {
        const chunks = []; let size = 0;
        response.on('data', chunk => {
          size += chunk.length;
          if (size > 4 * 1024 * 1024) response.destroy(new Error('Fixture response exceeds limit.'));
          else chunks.push(chunk);
        });
        response.once('error', reject);
        response.once('end', () => {
          entry.responseBody = Buffer.concat(chunks);
          const headers = new Headers();
          for (const [name, value] of Object.entries(response.headers)) {
            if (value !== undefined) headers.set(name, Array.isArray(value) ? value.join(', ') : value);
          }
          resolve(new Response(response.statusCode === 204 ? null : entry.responseBody, { status: response.statusCode, headers }));
        });
      });
      request.once('socket', socket => socket.once('secureConnect', () => {
        if (socket.authorized) connectionCount++;
      }));
      request.once('error', reject);
      request.end(entry.requestBody.length ? entry.requestBody : undefined);
    });
  }
  const startupDeadline = Date.now() + 15_000;
  for (;;) {
    if (childError) throw childError;
    if (child.exitCode !== null) throw new Error('Coordinator process exited during startup.');
    try {
      const health = await upstreamFetch(coordinatorOrigin + '/healthz', { signal: AbortSignal.timeout(1000) });
      if (health.status === 200) break;
    } catch (error) { if (Date.now() >= startupDeadline) throw error; }
    if (Date.now() >= startupDeadline) throw new Error('Coordinator health startup timed out.');
    await sleep(25);
  }
  report.checks.push('Compiled current Go source and started the real coordinator with isolated state and certificate-verified HTTPS');

  stage = 'required gateway proof';
  for (const headers of [{}, { 'x-openremote-gateway-token': 'incorrect', 'x-openremote-client-ip': '198.51.100.10' },
    { 'x-openremote-gateway-token': gatewayToken, 'x-openremote-client-ip': '198.51.100.10, 198.51.100.11' }]) {
    const denied = await upstreamFetch(coordinatorOrigin + '/v1/info', { headers });
    assert.equal(denied.status, 401);
    assert.deepEqual(await denied.json(), { error: 'gateway_auth_required' });
  }
  report.checks.push('The real backend rejects missing/wrong gateway credentials and malformed asserted client identity');

  const gateway = createGateway({ coordinatorOrigin, gatewayToken,
    clientIP: request => vercelClientIP(request, true), fetcher: upstreamFetch,
    readAsset: async () => { throw new Error('Assets are outside this runtime smoke.'); } });
  const ordinaryIP = '198.51.100.10';
  function routedFetch(clientIP) {
    return (url, options = {}) => {
      const headers = new Headers(options.headers);
      headers.set('x-vercel-forwarded-for', clientIP);
      return gateway(new Request(url, { ...options, headers }));
    };
  }
  async function call(method, route, { body, raw, token, clientIP = ordinaryIP, headers = {}, expected = 200 } = {}) {
    const requestHeaders = new Headers({ accept: 'application/json', ...headers });
    requestHeaders.set('x-vercel-forwarded-for', clientIP);
    if (body !== undefined || raw !== undefined) requestHeaders.set('content-type', 'application/json');
    if (token) requestHeaders.set('authorization', 'Bearer ' + token);
    const response = await gateway(new Request(publicOrigin + route, { method, headers: requestHeaders,
      body: raw ?? (body === undefined ? undefined : JSON.stringify(body)) }));
    assert.equal(response.status, expected, 'Unexpected HTTP status at ' + stage);
    assert.equal(response.headers.get('cache-control'), 'no-store');
    const bytes = Buffer.from(await response.arrayBuffer());
    return { response, bytes, value: JSON.parse(bytes.toString('utf8')) };
  }

  stage = 'real discovery through the Node gateway';
  const discovery = await call('GET', '/v1/info');
  const info = serverInfo(discovery.value);
  assert.equal(info.activationVersion, 1); assert.equal(info.maxActivationConnections, 4);
  assert.equal(info.automaticHostRegistrationVersion, 1); assert.equal(info.relay.tls, true);
  assert.equal(discovery.bytes.equals(wire.at(-1).responseBody), true);
  assert.equal(wire.at(-1).headers.get('x-openremote-gateway-token'), gatewayToken);
  assert.equal(wire.at(-1).headers.get('x-openremote-client-ip'), ordinaryIP);
  report.checks.push('Current production discovery passes the gateway capability policy and returns byte-identical real coordinator JSON');

  stage = 'origin-bound host registration and raw request preservation';
  const hostKey = generateKeyPairSync('ed25519');
  const publicKey = hostKey.publicKey.export({ format: 'der', type: 'spki' }).subarray(-32).toString('base64');
  const name = 'Gateway smoke PC · Windows';
  const raw = '{\n  "publicKey" : ' + JSON.stringify(publicKey) + ',\n  "name": ' + JSON.stringify(name) + ', "platform": "windows"\n}\n';
  const result = await call('POST', '/v1/hosts/register/challenge', { raw, expected: 201, headers: {
    'x-openremote-gateway-token': 'untrusted-incoming-token', 'x-openremote-client-ip': '203.0.113.201',
    'x-forwarded-for': '203.0.113.202', 'x-forwarded-proto': 'http', 'x-forwarded-host': 'attacker.invalid',
  } });
  assert.equal(wire.at(-1).requestBody.equals(Buffer.from(raw)), true);
  assert.equal(wire.at(-1).headers.get('x-openremote-gateway-token'), gatewayToken);
  assert.equal(wire.at(-1).headers.get('x-openremote-client-ip'), ordinaryIP);
  for (const header of ['x-forwarded-for', 'x-forwarded-proto', 'x-forwarded-host']) assert.equal(wire.at(-1).headers.has(header), false);
  const challenge = result.value, transcript = Buffer.from(challenge.challenge, 'base64');
  assert.equal(challenge.apiOrigin, publicOrigin);
  const lines = transcript.toString('utf8').split('\n');
  assert.equal(lines.length, 9);
  assert.deepEqual(lines.slice(0, 3), ['OpenRemote.HostRegistration.v1', publicOrigin, challenge.challengeId]);
  assert.equal(Buffer.from(lines[3], 'base64').length, 32);
  assert.deepEqual(lines.slice(4), [publicKey, name, 'windows', challenge.expiresAt, '']);
  const proof = { challengeId: challenge.challengeId, signature: sign(null, transcript, hostKey.privateKey).toString('base64') };
  const registered = (await call('POST', '/v1/hosts/register', { body: proof, expected: 201 })).value;
  await call('POST', '/v1/hosts/register', { body: proof, expected: 401 });
  const hostPath = `/v1/hosts/${registered.device.id}`;
  assert.match(registered.device.displayId, /^[0-9]{9}$/);
  await call('POST', hostPath + '/heartbeat', { token: registered.deviceToken });
  assert.equal(wire.at(-1).headers.get('authorization'), 'Bearer ' + registered.deviceToken);
  await call('POST', hostPath + '/heartbeat', { token: 'invalid-host-token', expected: 401 });
  report.checks.push('Raw UTF-8/whitespace body bytes survive forwarding; Ed25519 proof binds the public viewer origin, registers a permanent PC ID, rejects replay, and preserves host bearer authorization');
  report.checks.push('Untrusted incoming gateway/client/forwarding headers cannot replace the publisher token or platform client identity');

  stage = 'duplicate JSON stays invalid through the proxy';
  await call('POST', '/v1/hosts/register/challenge', { expected: 400,
    raw: '{"publicKey":' + JSON.stringify(publicKey) + ',"name":"first","name":"second","platform":"windows"}' });
  report.checks.push('The gateway preserves raw duplicate JSON so the production Go decoder rejects it instead of silently normalizing it');

  stage = 'production P-256 controller authentication';
  const api = new Coordinator(publicOrigin, routedFetch('198.51.100.20'));
  const controller = await api.authenticate(undefined, webcrypto);
  const lookup = await api.request('/v1/devices/' + registered.device.displayId, { token: controller.token });
  assert.equal(lookup.displayId, registered.device.displayId);
  assert.equal(lookup.publicKey, publicKey);
  assert.equal(wire.at(-1).headers.get('authorization'), 'Bearer ' + controller.token);
  report.checks.push('Unchanged browser Coordinator authenticates with real P-256 challenge/proof and its forwarded bearer authorizes device lookup');

  stage = 'PC-ID/PIN session protocol through the gateway';
  const permissions = { viewScreen: true, controlInput: true, clipboard: false, fileTransfer: false, audio: false, power: false };
  const pin = '003210';
  const activation = (await call('POST', hostPath + '/pairing', { token: registered.deviceToken,
    body: { mode: 'activation', pin, permissions }, expected: 201 })).value;
  assert.equal(activation.maxConnections, 4);
  const redeemed = await api.request('/v1/pairings/redeem', { method: 'POST', token: controller.token,
    body: { code: registered.device.displayId, pin, controllerName: 'Gateway smoke phone' } });
  const session = pairingSession(redeemed, controller.publicKey, registered.device.displayId);
  const sessionPath = '/v1/sessions/' + session.id;
  await call('POST', sessionPath + '/approve', { token: registered.deviceToken, body: { permissions: session.permissions } });
  const sdp = 'v=0\r\no=- 1 1 IN IP4 127.0.0.1\r\ns=-\r\nt=0 0\r\n';
  await api.request(sessionPath + '/signal', { method: 'POST', token: session.controllerToken,
    body: { kind: 'offer', payload: { type: 'offer', sdp } } });
  const signals = (await call('GET', sessionPath + '/signals?after=0', { token: registered.deviceToken })).value;
  assert.equal(signals.signals[0].payload.sdp, sdp);
  await api.request(sessionPath + '/end', { method: 'POST', token: session.controllerToken, body: {} });
  assert.equal((await api.request(sessionPath, { token: session.controllerToken })).session.status, 'ended');
  await call('DELETE', hostPath + '/pairing', { token: registered.deviceToken });
  report.checks.push('Real host activation, six-digit PIN redemption, owner approval, bearer-authenticated SDP signaling, session end and deactivation traverse the gateway; this is protocol-only evidence');

  stage = 'independent registration client rate limits';
  const registrationBody = { publicKey, name: 'Rate-limit fixture PC', platform: 'windows' };
  for (let i = 0; i < 10; i++) await call('POST', '/v1/hosts/register/challenge', {
    clientIP: '198.51.100.71', body: registrationBody, expected: 201,
  });
  const limitedRegistration = await call('POST', '/v1/hosts/register/challenge', {
    clientIP: '198.51.100.71', body: registrationBody, expected: 429,
    headers: { 'x-openremote-client-ip': '198.51.100.72', 'x-forwarded-for': '198.51.100.72' },
  });
  assert.equal(limitedRegistration.value.error, 'host_registration_rate_limited');
  assert.equal(limitedRegistration.response.headers.get('retry-after'), '60');
  await call('POST', '/v1/hosts/register/challenge', { clientIP: '198.51.100.72', body: registrationBody, expected: 201 });
  report.checks.push('Ten registration challenges exhaust only client A; spoofed ordinary forwarding headers cannot reset it, while independent client B still obtains a challenge behind the same gateway');

  stage = 'independent sensitive authentication rate limits';
  const authBody = { publicKey: controller.publicKey, algorithm: 'p256' };
  for (let i = 0; i < 30; i++) await call('POST', '/v1/auth/challenge', { clientIP: '198.51.100.81', body: authBody, expected: 201 });
  const limitedAuth = await call('POST', '/v1/auth/challenge', { clientIP: '198.51.100.81', body: authBody, expected: 429 });
  assert.equal(limitedAuth.value.error, 'rate_limited');
  assert.equal(limitedAuth.response.headers.get('retry-after'), '60');
  await call('POST', '/v1/auth/challenge', { clientIP: '198.51.100.82', body: authBody, expected: 201 });
  report.checks.push('The general sensitive-auth bucket also isolates independently asserted clients instead of collapsing all gateway traffic into the upstream socket IP');
  assert.ok(connectionCount > 0);
  report.verifiedTLSConnections = connectionCount;
  stage = 'source consistency';
  const after = await sourceHashes();
  report.sourceUnchangedDuringRun = JSON.stringify(before) === JSON.stringify(after);
  assert.equal(report.sourceUnchangedDuringRun, true);
  report.sourceSha256 = before;
  report.ok = true;
} catch (error) {
  // Never print an assertion's values: they may include an ephemeral bearer,
  // PIN, signature or gateway token. Stage + error code preserves safe triage.
  report.failure = { stage, type: error?.name || 'Error',
    ...(typeof error?.code === 'string' && /^[A-Z0-9_]+$/.test(error.code) ? { code: error.code } : {}),
    ...(typeof error?.status === 'number' ? { status: error.status } : {}),
    ...(Number.isInteger(child?.exitCode) ? { coordinatorExitCode: child.exitCode } : {}) };
  if (before) report.sourceSha256 = before;
  process.exitCode = 1;
} finally {
  await stopChild();
  transport?.destroy();
  await fs.rm(scratch, { recursive: true, force: true });
  report.finishedAtUtc = new Date().toISOString();
  await fs.mkdir(path.dirname(output), { recursive: true });
  await fs.writeFile(output, JSON.stringify(report, null, 2) + '\n');
  process.stdout.write(JSON.stringify(report, null, 2) + '\n');
}
