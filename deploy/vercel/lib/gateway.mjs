import { isIP } from 'node:net';
import { serverInfo, strictJson } from '../../../server/internal/webclient/assets/core.mjs';

// A stateless HTTPS bridge. The durable coordinator remains the authority for
// identity, PIN checks, permissions, admission, signaling and session lifetime.
export const MAX_REQUEST_BYTES = 300 * 1024;
export const MAX_RESPONSE_BYTES = 4 * 1024 * 1024;
export const UPSTREAM_TIMEOUT_MS = 28_000;

const SECURITY_HEADERS = Object.freeze({
  'cache-control': 'no-store',
  'cdn-cache-control': 'no-store',
  'vercel-cdn-cache-control': 'no-store',
  'x-content-type-options': 'nosniff',
  'x-frame-options': 'DENY',
  'referrer-policy': 'no-referrer',
  'strict-transport-security': 'max-age=31536000',
  'content-security-policy': "default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; media-src 'self' blob:; img-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'; object-src 'none'",
  'permissions-policy': 'camera=(), microphone=(), geolocation=(), payment=(), usb=(), serial=(), clipboard-read=(), clipboard-write=()',
  'cross-origin-resource-policy': 'same-origin',
});

const VIEWER_ASSETS = new Map([
  ['/', ['index.html', 'text/html; charset=utf-8']],
  ['/connect', ['index.html', 'text/html; charset=utf-8']],
  ['/connect/', ['index.html', 'text/html; charset=utf-8']],
  ...['app.js', 'core.mjs', 'boot.mjs'].map(name => [`/connect/${name}`, [name, 'text/javascript; charset=utf-8']]),
  ['/connect/style.css', ['style.css', 'text/css; charset=utf-8']],
]);

const ID = '[A-Za-z0-9_-]{16,64}';
const ROUTES = [
  [/^\/healthz$/, ['GET']],
  [/^\/v1\/info$/, ['GET']],
  [/^\/v1\/auth\/(challenge|verify)$/, ['POST']],
  [/^\/v1\/hosts\/register(?:\/challenge)?$/, ['POST']],
  [/^\/v1\/devices\/[0-9]{9}$/, ['GET']],
  [new RegExp(`^/v1/hosts/${ID}/heartbeat$`), ['POST']],
  [new RegExp(`^/v1/hosts/${ID}/(sessions|audit)$`), ['GET']],
  [new RegExp(`^/v1/hosts/${ID}/events$`), ['GET'], true],
  [new RegExp(`^/v1/hosts/${ID}/pairing$`), ['GET', 'POST', 'DELETE']],
  [new RegExp(`^/v1/hosts/${ID}/trusted$`), ['GET', 'PUT', 'DELETE']],
  [/^\/v1\/pairings\/redeem$/, ['POST']],
  [/^\/v1\/sessions$/, ['POST']],
  [new RegExp(`^/v1/sessions/${ID}$`), ['GET']],
  [new RegExp(`^/v1/sessions/${ID}/(approve|reject|end|signal|connected)$`), ['POST']],
  [new RegExp(`^/v1/sessions/${ID}/ice$`), ['GET']],
  [new RegExp(`^/v1/sessions/${ID}/signals$`), ['GET'], true],
];

function json(status, error, extraHeaders = {}) {
  return new Response(JSON.stringify(typeof error === 'string' ? { error } : error), {
    status,
    headers: { ...SECURITY_HEADERS, 'content-type': 'application/json; charset=utf-8', ...extraHeaders },
  });
}

export function parseCoordinatorOrigin(value) {
  if (typeof value !== 'string' || value.length > 512 || value === '') return null;
  try {
    const url = new URL(value);
    // Configuration is publisher-owned, never read from a query, body or header.
    // Canonical origins avoid unexpected normalization and signature mismatches.
    if (url.protocol !== 'https:' || url.origin !== value || url.username || url.password ||
        url.search || url.hash || url.pathname !== '/' ||
        url.hostname === 'localhost' || url.hostname.endsWith('.localhost') ||
        url.hostname === '127.0.0.1' || url.hostname === '[::1]') return null;
    return url.origin;
  } catch { return null; }
}

export function validGatewayToken(value) {
  return typeof value === 'string' && /^[\x21-\x7e]{32,512}$/.test(value);
}

export function vercelClientIP(request, platformIsVercel) {
  // Trust this platform-owned field only inside the Vercel runtime. Generic
  // X-Forwarded-For, user-supplied gateway fields and multi-hop lists are ignored.
  // https://vercel.com/docs/headers/request-headers#x-vercel-forwarded-for
  if (!platformIsVercel) return null;
  const value = request.headers.get('x-vercel-forwarded-for') ?? '';
  if (value.includes('%')) return null;
  const kind = isIP(value);
  if (kind === 4) return value;
  if (kind !== 6) return null;
  const normalized = new URL(`https://[${value}]/`).hostname.slice(1, -1);
  const mapped = /^::ffff:([0-9a-f]{1,4}):([0-9a-f]{1,4})$/.exec(normalized);
  if (!mapped) return normalized;
  const hi = Number.parseInt(mapped[1], 16), lo = Number.parseInt(mapped[2], 16);
  return [hi >>> 8, hi & 255, lo >>> 8, lo & 255].join('.');
}

class BodyLimit extends Error {}

async function boundedBytes(stream, limit, signal) {
  if (!stream) return new Uint8Array();
  const reader = stream.getReader();
  const chunks = [];
  let size = 0;
  const abort = () => { void reader.cancel().catch(() => {}); };
  signal?.addEventListener('abort', abort, { once: true });
  try {
    while (true) {
      signal?.throwIfAborted();
      const { done, value } = await reader.read();
      signal?.throwIfAborted();
      if (done) break;
      size += value.byteLength;
      if (size > limit) {
        void reader.cancel().catch(() => {});
        throw new BodyLimit();
      }
      chunks.push(value);
    }
    const bytes = new Uint8Array(size);
    let offset = 0;
    for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.byteLength; }
    return bytes;
  } finally {
    signal?.removeEventListener('abort', abort);
    reader.releaseLock();
  }
}

function validQuery(url, cursorAllowed) {
  if (!url.search) return true;
  if (!cursorAllowed) return false;
  const entries = [...url.searchParams];
  return entries.length === 1 && entries[0][0] === 'after' &&
    /^(0|[1-9][0-9]{0,18})$/.test(entries[0][1]) &&
    BigInt(entries[0][1]) <= 9223372036854775807n;
}

export function createGateway({ coordinatorOrigin = '', gatewayToken = '', fetcher = fetch,
  clientIP = () => null, readAsset, timeoutMs = UPSTREAM_TIMEOUT_MS } = {}) {
  const upstream = parseCoordinatorOrigin(coordinatorOrigin);
  const configured = upstream !== null && validGatewayToken(gatewayToken);
  return async function gateway(request) {
    const url = new URL(request.url);
    if (url.protocol !== 'https:') return json(426, 'https_required');
    const method = request.method;
    const viewer = VIEWER_ASSETS.get(url.pathname);
    if (viewer) {
      if (method !== 'GET' && method !== 'HEAD') return json(405, 'method_not_allowed', { allow: 'GET, HEAD' });
      if (url.search) return json(400, 'url_parameters_not_supported');
      try {
        const body = await readAsset(viewer[0]);
        return new Response(method === 'HEAD' ? null : body, {
          headers: { ...SECURITY_HEADERS, 'content-type': viewer[1] },
        });
      } catch { return json(503, 'viewer_unavailable'); }
    }
    if (url.pathname === '/deployment-status') {
      if (method !== 'GET') return json(405, 'method_not_allowed', { allow: 'GET' });
      if (url.search) return json(400, 'url_parameters_not_supported');
      // This reports configuration only. It must never claim media connectivity.
      return json(200, { component: 'openremote-web-gateway', version: '0.8.0',
        coordinatorConfigured: configured && upstream !== url.origin,
        remoteAccessVerified: false });
    }
    const route = ROUTES.find(([pattern]) => pattern.test(url.pathname));
    if (!route) return json(404, 'not_found');
    if (!route[1].includes(method)) return json(405, 'method_not_allowed', { allow: route[1].join(', ') });
    if (!validQuery(url, route[2])) return json(400, 'invalid_query');
    if (!configured || upstream === url.origin) return json(503, 'service_not_configured');
    const address = clientIP(request);
    if (!address || !isIP(address)) return json(503, 'client_identity_unavailable');

    const deadline = AbortSignal.timeout(timeoutMs);
    const signal = AbortSignal.any([request.signal, deadline]);
    let body;
    try {
      const length = request.headers.get('content-length');
      if (length !== null && (!/^[0-9]+$/.test(length) || Number(length) > MAX_REQUEST_BYTES)) {
        return json(413, 'request_too_large');
      }
      body = await boundedBytes(request.body, MAX_REQUEST_BYTES, signal);
    } catch (error) {
      if (error instanceof BodyLimit) return json(413, 'request_too_large');
      return json(deadline.aborted ? 504 : 400, deadline.aborted ? 'coordinator_timeout' : 'invalid_request');
    }
    if (body.byteLength && !/^application\/json(?:\s*;\s*charset=utf-8)?$/i.test(request.headers.get('content-type') ?? '')) {
      return json(415, 'json_required');
    }
    const headers = new Headers({ accept: 'application/json',
      'x-openremote-gateway-token': gatewayToken,
      'x-openremote-client-ip': address });
    const auth = request.headers.get('authorization');
    if (auth) {
      if (auth.length > 8192) return json(431, 'header_too_large');
      headers.set('authorization', auth);
    }
    if (body.byteLength) headers.set('content-type', 'application/json');
    try {
      const response = await fetcher(upstream + url.pathname + url.search, {
        method, headers, body: body.byteLength ? body : undefined,
        redirect: 'error', cache: 'no-store', credentials: 'omit',
        referrerPolicy: 'no-referrer', signal,
      });
      if (response.status >= 300 && response.status < 400) {
        void response.body?.cancel().catch(() => {});
        return json(502, 'invalid_coordinator_response');
      }
      if (!/^application\/json(?:\s*;|$)/i.test(response.headers.get('content-type') ?? '')) {
        void response.body?.cancel().catch(() => {});
        return json(502, 'invalid_coordinator_response');
      }
      const bytes = await boundedBytes(response.body, MAX_RESPONSE_BYTES, signal);
      if (url.pathname === '/v1/info' && response.ok) {
        // Human, native and direct agent clients all discover through this route.
        // Do not let a partially configured backend advertise usable admission.
        // This checks declared capability, never actual TURN/media connectivity.
        try {
          const info = serverInfo(strictJson(new TextDecoder('utf-8', { fatal: true }).decode(bytes), 16_384));
          const relay = info.relay;
          if (info.activationVersion !== 1 || info.maxActivationConnections !== 4 ||
              info.automaticHostRegistrationVersion !== 1 || !relay ||
              ['configured', 'udp', 'tcp', 'tls'].some(key => typeof relay[key] !== 'boolean') ||
              !relay.configured || !relay.tls) return json(503, 'service_not_ready');
        } catch { return json(503, 'service_not_ready'); }
      }
      const responseHeaders = { ...SECURITY_HEADERS, 'content-type': 'application/json; charset=utf-8' };
      const retryAfter = response.headers.get('retry-after');
      if (retryAfter && /^[0-9]{1,5}$/.test(retryAfter)) responseHeaders['retry-after'] = retryAfter;
      return new Response(response.status === 204 || response.status === 304 ? null : bytes, {
        status: response.status, headers: responseHeaders,
      });
    } catch {
      // No credentials, upstream URLs, request bodies or raw exception strings
      // are logged or reflected. In particular, PIN redemption is never retried.
      if (request.signal.aborted) return json(499, 'request_cancelled');
      return json(deadline.aborted ? 504 : 502, deadline.aborted ? 'coordinator_timeout' : 'coordinator_unavailable');
    }
  };
}
