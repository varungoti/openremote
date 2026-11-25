import { APIError, Coordinator, DisplayGate, UserMessage, friendlyError, normalizedPoint,
  boundedName, pairingSession, permissionsWithin, requireThat, serverInfo, sessionSnapshot, validCode, validId, validPin } from './core.mjs';

const $ = id => document.getElementById(id);
const api = new Coordinator(location.origin);
const video = $('desktop');
let sequence = 0, attempt = null, active = null;
const encoder = new TextEncoder();

function status(message, kind = '') {
  $('status').textContent = message; $('status').className = 'status' + (kind ? ' ' + kind : ''); $('status').hidden = !message;
}
function joining(value) {
  $('connect').disabled = value; $('connect').hidden = value; $('code').disabled = value; $('pin').disabled = value;
  $('cancel').hidden = !value; $('cancel').disabled = false;
  $('connect').firstChild.textContent = value ? 'Connecting… ' : 'Connect remote desktop ';
}
function current(ctx) { return active === ctx && !ctx.closed && ctx.generation === sequence; }
function pause(ms, signal) {
  return new Promise((resolve, reject) => {
    if (signal?.aborted) { reject(signal.reason); return; }
    const done = () => { signal?.removeEventListener('abort', stop); resolve(); };
    const timer = setTimeout(done, ms);
    const stop = () => { clearTimeout(timer); signal?.removeEventListener('abort', stop); reject(signal.reason); };
    signal?.addEventListener('abort', stop, { once: true });
  });
}
async function endOnServer(session, keepalive = false) {
  if (!validId(session?.id) || typeof session?.controllerToken !== 'string') return false;
  try {
    await api.request(`/v1/sessions/${session.id}/end`, { method: 'POST', token: session.controllerToken, body: {}, timeout: 5000, keepalive });
    return true;
  } catch { return false; }
}
function release(ctx = active) {
  if (ctx?.dc?.readyState === 'open') {
    try { ctx.dc.send('{"v":1,"type":"release_all"}'); } catch { /* Transport teardown also releases keys on the host. */ }
  }
}
function stopSession(message = 'Disconnected. Reconnect with the same PC ID and PIN while connection is active on the PC.', kind = 'success', keepalive = false) {
  const ctx = active;
  if (!ctx || ctx.closed) return;
  release(ctx); ctx.closed = true; ctx.gate.revoke(); sequence++; active = null;
  const stoppedGeneration = sequence;
  ctx.abort.abort(new UserMessage('Session stopped.'));
  clearInterval(ctx.timer); clearTimeout(ctx.recoveryTimer);
  if (ctx.frameCallback !== undefined) video.cancelVideoFrameCallback?.(ctx.frameCallback);
  ctx.pc?.close(); video.srcObject = null; $('remote-text').value = '';
  $('session-panel').hidden = true; $('welcome').hidden = false;
  $('pin').value = ''; joining(false); status(message, kind); $('code').focus();
  if (document.fullscreenElement && typeof document.exitFullscreen === 'function') {
    try { Promise.resolve(document.exitFullscreen()).catch(() => {}); } catch { /* A closed session never depends on full-screen support. */ }
  }
  ctx.ending = endOnServer(ctx.session, keepalive).then(ended => {
    if (!ended && sequence === stoppedGeneration && !active && !attempt && !keepalive) status(message + ' The service did not confirm the stop; close the host or use its Stop control if needed.', 'error');
    return ended;
  });
  return ctx.ending;
}
function fatal(ctx, error) {
  if (current(ctx)) stopSession(friendlyError(error), 'error');
}

async function startConnection(code, pin, controllerName = 'PC browser') {
  if (attempt || active) return;
  if (!validCode(code) || !validPin(pin) || !boundedName(controllerName)) { status('Enter the PC ID and exactly six digits for your PIN.', 'error'); return; }
  if (!isSecureContext || !crypto?.subtle || !globalThis.RTCPeerConnection || !video.requestVideoFrameCallback) {
    status('Use a current Chrome, Edge, Firefox or Safari browser over HTTPS to connect.', 'error'); return;
  }
  const job = { generation: ++sequence, abort: new AbortController(), canceled: false, redeeming: false, created: null };
  job.settled = new Promise(resolve => { job.resolveSettled = resolve; });
  attempt = job; $('pin').value = ''; joining(true); status('Checking the connection service…');
  try {
    const info = serverInfo(await api.request('/v1/info', { signal: job.abort.signal }));
    requireThat(code.length !== 9 || info.activationVersion === 1, 'This connection service needs the PC ID update.');
    const identity = await api.authenticate(job.abort.signal);
    if (job.canceled) return;
    status('Verifying your PC ID and PIN…'); job.redeeming = true;
    // Once sent, finish the bounded one-use POST even if the user cancels. A
    // known late session is explicitly ended; the POST is never auto-retried.
    const response = await api.request('/v1/pairings/redeem', { method: 'POST', token: identity.token,
      body: { code, pin, controllerName }, timeout: 35_000 });
    pin = '';
    if (validId(response?.session?.id) && typeof response?.controllerToken === 'string')
      job.created = { ...response.session, controllerToken: response.controllerToken };
    const session = pairingSession(response, identity.publicKey, code);
    job.created = session;
    if (job.canceled || job.generation !== sequence) { await endOnServer(session); return; }
    attempt = null; joining(false); status(''); $('code').value = '';
    const ctx = { generation: job.generation, session, permissionCeiling: session.permissions, gate: new DisplayGate(), abort: new AbortController(), closed: false,
      peerStarted: false, pc: null, dc: null, earlyIce: [], cursor: 0, started: performance.now(), lastMove: 0,
      inputWindow: performance.now(), inputCount: 0, hadInput: false, firstFrame: false, connectedReported: false };
    active = ctx; ctx.gate.permission = session.permissions.controlInput;
    $('host-name').textContent = session.hostName; $('welcome').hidden = true; $('session-panel').hidden = false;
    $('video-overlay').hidden = false; $('video-overlay').textContent = 'Waiting for the host to start sharing…';
    $('connection-state').textContent = 'Connecting'; $('display-name').textContent = '';
    ctx.timer = setInterval(() => maintain(ctx), 250);
    void watchSession(ctx);
    return ctx;
  } catch (error) {
    if (job.created) await endOnServer(job.created);
    if (!job.canceled && job.generation === sequence) status(friendlyError(error, job.redeeming && !(error instanceof APIError)), 'error');
  } finally {
    pin = '';
    if (attempt === job) { attempt = null; joining(false); }
    if (job.canceled && !active) status('Connection canceled. Check that connection is still active on your PC before trying again.');
    job.resolveSettled();
  }
}
$('connect-form').addEventListener('submit', async event => {
  event.preventDefault();
  const platform = typeof navigator === 'object' ? String(navigator.userAgentData?.platform || navigator.platform || '').replace(/[^A-Za-z0-9 ._-]/g, '').slice(0, 40) : '';
  return startConnection($('code').value, $('pin').value, platform ? `PC browser · ${platform}` : 'PC browser');
});

function cancelAttempt() {
  const job = attempt;
  $('pin').value = '';
  if (!job) return;
  job.canceled = true; sequence++; job.abort.abort(new UserMessage('Connection canceled.'));
  $('cancel').disabled = true;
  status(job.redeeming ? 'Finishing the request so any newly created session can be closed…' : 'Canceling connection…');
}
$('cancel').addEventListener('click', cancelAttempt);
$('disconnect').addEventListener('click', () => { void stopSession(); });

async function watchSession(ctx) {
  try {
    while (current(ctx)) {
      const result = await api.request(`/v1/sessions/${ctx.session.id}`, { token: ctx.session.controllerToken, signal: ctx.abort.signal, timeout: 12_000 });
      if (!current(ctx)) return;
      const snap = sessionSnapshot(result, ctx.session);
      requireThat(permissionsWithin(snap.permissions, ctx.permissionCeiling), 'The session attempted to restore a removed permission.');
      ctx.permissionCeiling = snap.permissions; ctx.gate.permission = snap.permissions.controlInput;
      if (!['pending', 'approved', 'connecting', 'active'].includes(snap.status)) {
        stopSession(snap.status === 'rejected' ? 'The PC could not accept this connection. Check its connection status.' : 'The PC ended the session. Check that connection is active before reconnecting.'); return;
      }
      requireThat(Date.parse(snap.expiresAt) > Date.now(), 'This session expired. Start remote desktop again on the PC to reconnect.');
      if (snap.status !== 'pending' && !ctx.peerStarted) {
        ctx.peerStarted = true; void startPeer(ctx).catch(error => fatal(ctx, error));
      }
      await pause(snap.status === 'pending' ? 1000 : 3000, ctx.abort.signal);
    }
  } catch (error) { fatal(ctx, error); }
}

function checkedIce(value) {
  requireThat(Array.isArray(value?.iceServers) && value.iceServers.length <= 32 && ['all', 'relay'].includes(value.iceTransportPolicy));
  const servers = value.iceServers.map(s => {
    const urls = typeof s.urls === 'string' ? [s.urls] : s.urls;
    requireThat(Array.isArray(urls) && urls.length >= 1 && urls.length <= 16 && urls.every(u => typeof u === 'string' &&
      u.length <= 1024 && /^(stun|stuns|turn|turns):[^\s]+$/.test(u)));
    requireThat((s.username === undefined || typeof s.username === 'string') && (s.credential === undefined || typeof s.credential === 'string'));
    return { urls, username: s.username || '', credential: s.credential || '' };
  });
  return { iceServers: servers, iceTransportPolicy: value.iceTransportPolicy, bundlePolicy: 'max-bundle' };
}
async function signal(ctx, kind, payload) {
  requireThat(current(ctx), 'Session stopped.');
  return api.request(`/v1/sessions/${ctx.session.id}/signal`, { method: 'POST', token: ctx.session.controllerToken,
    body: { kind, payload }, signal: ctx.abort.signal });
}
async function startPeer(ctx) {
  const ice = checkedIce(await api.request(`/v1/sessions/${ctx.session.id}/ice`, { token: ctx.session.controllerToken, signal: ctx.abort.signal }));
  if (!current(ctx)) return;
  const pc = new RTCPeerConnection(ice); ctx.pc = pc;
  pc.ondatachannel = event => event.channel.close();
  pc.ontrack = event => {
    if (!current(ctx)) { event.track.stop(); return; }
    if (event.track.kind !== 'video' || ctx.track) { fatal(ctx, new UserMessage('Unexpected media track.')); return; }
    ctx.track = event.track; video.srcObject = new MediaStream([event.track]);
    event.track.onended = () => fatal(ctx, new UserMessage('The host stopped sending video.'));
    void video.play().catch(() => { if (current(ctx)) status('Click the remote desktop to start video playback.'); });
    const frame = () => {
      if (!current(ctx)) return;
      if (video.videoWidth > 0 && video.videoHeight > 0 && !video.paused) {
        ctx.gate.lastFrame = performance.now(); ctx.firstFrame = true;
      }
      ctx.frameCallback = video.requestVideoFrameCallback(frame);
    };
    ctx.frameCallback = video.requestVideoFrameCallback(frame);
  };
  pc.onconnectionstatechange = () => {
    if (!current(ctx)) return;
    ctx.gate.transport = pc.connectionState === 'connected';
    if (ctx.gate.transport) {
      clearTimeout(ctx.recoveryTimer);
      if (!ctx.connectedReported) {
        ctx.connectedReported = true;
        void api.request(`/v1/sessions/${ctx.session.id}/connected`, { method: 'POST', token: ctx.session.controllerToken,
          body: {}, signal: ctx.abort.signal }).catch(error => fatal(ctx, error));
      }
    } else if (['failed', 'disconnected'].includes(pc.connectionState)) {
      release(ctx); $('connection-state').textContent = 'Reconnecting';
      clearTimeout(ctx.recoveryTimer);
      ctx.recoveryTimer = setTimeout(() => fatal(ctx, new UserMessage('The network connection was lost. Start remote desktop again on the PC to reconnect.')), 20_000);
    } else if (pc.connectionState === 'closed') fatal(ctx, new UserMessage('The remote connection closed.'));
  };
  const transceiver = pc.addTransceiver('video', { direction: 'recvonly' });
  const h264 = RTCRtpReceiver.getCapabilities('video')?.codecs.filter(c => c.mimeType.toLowerCase() === 'video/h264') || [];
  requireThat(h264.length > 0, 'This browser does not have the H.264 decoder needed by the PC.');
  transceiver.setCodecPreferences(h264);
  const dc = pc.createDataChannel('control', { ordered: true }); ctx.dc = dc;
  dc.onopen = () => { if (current(ctx)) ctx.gate.channel = true; };
  dc.onclose = () => fatal(ctx, new UserMessage('The host closed the control channel.'));
  dc.onerror = () => fatal(ctx, new UserMessage('The control connection failed.'));
  dc.onmessage = event => {
    if (!current(ctx)) return;
    try { requireThat(typeof event.data === 'string'); ctx.gate.receive(event.data); }
    catch (error) { fatal(ctx, error); }
  };
  await pc.setLocalDescription(await pc.createOffer());
  // A complete initial offer avoids candidate/offer reorder races and excessive
  // signal POSTs. Remote candidates still arrive on the existing polling API.
  if (pc.iceGatheringState !== 'complete') await new Promise((resolve, reject) => {
    const cleanup = () => { clearTimeout(timer); pc.removeEventListener('icegatheringstatechange', check); ctx.abort.signal.removeEventListener('abort', cancel); };
    const check = () => { if (pc.iceGatheringState === 'complete') { cleanup(); resolve(); } };
    const cancel = () => { cleanup(); reject(new UserMessage('Session stopped.')); };
    const timer = setTimeout(() => { cleanup(); reject(new UserMessage('The connection took too long. Check your Internet connection and try again.')); }, 15_000);
    pc.addEventListener('icegatheringstatechange', check); ctx.abort.signal.addEventListener('abort', cancel, { once: true });
    if (ctx.abort.signal.aborted) cancel(); else check();
  });
  await signal(ctx, 'offer', { type: 'offer', sdp: pc.localDescription.sdp });
  if (current(ctx)) void pollSignals(ctx).catch(error => fatal(ctx, error));
}
async function pollSignals(ctx) {
  while (current(ctx)) {
    const response = await api.request(`/v1/sessions/${ctx.session.id}/signals?after=${ctx.cursor}`, {
      token: ctx.session.controllerToken, signal: ctx.abort.signal });
    if (!current(ctx)) return;
    requireThat(Array.isArray(response.signals) && response.signals.length <= 512 && Number.isSafeInteger(response.cursor) && response.cursor >= ctx.cursor);
    let latest = ctx.cursor;
    for (const item of response.signals) {
      requireThat(item.from === 'host' && Number.isSafeInteger(item.seq) && item.seq > latest && item.seq <= response.cursor);
      latest = item.seq;
      if (item.kind === 'answer') {
        requireThat(ctx.pc.signalingState === 'have-local-offer' && item.payload?.type === 'answer' && typeof item.payload.sdp === 'string' && item.payload.sdp.length <= 262144);
        await ctx.pc.setRemoteDescription(item.payload);
        for (const candidate of ctx.earlyIce) await ctx.pc.addIceCandidate(candidate);
        ctx.earlyIce = [];
      } else if (item.kind === 'ice') {
        requireThat(typeof item.payload?.candidate === 'string' && item.payload.candidate.length <= 4096);
        if (ctx.pc.remoteDescription) await ctx.pc.addIceCandidate(item.payload);
        else { requireThat(ctx.earlyIce.length < 256); ctx.earlyIce.push(item.payload); }
      } else throw new UserMessage('The host requested a new connection. Start remote desktop again on the PC to reconnect.');
    }
    ctx.cursor = response.cursor;
  }
}

function maintain(ctx) {
  if (!current(ctx)) return;
  const now = performance.now(), live = ctx.gate.ready(now, document.visibilityState === 'visible');
  if (ctx.hadInput && !live) release(ctx);
  ctx.hadInput = live;
  const fresh = ctx.gate.transport && ctx.firstFrame && now - ctx.gate.lastFrame <= 2500;
  $('connection-state').textContent = fresh ? (ctx.gate.permission ? 'Connected' : 'View only') : (ctx.firstFrame ? 'Reconnecting' : 'Connecting');
  $('video-overlay').hidden = fresh;
  if (ctx.firstFrame && !fresh) $('video-overlay').textContent = 'Connection paused. Waiting for fresh video…';
  $('control-hint').textContent = live ? 'Click the desktop to control it. Press Escape to release your keyboard.' :
    fresh && !ctx.gate.permission ? 'View-only session. Input is disabled by the host.' : 'Input stays paused until the PC confirms its display and video is ready.';
  const selected = ctx.gate.catalog?.monitors.find(m => m.id === ctx.gate.catalog.selectedMonitorId);
  $('display-name').textContent = selected ? `${selected.name} · ${selected.width} × ${selected.height}` : '';
  for (const id of ['remote-escape', 'remote-windows']) $(id).disabled = !live;
  for (const id of ['remote-text', 'send-text']) $(id).disabled = !live || !ctx.gate.capabilities?.text;
  if (!ctx.firstFrame && now - ctx.started > 90_000) fatal(ctx, new UserMessage('The PC did not start video in time. Keep the host open and start remote desktop again.'));
  else if (ctx.firstFrame && now - ctx.gate.lastFrame > 25_000) fatal(ctx, new UserMessage('Video stopped arriving. The session was closed to keep input safe.'));
}
function sendInput(value, needFocus = false) {
  const ctx = active, now = performance.now();
  if (!ctx || !current(ctx) || !ctx.gate.ready(now, document.visibilityState === 'visible') ||
      (needFocus && document.activeElement !== video) || ctx.dc?.readyState !== 'open') return false;
  if (ctx.dc.bufferedAmount > 65_536) { release(ctx); return false; }
  if (now - ctx.inputWindow >= 1000) { ctx.inputWindow = now; ctx.inputCount = 0; }
  if (++ctx.inputCount > 250) { release(ctx); return false; }
  try { ctx.dc.send(JSON.stringify({ v: 1, ...value })); return true; }
  catch (error) { fatal(ctx, error); return false; }
}
video.addEventListener('click', () => { video.focus(); void video.play().catch(() => {}); });
for (const [eventName, action] of [['pointerdown', 'down'], ['pointerup', 'up'], ['pointermove', 'move']]) {
  video.addEventListener(eventName, event => {
    const ctx = active; if (!ctx) return;
    if (action === 'down') video.focus();
    if (action === 'move' && performance.now() - ctx.lastMove < 16) return;
    const point = normalizedPoint(event.clientX, event.clientY, video.getBoundingClientRect(), video.videoWidth, video.videoHeight);
    if (!point) { if (action === 'up') release(ctx); return; }
    if (sendInput({ type: 'pointer', action, ...point, buttons: event.buttons & 7 }, true)) {
      event.preventDefault(); ctx.lastMove = performance.now();
      if (action === 'down') video.setPointerCapture(event.pointerId);
    }
  });
}
video.addEventListener('pointercancel', () => release());
video.addEventListener('lostpointercapture', () => release());
video.addEventListener('contextmenu', event => event.preventDefault());
video.addEventListener('wheel', event => {
  const factor = event.deltaMode === 1 ? 40 : event.deltaMode === 2 ? 800 : 1;
  if (sendInput({ type: 'scroll', dx: Math.max(-2400, Math.min(2400, event.deltaX * factor)), dy: Math.max(-2400, Math.min(2400, -event.deltaY * factor)) }, true)) event.preventDefault();
}, { passive: false });
const keyPattern = /^(Key[A-Z]|Digit[0-9]|F([1-9]|1[0-2])|Enter|Tab|Space|Backspace|Delete|Insert|Home|End|PageUp|PageDown|Arrow(Left|Right|Up|Down)|(Control|Alt|Shift|Meta)(Left|Right))$/;
for (const [name, down] of [['keydown', true], ['keyup', false]]) video.addEventListener(name, event => {
  if (event.code === 'Escape') { event.preventDefault(); release(); $('disconnect').focus(); return; }
  if (keyPattern.test(event.code) && sendInput({ type: 'key', code: event.code, down, modifiers: 0 }, true)) event.preventDefault();
});
video.addEventListener('blur', () => release());
window.addEventListener('blur', () => release());
function remoteKey(code) { sendInput({ type: 'key', code, down: true }); sendInput({ type: 'key', code, down: false }); }
$('remote-escape').addEventListener('click', () => remoteKey('Escape'));
$('remote-windows').addEventListener('click', () => remoteKey('MetaLeft'));
$('text-form').addEventListener('submit', event => {
  event.preventDefault(); const text = $('remote-text').value;
  if (!text || text.includes('\0') || encoder.encode(text).length > 4096) { status('Send between 1 and 4096 bytes of text at a time.', 'error'); return; }
  if (active?.gate.capabilities?.text && sendInput({ type: 'text', text })) { $('remote-text').value = ''; status('Text sent.', 'success'); }
});
$('fullscreen').addEventListener('click', () => {
  const unavailable = () => status('Full screen is unavailable in this browser. You can keep using the desktop in this window.');
  const target = document.fullscreenElement ? document : $('stage');
  const operation = document.fullscreenElement ? target.exitFullscreen : target.requestFullscreen;
  if (typeof operation !== 'function') { unavailable(); return; }
  try { Promise.resolve(operation.call(target)).catch(unavailable); } catch { unavailable(); }
});

// The local stdio agent bridge uses the same authenticated WebRTC client as a
// human. This interface never returns bearer/identity keys, exposes arbitrary
// JavaScript execution, or bypasses the host's permission and live-video gates.
function agentArguments(value, allowed) {
  requireThat(value !== null && typeof value === 'object' && !Array.isArray(value) &&
    Object.keys(value).every(key => allowed.includes(key)), 'Invalid agent request.');
}
function observedSession() {
  const ctx = active, now = performance.now();
  requireThat(ctx && current(ctx) && ctx.firstFrame && ctx.gate.transport && ctx.gate.channel &&
    ctx.gate.bindingReady && document.visibilityState === 'visible' &&
    now >= ctx.gate.lastFrame && now - ctx.gate.lastFrame <= 2500 &&
    video.videoWidth > 0 && video.videoHeight > 0, 'A fresh remote desktop frame is required.');
  return ctx;
}
function agentStatus() {
  if (!active || !current(active)) return { state: attempt ? 'connecting' : 'idle', canControl: false };
  const ctx = active, now = performance.now();
  const live = ctx.firstFrame && ctx.gate.transport && ctx.gate.channel && ctx.gate.bindingReady &&
    document.visibilityState === 'visible' && now >= ctx.gate.lastFrame && now - ctx.gate.lastFrame <= 2500;
  const canControl = live && ctx.gate.ready(now, true);
  return { state: live ? (canControl ? 'connected' : 'view_only') : 'connecting',
    pcId: ctx.session.hostDisplayId, hostName: ctx.session.hostName, canControl: Boolean(canControl) };
}
function agentScreenshot() {
  const ctx = observedSession(), width = video.videoWidth, height = video.videoHeight;
  requireThat(Number.isInteger(width) && Number.isInteger(height) && width <= 4096 && height <= 4096 &&
    width * height <= 8_294_400, 'The remote frame exceeds the agent image limit.');
  const canvas = document.createElement('canvas'); canvas.width = width; canvas.height = height;
  const drawing = canvas.getContext('2d', { alpha: false });
  requireThat(drawing, 'This browser cannot capture remote video.');
  drawing.drawImage(video, 0, 0, width, height);
  const encoded = canvas.toDataURL('image/png');
  requireThat(encoded.startsWith('data:image/png;base64,') && encoded.length <= 22_369_645,
    'The remote image could not be captured within the size limit.');
  const frameToken = crypto.randomUUID();
  ctx.observation = { token: frameToken, generation: ctx.generation, at: performance.now(),
    display: ctx.gate.catalog.selectedMonitorId, width, height };
  return { mimeType: 'image/png', data: encoded.slice('data:image/png;base64,'.length), width, height, frameToken };
}
function agentInput(request) {
  agentArguments(request, ['frameToken', 'type', 'x', 'y', 'button', 'dx', 'dy', 'codes', 'text']);
  const ctx = observedSession(), observation = ctx.observation;
  requireThat(ctx.gate.ready(performance.now(), true), 'This connection has view-only access.');
  requireThat(observation && request.frameToken === observation.token && observation.generation === ctx.generation &&
    observation.display === ctx.gate.catalog.selectedMonitorId && observation.width === video.videoWidth && observation.height === video.videoHeight &&
    performance.now() >= observation.at && performance.now() - observation.at <= 15_000,
    'Capture a new screenshot before sending input.');
  // Every image token authorizes one bounded action. Retrying never replays it.
  ctx.observation = null;
  const send = value => requireThat(sendInput(value), 'Remote input is no longer available.');
  try {
    if (request.type === 'click' || request.type === 'move') {
      agentArguments(request, ['frameToken', 'type', 'x', 'y', 'button']);
      requireThat(Number.isInteger(request.x) && Number.isInteger(request.y) && request.x >= 0 && request.y >= 0 &&
        request.x < observation.width && request.y < observation.height, 'Input coordinates are outside the captured frame.');
      const point = { x: request.x / Math.max(1, observation.width - 1), y: request.y / Math.max(1, observation.height - 1) };
      if (request.type === 'move') {
        requireThat(request.button === undefined, 'Move does not accept a mouse button.');
        send({ type: 'pointer', action: 'move', ...point, buttons: 0 });
      } else {
        const button = request.button ?? 'left';
        requireThat(['left', 'right', 'middle'].includes(button), 'Unsupported mouse button.');
        const buttons = { left: 1, right: 2, middle: 4 }[button];
        send({ type: 'pointer', action: 'down', ...point, buttons });
        send({ type: 'pointer', action: 'up', ...point, buttons: 0 });
      }
    } else if (request.type === 'scroll') {
      agentArguments(request, ['frameToken', 'type', 'dx', 'dy']);
      requireThat(Number.isInteger(request.dx) && Number.isInteger(request.dy) &&
        Math.abs(request.dx) <= 2400 && Math.abs(request.dy) <= 2400, 'Scroll is outside the supported range.');
      send({ type: 'scroll', dx: request.dx, dy: request.dy });
    } else if (request.type === 'key') {
      agentArguments(request, ['frameToken', 'type', 'codes']);
      requireThat(Array.isArray(request.codes) && request.codes.length >= 1 && request.codes.length <= 5 &&
        new Set(request.codes).size === request.codes.length && request.codes.every(code =>
          typeof code === 'string' && (code === 'Escape' || keyPattern.test(code))), 'Unsupported key combination.');
      for (const code of request.codes) send({ type: 'key', code, down: true, modifiers: 0 });
      for (const code of [...request.codes].reverse()) send({ type: 'key', code, down: false, modifiers: 0 });
    } else if (request.type === 'text') {
      agentArguments(request, ['frameToken', 'type', 'text']);
      requireThat(typeof request.text === 'string' && request.text.length > 0 && !request.text.includes('\0') &&
        encoder.encode(request.text).length <= 4096 && ctx.gate.capabilities.text, 'This text cannot be sent to the PC.');
      send({ type: 'text', text: request.text });
    } else throw new UserMessage('Unsupported agent input.');
  } finally {
    // Includes partial-send failures: never leave held mouse buttons or keys.
    release(ctx);
  }
  return { accepted: true, ...agentStatus() };
}
export const agentAPI = Object.freeze({
  async connect(request) {
    agentArguments(request, ['pcId', 'pin', 'name']);
    requireThat(typeof request.pcId === 'string' && /^[0-9]{9}$/.test(request.pcId) && validPin(request.pin), 'Enter a nine-digit PC ID and six-digit PIN.');
    requireThat(boundedName(request.name) && encoder.encode(request.name).length <= 64 &&
      !/[\u202a-\u202e\u2066-\u2069]/u.test(request.name), 'Enter a readable agent name.');
    requireThat(!active && !attempt, 'A connection is already running.');
    let pin = request.pin; request.pin = '';
    let ctx;
    try { ctx = await startConnection(request.pcId, pin, `AI agent · ${request.name}`); }
    finally { pin = ''; }
    requireThat(ctx && current(ctx), 'The agent connection could not be established.');
    const deadline = performance.now() + 45_000;
    try {
      while (current(ctx) && performance.now() < deadline) {
        const state = agentStatus();
        if (state.state === 'connected' || state.state === 'view_only') return state;
        await pause(100, ctx.abort.signal);
      }
      throw new UserMessage('The PC did not deliver a desktop frame in time.');
    } catch (error) {
      if (current(ctx)) await stopSession('Agent connection stopped.', 'error');
      throw error;
    }
  },
  status: agentStatus,
  screenshot: agentScreenshot,
  input: agentInput,
  async disconnect() {
    const pending = attempt;
    cancelAttempt();
    const ended = active ? stopSession('Agent disconnected.', 'success') : null;
    const results = await Promise.all([pending?.settled, ended]);
    return { ...agentStatus(), stopConfirmed: ended === null ? null : results[1] };
  },
});
document.addEventListener('visibilitychange', () => {
  if (document.visibilityState !== 'visible') { $('pin').value = ''; release(); }
});
window.addEventListener('pagehide', () => { cancelAttempt(); if (active) stopSession('Disconnected.', 'success', true); });
window.addEventListener('offline', () => { cancelAttempt(); if (active) stopSession('Your Internet connection went offline. Start remote desktop again on the PC after reconnecting.', 'error'); });
