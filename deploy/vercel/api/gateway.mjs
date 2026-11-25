import { readFile } from 'node:fs/promises';
import { createGateway, vercelClientIP } from '../lib/gateway.mjs';

// These files are included in the Function bundle, not published as static
// files. Every request therefore gets the same method, query and CSP checks.
const assetCache = new Map();
async function readAsset(name) {
  if (!assetCache.has(name)) {
    const bytes = await readFile(new URL(`../dist/connect/${name}`, import.meta.url));
    assetCache.set(name, bytes);
  }
  return assetCache.get(name);
}

const gateway = createGateway({
  coordinatorOrigin: process.env.OPENREMOTE_COORDINATOR_ORIGIN ?? '',
  gatewayToken: process.env.OPENREMOTE_GATEWAY_TOKEN ?? '',
  clientIP: request => vercelClientIP(request, process.env.VERCEL === '1'),
  readAsset,
});

export default { fetch: gateway };
