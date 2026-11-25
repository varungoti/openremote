import { BridgeError } from './errors.mjs';

const methods = new Set(['connect', 'status', 'screenshot', 'input', 'disconnect']);

export async function createBrowserAdapter({ origin, signal }) {
  if (!origin) throw new BridgeError('service_not_configured');
  // Playwright protocol debugging can print evaluate arguments, including a
  // login PIN. Refuse that developer mode before loading the browser library.
  if (process.env.DEBUG || process.env.PWDEBUG) throw new BridgeError('debug_logging_disabled');
  const { chromium } = await import('playwright');
  signal?.throwIfAborted();
  // Keep Chromium's sandbox enabled. The bridge has no launch flags, remote
  // debugging listener, persistent profile, user URL or arbitrary script tool.
  const browser = await chromium.launch({ headless: true, chromiumSandbox: true, timeout: 15_000 });
  let page;
  let closing;
  let closed = false;
  async function close() {
    if (closing) return closing;
    closed = true;
    closing = (async () => {
      let stopConfirmed = page ? false : null;
      if (page && !page.isClosed()) {
        let timer;
        try {
          const result = await Promise.race([
            page.evaluate(async () => (await import('/connect/app.js')).agentAPI.disconnect()),
            new Promise(resolve => { timer = setTimeout(resolve, 750); })
          ]);
          if (result && (typeof result.stopConfirmed === 'boolean' || result.stopConfirmed === null)) stopConfirmed = result.stopConfirmed;
        } catch {
          // Closing the owned browser remains mandatory after a failed end call.
        } finally {
          clearTimeout(timer);
        }
      }
      await browser.close().catch(() => {});
      return { stopConfirmed };
    })();
    return closing;
  }
  const abort = () => { void close(); };
  signal?.addEventListener('abort', abort, { once: true });
  try {
    signal?.throwIfAborted();
    const context = await browser.newContext({ acceptDownloads: false, serviceWorkers: 'block' });
    await context.route('**/*', async route => {
      try {
        if (new URL(route.request().url()).origin !== origin) return await route.abort('blockedbyclient');
        await route.continue();
      } catch { await route.abort().catch(() => {}); }
    });
    page = await context.newPage();
    await page.goto(origin + '/connect', { waitUntil: 'domcontentloaded', timeout: 15_000 });
    if (new URL(page.url()).origin !== origin) throw new Error();
    signal?.throwIfAborted();
    return {
      async invoke(method, args) {
        if (!methods.has(method)) throw new BridgeError('invalid_method');
        if (closed || page.isClosed() || new URL(page.url()).origin !== origin) throw new BridgeError('not_connected');
        return page.evaluate(async ({ method, args }) => {
          const { agentAPI } = await import('/connect/app.js');
          switch (method) {
            case 'connect': return agentAPI.connect(args);
            case 'status': return agentAPI.status();
            case 'screenshot': return agentAPI.screenshot();
            case 'input': return agentAPI.input(args);
            case 'disconnect': return agentAPI.disconnect();
            default: throw new Error('unsupported operation');
          }
        }, { method, args });
      },
      close: async () => {
        signal?.removeEventListener('abort', abort);
        return close();
      }
    };
  } catch {
    signal?.removeEventListener('abort', abort);
    await close();
    throw new BridgeError('connection_failed');
  }
}
