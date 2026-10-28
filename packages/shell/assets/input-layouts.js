// The DOM supplies physical codes, layout labels and button identities only.
// Rust resolves commands and owns navigation and settings.
const typing = target => target instanceof Element && !!target.closest(
  'input,textarea,select,[contenteditable]:not([contenteditable="false"]),.cm-editor,.mk-native-surface,.mk-terminal,.mk-tn');
const primary = e => /Mac|iPhone|iPad/.test(navigator.platform) ? e.metaKey : e.ctrlKey;
const reportLayout = async () => {
  try {
    const map = await navigator.keyboard?.getLayoutMap();
    if (map) dioxus.send({kind: 'layout', labels: Object.fromEntries([...map].filter(([code, value]) => code.length <= 32 && value.length <= 16))});
  } catch (_) { /* Unsupported/denied platforms retain US labels. */ }
};
void reportLayout();
navigator.keyboard?.addEventListener?.('layoutchange', reportLayout);
document.addEventListener('keydown', e => {
  if (e.isComposing || e.getModifierState('AltGraph')) return;
  const bindings = window.moonkaleInputBindings || [];
  const match = bindings.find(b => b.ctrl === primary(e) && b.shift === e.shiftKey && b.alt === e.altKey &&
    (b.code ? b.code === e.code : b.key.toLowerCase() === e.key.toLowerCase()));
  // Capture positional punctuation, macro keys and registered Ctrl+Alt
  // chords. Other keys keep their editor handlers and Rust bubbling dispatcher.
  if (e.target.closest?.('.mk-input-test')) return;
  if (!match || (!match.code && !(match.ctrl && match.alt) && !/^F(1[3-9]|2[0-4])$/.test(e.key))) return;
  if (match.code && !match.ctrl && !match.alt && typing(e.target)) return;
  e.preventDefault(); e.stopImmediatePropagation();
  dioxus.send({kind: 'command', id: match.id});
}, true);
for (const type of ['pointerdown', 'pointerup', 'mousedown', 'mouseup', 'auxclick']) {
  window.addEventListener(type, e => {
    if (e.button !== 3 && e.button !== 4 && e.button !== 1) return;
    const name = e.button === 3 ? 'back' : e.button === 4 ? 'forward' : 'middle';
    const command = window.moonkaleMouseBindings?.[name];
    if (e.target.closest?.('.mk-input-test')) { e.preventDefault(); return; }
    if (name === 'middle' && command === 'contextual') return; // contextual open/close/pan
    e.preventDefault(); e.stopImmediatePropagation();
    if (type === (window.PointerEvent ? 'pointerup' : 'mouseup') && command) dioxus.send({kind: 'mouse', id: name});
  }, {capture: true, passive: false});
}
window.addEventListener('auxclick', e => {
  if (e.button !== 1 || window.moonkaleMouseBindings?.middle !== 'contextual') return;
  const item = e.target.closest?.('.wb-tab-item');
  const tab = item?.querySelector(':scope > .wb-tab');
  if (!tab || !tab.getAttribute('aria-controls')?.startsWith('wb-panel-')) return;
  e.preventDefault(); item.querySelector(':scope > .wb-tab-close')?.click();
}, true);
await new Promise(() => {});
