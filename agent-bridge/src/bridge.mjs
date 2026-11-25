import { BridgeError } from './errors.mjs';

const idleStatus = () => ({ state: 'idle', pcId: '', hostName: '', canControl: false });
const states = new Set(['idle', 'connecting', 'connected', 'view_only']);
const maxImageBytes = 16 * 1024 * 1024;
const maxImagePixels = 8_294_400;

function displayName(value) {
  if (typeof value !== 'string') return '';
  let out = '';
  for (const char of value.replace(/[\p{Cc}\p{Cf}]/gu, '').trim()) {
    if (Buffer.byteLength(out + char) > 80) break;
    out += char;
  }
  return out;
}

export function sanitizeStatus(value) {
  const pcId = value?.pcId ?? '';
  if (!value || !states.has(value.state) || typeof value.canControl !== 'boolean' ||
      typeof pcId !== 'string' || (pcId !== '' && !/^\d{9}$/.test(pcId)) ||
      (['connected', 'view_only'].includes(value.state) && pcId === '') ||
      (value.canControl && value.state !== 'connected')) throw new BridgeError('invalid_status');
  return { state: value.state, pcId, hostName: displayName(value.hostName), canControl: value.canControl };
}

export function validateFrame(value) {
  if (!value || value.mimeType !== 'image/png' || typeof value.data !== 'string' ||
      value.data.length > Math.ceil(maxImageBytes / 3) * 4 || !/^[A-Za-z0-9+/]+={0,2}$/.test(value.data) ||
      !Number.isInteger(value.width) || value.width < 1 || value.width > 4096 ||
      !Number.isInteger(value.height) || value.height < 1 || value.height > 4096 ||
      value.width * value.height > maxImagePixels ||
      typeof value.frameToken !== 'string' || value.frameToken.length < 16 || value.frameToken.length > 256 ||
      /[\p{Cc}\p{Cf}]/u.test(value.frameToken)) throw new BridgeError('invalid_frame');
  const raw = Buffer.from(value.data, 'base64');
  if (raw.length < 24 || raw.length > maxImageBytes || raw.toString('base64') !== value.data ||
      raw.subarray(0, 8).toString('hex') !== '89504e470d0a1a0a' || raw.toString('ascii', 12, 16) !== 'IHDR' ||
      raw.readUInt32BE(16) !== value.width || raw.readUInt32BE(20) !== value.height) throw new BridgeError('invalid_frame');
  return { mimeType: 'image/png', data: value.data, width: value.width, height: value.height, frameToken: value.frameToken };
}

export class DesktopBridge {
  constructor({ origin, createAdapter, now = Date.now, idleMs = 300_000, operationMs = 10_000, connectMs = 45_000 }) {
    this.origin = origin;
    this.createAdapter = createAdapter;
    this.now = now;
    this.idleMs = idleMs;
    this.operationMs = operationMs;
    this.connectMs = connectMs;
    this.generation = 0;
    this.abort = new AbortController();
    this.adapter = null;
    this.frame = null;
    this.queue = Promise.resolve();
    this.pending = 0;
    this.closed = false;
  }

  async close() {
    this.closed = true;
    await this.disconnect();
  }

  disconnect() {
    this.generation++;
    this.abort.abort();
    this.abort = new AbortController();
    clearTimeout(this.idleTimer);
    this.frame = null;
    const adapter = this.adapter;
    this.adapter = null;
    return this.closeAdapter(adapter).then(stopConfirmed => ({ ...idleStatus(), stopConfirmed }));
  }

  async closeAdapter(adapter) {
    if (!adapter) return null;
    let timer;
    try {
      const result = await Promise.race([
        Promise.resolve().then(() => adapter.close()).catch(() => ({ stopConfirmed: false })),
        new Promise(resolve => { timer = setTimeout(() => resolve({ stopConfirmed: false }), 5000); })
      ]);
      return result && (typeof result.stopConfirmed === 'boolean' || result.stopConfirmed === null) ? result.stopConfirmed : false;
    } finally { clearTimeout(timer); }
  }

  touch(generation) {
    clearTimeout(this.idleTimer);
    if (generation !== this.generation || !this.adapter) return;
    this.idleTimer = setTimeout(() => {
      if (generation === this.generation) void this.disconnect();
    }, this.idleMs);
    this.idleTimer.unref?.();
  }

  enqueue(generation, signal, work, timeout = this.operationMs) {
    if (this.closed) return Promise.reject(new BridgeError('bridge_closed'));
    if (this.pending >= 16) return Promise.reject(new BridgeError('bridge_busy'));
    this.pending++;
    const sessionSignal = this.abort.signal;
    const cancel = () => { if (generation === this.generation) void this.disconnect(); };
    signal?.addEventListener('abort', cancel, { once: true });
    const result = this.queue.then(async () => {
      if (generation !== this.generation || signal?.aborted || sessionSignal.aborted) throw new BridgeError('operation_cancelled');
      let timer;
      let abortListener;
      try {
        const result = await Promise.race([
          Promise.resolve().then(work),
          new Promise((_, reject) => {
            abortListener = () => reject(new BridgeError('operation_cancelled'));
            sessionSignal.addEventListener('abort', abortListener, { once: true });
          }),
          new Promise((_, reject) => {
            timer = setTimeout(() => {
              reject(new BridgeError('operation_timed_out'));
              cancel();
            }, timeout);
          })
        ]);
        if (generation !== this.generation || signal?.aborted) throw new BridgeError('operation_cancelled');
        this.touch(generation);
        return result;
      } finally {
        clearTimeout(timer);
        sessionSignal.removeEventListener('abort', abortListener);
      }
    });
    this.queue = result.catch(() => {});
    return result.catch(async error => {
      const recoverable = error instanceof BridgeError && ['view_only', 'not_connected', 'fresh_frame_required', 'invalid_coordinates'].includes(error.code);
      if (!recoverable && generation === this.generation) await this.disconnect();
      throw error;
    }).finally(() => {
      this.pending--;
      signal?.removeEventListener('abort', cancel);
    });
  }

  connect(args, signal) {
    const cleanup = this.disconnect();
    const generation = this.generation;
    const sessionSignal = this.abort.signal;
    return this.enqueue(generation, signal, async () => {
      await cleanup;
      if (!this.origin) throw new BridgeError('service_not_configured');
      let adapter;
      try {
        adapter = await this.createAdapter({ origin: this.origin, signal: sessionSignal });
        if (generation !== this.generation || sessionSignal.aborted) {
          await this.closeAdapter(adapter);
          throw new BridgeError('operation_cancelled');
        }
        this.adapter = adapter;
        const status = sanitizeStatus(await adapter.invoke('connect', args));
        if (!['connected', 'view_only'].includes(status.state) || status.pcId !== args.pcId) throw new BridgeError('connection_failed');
        return status;
      } catch (error) {
        if (generation === this.generation) await this.disconnect();
        throw error instanceof BridgeError ? error : new BridgeError('connection_failed');
      } finally {
        // JavaScript strings cannot be securely erased, but credentials are not
        // retained as bridge state, diagnostics, files, CLI args or environment.
        args.pin = '';
      }
    }, this.connectMs).finally(() => { args.pin = ''; });
  }

  status(signal) {
    const generation = this.generation;
    return this.enqueue(generation, signal, async () => {
      if (!this.adapter) return idleStatus();
      return sanitizeStatus(await this.adapter.invoke('status'));
    });
  }

  screenshot(signal) {
    const generation = this.generation;
    return this.enqueue(generation, signal, async () => {
      if (!this.adapter) throw new BridgeError('not_connected');
      const adapter = this.adapter;
      const status = sanitizeStatus(await adapter.invoke('status'));
      if (generation !== this.generation) throw new BridgeError('operation_cancelled');
      if (!['connected', 'view_only'].includes(status.state)) throw new BridgeError('not_connected');
      const frame = validateFrame(await adapter.invoke('screenshot'));
      if (generation !== this.generation) throw new BridgeError('operation_cancelled');
      this.frame = { generation, token: frame.frameToken, pcId: status.pcId, width: frame.width, height: frame.height, at: this.now() };
      return frame;
    });
  }

  input(args, signal) {
    const generation = this.generation;
    return this.enqueue(generation, signal, async () => {
      if (!this.adapter) throw new BridgeError('not_connected');
      const adapter = this.adapter;
      const status = sanitizeStatus(await adapter.invoke('status'));
      if (generation !== this.generation) throw new BridgeError('operation_cancelled');
      if (status.state === 'view_only') throw new BridgeError('view_only');
      if (status.state !== 'connected' || !status.canControl) throw new BridgeError('not_connected');
      const frame = this.frame;
      if (!frame || frame.token !== args.frameToken || frame.generation !== generation || frame.pcId !== status.pcId ||
          this.now() - frame.at < 0 || this.now() - frame.at > 15_000) throw new BridgeError('fresh_frame_required');
      if (['click', 'move'].includes(args.type) && (args.x >= frame.width || args.y >= frame.height)) throw new BridgeError('invalid_coordinates');
      this.frame = null;
      await adapter.invoke('input', args);
      return { ok: true };
    });
  }
}
