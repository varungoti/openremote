// Synthetic DOM/HTTP fixtures execute the built gate and production decoder.
// The agent denial case also executes unchanged production app.js. These are
// admission tests, not proof of a live desktop, relay, or browser rendering.
import { test, after } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, readFile, readdir, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import vm from 'node:vm';
import { buildViewer } from '../build.mjs';

const projectRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const sourceRoot = path.resolve(projectRoot, '../../server/internal/webclient/assets');
const scratch = await mkdtemp(path.join(tmpdir(), 'openremote-viewer-admission-'));
after(() => rm(scratch, { recursive: true, force: true }));
const outputRoot = path.join(scratch, 'dist'), publicRoot = path.join(scratch, 'public');
await buildViewer({ outputRoot, publicRoot });
const built = name => path.join(outputRoot, 'connect', name);
const html = await readFile(built('index.html'), 'utf8');
const { createViewerReadinessGate } = await import(pathToFileURL(built('boot.mjs')));
const origin = 'https://openremote.example';

// Mirrors the current Go /v1/info shape. This is public discovery only; it is
// deliberately not an auth/session response and grants no remote permission.
const currentInfo = () => ({ service: 'openremote', protocolVersion: 1, version: '0.8.0',
  phoneSetupVersion: 1, monitorSelection: true, pairingCodeVersion: 1, webController: true,
  activationVersion: 1, maxActivationConnections: 4, automaticHostRegistrationVersion: 1,
  relay: { configured: true, udp: true, tcp: true, tls: true }, relayOnly: true });
const response = (value, status = 200) => new Response(JSON.stringify(value), {
  status, headers: { 'Content-Type': 'application/json' },
});
const unavailable = () => response({ error: 'service_not_configured' }, 503);

class Events {
  listeners = [];
  addEventListener(type, callback, options) {
    this.listeners.push({ type, callback, capture: options === true || options?.capture === true });
  }
  removeEventListener(type, callback, options) {
    const capture = options === true || options?.capture === true;
    this.listeners = this.listeners.filter(item => item.type !== type || item.callback !== callback || item.capture !== capture);
  }
  async fire(type) {
    const event = { type, defaultPrevented: false, stopped: false,
      preventDefault() { this.defaultPrevented = true; },
      stopImmediatePropagation() { this.stopped = true; } };
    const listeners = this.listeners.filter(item => item.type === type).sort((a, b) => Number(b.capture) - Number(a.capture));
    for (const item of listeners) {
      await item.callback(event);
      if (event.stopped) break;
    }
    return event;
  }
}
class Element extends Events {
  constructor(id, document) {
    super(); Object.assign(this, { id, document, value: '', hidden: false, disabled: false,
      type: '', textContent: '', className: '', firstChild: { textContent: '' },
      videoWidth: 0, videoHeight: 0, srcObject: null });
  }
  focus() { this.document.activeElement = this; }
  requestVideoFrameCallback() { throw new Error('No frame is admitted in this fixture.'); }
}
function fixtureDocument() {
  const document = new Events(), elements = new Map();
  document.getElementById = id => elements.get(id);
  document.visibilityState = 'visible'; document.activeElement = null; document.fullscreenElement = null;
  for (const match of html.matchAll(/\bid="([^"]+)"/g)) elements.set(match[1], new Element(match[1], document));
  return document;
}
function harness(fetchResponse = unavailable, importController = async () => {}) {
  const document = fixtureDocument(), requests = [], imports = [];
  const gate = createViewerReadinessGate({ document, origin,
    fetcher: async (url, options) => { requests.push({ url, options }); return fetchResponse(url, options); },
    importController: async () => { imports.push('app.js'); return importController(document); } });
  return { gate, document, requests, imports, get: id => document.getElementById(id) };
}
function assertDiscoveryOnly(requests) {
  assert.ok(requests.length > 0);
  for (const { url, options } of requests) {
    assert.equal(url, origin + '/v1/info');
    assert.equal(options.method, 'GET');
    assert.equal(options.body, undefined);
    assert.equal(options.headers.Authorization, undefined);
    assert.equal(options.credentials, 'omit');
    assert.equal(options.cache, 'no-store');
    assert.equal(options.redirect, 'error');
    assert.equal(options.referrerPolicy, 'no-referrer');
  }
}
function assertLocked(h) {
  assert.equal(h.get('code').disabled, true);
  assert.equal(h.get('pin').disabled, true);
  assert.equal(h.get('pin').value, '');
  assert.equal(h.get('connect').type, 'button');
  assert.equal(h.get('cancel').hidden, true);
}
function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

async function timedGate() {
  const timers = new Map();
  let serial = 0;
  const context = vm.createContext({ TextEncoder, TextDecoder, URL, AbortController,
    setTimeout(callback, delay) { const id = ++serial; timers.set(id, { callback, delay }); return id; },
    clearTimeout(id) { timers.delete(id); } });
  const core = new vm.SourceTextModule(await readFile(built('core.mjs'), 'utf8'), { context, identifier: built('core.mjs') });
  const boot = new vm.SourceTextModule(await readFile(built('boot.mjs'), 'utf8'), { context, identifier: built('boot.mjs') });
  await boot.link(specifier => { assert.equal(specifier, './core.mjs'); return core; });
  await boot.evaluate();
  return { createGate: boot.namespace.createViewerReadinessGate, timers };
}

test('build preserves production controller bytes, disables entry before JS, and leaves public empty', async () => {
  for (const name of ['app.js', 'core.mjs', 'style.css']) {
    assert.deepEqual(await readFile(built(name)), await readFile(path.join(sourceRoot, name)));
  }
  assert.deepEqual(await readFile(built('boot.mjs')), await readFile(path.join(projectRoot, 'web/boot.mjs')));
  assert.deepEqual(await readdir(publicRoot), []);
  assert.match(html, /<script type="module" src="\/connect\/boot\.mjs"><\/script>/);
  assert.doesNotMatch(html, /<script[^>]*src="\/connect\/app\.js"/);
  for (const id of ['code', 'pin', 'connect']) assert.match(html, new RegExp('<(?:input|button)\\b[^>]*id="' + id + '"[^>]*\\bdisabled\\b'));
  assert.ok(html.indexOf('id="status"') < html.indexOf('id="pin"'));
  assert.match(html, /id="status"[^>]*>Checking the connection service…<\/div>/);
  assert.match(html, /OpenRemote · 0\.8/);
  assert.doesNotMatch(html, /One code\. One session\./);
  assert.match(html, /choose Activate Connection, set your PIN, then activate access/);
  assert.doesNotMatch(html, /<input[^>]*(?:server-url|endpoint|server-address)/i);
});

test('unbound 503 does not collect a PIN, load the controller, or submit an authentication request', async () => {
  const h = harness();
  h.get('pin').value = '123456';
  let submissions = 0;
  h.get('connect-form').addEventListener('submit', () => { submissions++; });
  assert.equal(await h.gate.check(), false);
  assertLocked(h);
  assert.equal(h.imports.length, 0);
  assert.equal(h.requests.length, 1);
  assertDiscoveryOnly(h.requests);
  assert.equal(h.get('status').hidden, false);
  assert.match(h.get('status').textContent, /Remote access is not available yet/);
  assert.match(h.get('connect').firstChild.textContent, /Retry connection check/);
  assert.equal(h.get('connect').disabled, false);
  assert.equal((await h.get('connect-form').fire('submit')).defaultPrevented, true);
  assert.equal(submissions, 0);
  assert.equal(h.requests.length, 1);
});

test('discovery must pass the production protocol decoder and current activation/registration/TLS-relay requirements', async t => {
  const invalid = [
    ['wrong service', info => { info.service = 'another'; }],
    ['wrong protocol', info => { info.protocolVersion = 2; }],
    ['browser controller disabled', info => { info.webController = false; }],
    ['old activation', info => { delete info.activationVersion; delete info.maxActivationConnections; }],
    ['wrong activation capacity', info => { info.maxActivationConnections = 5; }],
    ['automatic registration disabled', info => { info.automaticHostRegistrationVersion = 0; }],
    ['automatic registration missing', info => { delete info.automaticHostRegistrationVersion; }],
    ['automatic registration wrong type', info => { info.automaticHostRegistrationVersion = '1'; }],
    ['relay not configured', info => { info.relay.configured = false; }],
    ['TLS relay absent', info => { info.relay.tls = false; }],
    ['relay boolean mistyped', info => { info.relay.udp = 'true'; }],
    ['relay fields absent', info => { info.relay = { configured: true, tls: true }; }],
    ['relay absent', info => { delete info.relay; }],
  ];
  for (const [name, mutate] of invalid) await t.test(name, async () => {
    const info = currentInfo(); mutate(info);
    const h = harness(() => response(info));
    assert.equal(await h.gate.check(), false);
    assertLocked(h); assert.equal(h.imports.length, 0); assertDiscoveryOnly(h.requests);
  });
});

test('duplicate JSON, oversized discovery, and HTML error pages stay closed through the real decoder', async t => {
  const replies = [
    ['duplicate', () => new Response(JSON.stringify(currentInfo()).replace('"service":', '"service":"untrusted","service":'), { headers: { 'Content-Type': 'application/json' } })],
    ['oversized', () => response({ ...currentInfo(), padding: 'x'.repeat(16_384) })],
    ['HTML', () => new Response('<html>Sign in</html>', { headers: { 'Content-Type': 'text/html' } })],
  ];
  for (const [name, reply] of replies) await t.test(name, async () => {
    const h = harness(reply);
    assert.equal(await h.gate.check(), false);
    assertLocked(h); assert.equal(h.imports.length, 0); assertDiscoveryOnly(h.requests);
  });
});

test('Retry repeats only discovery and enables normal Connect only after controller loading finishes', async () => {
  const loaded = deferred(), enteredImport = deferred();
  let attempts = 0, productionSubmissions = 0;
  const h = harness(() => ++attempts === 1 ? unavailable() : response(currentInfo()), async document => {
    document.getElementById('connect-form').addEventListener('submit', () => { productionSubmissions++; });
    enteredImport.resolve();
    await loaded.promise;
  });
  assert.equal(await h.gate.check(), false);
  await h.get('connect').fire('click');
  const retry = h.gate.check();
  await enteredImport.promise;
  assertLocked(h);
  assert.equal(h.get('connect').disabled, true);
  await h.get('connect-form').fire('submit');
  assert.equal(productionSubmissions, 0);
  loaded.resolve();
  assert.equal(await retry, true);
  assert.equal(h.requests.length, 2); assertDiscoveryOnly(h.requests);
  assert.equal(h.imports.length, 1);
  assert.equal(h.get('code').disabled, false);
  assert.equal(h.get('pin').disabled, false);
  assert.equal(h.get('connect').disabled, false);
  assert.equal(h.get('connect').type, 'submit');
  assert.equal(h.get('connect').firstChild.textContent, 'Connect remote desktop ');
  assert.equal(h.get('status').hidden, true);
  await h.get('connect-form').fire('submit');
  assert.equal(productionSubmissions, 1);
  assert.equal(await h.gate.check(), true);
  assert.equal(h.requests.length, 2);
});

test('simultaneous readiness checks share one request and one controller import', async () => {
  const reply = deferred();
  const h = harness(() => reply.promise);
  const first = h.gate.check(), second = h.gate.check();
  assert.equal(first, second);
  assert.equal(h.requests.length, 1);
  assertLocked(h);
  reply.resolve(response(currentInfo()));
  assert.equal(await first, true);
  assert.equal(h.imports.length, 1);
});

test('a controller module failure keeps fields and submit disabled, including partially attached handlers', async () => {
  let submissions = 0;
  const h = harness(() => response(currentInfo()), async document => {
    document.getElementById('connect-form').addEventListener('submit', () => { submissions++; });
    throw new Error('private-module-error');
  });
  assert.equal(await h.gate.check(), false);
  assertLocked(h);
  assert.equal(h.get('connect').disabled, true);
  assert.equal(h.get('status').textContent, 'OpenRemote could not load. Refresh this page to try again.');
  await h.get('connect-form').fire('submit');
  assert.equal(submissions, 0);
  assert.equal(await h.gate.check(), false);
  assert.equal(h.requests.length, 1); assert.equal(h.imports.length, 1);
});

test('network exceptions never appear in public UI and never cause an automatic retry', async () => {
  const h = harness(() => { throw new Error('private-upstream-key: do-not-display'); });
  assert.equal(await h.gate.check(), false);
  assertLocked(h);
  assert.doesNotMatch(h.get('status').textContent, /private|upstream|do-not-display/);
  assert.equal(h.requests.length, 1); assert.equal(h.imports.length, 0);
});

test('discovery is aborted after ten seconds without authentication or automatic retry', async () => {
  const { createGate, timers } = await timedGate();
  const document = fixtureDocument(), requests = [];
  let imported = false;
  const gate = createGate({ document, origin,
    fetcher: (url, options) => {
      requests.push({ url, options });
      return new Promise((_, reject) => options.signal.addEventListener('abort', () => reject(options.signal.reason), { once: true }));
    },
    importController: async () => { imported = true; } });
  const checking = gate.check();
  assert.equal(timers.size, 1);
  const timer = [...timers.values()][0];
  assert.equal(timer.delay, 10_000);
  timer.callback();
  assert.equal(await checking, false);
  assert.equal(requests[0].options.signal.aborted, true);
  assert.equal(imported, false); assert.equal(timers.size, 0);
  assert.equal(requests.length, 1); assertDiscoveryOnly(requests);
  assertLocked({ get: id => document.getElementById(id) });
});

test('a stalled controller load stays closed after its deadline, even if its module finishes later', async () => {
  const { createGate, timers } = await timedGate();
  const document = fixtureDocument(), loading = deferred(), started = deferred();
  let submissions = 0;
  const gate = createGate({ document, origin, fetcher: async () => response(currentInfo()),
    importController: async () => {
      started.resolve(); await loading.promise;
      document.getElementById('connect-form').addEventListener('submit', () => { submissions++; });
    } });
  const checking = gate.check();
  await started.promise;
  assert.equal(timers.size, 1);
  const timer = [...timers.values()][0];
  assert.equal(timer.delay, 10_000);
  timer.callback();
  assert.equal(await checking, false);
  loading.resolve();
  await new Promise(resolve => setImmediate(resolve));
  assertLocked({ get: id => document.getElementById(id) });
  assert.equal(document.getElementById('connect').disabled, true);
  await document.getElementById('connect-form').fire('submit');
  assert.equal(submissions, 0);
  assert.equal(await gate.check(), false);
  assert.equal(timers.size, 0);
});

test('direct production agent API against an unbound service cannot authenticate, redeem a PIN, or control a desktop', async () => {
  const document = fixtureDocument(), requests = [];
  let keyGenerations = 0, peers = 0;
  const context = vm.createContext({ document, window: new Events(), location: { origin },
    isSecureContext: true, TextEncoder, TextDecoder, URL, AbortController, setTimeout, clearTimeout,
    setInterval, clearInterval, performance,
    crypto: { subtle: { generateKey() { keyGenerations++; throw new Error('Authentication must not start.'); } } },
    RTCPeerConnection: class { constructor() { peers++; throw new Error('Media must not start.'); } },
    fetch: async (url, options) => { requests.push({ url, options }); return unavailable(); } });
  const core = new vm.SourceTextModule(await readFile(built('core.mjs'), 'utf8'), { context, identifier: built('core.mjs') });
  const app = new vm.SourceTextModule(await readFile(built('app.js'), 'utf8'), { context, identifier: built('app.js') });
  await app.link(specifier => { assert.equal(specifier, './core.mjs'); return core; });
  await app.evaluate();
  const { agentAPI } = app.namespace;
  const request = { pcId: '012345678', pin: '123456', name: 'Admission test agent' };
  await assert.rejects(agentAPI.connect(request), /agent connection could not be established/);
  assert.equal(request.pin, '');
  assert.equal(requests.length, 1); assertDiscoveryOnly(requests);
  assert.equal(keyGenerations, 0); assert.equal(peers, 0);
  assert.equal(agentAPI.status().state, 'idle');
  assert.equal(agentAPI.status().canControl, false);
  assert.throws(() => agentAPI.screenshot(), /fresh remote desktop frame is required/);
  assert.throws(() => agentAPI.input({ frameToken: 'untrusted', type: 'text', text: 'never sent' }), /fresh remote desktop frame is required/);
  assert.equal(requests.length, 1);
});
