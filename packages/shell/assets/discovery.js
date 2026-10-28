// Presentation and focus only. Command identities and persistence stay in Rust.
if (!window.__mkDiscovery) {
  window.__mkDiscovery = true;
  const typing = target => target instanceof Element && !!target.closest(
    'input,textarea,select,[contenteditable]:not([contenteditable="false"]),.cm-editor,.mk-native-surface,.mk-terminal,.mk-tn');
  document.addEventListener('keydown', e => {
    if (e.defaultPrevented || e.isComposing || e.ctrlKey || e.metaKey || e.altKey || typing(e.target)) return;
    if ([...e.key].length !== 1 || e.key.trim() === '') return;
    // Flow/table letter navigation keeps its letters; graph navigation consumes
    // its own events before this bubbling listener. Slash explicitly searches.
    if (e.key !== '/' && e.target.closest?.('[role="grid"],.mk-flow')) return;
    e.preventDefault();
    dioxus.send(['search', e.key === '/' ? '' : e.key, '']);
  });
  document.addEventListener('click', e => {
    if (!e.isTrusted || e.detail === 0) return;
    const surface = e.target.closest?.('button,[role="button"],[role="menuitem"],.mk-palette-item');
    const id = surface?.dataset.command || surface?.querySelector('[data-command]')?.dataset.command || surface?.dataset.commandId;
    if (!id) return;
    const now = new Date();
    const day = `${now.getFullYear()}-${String(now.getMonth()+1).padStart(2,'0')}-${String(now.getDate()).padStart(2,'0')}`;
    dioxus.send(['mouse', id, day]);
  }, true);
  window.moonkaleShortcutHint = text => {
    // Quiet status text. Suppress while a command leaves a text editor active.
    if (typing(document.activeElement)) return;
    document.querySelector('.mk-shortcut-nudge')?.remove();
    const hint = document.createElement('span');
    hint.className = 'mk-shortcut-nudge';
    hint.setAttribute('role', 'status');
    hint.textContent = text;
    document.querySelector('.wb-status-bar')?.append(hint);
    setTimeout(() => hint.remove(), 4500);
  };
}
await new Promise(() => {});
