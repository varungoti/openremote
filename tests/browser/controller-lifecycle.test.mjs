// Execute the unmodified production modules with explicit DOM/RTC/HTTP doubles.
// This verifies lifecycle state and control gating, not browser rendering or media.
// Run: node --experimental-vm-modules --test tests/browser/controller-lifecycle.test.mjs
import { test, after } from 'node:test';
import assert from 'node:assert/strict';
import vm from 'node:vm';
import fs from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash, webcrypto } from 'node:crypto';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const assets = path.join(root, 'server/internal/webclient/assets');
const sourcePaths = ['app.js', 'core.mjs', 'index.html'].map(name => path.join(assets, name));
sourcePaths.push(fileURLToPath(import.meta.url));
const sources = new Map(await Promise.all(sourcePaths.map(async p => [p, await fs.readFile(p, 'utf8')])));
const hash = value => createHash('sha256').update(value).digest('hex');
const beforeHashes = Object.fromEntries([...sources].map(([p, value]) => [path.relative(root, p), hash(value)]));
const outcomes = [];
const realTurn = () => new Promise(resolve => setImmediate(resolve));
async function settle() { for (let i = 0; i < 8; i++) await realTurn(); }
async function until(predicate, label) {
  const deadline = Date.now() + 3000;
  while (!predicate()) {
    if (Date.now() > deadline) throw new Error('Timed out waiting for ' + label);
    await realTurn();
  }
  await settle();
}
function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
function abortable(promise, signal) {
  if (!signal) return promise;
  if (signal.aborted) return Promise.reject(signal.reason);
  return new Promise((resolve, reject) => {
    const abort = () => { cleanup(); reject(signal.reason); };
    const cleanup = () => signal.removeEventListener('abort', abort);
    signal.addEventListener('abort', abort, { once: true });
    promise.then(value => { cleanup(); resolve(value); }, error => { cleanup(); reject(error); });
  });
}

class Events {
  listeners = new Map();
  addEventListener(type, callback) {
    if (!this.listeners.has(type)) this.listeners.set(type, new Set());
    this.listeners.get(type).add(callback);
  }
  removeEventListener(type, callback) { this.listeners.get(type)?.delete(callback); }
  fire(type, detail = {}) {
    const event = { type, defaultPrevented: false, preventDefault() { this.defaultPrevented = true; }, ...detail };
    const results = [...(this.listeners.get(type) || [])].map(fn => fn(event));
    return Promise.all(results);
  }
}

class Clock {
  now = 10_000;
  serial = 0;
  jobs = new Map();
  set = (fn, ms = 0, repeat = false) => {
    const id = ++this.serial;
    this.jobs.set(id, { fn, due: this.now + ms, repeat, ms });
    return id;
  };
  clear = id => { this.jobs.delete(id); };
  async advance(ms) {
    const target = this.now + ms;
    for (;;) {
      const candidate = [...this.jobs].filter(([, job]) => job.due <= target).sort((a, b) => a[1].due - b[1].due)[0];
      if (!candidate) break;
      const [id, job] = candidate;
      this.now = job.due;
      if (job.repeat) job.due += job.ms; else this.jobs.delete(id);
      job.fn();
      await settle();
    }
    this.now = target;
    await settle();
  }
}

class Element extends Events {
  constructor(id, doc) {
    super(); this.id = id; this.doc = doc; this.value = ''; this.textContent = ''; this.className = '';
    this.disabled = false; this.hidden = ['session-panel', 'status', 'cancel'].includes(id);
    this.firstChild = { textContent: '' }; this.srcObject = null; this.videoWidth = 0; this.videoHeight = 0;
    this.paused = true; this.frames = new Map(); this.frameSerial = 0;
  }
  focus() {
    if (this.doc.activeElement === this) return;
    const previous = this.doc.activeElement; this.doc.activeElement = this;
    void previous?.fire('blur');
  }
  play() { this.paused = false; return Promise.resolve(); }
  requestVideoFrameCallback(fn) { const id = ++this.frameSerial; this.frames.set(id, fn); return id; }
  cancelVideoFrameCallback(id) { this.frames.delete(id); }
  frame(width = 640, height = 360) {
    this.videoWidth = width; this.videoHeight = height;
    const callbacks = [...this.frames.values()]; this.frames.clear();
    for (const callback of callbacks) callback(0, { width, height, presentedFrames: 1 });
  }
  setPointerCapture(id) { this.pointerId = id; }
  getBoundingClientRect() { return { left: 0, top: 0, width: 640, height: 360 }; }
  requestFullscreen() { this.doc.fullscreenElement = this; return Promise.resolve(); }
}

const granted = () => ({ viewScreen: true, controlInput: true, clipboard: false, fileTransfer: false, audio: false, power: false });
const hostKey = Buffer.alloc(32, 8).toString('base64');
const capabilities = { v: 1, type: 'capabilities', text: true, clipboard: false, fileTransfer: false, multiMonitor: true, audio: false, power: false };
const catalog = { v: 1, type: 'display_catalog', selectionMode: 'reconnect', selectedMonitorId: 'a'.repeat(64),
  monitors: [{ id: 'a'.repeat(64), name: 'Primary', width: 640, height: 360, primary: true }] };

async function harness(options = {}) {
  const document = new Events(), window = new Events(), elements = new Map(), clock = new Clock();
  document.visibilityState = 'visible'; document.activeElement = null; document.fullscreenElement = null;
  document.getElementById = id => elements.get(id);
  // This canvas is an explicit test double. It checks capture ownership and
  // dimensions, never claims that a browser decoded or encoded real pixels.
  document.createElement = tag => {
    assert.equal(tag, 'canvas');
    return { width: 0, height: 0, getContext: () => ({ drawImage: (source, x, y, width, height) => {
      assert.equal(source, elements.get('desktop')); assert.equal(x, 0); assert.equal(y, 0);
      assert.equal(width, source.videoWidth); assert.equal(height, source.videoHeight);
    } }), toDataURL: () => 'data:image/png;base64,' + Buffer.from('explicit-canvas-test-double').toString('base64') };
  };
  document.exitFullscreen = () => { document.fullscreenElement = null; return Promise.resolve(); };
  for (const match of sources.get(path.join(assets, 'index.html')).matchAll(/\bid="([^"]+)"/g))
    elements.set(match[1], new Element(match[1], document));
  const el = id => { const value = elements.get(id); assert(value, 'DOM fixture has ' + id); return value; };
  const calls = [], peers = [], mutations = [];
  let publicKey = null, nextSession = 0, responseSession = null, snapshotCount = 0;
  let snapshot = options.status || 'pending';
  const pendingRequests = [];
  const never = signal => {
    const pending = deferred(); pendingRequests.push(pending);
    return abortable(pending.promise, signal);
  };
  function newSession() {
    const index = ++nextSession;
    responseSession = { id: String(index).padStart(24, 's'), status: 'pending', pairingId: 'p'.repeat(24),
      hostDisplayId: '123456789', hostName: 'Owner PC', hostPublicKey: hostKey, controllerPublicKey: publicKey,
      permissions: { ...granted(), controlInput: !options.viewOnly }, expiresAt: new Date(Date.now() + 3_600_000).toISOString() };
    if (options.activation) responseSession.mode = 'activation';
    return { session: structuredClone(responseSession), controllerToken: 't'.repeat(43) };
  }
  const fetcher = async (url, init = {}) => {
    const path = new URL(url).pathname;
    const body = init.body ? JSON.parse(init.body) : undefined;
    const call = { path, method: init.method, body, signal: init.signal, keepalive: init.keepalive };
    calls.push(call);
    let result;
    if (path === '/v1/info') {
      result = options.info ? await abortable(options.info.promise, init.signal) :
        { service: 'openremote', protocolVersion: 1, pairingCodeVersion: 1, webController: true,
          ...(options.activation ? { activationVersion: 1, maxActivationConnections: 4 } : {}) };
    } else if (path === '/v1/auth/challenge') {
      publicKey = body.publicKey;
      result = { challengeId: 'c'.repeat(24), challenge: Buffer.alloc(32, 5).toString('base64'), expiresAt: new Date(Date.now() + 60_000).toISOString() };
    } else if (path === '/v1/auth/verify') {
      if (options.verify) await abortable(options.verify.promise, init.signal);
      result = { publicKey, accessToken: 'a'.repeat(43), expiresAt: new Date(Date.now() + 60_000).toISOString() };
    } else if (path === '/v1/pairings/redeem') {
      if (options.redeem) {
        const outcome = await abortable(options.redeem.promise, init.signal);
        result = outcome === undefined ? newSession() : outcome;
      } else result = newSession();
      if (options.mutateGrant) options.mutateGrant(result);
      if (options.rejectRedeem) throw new Error('Network failure with SYNTHETIC_PRIVATE_DETAIL');
    } else if (path.endsWith('/end')) {
      mutations.push('end');
      if (options.end) await abortable(options.end.promise, init.signal);
      result = { ok: true };
    } else if (path.endsWith('/ice')) {
      result = { iceServers: [], iceTransportPolicy: 'all' };
    } else if (path.endsWith('/connected')) {
      mutations.push('connected'); result = { ok: true };
    } else if (path.endsWith('/signal')) {
      mutations.push('signal'); result = { seq: 1 };
    } else if (path.endsWith('/signals')) return never(init.signal);
    else if (path.startsWith('/v1/sessions/')) {
      snapshotCount++;
      if (options.pendingSnapshots) return never(init.signal);
      result = { session: { ...structuredClone(responseSession), status: snapshot } };
      if (options.mutateSnapshot) options.mutateSnapshot(result, snapshotCount);
    } else throw new Error('Unexpected route: ' + path);
    return new Response(JSON.stringify(result), { headers: { 'Content-Type': 'application/json' } });
  };
  class FakeChannel {
    readyState = 'connecting'; bufferedAmount = 0; messages = [];
    send(text) { assert.equal(this.readyState, 'open'); this.messages.push(JSON.parse(text)); }
    open() { this.readyState = 'open'; this.onopen?.(); }
    receive(value) { this.onmessage?.({ data: JSON.stringify(value) }); }
    close() { this.readyState = 'closed'; this.onclose?.(); }
  }
  class FakePeer extends Events {
    constructor(config) { super(); this.config = config; this.connectionState = 'new'; this.iceGatheringState = 'complete'; this.signalingState = 'stable'; this.closeCount = 0; peers.push(this); }
    addTransceiver(kind, settings) { assert.equal(kind, 'video'); assert.equal(settings.direction, 'recvonly'); return { setCodecPreferences(codecs) { assert(codecs.length); } }; }
    createDataChannel(name, settings) { assert.equal(name, 'control'); assert.equal(settings.ordered, true); this.dc = new FakeChannel(); return this.dc; }
    async createOffer() { return { type: 'offer', sdp: 'v=0\r\nsynthetic-test-offer' }; }
    async setLocalDescription(value) { this.localDescription = value; this.signalingState = 'have-local-offer'; }
    async setRemoteDescription(value) { this.remoteDescription = value; this.signalingState = 'stable'; }
    async addIceCandidate() {}
    connect() { this.connectionState = 'connected'; this.onconnectionstatechange?.(); }
    state(value) { this.connectionState = value; this.onconnectionstatechange?.(); }
    track() { this.ontrack?.({ track: { kind: 'video', stop() {} } }); }
    close() { this.closeCount++; this.connectionState = 'closed'; this.dc.readyState = 'closed'; this.onconnectionstatechange?.(); }
  }
  const context = vm.createContext({
    document, window, location: { origin: 'https://remote.company.tld', host: 'remote.company.tld' },
    isSecureContext: true, crypto: webcrypto, fetch: fetcher, URL, TextEncoder, TextDecoder, AbortController,
    Response, btoa, atob, console, RTCPeerConnection: FakePeer,
    RTCRtpReceiver: { getCapabilities: () => ({ codecs: [{ mimeType: 'video/H264', clockRate: 90_000 }] }) },
    MediaStream: class { constructor(tracks) { this.tracks = tracks; } }, performance: { now: () => clock.now },
    setTimeout: (fn, ms) => clock.set(fn, ms), clearTimeout: clock.clear,
    setInterval: (fn, ms) => clock.set(fn, ms, true), clearInterval: clock.clear,
  });
  const core = new vm.SourceTextModule(sources.get(path.join(assets, 'core.mjs')), { context, identifier: 'core.mjs' });
  const app = new vm.SourceTextModule(sources.get(path.join(assets, 'app.js')), { context, identifier: 'app.js' });
  await app.link(specifier => { assert.equal(specifier, './core.mjs'); return core; });
  await app.evaluate();
  const fill = () => { el('code').value = '000001'; el('pin').value = '001234'; };
  return {
    el, calls, peers, clock, document, window, mutations, fill, agentAPI: app.namespace.agentAPI,
    submit() { fill(); return el('connect-form').fire('submit'); },
    setSnapshot(value) { snapshot = value; },
    async ready() {
      await this.submit(); await until(() => peers.length === 1 && calls.some(c => c.path.endsWith('/signal')), 'approved peer');
      const pc = peers[0]; pc.dc.open(); pc.connect(); pc.track(); await settle(); return pc;
    },
    async dispose() {
      options.end?.resolve(); options.redeem?.resolve(); options.verify?.resolve(); options.info?.resolve();
      await window.fire('pagehide'); await settle(); clock.jobs.clear();
      for (const pending of pendingRequests) pending.resolve(new Response('{"signals":[],"cursor":0}', { headers: { 'Content-Type': 'application/json' } }));
      await settle();
    },
  };
}

function scenario(name, body) {
  test(name, async () => {
    const entry = { name, ok: false }; outcomes.push(entry);
    try { await body(); entry.ok = true; }
    catch (error) { entry.error = { name: error.name, message: error.message.slice(0, 1200) }; throw error; }
  });
}

scenario('cancellation during discovery aborts before authentication or redemption', async () => {
  const info = deferred(), h = await harness({ info });
  try {
    const submitting = h.submit(); await until(() => h.calls.length === 1, 'discovery');
    assert.equal(h.el('pin').value, '');
    assert.equal(h.el('connect').hidden, true, 'Only Cancel is visible during connection');
    assert.equal(h.el('cancel').hidden, false);
    await h.el('cancel').fire('click'); await submitting;
    assert.equal(h.el('connect').hidden, false);
    assert.equal(h.el('cancel').hidden, true, 'Only Connect is visible after cancellation');
    assert.equal(h.calls.length, 1); assert(h.calls[0].signal.aborted);
    assert.equal(h.el('connect').disabled, false); assert.equal(h.el('session-panel').hidden, true);
  } finally { await h.dispose(); }
});

scenario('cancellation during proof verification never sends the code or PIN', async () => {
  const verify = deferred(), h = await harness({ verify });
  try {
    const submitting = h.submit(); await until(() => h.calls.some(c => c.path.endsWith('/verify')), 'verification');
    await h.el('cancel').fire('click'); await submitting;
    assert.equal(h.calls.filter(c => c.path === '/v1/pairings/redeem').length, 0);
    assert.equal(h.el('pin').value, ''); assert.equal(h.el('connect').disabled, false);
  } finally { await h.dispose(); }
});

scenario('a canceled one-use POST settles only to close its known late session', async () => {
  const redeem = deferred(), h = await harness({ redeem });
  try {
    const submitting = h.submit(); await until(() => h.calls.some(c => c.path.endsWith('/redeem')), 'redemption');
    assert.equal(h.el('pin').value, ''); await h.el('cancel').fire('click');
    assert.equal(h.el('connect').disabled, true); assert.equal(h.el('cancel').disabled, true);
    await h.el('connect-form').fire('submit');
    assert.equal(h.calls.filter(c => c.path.endsWith('/redeem')).length, 1);
    assert.equal(h.calls.find(c => c.path.endsWith('/redeem')).signal.aborted, false);
    redeem.resolve(); await submitting;
    assert.equal(h.mutations.filter(v => v === 'end').length, 1);
    assert.equal(h.el('session-panel').hidden, true); assert.equal(h.peers.length, 0);
    assert.equal(h.el('connect').disabled, false); assert.match(h.el('status').textContent, /canceled/);
  } finally { await h.dispose(); }
});

scenario('an uncertain POST failure is not retried and never exposes transport details', async () => {
  const h = await harness({ rejectRedeem: true });
  try {
    await h.submit();
    assert.equal(h.calls.filter(c => c.path.endsWith('/redeem')).length, 1);
    assert.equal(h.el('session-panel').hidden, true); assert.equal(h.el('connect').disabled, false);
    assert.match(h.el('status').textContent, /connection list/);
    assert.doesNotMatch(h.el('status').textContent, /SYNTHETIC_PRIVATE_DETAIL/);
  } finally { await h.dispose(); }
});

scenario('a malformed or escalated grant is closed without displaying a desktop', async () => {
  const h = await harness({ mutateGrant: value => { value.session.permissions.fileTransfer = true; } });
  try {
    await h.submit(); assert.equal(h.mutations.filter(v => v === 'end').length, 1);
    assert.equal(h.el('session-panel').hidden, true); assert.equal(h.peers.length, 0);
    assert.equal(h.el('pin').value, '');
  } finally { await h.dispose(); }
});

scenario('pending sessions expose no peer or input and repeated disconnect is idempotent', async () => {
  const h = await harness();
  try {
    await h.submit(); await settle();
    assert.equal(h.el('session-panel').hidden, false); assert.equal(h.peers.length, 0);
    await h.el('disconnect').fire('click'); await h.el('disconnect').fire('click'); await settle();
    assert.equal(h.mutations.filter(v => v === 'end').length, 1);
    assert.equal(h.el('session-panel').hidden, true); assert.equal(h.el('welcome').hidden, false);
  } finally { await h.dispose(); }
});

scenario('input requires the actual app peer, channel, catalog, permission and fresh frame', async () => {
  const h = await harness({ status: 'approved' });
  try {
    const pc = await h.ready(), video = h.el('desktop');
    const down = () => video.fire('pointerdown', { clientX: 320, clientY: 180, buttons: 1, pointerId: 1 });
    await down(); assert.equal(pc.dc.messages.filter(m => m.type === 'pointer').length, 0);
    pc.dc.receive(capabilities); pc.dc.receive(catalog);
    await down(); assert.equal(pc.dc.messages.filter(m => m.type === 'pointer').length, 0);
    video.frame(); await down(); assert.equal(pc.dc.messages.filter(m => m.type === 'pointer').length, 1);
    await h.clock.advance(2600); await down();
    assert.equal(pc.dc.messages.filter(m => m.type === 'pointer').length, 1);
    // Input is blocked synchronously; the 250ms maintenance tick releases
    // previously held buttons on its first tick after the 2500ms deadline.
    await h.clock.advance(150);
    assert(pc.dc.messages.some(m => m.type === 'release_all'));
  } finally { await h.dispose(); }
});

scenario('fresh video without a primary-display catalog cannot authorize input', async () => {
  const h = await harness({ status: 'approved' });
  try {
    const pc = await h.ready(), video = h.el('desktop');
    pc.dc.receive(capabilities); video.frame();
    await video.fire('pointerdown', { clientX: 320, clientY: 180, buttons: 1, pointerId: 1 });
    assert.equal(pc.dc.messages.filter(m => m.type === 'pointer').length, 0);
    pc.dc.receive({ ...catalog, selectedMonitorId: 'b'.repeat(64) }); await settle();
    assert.equal(h.el('session-panel').hidden, true); assert.equal(pc.closeCount, 1);
  } finally { await h.dispose(); }
});

scenario('page hide releases held input, closes once, and requests a keepalive end', async () => {
  const h = await harness({ status: 'approved' });
  try {
    const pc = await h.ready(), video = h.el('desktop');
    pc.dc.receive(capabilities); pc.dc.receive(catalog); video.frame(); video.focus();
    await video.fire('keydown', { code: 'KeyA' });
    assert(pc.dc.messages.some(m => m.type === 'key' && m.down));
    await h.window.fire('pagehide'); await h.window.fire('pagehide'); await settle();
    assert(pc.dc.messages.some(m => m.type === 'release_all'));
    assert.equal(pc.closeCount, 1); assert.equal(h.mutations.filter(v => v === 'end').length, 1);
    assert.equal(h.calls.find(c => c.path.endsWith('/end')).keepalive, true);
    assert.equal(h.el('desktop').srcObject, null); assert.equal(h.el('pin').value, '');
  } finally { await h.dispose(); }
});

scenario('focus or visibility loss releases keys and hidden pages cannot send input', async () => {
  const h = await harness({ status: 'approved' });
  try {
    const pc = await h.ready(), video = h.el('desktop');
    pc.dc.receive(capabilities); pc.dc.receive(catalog); video.frame(); video.focus();
    await video.fire('keydown', { code: 'ControlLeft' }); await h.window.fire('blur');
    assert(pc.dc.messages.some(m => m.type === 'release_all'));
    h.document.visibilityState = 'hidden'; await h.document.fire('visibilitychange');
    const before = pc.dc.messages.filter(m => m.type === 'key').length;
    await video.fire('keydown', { code: 'KeyA' });
    assert.equal(pc.dc.messages.filter(m => m.type === 'key').length, before);
  } finally { await h.dispose(); }
});

scenario('permission removal immediately gates input and later restoration ends the session', async () => {
  let control = true;
  const h = await harness({ status: 'approved', mutateSnapshot: value => { value.session.permissions.controlInput = control; } });
  try {
    const pc = await h.ready(), video = h.el('desktop');
    pc.dc.receive(capabilities); pc.dc.receive(catalog); video.frame(); video.focus();
    await video.fire('keydown', { code: 'KeyA' }); const initial = pc.dc.messages.filter(m => m.type === 'key').length;
    control = false; await h.clock.advance(3000); video.frame();
    await video.fire('keydown', { code: 'KeyB' }); assert.equal(pc.dc.messages.filter(m => m.type === 'key').length, initial);
    control = true; await h.clock.advance(3000); await settle();
    assert.equal(h.el('session-panel').hidden, true); assert.equal(pc.closeCount, 1);
    assert.match(h.el('status').textContent, /removed permission/);
  } finally { await h.dispose(); }
});

scenario('a changed host identity ends the active session instead of reusing its peer', async () => {
  const h = await harness({ status: 'approved', mutateSnapshot: (value, count) => { if (count > 1) value.session.hostPublicKey = Buffer.alloc(32, 9).toString('base64'); } });
  try {
    const pc = await h.ready(); await h.clock.advance(3000); await settle();
    assert.equal(h.el('session-panel').hidden, true); assert.equal(pc.closeCount, 1);
    assert.match(h.el('status').textContent, /identity changed/);
    assert.equal(h.mutations.filter(v => v === 'end').length, 1);
  } finally { await h.dispose(); }
});

scenario('a delayed old stop failure cannot overwrite the result of a newer attempt', async () => {
  const end = deferred(), options = { end, pendingSnapshots: true };
  const h = await harness(options);
  try {
    await h.submit(); await h.el('disconnect').fire('click'); await settle();
    options.rejectRedeem = true; await h.submit();
    const message = h.el('status').textContent; assert.match(message, /connection list/);
    end.reject(new Error('Synthetic old stop failure')); await settle();
    assert.equal(h.el('status').textContent, message);
  } finally { await h.dispose(); }
});

scenario('unsupported fullscreen is an actionable message rather than an uncaught exception', async () => {
  const h = await harness();
  try {
    h.el('stage').requestFullscreen = undefined;
    await h.el('fullscreen').fire('click');
    assert.match(h.el('status').textContent, /unavailable/);
  } finally { await h.dispose(); }
});

scenario('agent connection binds the permanent PC ID and labels itself before first-frame admission', async () => {
  const h = await harness({ activation: true, status: 'approved' });
  try {
    const request = { pcId: '123456789', pin: '001234', name: 'Codex' };
    const connected = h.agentAPI.connect(request);
    await until(() => h.peers.length === 1 && h.calls.some(c => c.path.endsWith('/signal')), 'agent peer');
    assert.equal(request.pin, '', 'Agent API releases the submitted PIN reference');
    assert.equal(h.agentAPI.status().state, 'connecting');
    assert.throws(() => h.agentAPI.screenshot(), /fresh remote desktop frame/);
    const pc = h.peers[0]; pc.dc.open(); pc.connect(); pc.track(); await settle();
    pc.dc.receive(capabilities); pc.dc.receive(catalog); h.el('desktop').frame();
    await h.clock.advance(100);
    const state = await connected;
    assert.equal(state.state, 'connected'); assert.equal(state.pcId, '123456789');
    const requestBody = h.calls.find(c => c.path === '/v1/pairings/redeem').body;
    assert.equal(requestBody.controllerName, 'AI agent · Codex'); assert.equal(requestBody.code, '123456789');
    assert.equal(Object.hasOwn(state, 'controllerToken'), false); assert.equal(Object.hasOwn(state, 'pin'), false);
    const stopped = await h.agentAPI.disconnect();
    assert.equal(stopped.state, 'idle'); assert.equal(stopped.stopConfirmed, true);
    assert.throws(() => h.agentAPI.screenshot());
  } finally { await h.dispose(); }
});

scenario('agent image tokens authorize one action and never replay across fresh input', async () => {
  const h = await harness({ status: 'approved' });
  try {
    const pc = await h.ready(); pc.dc.receive(capabilities); pc.dc.receive(catalog); h.el('desktop').frame();
    const image = h.agentAPI.screenshot();
    assert.equal(image.width, 640); assert.equal(image.height, 360); assert.equal(image.mimeType, 'image/png');
    const request = { type: 'click', x: 320, y: 180, frameToken: image.frameToken };
    assert.equal(h.agentAPI.input(request).accepted, true);
    const pointers = pc.dc.messages.filter(m => m.type === 'pointer');
    assert.equal(pointers.length, 2); assert.equal(pointers[0].action, 'down'); assert.equal(pointers[1].action, 'up');
    assert.equal(pointers[1].buttons, 0); assert(pc.dc.messages.some(m => m.type === 'release_all'));
    const before = pc.dc.messages.length;
    assert.throws(() => h.agentAPI.input(request), /new screenshot/);
    assert.equal(pc.dc.messages.length, before);
  } finally { await h.dispose(); }
});

scenario('agent input rejects stale observations even when newly arriving video is fresh', async () => {
  const h = await harness({ status: 'approved' });
  try {
    const pc = await h.ready(); pc.dc.receive(capabilities); pc.dc.receive(catalog); h.el('desktop').frame();
    const image = h.agentAPI.screenshot(); await h.clock.advance(15_100); h.el('desktop').frame();
    assert.throws(() => h.agentAPI.input({ type: 'move', x: 2, y: 2, frameToken: image.frameToken }), /new screenshot/);
    assert.equal(pc.dc.messages.filter(m => m.type === 'pointer').length, 0);
    const next = h.agentAPI.screenshot(); h.el('desktop').frame(800, 450);
    assert.throws(() => h.agentAPI.input({ type: 'move', x: 2, y: 2, frameToken: next.frameToken }), /new screenshot/);
  } finally { await h.dispose(); }
});

scenario('agent can observe a viewer session but cannot inject any input', async () => {
  const h = await harness({ status: 'approved', viewOnly: true });
  try {
    const pc = await h.ready(); pc.dc.receive(capabilities); pc.dc.receive(catalog); h.el('desktop').frame();
    assert.equal(h.agentAPI.status().state, 'view_only'); assert.equal(h.agentAPI.status().canControl, false);
    const image = h.agentAPI.screenshot();
    assert.throws(() => h.agentAPI.input({ type: 'text', text: 'Hello', frameToken: image.frameToken }), /view-only/);
    assert.equal(pc.dc.messages.filter(m => ['pointer', 'text', 'key', 'scroll'].includes(m.type)).length, 0);
  } finally { await h.dispose(); }
});

scenario('agent actions are bounded and release a complete key combination', async () => {
  const h = await harness({ status: 'approved' });
  try {
    const pc = await h.ready(); pc.dc.receive(capabilities); pc.dc.receive(catalog); h.el('desktop').frame();
    const token = () => h.agentAPI.screenshot().frameToken;
    for (const action of [{ type: 'click', x: -1, y: 0 }, { type: 'click', x: 640, y: 0 },
      { type: 'key', codes: ['LaunchShell'] }, { type: 'text', text: 'x'.repeat(4097) },
      { type: 'text', text: 'x\0y' }, { type: 'scroll', dx: 1.5, dy: 0 }, { type: 'execute', command: 'arbitrary' }]) {
      assert.throws(() => h.agentAPI.input({ frameToken: token(), ...action }));
    }
    assert.equal(pc.dc.messages.filter(m => ['pointer', 'key', 'text', 'scroll'].includes(m.type)).length, 0);
    h.agentAPI.input({ type: 'key', codes: ['ControlLeft', 'KeyA'], frameToken: token() });
    const keys = pc.dc.messages.filter(m => m.type === 'key');
    assert.deepEqual(keys.map(m => [m.code, m.down]), [['ControlLeft', true], ['KeyA', true], ['KeyA', false], ['ControlLeft', false]]);
    assert.equal(pc.dc.messages.at(-1).type, 'release_all');
  } finally { await h.dispose(); }
});

scenario('disconnect during an agent redemption waits to end the known late session', async () => {
  const redeem = deferred(), h = await harness({ activation: true, redeem });
  try {
    const joining = h.agentAPI.connect({ pcId: '123456789', pin: '001234', name: 'Codex' });
    // Attach the rejection handler before asynchronous cancellation can settle.
    const rejected = assert.rejects(joining, /could not be established/);
    await until(() => h.calls.some(c => c.path.endsWith('/redeem')), 'agent PIN submission');
    const stopped = h.agentAPI.disconnect(); redeem.resolve();
    await rejected; const result = await stopped;
    assert.equal(result.state, 'idle'); assert.equal(h.mutations.filter(m => m === 'end').length, 1);
    assert.equal(h.peers.length, 0);
  } finally { await h.dispose(); }
});

after(async () => {
  const afterHashes = Object.fromEntries(await Promise.all(sourcePaths.map(async p => [path.relative(root, p), hash(await fs.readFile(p))])));
  const evidence = { recordedAt: new Date().toISOString(), scope: 'Actual production app.js/core.mjs in Node VM; DOM, HTTP and RTC are explicit test doubles. No browser rendering, decoded media or native input execution.',
    runtime: process.version, sourceInputs: beforeHashes,
    changedInputsDuringRun: Object.keys(beforeHashes).filter(p => beforeHashes[p] !== afterHashes[p]),
    tests: outcomes, passed: outcomes.filter(v => v.ok).length, failed: outcomes.filter(v => !v.ok).length };
  const output = process.env.OPENREMOTE_BROWSER_LIFECYCLE_EVIDENCE || path.join(root, 'evidence/activation-tray-0.8.0/browser-lifecycle.json');
  await fs.mkdir(path.dirname(output), { recursive: true });
  await fs.writeFile(output, JSON.stringify(evidence, null, 2) + '\n');
});
