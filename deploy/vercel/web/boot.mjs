import { Coordinator, requireThat, serverInfo } from './core.mjs';

// A deployed page alone is not a connection service. Discovery must pass the
// production decoder before the page loads any controller or accepts a PIN.
export function createViewerReadinessGate({ document, origin, fetcher = fetch,
  importController = () => import('./app.js') }) {
  const code = document.getElementById('code');
  const pin = document.getElementById('pin');
  const connect = document.getElementById('connect');
  const cancel = document.getElementById('cancel');
  const form = document.getElementById('connect-form');
  const status = document.getElementById('status');
  const api = new Coordinator(origin, fetcher);
  let phase = 'pending', inFlight = null;

  function message(text, kind = '') {
    status.textContent = text;
    status.className = 'status' + (kind ? ' ' + kind : '');
    status.hidden = !text;
  }
  function lock() {
    code.disabled = true;
    pin.disabled = true;
    pin.value = '';
    connect.disabled = true;
    connect.hidden = false;
    connect.type = 'button';
    cancel.hidden = true;
    cancel.disabled = true;
  }
  function blockSubmit(event) {
    if (phase !== 'ready') {
      event.preventDefault();
      event.stopImmediatePropagation();
    }
  }
  function retry(event) {
    if (phase !== 'ready') {
      event.preventDefault();
      event.stopImmediatePropagation();
      if (phase === 'unavailable') void check();
    }
  }
  form.addEventListener('submit', blockSubmit, true);
  connect.addEventListener('click', retry, true);
  lock();
  connect.firstChild.textContent = 'Checking connection service… ';
  message('Checking the connection service…');

  async function runCheck() {
    phase = 'checking';
    lock();
    connect.firstChild.textContent = 'Checking connection service… ';
    message('Checking the connection service…');
    try {
      const info = serverInfo(await api.request('/v1/info', { timeout: 10_000 }));
      const relay = info.relay;
      requireThat(info.activationVersion === 1 && info.maxActivationConnections === 4 &&
        info.automaticHostRegistrationVersion === 1 && relay !== null &&
        typeof relay === 'object' && !Array.isArray(relay) &&
        ['configured', 'udp', 'tcp', 'tls'].every(key => typeof relay[key] === 'boolean') &&
        relay.configured && relay.tls);
    } catch {
      phase = 'unavailable';
      lock();
      message('Remote access is not available yet. Please try again shortly.', 'error');
      connect.firstChild.textContent = 'Retry connection check ';
      connect.disabled = false;
      return false;
    }

    phase = 'loading';
    connect.firstChild.textContent = 'Loading remote desktop… ';
    let loadingTimer;
    try {
      await Promise.race([importController(), new Promise((_, reject) => {
        loadingTimer = setTimeout(() => reject(new Error('Controller load timed out.')), 10_000);
      })]);
    } catch {
      phase = 'failed';
      lock();
      connect.firstChild.textContent = 'Remote desktop unavailable ';
      message('OpenRemote could not load. Refresh this page to try again.', 'error');
      return false;
    } finally {
      clearTimeout(loadingTimer);
    }
    phase = 'ready';
    form.removeEventListener('submit', blockSubmit, true);
    connect.removeEventListener('click', retry, true);
    code.disabled = false;
    pin.disabled = false;
    connect.type = 'submit';
    connect.disabled = false;
    connect.firstChild.textContent = 'Connect remote desktop ';
    message('');
    return true;
  }

  function check() {
    if (phase === 'ready') return Promise.resolve(true);
    if (phase === 'failed') return Promise.resolve(false);
    if (!inFlight) inFlight = runCheck().finally(() => { inFlight = null; });
    return inFlight;
  }
  return Object.freeze({ check });
}

if (typeof document !== 'undefined' && typeof window !== 'undefined') {
  void createViewerReadinessGate({ document, origin: location.origin }).check();
}
