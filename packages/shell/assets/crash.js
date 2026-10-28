(() => {
  if (globalThis.moonkaleStartup) return;
  const key = 'moonkale.startup-health';
  const read = () => {
    try {
      const raw = localStorage.getItem(key) || '{}';
      if (raw.length > 128) return {};
      const value = JSON.parse(raw);
      return value && typeof value === 'object' && !Array.isArray(value) ? value : {};
    } catch { return {}; }
  };
  const write = (v) => { try { localStorage.setItem(key, JSON.stringify(v)); } catch {} };
  const previous = read();
  const failures = Math.min(255, (Number.isInteger(previous.failures) ? Math.max(0, previous.failures) : 0) + (previous.pending === true ? 1 : 0));
  write({ pending: true, failures });
  const safe = new URLSearchParams(location.search).has('safe');
  globalThis.moonkaleStartup = { safe, offer: failures >= 2 };
  let crashed = false;
  globalThis.moonkaleCrash = () => {
    crashed = true;
    if (document.getElementById('mk-crash-overlay')) return;
    const words = globalThis.moonkaleCrashWords;
    const overlay = document.createElement('div');
    overlay.id = 'mk-crash-overlay'; overlay.className = 'mk-crash-overlay'; overlay.setAttribute('role', 'alert');
    const text = document.createElement('p'); text.textContent = words[0]; overlay.append(text);
    for (const [label, action] of [
      [words[1], () => location.reload()],
      [words[2], () => navigator.clipboard?.writeText('Moonkale wasm client stopped. Reload restores the saved session.')],
      [words[3], () => { const url = new URL(location.href); url.searchParams.set('safe', ''); location.assign(url); }],
    ]) { const button = document.createElement('button'); button.className = 'mk-btn'; button.textContent = label; button.onclick = action; overlay.append(button); }
    (document.querySelector(".mk-frame") || document.body || document.documentElement).append(overlay);
    overlay.querySelector('button').focus();
  };
  // JS errors unrelated to wasm do not declare the client dead.
  const wasmFailure = e => e instanceof WebAssembly.RuntimeError;
  addEventListener('error', e => { if (wasmFailure(e.error)) globalThis.moonkaleCrash(); });
  addEventListener('unhandledrejection', e => { if (wasmFailure(e.reason)) globalThis.moonkaleCrash(); });
  globalThis.moonkaleStartup.healthy = () => { if (!crashed) write({pending: false, failures: 0}); };
})();
