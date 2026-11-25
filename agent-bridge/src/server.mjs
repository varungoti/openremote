import { McpServer } from '@modelcontextprotocol/server';
import { StdioServerTransport, serveStdio } from '@modelcontextprotocol/server/stdio';
import * as z from 'zod/v4';
import { publicError } from './errors.mjs';

const noArguments = z.strictObject({});
const frameToken = z.string().min(16).max(256);
const coordinate = z.number().int().min(0).max(32767);
const keyCode = z.string().regex(/^(?:Key[A-Z]|Digit[0-9]|F(?:[1-9]|1[0-2])|Enter|Tab|Space|Backspace|Delete|Insert|Home|End|PageUp|PageDown|Arrow(?:Left|Right|Up|Down)|(?:Control|Alt|Shift|Meta)(?:Left|Right)|Escape)$/);
const name = z.string().min(1).max(64).refine(value => value.trim() === value && Buffer.byteLength(value) <= 64 && !/[\p{Cc}\p{Cf}]/u.test(value), 'Use a short plain display name.');
const inputAnnotations = { readOnlyHint: false, destructiveHint: true, idempotentHint: false, openWorldHint: true };
const readAnnotations = { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: true };

const definitions = [
  ['connect', 'Connect to an owner-activated PC using its permanent nine-digit PC ID and six-digit PIN. The host sees an AI Agent label. Waits for decoded desktop media; additional peers may be view-only.', z.strictObject({ pcId: z.string().regex(/^\d{9}$/), pin: z.string().regex(/^\d{6}$/), name: name.default('OpenRemote MCP') }), inputAnnotations],
  ['status', 'Read the current connection state and whether this session may control input. Display names are claims, not verified identities.', noArguments, readAnnotations],
  ['screenshot', 'Capture the current desktop image. Returns an observation token required for one input action within 15 seconds. View-only sessions can capture images.', noArguments, readAnnotations],
  ['click', 'Click a pixel inside the last screenshot. Requires its unused, fresh observation token and input permission.', z.strictObject({ frameToken, x: coordinate, y: coordinate, button: z.enum(['left', 'right', 'middle']).default('left') }), inputAnnotations],
  ['move', 'Move the pointer to a pixel inside the last screenshot. Requires its unused, fresh observation token and input permission.', z.strictObject({ frameToken, x: coordinate, y: coordinate }), inputAnnotations],
  ['scroll', 'Scroll the remote desktop by bounded horizontal/vertical deltas. Requires an unused, fresh screenshot token and input permission.', z.strictObject({ frameToken, dx: z.number().int().min(-1000).max(1000), dy: z.number().int().min(-1000).max(1000) }), inputAnnotations],
  ['key', 'Press and release up to five distinct supported DOM key codes as one chord. Requires an unused, fresh screenshot token and input permission.', z.strictObject({ frameToken, codes: z.array(keyCode).min(1).max(5).refine(value => new Set(value).size === value.length, 'Key codes must be distinct.') }), inputAnnotations],
  ['type_text', 'Type bounded text into the remote desktop using its input channel. Requires an unused, fresh screenshot token and input permission.', z.strictObject({ frameToken, text: z.string().min(1).max(1000).refine(value => Buffer.byteLength(value) <= 4096 && !/[\u0000\ud800-\udfff]/u.test(value), 'Text must be bounded valid Unicode without NUL.') }), inputAnnotations],
  ['disconnect', 'Close this local desktop connection and request remote session end. stopConfirmed=false means remote end was not acknowledged; it does not leave local input enabled.', noArguments, { ...inputAnnotations, destructiveHint: false, idempotentHint: true }]
];

function textResult(value) {
  return { content: [{ type: 'text', text: JSON.stringify(value) }], structuredContent: value };
}

export function createMcpServer(bridge) {
  // Omit SDK diagnostics rather than risking credential-bearing exception text.
  const server = new McpServer({ name: 'openremote-agent-bridge', version: '0.8.0' }, { logger: {} });
  for (const [toolName, description, inputSchema, annotations] of definitions) {
    server.registerTool(toolName, { description, inputSchema, annotations }, async (args, context) => {
      const signal = context.mcpReq.signal;
      try {
        if (toolName === 'disconnect') return textResult(await bridge.disconnect());
        if (toolName === 'connect') return textResult(await bridge.connect({ ...args }, signal));
        if (toolName === 'status') return textResult(await bridge.status(signal));
        if (toolName === 'screenshot') {
          const { data, mimeType, ...metadata } = await bridge.screenshot(signal);
          return { content: [{ type: 'text', text: JSON.stringify(metadata) }, { type: 'image', mimeType, data }], structuredContent: metadata };
        }
        return textResult(await bridge.input({ ...args, type: toolName === 'type_text' ? 'text' : toolName }, signal));
      } catch (error) {
        return { ...textResult(publicError(error)), isError: true };
      }
    });
  }
  server.server.onclose = () => { void bridge.close(); };
  server.server.onerror = () => {}; // protocol/library errors never echo raw request data
  return server;
}

export function serveBridge(bridge, { input = process.stdin, output = process.stdout } = {}) {
  const transport = new StdioServerTransport(input, output, { maxBufferSize: 65_536 });
  return serveStdio(() => createMcpServer(bridge), { transport, maxSubscriptions: 0, onerror: () => {} });
}
