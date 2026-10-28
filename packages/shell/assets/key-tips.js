// Presentation only: shortcut text comes from the Rust command registry.
// Fixed DOM APIs keep labels as text, never script or HTML.
(() => {
  if (window.__mkKeyTips) return;
  window.__mkKeyTips = true;
  const mac = /Mac|iPhone|iPad/.test(navigator.platform + navigator.userAgent);
  let timer = null, held = false, layer = null;
  const pressed = new Set();
  const typing = target => target instanceof Element &&
    !!target.closest('input, textarea, select, [contenteditable="true"], .cm-editor, .mk-native-surface, .mk-terminal');
  const hide = () => { clearTimeout(timer); timer = null; layer?.remove(); layer = null; };
  const show = () => {
    if (!held || typing(document.activeElement)) return;
    layer = document.createElement('div');
    layer.className = 'mk-key-tips';
    layer.setAttribute('aria-hidden', 'true');
    for (const button of document.querySelectorAll('button, [role="button"]')) {
      const shortcut = button.dataset.shortcut || button.querySelector("[data-shortcut]")?.dataset.shortcut;
      const rect = button.getBoundingClientRect();
      if (!shortcut || !button.getClientRects().length ||
          getComputedStyle(button).visibility === 'hidden' || rect.bottom <= 0 || rect.right <= 0 ||
          rect.top >= innerHeight || rect.left >= innerWidth) continue;
      const center = document.elementFromPoint(
        Math.max(0, Math.min((rect.left + rect.right) / 2, innerWidth - 1)),
        Math.max(0, Math.min((rect.top + rect.bottom) / 2, innerHeight - 1)));
      if (!center || !button.contains(center)) continue;
      const badge = document.createElement('span');
      badge.className = 'mk-key-tip';
      badge.textContent = shortcut;
      badge.style.left = `${Math.max(0, Math.min(rect.left, innerWidth - 120))}px`;
      badge.style.top = `${Math.max(0, Math.min(rect.bottom - 4, innerHeight - 24))}px`;
      layer.append(badge);
    }
    document.querySelector('.mk-frame')?.append(layer);
  };
  document.addEventListener('keydown', e => {
    pressed.add(e.key);
    const primary = mac ? e.key === 'Meta' : e.key === 'Control';
    if (primary && pressed.size === 1 && !e.repeat && !held && !e.shiftKey && !e.altKey &&
        !(mac ? e.ctrlKey : e.metaKey) && !typing(e.target)) {
      held = true;
      timer = setTimeout(show, 500);
    } else if (!primary) hide();
  }, true);
  document.addEventListener('keyup', e => {
    pressed.delete(e.key);
    if (mac ? e.key === 'Meta' : e.key === 'Control') held = false;
    hide();
  }, true);
  window.addEventListener('blur', () => { held = false; pressed.clear(); hide(); });
  document.addEventListener('visibilitychange', () => { held = false; pressed.clear(); hide(); });
  document.addEventListener('focusin', e => { if (typing(e.target)) hide(); }, true);
  document.addEventListener('scroll', hide, true);
  window.addEventListener('resize', hide);
})();
