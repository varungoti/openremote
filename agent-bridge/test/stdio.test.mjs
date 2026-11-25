import test from 'node:test';
import assert from 'node:assert/strict';
import { PassThrough } from 'node:stream';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { fileURLToPath } from 'node:url';
import { DesktopBridge } from '../src/bridge.mjs';
import { serveBridge } from '../src/server.mjs';

function lineReader(output) {
  let buffer = '';
  const messages = [];
  const waiters = new Map();
  output.setEncoding('utf8');
  output.on('data', chunk => {
    buffer += chunk;
    for (;;) {
      const end = buffer.indexOf('\n');
      if (end < 0) break;
      const message = JSON.parse(buffer.slice(0, end));
      buffer = buffer.slice(end + 1);
      messages.push(message);
      const waiter = waiters.get(message.id);
      if (waiter) { waiters.delete(message.id); waiter(message); }
    }
  });
  return { messages, response(id) {
    const existing = messages.find(m => m.id === id);
    if (existing) return Promise.resolve(existing);
    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => { waiters.delete(id); reject(new Error('protocol response timeout')); }, 5000);
      waiters.set(id, value => { clearTimeout(timeout); resolve(value); });
    });
  } };
}

async function initialize(input, reader) {
  input.write(JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '2025-11-25', capabilities: {}, clientInfo: { name: 'openremote-wire-test', version: '1.0' } } }) + '\n');
  const result = await reader.response(1);
  assert.equal(result.result.serverInfo.name, 'openremote-agent-bridge');
  input.write(JSON.stringify({ jsonrpc: '2.0', method: 'notifications/initialized' }) + '\n');
}

test('real stdio MCP framing lists only bounded desktop tools and validates negative arguments', async () => {
  const input = new PassThrough();
  const output = new PassThrough();
  const bridge = new DesktopBridge({ origin: null, createAdapter: async () => assert.fail('browser must not launch') });
  const handle = serveBridge(bridge, { input, output });
  const reader = lineReader(output);
  try {
    await initialize(input, reader);
    input.write(JSON.stringify({ jsonrpc: '2.0', id: 2, method: 'tools/list', params: {} }) + '\n');
    const listed = await reader.response(2);
    assert.deepEqual(listed.result.tools.map(t => t.name).sort(), ['click', 'connect', 'disconnect', 'key', 'move', 'screenshot', 'scroll', 'status', 'type_text']);
    assert.equal(listed.result.tools.find(t => t.name === 'connect').inputSchema.additionalProperties, false);
    const cases = [
      ['connect', { pcId: '123456789', pin: 'SENSITIVE_INVALID_PIN' }],
      ['connect', { pcId: '123456789', pin: '001234', url: 'https://arbitrary.invalid' }],
      ['connect', { pcId: '123456789', pin: '001234', name: 'x'.repeat(65) }],
      ['click', { frameToken: 'x'.repeat(36), x: -1, y: 0 }],
      ['key', { frameToken: 'x'.repeat(36), codes: ['Enter', 'Enter'] }],
      ['key', { frameToken: 'x'.repeat(36), codes: ['arbitrary_script()'] }],
      ['scroll', { frameToken: 'x'.repeat(36), dx: 0, dy: 1001 }],
      ['type_text', { frameToken: 'x'.repeat(36), text: 'x'.repeat(1001) }]
    ];
    let id = 10;
    for (const [name, args] of cases) {
      input.write(JSON.stringify({ jsonrpc: '2.0', id, method: 'tools/call', params: { name, arguments: args } }) + '\n');
      const response = await reader.response(id++);
      assert.ok(response.error || response.result?.isError);
      assert.equal(JSON.stringify(response).includes('SENSITIVE_INVALID_PIN'), false);
    }
    input.write(JSON.stringify({ jsonrpc: '2.0', id: 30, method: 'tools/call', params: { name: 'connect', arguments: { pcId: '123456789', pin: '001234' } } }) + '\n');
    const unavailable = await reader.response(30);
    assert.equal(unavailable.result.structuredContent.error, 'service_not_configured');
    assert.equal(JSON.stringify(unavailable).includes('001234'), false);
    input.write(JSON.stringify({ jsonrpc: '2.0', id: 31, method: 'tools/call', params: { name: 'evaluate', arguments: { script: 'process.exit()' } } }) + '\n');
    const absent = await reader.response(31);
    assert.ok(absent.error || absent.result?.isError);
  } finally { input.end(); await handle.close(); await bridge.close(); }
});

test('stdio cancellation and EOF tear down the actual tool handler resource', async () => {
  let closed = 0;
  let started;
  const entered = new Promise(resolve => { started = resolve; });
  const bridge = new DesktopBridge({ origin: 'https://service.example', createAdapter: async () => ({ invoke: async () => { started(); return new Promise(() => {}); }, close: async () => { closed++; return { stopConfirmed: false }; } }) });
  const input = new PassThrough();
  const output = new PassThrough();
  const reader = lineReader(output);
  const handle = serveBridge(bridge, { input, output });
  await initialize(input, reader);
  input.write(JSON.stringify({ jsonrpc: '2.0', id: 2, method: 'tools/call', params: { name: 'connect', arguments: { pcId: '123456789', pin: '001234' } } }) + '\n');
  await entered;
  input.write(JSON.stringify({ jsonrpc: '2.0', method: 'notifications/cancelled', params: { requestId: 2, reason: 'test cancel' } }) + '\n');
  await new Promise(resolve => setTimeout(resolve, 20));
  assert.equal(closed, 1);
  assert.equal(reader.messages.some(m => m.id === 2), false);
  input.end();
  await handle.close();
  assert.equal(bridge.closed, true);
});

test('modern 2026 stdio requests use their per-request envelope without a legacy handshake', async () => {
  const input = new PassThrough();
  const output = new PassThrough();
  const bridge = new DesktopBridge({ origin: null, createAdapter: async () => assert.fail('browser must not launch') });
  const reader = lineReader(output);
  const handle = serveBridge(bridge, { input, output });
  const meta = { 'io.modelcontextprotocol/protocolVersion': '2026-07-28', 'io.modelcontextprotocol/clientCapabilities': {}, 'io.modelcontextprotocol/clientInfo': { name: 'modern-wire-test', version: '1.0' } };
  try {
    input.write(JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'tools/list', params: { _meta: meta } }) + '\n');
    const listed = await reader.response(1);
    assert.equal(listed.result.tools.length, 9);
    input.write(JSON.stringify({ jsonrpc: '2.0', id: 2, method: 'tools/call', params: { _meta: meta, name: 'status', arguments: {} } }) + '\n');
    assert.equal((await reader.response(2)).result.structuredContent.state, 'idle');
  } finally { input.end(); await handle.close(); await bridge.close(); }
});

test('EOF closes a ready browser resource and removes all local input state', async () => {
  let closed = 0;
  const bridge = new DesktopBridge({ origin: 'https://service.example', createAdapter: async () => ({ invoke: async () => ({ state: 'connected', pcId: '123456789', hostName: 'Test PC', canControl: true }), close: async () => { closed++; return { stopConfirmed: true }; } }) });
  const input = new PassThrough();
  const output = new PassThrough();
  const reader = lineReader(output);
  const handle = serveBridge(bridge, { input, output });
  await initialize(input, reader);
  input.write(JSON.stringify({ jsonrpc: '2.0', id: 2, method: 'tools/call', params: { name: 'connect', arguments: { pcId: '123456789', pin: '001234' } } }) + '\n');
  assert.equal((await reader.response(2)).result.structuredContent.canControl, true);
  input.end();
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(closed, 1);
  assert.equal(bridge.closed, true);
  assert.equal(bridge.adapter, null);
  await handle.close();
});

test('oversized stdio frame closes the bounded transport', async () => {
  const input = new PassThrough();
  const output = new PassThrough();
  const bridge = new DesktopBridge({ origin: null, createAdapter: async () => assert.fail('browser must not launch') });
  const handle = serveBridge(bridge, { input, output });
  const reader = lineReader(output);
  await initialize(input, reader);
  input.write('x'.repeat(65_537));
  await new Promise(resolve => setTimeout(resolve, 20));
  assert.equal(bridge.closed, true);
  await handle.close();
});

test('production child process supports initialize/status and exits on stdin EOF without extra output', async () => {
  const entry = fileURLToPath(new URL('../src/index.mjs', import.meta.url));
  const child = spawn(process.execPath, [entry], { stdio: ['pipe', 'pipe', 'pipe'] });
  const reader = lineReader(child.stdout);
  let stderr = '';
  child.stderr.setEncoding('utf8');
  child.stderr.on('data', value => { stderr += value; });
  try {
    await initialize(child.stdin, reader);
    child.stdin.write(JSON.stringify({ jsonrpc: '2.0', id: 2, method: 'tools/call', params: { name: 'status', arguments: {} } }) + '\n');
    const result = await reader.response(2);
    assert.equal(result.result.structuredContent.state, 'idle');
    const closed = once(child, 'exit');
    child.stdin.end();
    assert.deepEqual(await closed, [0, null]);
    assert.equal(stderr, '');
  } finally { if (child.exitCode === null) child.kill('SIGTERM'); }
});
