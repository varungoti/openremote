// Protocol and lifecycle rules shared with the browser's executable tests.
export const permissionKeys = Object.freeze(['viewScreen', 'controlInput', 'clipboard', 'fileTransfer', 'audio', 'power']);
const utf8 = new TextEncoder();
const idPattern = /^[A-Za-z0-9_-]{16,64}$/;
const monitorPattern = /^[a-f0-9]{64}$/;
// Only fixed, first-party messages may be rendered. Browser/parser exceptions
// can include remote response snippets, SDP or credential-bearing text.
export class UserMessage extends Error {}
export function requireThat(ok, message = 'The server returned an invalid response.') {
  if (!ok) throw new UserMessage(message);
}
export const validCode = value => typeof value === 'string' && /^(?:[0-9]{6}|[0-9]{9})$/.test(value);
export const validPin = value => typeof value === 'string' && /^[0-9]{6}$/.test(value);
export const validId = value => typeof value === 'string' && idPattern.test(value);
export function boundedName(value) {
  return typeof value === 'string' && value.trim() === value && utf8.encode(value).length >= 1 &&
    utf8.encode(value).length <= 80 && !/[\u0000-\u001f\u007f-\u009f]/u.test(value);
}
function plainObject(value) { return value !== null && typeof value === 'object' && !Array.isArray(value); }
function fields(value, keys, optional = []) {
  requireThat(plainObject(value) && keys.every(k => Object.hasOwn(value, k)) &&
    Object.keys(value).every(k => keys.includes(k) || optional.includes(k)));
}

// JSON.parse alone silently accepts duplicate authorization fields. Scan before
// decoding so all clients interpret one canonical object, with bounded depth.
export function strictJson(text, maxBytes = 1_048_576) {
  requireThat(typeof text === 'string' && utf8.encode(text).length <= maxBytes);
  let i = 0;
  const space = () => { while (i < text.length && /[\x20\t\r\n]/.test(text[i])) i++; };
  function string() {
    requireThat(text[i] === '"');
    const start = i++;
    let escaped = false;
    while (i < text.length) {
      const c = text[i++];
      if (!escaped && c === '"') return JSON.parse(text.slice(start, i));
      if (!escaped && c === '\\') escaped = true;
      else escaped = false;
    }
    throw new Error('Incomplete JSON string.');
  }
  function value(depth) {
    requireThat(depth <= 32);
    space();
    if (text[i] === '{') {
      i++; space(); const seen = new Set();
      if (text[i] === '}') { i++; return; }
      for (;;) {
        space(); const key = string();
        requireThat(!seen.has(key), 'Duplicate response field.'); seen.add(key);
        space(); requireThat(text[i++] === ':'); value(depth + 1); space();
        const end = text[i++]; if (end === '}') return;
        requireThat(end === ',');
      }
    }
    if (text[i] === '[') {
      i++; space(); if (text[i] === ']') { i++; return; }
      for (;;) { value(depth + 1); space(); const end = text[i++]; if (end === ']') return; requireThat(end === ','); }
    }
    if (text[i] === '"') { string(); return; }
    const start = i;
    while (i < text.length && !/[\x20\t\r\n,}\]]/.test(text[i])) i++;
    requireThat(i > start); JSON.parse(text.slice(start, i));
  }
  try {
    value(0); space(); requireThat(i === text.length);
    return JSON.parse(text);
  } catch { throw new UserMessage('The service sent an invalid or ambiguous JSON response.'); }
}

export function base64(bytes) {
  const value = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
  let out = '';
  for (const x of value) out += String.fromCharCode(x);
  return btoa(out);
}
export function unbase64(value, size) {
  requireThat(typeof value === 'string' && value.length <= 1024 && /^[A-Za-z0-9+/]*={0,2}$/.test(value));
  const bytes = Uint8Array.from(atob(value), c => c.charCodeAt(0));
  requireThat(base64(bytes) === value && (size === undefined || bytes.length === size));
  return bytes;
}
// WebCrypto P-256 yields fixed-width r || s; Go VerifyASN1 expects DER.
// https://www.w3.org/TR/webcrypto-1/#ecdsa-operations
export function p256SignatureDER(raw) {
  const bytes = new Uint8Array(raw);
  requireThat(bytes.length === 64, 'Invalid signing result.');
  function integer(part) {
    let start = 0;
    while (start < part.length - 1 && part[start] === 0) start++;
    const trimmed = part.slice(start), pad = trimmed[0] & 0x80 ? [0] : [];
    return [2, trimmed.length + pad.length, ...pad, ...trimmed];
  }
  const values = [...integer(bytes.slice(0, 32)), ...integer(bytes.slice(32))];
  return new Uint8Array([0x30, values.length, ...values]);
}

export function permissions(value) {
  fields(value, permissionKeys);
  requireThat(permissionKeys.every(k => typeof value[k] === 'boolean') && value.viewScreen);
  return Object.freeze({ ...value });
}
export function permissionsWithin(value, ceiling) {
  return permissionKeys.every(k => !value[k] || ceiling[k]);
}
export function pairingSession(envelope, expectedPublicKey, requestedPcId = undefined) {
  requireThat(plainObject(envelope) && validId(envelope.session?.id) &&
    typeof envelope.controllerToken === 'string' && /^[A-Za-z0-9_-]{40,64}$/.test(envelope.controllerToken));
  const s = envelope.session;
  requireThat(s.status === 'pending' && validId(s.pairingId) && /^[0-9]{9}$/.test(s.hostDisplayId) &&
    boundedName(s.hostName) && s.controllerPublicKey === expectedPublicKey && !Object.hasOwn(s, 'monitorId'));
  requireThat(!Object.hasOwn(s, 'mode') || s.mode === 'activation', 'The connection mode is invalid.');
  if (requestedPcId?.length === 9) requireThat(s.mode === 'activation' && s.hostDisplayId === requestedPcId, 'The service returned a different PC.');
  unbase64(s.hostPublicKey, 32);
  const p = permissions(s.permissions);
  requireThat(!p.clipboard && !p.fileTransfer && !p.audio && !p.power);
  requireThat(Number.isFinite(Date.parse(s.expiresAt)));
  return Object.freeze({ ...s, permissions: p, controllerToken: envelope.controllerToken });
}
export function sessionSnapshot(envelope, initial) {
  const s = envelope?.session;
  requireThat(plainObject(s) && s.id === initial.id && s.hostPublicKey === initial.hostPublicKey &&
    s.hostDisplayId === initial.hostDisplayId && s.controllerPublicKey === initial.controllerPublicKey &&
    s.pairingId === initial.pairingId && s.mode === initial.mode && !Object.hasOwn(s, 'monitorId'), 'The session identity changed.');
  const p = permissions(s.permissions);
  requireThat(permissionsWithin(p, initial.permissions), 'The session requested additional permissions.');
  requireThat(['pending', 'approved', 'connecting', 'active', 'ended', 'expired', 'rejected'].includes(s.status) &&
    Number.isFinite(Date.parse(s.expiresAt)));
  return { ...s, permissions: p };
}
export function serverInfo(value) {
  requireThat(value?.service === 'openremote' && value.protocolVersion === 1 && value.pairingCodeVersion === 1 &&
    value.webController === true, 'This connection service needs an update.');
  const activation = Object.hasOwn(value, 'activationVersion') ? value.activationVersion : 0;
  const capacity = Object.hasOwn(value, 'maxActivationConnections') ? value.maxActivationConnections : 0;
  requireThat(Number.isInteger(activation) && Number.isInteger(capacity) &&
    ((activation === 0 && capacity === 0) || (activation === 1 && capacity === 4)), 'The connection service has an incompatible activation feature.');
  return value;
}

export class DisplayGate {
  capabilities = null;
  catalog = null;
  lastFrame = -Infinity;
  transport = false;
  channel = false;
  permission = false;
  receive(raw) {
    const v = strictJson(raw, 16_384);
    requireThat(v?.v === 1);
    if (v.type === 'capabilities') {
      fields(v, ['v', 'type'], ['text', 'clipboard', 'fileTransfer', 'multiMonitor', 'audio', 'power']);
      const flags = ['text', 'clipboard', 'fileTransfer', 'multiMonitor', 'audio', 'power'];
      requireThat(flags.every(k => !Object.hasOwn(v, k) || typeof v[k] === 'boolean'));
      requireThat(v.multiMonitor === true, 'Update the host to confirm its display identity before connecting.');
      const next = Object.fromEntries(flags.map(k => [k, v[k] === true]));
      requireThat(this.capabilities === null || JSON.stringify(this.capabilities) === JSON.stringify(next), 'Host capabilities changed.');
      this.capabilities = next;
    } else if (v.type === 'display_catalog') {
      requireThat(this.capabilities?.multiMonitor === true);
      fields(v, ['v', 'type', 'selectionMode', 'selectedMonitorId', 'monitors']);
      requireThat(v.selectionMode === 'reconnect' && monitorPattern.test(v.selectedMonitorId) &&
        Array.isArray(v.monitors) && v.monitors.length >= 1 && v.monitors.length <= 16);
      for (const m of v.monitors) {
        fields(m, ['id', 'name', 'width', 'height', 'primary']);
        requireThat(monitorPattern.test(m.id) && boundedName(m.name) && typeof m.primary === 'boolean' &&
          Number.isInteger(m.width) && m.width > 0 && m.width <= 32768 &&
          Number.isInteger(m.height) && m.height > 0 && m.height <= 32768);
      }
      requireThat(new Set(v.monitors.map(m => m.id)).size === v.monitors.length &&
        v.monitors.filter(m => m.primary).length === 1 && v.monitors.some(m => m.id === v.selectedMonitorId && m.primary),
      'The host did not confirm its primary display.');
      // Normalize field order before immutable-session comparison.
      const next = { selectedMonitorId: v.selectedMonitorId, monitors: v.monitors.map(m =>
        ({ id: m.id, name: m.name, width: m.width, height: m.height, primary: m.primary })) };
      requireThat(this.catalog === null || JSON.stringify(this.catalog) === JSON.stringify(next), 'The host display layout changed.');
      this.catalog = next;
    } else throw new Error('Unsupported host control message.');
  }
  get bindingReady() { return this.capabilities?.multiMonitor === true && this.catalog !== null; }
  ready(now, visible = true) {
    return visible && this.permission && this.transport && this.channel && this.bindingReady &&
      now >= this.lastFrame && now - this.lastFrame <= 2500;
  }
  revoke() { this.permission = false; this.transport = false; this.channel = false; this.lastFrame = -Infinity; }
}

export function normalizedPoint(clientX, clientY, rect, width, height) {
  if (![clientX, clientY, rect.left, rect.top, rect.width, rect.height, width, height].every(Number.isFinite) ||
      rect.width <= 0 || rect.height <= 0 || width <= 0 || height <= 0) return null;
  const scale = Math.min(rect.width / width, rect.height / height);
  const w = width * scale, h = height * scale;
  const x = (clientX - rect.left - (rect.width - w) / 2) / w;
  const y = (clientY - rect.top - (rect.height - h) / 2) / h;
  return x >= 0 && x <= 1 && y >= 0 && y <= 1 ? { x, y } : null;
}

export class APIError extends Error {
  constructor(status, code) { super('The connection request failed.'); this.status = status; this.code = code; }
}
export class Coordinator {
  constructor(origin, fetcher = fetch) {
    const u = new URL(origin);
    requireThat(u.origin === origin && !u.username && !u.password &&
      (u.protocol === 'https:' || (u.protocol === 'http:' && ['localhost', '127.0.0.1', '[::1]'].includes(u.hostname))),
    'OpenRemote requires a secure server address.');
    this.origin = origin; this.fetcher = fetcher;
  }
  async request(path, { method = 'GET', token, body, signal, timeout = 30_000, keepalive = false } = {}) {
    requireThat(/^\/v1\/[A-Za-z0-9_/?=&-]+$/.test(path));
    const abort = new AbortController();
    const onAbort = () => abort.abort(signal?.reason);
    if (signal?.aborted) onAbort(); else signal?.addEventListener('abort', onAbort, { once: true });
    const timer = setTimeout(() => abort.abort(new Error('Connection timed out.')), timeout);
    try {
      const response = await this.fetcher(this.origin + path, {
        method, headers: { Accept: 'application/json', ...(body === undefined ? {} : { 'Content-Type': 'application/json' }),
          ...(token ? { Authorization: 'Bearer ' + token } : {}) },
        body: body === undefined ? undefined : JSON.stringify(body), signal: abort.signal,
        credentials: 'omit', cache: 'no-store', redirect: 'error', referrerPolicy: 'no-referrer', keepalive,
      });
      requireThat((response.headers.get('content-type') || '').toLowerCase().startsWith('application/json'));
      const max = path === '/v1/info' ? 16_384 : 1_048_576;
      const reader = response.body?.getReader(); requireThat(reader);
      const decoder = new TextDecoder('utf-8', { fatal: true }); let text = '', size = 0;
      try {
        for (;;) {
          const { done, value } = await reader.read(); if (done) break;
          size += value.byteLength; requireThat(size <= max, 'Server response exceeded its size limit.');
          text += decoder.decode(value, { stream: true });
        }
        text += decoder.decode();
      } catch (error) { await reader.cancel().catch(() => {}); throw error; }
      finally { reader.releaseLock(); }
      const parsed = strictJson(text, max);
      if (!response.ok) throw new APIError(response.status,
        typeof parsed?.error === 'string' && /^[a-z][a-z0-9_]{0,63}$/.test(parsed.error) ? parsed.error : 'http_error');
      return parsed;
    } finally { clearTimeout(timer); signal?.removeEventListener('abort', onAbort); }
  }
  async authenticate(signal, cryptoProvider = crypto) {
    const key = await cryptoProvider.subtle.generateKey({ name: 'ECDSA', namedCurve: 'P-256' }, false, ['sign', 'verify']);
    const publicKey = base64(await cryptoProvider.subtle.exportKey('spki', key.publicKey));
    const challenge = await this.request('/v1/auth/challenge', { method: 'POST', body: { publicKey, algorithm: 'p256' }, signal });
    requireThat(validId(challenge.challengeId) && Date.parse(challenge.expiresAt) > Date.now());
    const signature = p256SignatureDER(await cryptoProvider.subtle.sign({ name: 'ECDSA', hash: 'SHA-256' }, key.privateKey,
      unbase64(challenge.challenge, 32)));
    const proof = await this.request('/v1/auth/verify', { method: 'POST', body: {
      challengeId: challenge.challengeId, signature: base64(signature) }, signal });
    requireThat(proof.publicKey === publicKey && typeof proof.accessToken === 'string' &&
      /^[A-Za-z0-9_-]{40,64}$/.test(proof.accessToken) && Date.parse(proof.expiresAt) > Date.now());
    return { token: proof.accessToken, publicKey };
  }
}

export function friendlyError(error, duringRedemption = false) {
  if (error instanceof APIError) {
    if (error.status === 429) return 'Too many connection attempts. Wait a minute, then start remote desktop again on the PC.';
    if (error.code === 'pairing_unavailable') return 'Check the PC ID and PIN, and make sure connection is active on your PC.';
    if (error.code === 'host_busy') return 'This PC is already in a session. Disconnect that session and start remote desktop again on the PC.';
    if (error.status === 503) return 'The connection service is temporarily unavailable. Please try again shortly.';
    if (error.status === 401) return 'This connection is no longer authorized. Start remote desktop again on the PC to reconnect.';
    return 'The host or connection service could not continue this session. Start remote desktop again on the PC and try again.';
  }
  if (duringRedemption) return 'The request could not be confirmed. Check the connection list on your PC before trying again.';
  return error instanceof UserMessage ? error.message.slice(0, 220)
    : 'The connection could not be completed. Check your Internet connection and try again.';
}
