#!/usr/bin/env node
import { DesktopBridge } from './bridge.mjs';
import { createBrowserAdapter } from './browser.mjs';
import { loadServiceOrigin } from './profile.mjs';
import { serveBridge } from './server.mjs';
import { publicError } from './errors.mjs';

// Credentials and service addresses are accepted neither in argv nor env.
if (process.argv.length !== 2) {
  process.stderr.write('OpenRemote agent bridge accepts no command-line arguments.\n');
  process.exitCode = 1;
} else {
  try {
    const bridge = new DesktopBridge({ origin: await loadServiceOrigin(), createAdapter: createBrowserAdapter });
    const handle = serveBridge(bridge);
    let stopping;
    const stop = () => {
      stopping ??= Promise.allSettled([bridge.close(), handle.close()]);
      return stopping;
    };
    process.once('SIGINT', () => { void stop(); });
    process.once('SIGTERM', () => { void stop(); });
    process.stdin.once('end', () => { void stop(); });
    process.stdin.once('close', () => { void stop(); });
  } catch (error) {
    process.stderr.write(publicError(error).message + '\n');
    process.exitCode = 1;
  }
}
