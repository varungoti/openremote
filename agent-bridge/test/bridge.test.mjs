import test from 'node:test';
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { DesktopBridge, sanitizeStatus, validateFrame } from '../src/bridge.mjs';
import { parseServiceProfile } from '../src/profile.mjs';
import { publicError } from '../src/errors.mjs';
import { createBrowserAdapter } from '../src/browser.mjs';

const png = 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+j0V8AAAAASUVORK5CYII=';
const args = () => ({ pcId: '123456789', pin: '001234', name: 'Test agent' });
const tick = () => new Promise(resolve => setImmediate(resolve));
function deferred() {
  let resolve;
  const promise = new Promise(r => { resolve = r; });
  return { promise, resolve };
}
function mockAdapter({ canControl = true, stopConfirmed = true } = {}) {
  const adapter = {
    calls: [], closed: 0, canControl,
    async invoke(method, value) {
      this.calls.push({ method, value });
      if (method === 'connect' || method === 'status') return { state: this.canControl ? 'connected' : 'view_only', pcId: '123456789', hostName: 'Test PC', canControl: this.canControl };
      if (method === 'screenshot') return { mimeType: 'image/png', data: png, width: 1, height: 1, frameToken: randomUUID() };
      if (method === 'input') return { ok: true };
      throw new Error('unsupported test method');
    },
    async close() { this.closed++; return { stopConfirmed }; }
  };
  return adapter;
}
function bridgeFor(adapter, options = {}) {
  return new DesktopBridge({ origin: 'https://service.example', createAdapter: async () => adapter, ...options });
}

test('publisher profile is fixed HTTPS and never accepts a controller-supplied endpoint', () => {
  const profile = origin => JSON.stringify({ schemaVersion: 1, serviceName: 'OpenRemote', apiOrigin: origin });
  assert.equal(parseServiceProfile(profile('')), null);
  assert.equal(parseServiceProfile(profile('https://service.example')), 'https://service.example');
  for (const origin of ['http://service.example', 'https://service.example/', 'https://service.example/path', 'https://name:secret@service.example', 'https://service.example?x=1']) {
    assert.throws(() => parseServiceProfile(profile(origin)), { code: 'invalid_service_profile' });
  }
  assert.throws(() => parseServiceProfile('x'.repeat(16_385)), { code: 'invalid_service_profile' });
  assert.throws(() => parseServiceProfile(JSON.stringify({ schemaVersion: 1, serviceName: 'OpenRemote', apiOrigin: '', pin: '001234' })), { code: 'invalid_service_profile' });
});

test('unconfigured product still reports status and fails before browser launch', async () => {
  const bridge = new DesktopBridge({ origin: null, createAdapter: async () => assert.fail('browser must not launch') });
  assert.equal((await bridge.status()).state, 'idle');
  await assert.rejects(bridge.connect(args()), { code: 'service_not_configured' });
  await bridge.close();
});

test('browser debugging is refused before loading a library that could log PIN arguments', async () => {
  const previous = process.env.DEBUG;
  try {
    process.env.DEBUG = 'pw:protocol';
    await assert.rejects(createBrowserAdapter({ origin: 'https://service.example' }), { code: 'debug_logging_disabled' });
  } finally {
    if (previous === undefined) delete process.env.DEBUG;
    else process.env.DEBUG = previous;
  }
});

test('bounded status and PNG outputs discard arbitrary browser fields', () => {
  assert.deepEqual(sanitizeStatus({ state: 'idle', canControl: false }), { state: 'idle', pcId: '', hostName: '', canControl: false });
  assert.deepEqual(sanitizeStatus({ state: 'connected', pcId: '123456789', hostName: 'Test\u202e PC\n', canControl: true, token: 'secret' }), { state: 'connected', pcId: '123456789', hostName: 'Test PC', canControl: true });
  assert.throws(() => sanitizeStatus({ state: 'view_only', pcId: '123456789', hostName: 'PC', canControl: true }), { code: 'invalid_status' });
  const frame = { mimeType: 'image/png', data: png, width: 1, height: 1, frameToken: randomUUID() };
  assert.deepEqual(validateFrame({ ...frame, token: 'secret' }), frame);
  for (const patch of [{ width: 2 }, { width: 4097 }, { height: 8_300_001 }, { data: 'not png' }, { data: 'A'.repeat(24_000_000) }, { mimeType: 'text/html' }, { frameToken: 'short' }]) {
    assert.throws(() => validateFrame({ ...frame, ...patch }), { code: 'invalid_frame' });
  }
  assert.equal(JSON.stringify(publicError(new Error('PIN 001234 and bearer secret'))).includes('001234'), false);
});

test('connect waits for media, clears PIN reference, and each screenshot permits only one bounded action', async () => {
  const adapter = mockAdapter();
  const bridge = bridgeFor(adapter);
  const credentials = args();
  assert.equal((await bridge.connect(credentials)).canControl, true);
  assert.equal(credentials.pin, '');
  const frame = await bridge.screenshot();
  await assert.rejects(bridge.input({ type: 'click', frameToken: frame.frameToken, x: 1, y: 0 }), { code: 'invalid_coordinates' });
  const firstInput = bridge.input({ type: 'click', frameToken: frame.frameToken, x: 0, y: 0 });
  const replayRejected = assert.rejects(bridge.input({ type: 'click', frameToken: frame.frameToken, x: 0, y: 0 }), { code: 'fresh_frame_required' });
  assert.deepEqual(await firstInput, { ok: true });
  await replayRejected;
  assert.equal(adapter.calls.filter(v => v.method === 'input').length, 1);
  const result = await bridge.disconnect();
  assert.equal(result.stopConfirmed, true);
  assert.equal(adapter.closed, 1);
});

test('view-only screenshots work but input never reaches browser API', async () => {
  const adapter = mockAdapter({ canControl: false });
  const bridge = bridgeFor(adapter);
  assert.equal((await bridge.connect(args())).state, 'view_only');
  const frame = await bridge.screenshot();
  await assert.rejects(bridge.input({ type: 'key', frameToken: frame.frameToken, codes: ['Enter'] }), { code: 'view_only' });
  assert.equal(adapter.calls.some(v => v.method === 'input'), false);
  await bridge.close();
});

test('stale and cross-generation screenshot tokens cannot authorize input', async () => {
  let now = 1000;
  const adapters = [mockAdapter(), mockAdapter()];
  let index = 0;
  const bridge = new DesktopBridge({ origin: 'https://service.example', createAdapter: async () => adapters[index++], now: () => now });
  await bridge.connect(args());
  const old = await bridge.screenshot();
  now += 15_001;
  await assert.rejects(bridge.input({ type: 'click', frameToken: old.frameToken, x: 0, y: 0 }), { code: 'fresh_frame_required' });
  const current = await bridge.screenshot();
  await bridge.connect(args());
  await assert.rejects(bridge.input({ type: 'click', frameToken: current.frameToken, x: 0, y: 0 }), { code: 'fresh_frame_required' });
  assert.equal(adapters[0].closed, 1);
  await bridge.close();
});

test('disconnect cancels connection setup and closes a browser that resolves late', async () => {
  const ready = deferred();
  const adapter = mockAdapter();
  const bridge = new DesktopBridge({ origin: 'https://service.example', createAdapter: () => ready.promise });
  const connecting = bridge.connect(args());
  await tick();
  const rejected = assert.rejects(connecting, { code: 'operation_cancelled' });
  await bridge.disconnect();
  ready.resolve(adapter);
  await rejected;
  await tick();
  assert.equal(adapter.closed, 1);
  assert.equal((await bridge.status()).state, 'idle');
});

test('request cancellation closes the session and discards queued mutations', async () => {
  const adapter = mockAdapter();
  const original = adapter.invoke.bind(adapter);
  const waiting = deferred();
  adapter.invoke = async function (method, value) {
    if (method === 'input') { this.calls.push({ method, value }); await waiting.promise; return { ok: true }; }
    return original(method, value);
  };
  const bridge = bridgeFor(adapter);
  await bridge.connect(args());
  const frame = await bridge.screenshot();
  const abort = new AbortController();
  const input = bridge.input({ type: 'click', frameToken: frame.frameToken, x: 0, y: 0 }, abort.signal);
  const queued = bridge.status();
  const rejected = assert.rejects(input, { code: 'operation_cancelled' });
  const discarded = assert.rejects(queued, { code: 'operation_cancelled' });
  await tick();
  abort.abort('sensitive cancellation reason must not be logged');
  await Promise.all([rejected, discarded]);
  waiting.resolve();
  assert.equal(adapter.closed, 1);
  assert.equal((await bridge.status()).state, 'idle');
});

test('idle expiry and operation timeout close resources without claiming remote end acknowledgment', async () => {
  const adapter = mockAdapter({ stopConfirmed: false });
  const bridge = bridgeFor(adapter, { idleMs: 20 });
  await bridge.connect(args());
  await new Promise(resolve => setTimeout(resolve, 50));
  assert.equal(adapter.closed, 1);
  assert.equal((await bridge.status()).state, 'idle');
  const slow = mockAdapter({ stopConfirmed: false });
  const slowBridge = bridgeFor(slow, { operationMs: 20 });
  await slowBridge.connect(args());
  slow.invoke = () => new Promise(() => {});
  await assert.rejects(slowBridge.status(), { code: 'operation_timed_out' });
  assert.equal(slow.closed, 1);
  const unconfirmed = bridgeFor(mockAdapter({ stopConfirmed: false }));
  await unconfirmed.connect(args());
  assert.equal((await unconfirmed.disconnect()).stopConfirmed, false);
});

test('a cancelled screenshot cannot overwrite a newer connection observation', async () => {
  const old = mockAdapter();
  const current = mockAdapter();
  const original = old.invoke.bind(old);
  const pendingFrame = deferred();
  old.invoke = async (method, value) => method === 'screenshot' ? pendingFrame.promise : original(method, value);
  let index = 0;
  const bridge = new DesktopBridge({ origin: 'https://service.example', createAdapter: async () => [old, current][index++] });
  await bridge.connect(args());
  const capture = bridge.screenshot();
  const cancelled = assert.rejects(capture, { code: 'operation_cancelled' });
  await tick();
  await bridge.connect(args());
  await cancelled;
  const currentFrame = await bridge.screenshot();
  pendingFrame.resolve({ mimeType: 'image/png', data: png, width: 1, height: 1, frameToken: randomUUID() });
  await tick();
  assert.equal(bridge.frame.token, currentFrame.frameToken);
  await bridge.input({ type: 'click', frameToken: currentFrame.frameToken, x: 0, y: 0 });
  assert.equal(current.calls.filter(v => v.method === 'input').length, 1);
  assert.equal(old.calls.filter(v => v.method === 'input').length, 0);
  await bridge.close();
});

test('the serialized operation queue is bounded and disconnect discards every queued operation', async () => {
  const adapter = mockAdapter();
  const bridge = bridgeFor(adapter);
  await bridge.connect(args());
  adapter.invoke = () => new Promise(() => {});
  const pending = Array.from({ length: 16 }, () => bridge.status().catch(error => error));
  await assert.rejects(bridge.status(), { code: 'bridge_busy' });
  await bridge.disconnect();
  const outcomes = await Promise.all(pending);
  assert.ok(outcomes.every(error => error.code === 'operation_cancelled'));
  assert.equal(bridge.pending, 0);
  assert.equal(adapter.closed, 1);
});
