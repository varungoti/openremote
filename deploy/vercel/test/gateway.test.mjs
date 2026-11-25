import assert from 'node:assert/strict';
import test from 'node:test';
import { createGateway, parseCoordinatorOrigin, validGatewayToken, vercelClientIP, MAX_REQUEST_BYTES, MAX_RESPONSE_BYTES } from '../lib/gateway.mjs';

const ORIGIN = 'https://openremote.example';
const UPSTREAM = 'https://coordinator.example';
const ID = 'aBcd0123_efG4567-';
const TOKEN = 'test-only-gateway-token-not-a-production-secret';
const info = () => ({ service: 'openremote', protocolVersion: 1, version: '0.8.0',
  phoneSetupVersion: 1, monitorSelection: true, pairingCodeVersion: 1, webController: true,
  activationVersion: 1, maxActivationConnections: 4, automaticHostRegistrationVersion: 1,
  relay: { configured: true, udp: true, tcp: true, tls: true }, relayOnly: true });
function request(path, options = {}) { return new Request(ORIGIN + path, options); }
function json(value, status = 200, headers = {}) {
  return new Response(JSON.stringify(value), { status, headers: { 'content-type': 'application/json', ...headers } });
}
function connected(fetcher, options = {}) {
  return createGateway({ coordinatorOrigin: UPSTREAM, gatewayToken: TOKEN,
    clientIP: () => '198.51.100.42', fetcher, ...options });
}
function post(path, body = '{}', headers = {}) {
  return request(path, { method: 'POST', body, headers: { 'content-type': 'application/json', ...headers } });
}

test('unconfigured service refuses discovery, PIN redemption and registration without upstream traffic', async () => {
  const gateway = createGateway({ fetcher: () => assert.fail('must not contact any service') });
  for (const req of [request('/v1/info'), post('/v1/pairings/redeem', '{"pin":"654321"}'),
    post('/v1/hosts/register'), request('/healthz')]) {
    const response = await gateway(req);
    assert.equal(response.status, 503);
    assert.deepEqual(await response.json(), { error: 'service_not_configured' });
    assert.equal(response.headers.get('cache-control'), 'no-store');
  }
  const status = await (await gateway(request('/deployment-status'))).json();
  assert.equal(status.coordinatorConfigured, false);
  assert.equal(status.remoteAccessVerified, false);
  assert.equal('apiOrigin' in status, false);
});

test('only canonical HTTPS publisher origins are accepted; self-reference fails closed', async () => {
  for (const value of ['', 'http://coordinator.example', 'https://user:pass@coordinator.example',
    'https://coordinator.example/', 'https://coordinator.example/path', 'https://coordinator.example?x=1',
    'https://coordinator.example#fragment', 'https://localhost', 'https://127.0.0.1',
    'https://[::1]', 'https://COORDINATOR.example', 'https://coordinator.example:443']) {
    assert.equal(parseCoordinatorOrigin(value), null, value);
  }
  assert.equal(parseCoordinatorOrigin(UPSTREAM), UPSTREAM);
  const gateway = createGateway({ coordinatorOrigin: ORIGIN, fetcher: () => assert.fail('recursive proxy') });
  assert.equal((await gateway(request('/v1/info'))).status, 503);
});

test('upstream requires a valid gateway token and a platform-verified address', async () => {
  for (const value of ['', 'too-short', 'a'.repeat(513), 'a'.repeat(32) + ' ', 'a'.repeat(32) + '\n', 'é'.repeat(32)]) {
    assert.equal(validGatewayToken(value), false);
    const gateway = connected(() => assert.fail('missing gateway proof must not forward'), { gatewayToken: value });
    assert.equal((await gateway(request('/v1/info'))).status, 503);
    assert.equal((await (await gateway(request('/deployment-status'))).json()).coordinatorConfigured, false);
  }
  assert.equal(validGatewayToken(TOKEN), true);
  const gateway = connected(() => assert.fail('unverified client IP must not forward'), { clientIP: () => null });
  assert.deepEqual(await (await gateway(request('/v1/info'))).json(), { error: 'client_identity_unavailable' });
});

test('Vercel IP extraction is platform-specific, strict and canonical, with mapped IPv6 unmapped', () => {
  const req = ip => request('/v1/info', { headers: { 'x-vercel-forwarded-for': ip,
    'x-forwarded-for': '1.2.3.4', 'x-openremote-client-ip': '5.6.7.8' } });
  assert.equal(vercelClientIP(req('198.51.100.42'), false), null);
  assert.equal(vercelClientIP(request('/v1/info', { headers: { 'x-forwarded-for': '1.2.3.4' } }), true), null);
  for (const invalid of ['198.51.100.42, 1.2.3.4', 'not-an-IP', '127.000.000.001', 'fe80::1%eth0']) {
    assert.equal(vercelClientIP(req(invalid), true), null, invalid);
  }
  assert.equal(vercelClientIP(req('198.51.100.42'), true), '198.51.100.42');
  assert.equal(vercelClientIP(req('2001:DB8:0000:0:0:0:0:1'), true), '2001:db8::1');
  assert.equal(vercelClientIP(req('::FFFF:192.0.2.1'), true), '192.0.2.1');
  assert.equal(vercelClientIP(req('::ffff:c000:201'), true), '192.0.2.1');
});

test('viewer uses explicit files and GET/HEAD only with CSP and no query credentials', async () => {
  const names = [];
  const gateway = createGateway({ readAsset: async name => { names.push(name); return '<main>viewer</main>'; } });
  for (const [path, name] of [['/', 'index.html'], ['/connect', 'index.html'], ['/connect/', 'index.html'],
    ['/connect/app.js', 'app.js'], ['/connect/core.mjs', 'core.mjs'], ['/connect/boot.mjs', 'boot.mjs'],
    ['/connect/style.css', 'style.css']]) {
    const response = await gateway(request(path));
    assert.equal(response.status, 200);
    assert.equal(names.at(-1), name);
    assert.match(response.headers.get('content-security-policy'), /frame-ancestors 'none'/);
    assert.equal(response.headers.get('referrer-policy'), 'no-referrer');
    assert.equal(response.headers.get('vercel-cdn-cache-control'), 'no-store');
    assert.equal((await gateway(request(path, { method: 'HEAD' }))).body, null);
    assert.equal((await gateway(post(path))).status, 405);
    assert.equal((await gateway(request(path + '?pin=654321'))).status, 400);
  }
  for (const path of ['/connect/index.html', '/connect/missing.js', '/api/gateway', '/.env',
    '/v1/admin/hosts/' + ID + '/revoke', '/v1/devices/enroll', '/mcp', '/connect/%2fapp.js']) {
    assert.equal((await gateway(request(path))).status, 404, path);
  }
  assert.equal((await createGateway({ readAsset: async () => { throw Error('private path'); } })(request('/'))).status, 503);
  assert.equal((await gateway(new Request('http://openremote.example/connect'))).status, 426);
});

test('host, mobile, browser and agent public protocol routes reach the fixed coordinator', async () => {
  let calls = 0;
  const gateway = connected(async (url, options) => {
    calls++;
    assert.equal(new URL(url).origin, UPSTREAM);
    assert.equal(options.redirect, 'error');
    return json(new URL(url).pathname === '/v1/info' ? info() : { ok: true });
  });
  const routes = [
    ['GET', '/v1/info'], ['GET', '/healthz'], ['POST', '/v1/auth/challenge'], ['POST', '/v1/auth/verify'],
    ['POST', '/v1/hosts/register/challenge'], ['POST', '/v1/hosts/register'], ['GET', '/v1/devices/123456789'],
    ['POST', `/v1/hosts/${ID}/heartbeat`], ['GET', `/v1/hosts/${ID}/sessions`], ['GET', `/v1/hosts/${ID}/audit`],
    ['GET', `/v1/hosts/${ID}/events?after=0`], ['GET', `/v1/hosts/${ID}/pairing`], ['POST', `/v1/hosts/${ID}/pairing`],
    ['DELETE', `/v1/hosts/${ID}/pairing`], ['GET', `/v1/hosts/${ID}/trusted`], ['PUT', `/v1/hosts/${ID}/trusted`],
    ['DELETE', `/v1/hosts/${ID}/trusted`], ['POST', '/v1/pairings/redeem'], ['POST', '/v1/sessions'],
    ['GET', `/v1/sessions/${ID}`], ['GET', `/v1/sessions/${ID}/ice`], ['GET', `/v1/sessions/${ID}/signals?after=9223372036854775807`],
    ...['approve', 'reject', 'end', 'signal', 'connected'].map(action => ['POST', `/v1/sessions/${ID}/${action}`]),
  ];
  for (const [method, path] of routes) assert.equal((await gateway(request(path, { method }))).status, 200, path);
  assert.equal(calls, routes.length);
  const response = await gateway(post('/v1/info'));
  assert.equal(response.status, 405);
  assert.equal(response.headers.get('allow'), 'GET');
});

test('gateway discovery applies the same activation and relay declarations to native and direct agent clients', async () => {
  for (const value of [{ ok: true }, { ...info(), automaticHostRegistrationVersion: 0 },
    { ...info(), relay: { configured: true, udp: true, tcp: true, tls: false } },
    { ...info(), activationVersion: 0, maxActivationConnections: 0 }]) {
    const response = await connected(async () => json(value))(request('/v1/info'));
    assert.equal(response.status, 503);
    assert.deepEqual(await response.json(), { error: 'service_not_ready' });
  }
  const raw = JSON.stringify(info());
  const valid = await connected(async () => new Response(raw, { headers: { 'content-type': 'application/json' } }))(request('/v1/info'));
  assert.equal(valid.status, 200);
  assert.equal(await valid.text(), raw); // strict Android schema receives no gateway fields
  const duplicate = raw.replace('"service":"openremote"', '"service":"openremote","service":"openremote"');
  const invalid = await connected(async () => new Response(duplicate, { headers: { 'content-type': 'application/json' } }))(request('/v1/info'));
  assert.equal(invalid.status, 503);
});

test('query and path injection cannot select an upstream or smuggle credentials', async () => {
  const gateway = connected(() => assert.fail('must not forward invalid route or query'));
  for (const path of ['/v1/info?pin=123456', '/v1/info?upstream=https://evil.example',
    `/v1/hosts/${ID}/events?after=0&after=1`, `/v1/hosts/${ID}/events?after=-1`,
    `/v1/hosts/${ID}/events?after=9223372036854775808`, `/v1/hosts/${ID}/events?after=01`,
    `/v1/hosts/${ID}/events?after=`, `/v1/sessions/${ID}/signals?after=0&pin=123456`]) {
    assert.equal((await gateway(request(path))).status, 400, path);
  }
  assert.equal((await gateway(request('/v1/hosts/short/events'))).status, 404);
  assert.equal((await gateway(request('/v1/info%2fother'))).status, 404);
});

test('raw JSON and bearer preserved; cookies, forwarding, origin, host and response secrets are stripped', async () => {
  const raw = '{"name":"first","name":"duplicate","pin":"654321"}';
  let calls = 0;
  const gateway = connected(async (url, options) => {
    calls++;
    assert.equal(url, UPSTREAM + '/v1/pairings/redeem');
    assert.equal(new TextDecoder().decode(options.body), raw);
    assert.equal(options.headers.get('authorization'), 'Bearer fixture-only');
    assert.deepEqual([...options.headers.keys()].sort(), ['accept', 'authorization', 'content-type',
      'x-openremote-client-ip', 'x-openremote-gateway-token']);
    assert.equal(options.headers.get('x-openremote-gateway-token'), TOKEN);
    assert.equal(options.headers.get('x-openremote-client-ip'), '198.51.100.42');
    assert.equal(options.credentials, 'omit');
    return json({ error: 'pairing_invalid' }, 429, {
      'set-cookie': 'secret=value', 'x-forwarded-host': 'private.example', 'retry-after': '60',
      'access-control-allow-origin': '*', 'cache-control': 'public, max-age=3600',
    });
  });
  const response = await gateway(post('/v1/pairings/redeem', raw, {
    authorization: 'Bearer fixture-only', cookie: 'session=private', origin: 'https://evil.example',
    host: 'evil.example', 'x-forwarded-for': '127.0.0.1', 'x-forwarded-proto': 'http',
    forwarded: 'for=127.0.0.1;proto=http', 'x-openremote-upstream': 'https://evil.example',
    'x-openremote-gateway-token': 'attacker-value', 'x-openremote-client-ip': '127.0.0.1',
  }));
  assert.equal(calls, 1);
  assert.equal(response.status, 429);
  assert.deepEqual(await response.json(), { error: 'pairing_invalid' });
  assert.equal(response.headers.get('retry-after'), '60');
  assert.equal(response.headers.get('cache-control'), 'no-store');
  for (const name of ['set-cookie', 'x-forwarded-host', 'access-control-allow-origin']) assert.equal(response.headers.get(name), null);
});

test('body limit covers content length and streamed payload without trusting declarations', async () => {
  const gateway = connected(() => assert.fail('oversize body must not forward'));
  assert.equal((await gateway(post('/v1/pairings/redeem', '{}', { 'content-length': String(MAX_REQUEST_BYTES + 1) }))).status, 413);
  assert.equal((await gateway(post('/v1/pairings/redeem', 'x'.repeat(MAX_REQUEST_BYTES + 1), { 'content-length': '1' }))).status, 413);
  assert.equal((await gateway(post('/v1/pairings/redeem', '{}', { 'content-type': 'text/plain' }))).status, 415);
  assert.equal((await gateway(post('/v1/pairings/redeem', '{}', { authorization: 'x'.repeat(8193) }))).status, 431);
});

test('upstream redirects, HTML and oversized responses fail without secret disclosure', async () => {
  for (const response of [new Response('redirect-secret', { status: 302, headers: { location: 'https://evil.example' } }),
    new Response('<h1>private infrastructure detail</h1>', { headers: { 'content-type': 'text/html' } }),
    new Response('x'.repeat(MAX_RESPONSE_BYTES + 1), { headers: { 'content-type': 'application/json' } })]) {
    const result = await connected(async () => response)(request('/v1/info'));
    assert.equal(result.status, 502);
    assert.doesNotMatch(await result.text(), /secret|private|evil/);
  }
});

test('failed or timed-out PIN submission is attempted exactly once with sanitized error', async () => {
  let calls = 0;
  const failed = connected(async () => { calls++; throw Error('secret-pin-654321 private.example'); });
  const response = await failed(post('/v1/pairings/redeem'));
  assert.equal(response.status, 502);
  assert.deepEqual(await response.json(), { error: 'coordinator_unavailable' });
  assert.equal(calls, 1);
  const timer = setTimeout(() => {}, 1000); // keep test alive for AbortSignal.timeout's unref timer
  try {
    const slow = connected(async (_url, { signal }) => {
      calls++;
      await new Promise((_resolve, reject) => signal.addEventListener('abort', () => reject(signal.reason), { once: true }));
    }, { timeoutMs: 15 });
    const result = await slow(post('/v1/pairings/redeem'));
    assert.equal(result.status, 504);
    assert.deepEqual(await result.json(), { error: 'coordinator_timeout' });
    assert.equal(calls, 2);
  } finally { clearTimeout(timer); }
});

test('client cancellation aborts the upstream long poll', async () => {
  const controller = new AbortController();
  let receivedSignal;
  const gateway = connected(async (_url, { signal }) => {
    receivedSignal = signal;
    controller.abort();
    signal.throwIfAborted();
  });
  const response = await gateway(request(`/v1/sessions/${ID}/signals?after=0`, { signal: controller.signal }));
  assert.equal(response.status, 499);
  assert.equal(receivedSignal.aborted, true);
});
