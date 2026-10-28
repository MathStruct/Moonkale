// Focus is webview presentation, never editor state. Retain actual DOM nodes
// and stable ids so a workbench rerender can replace the original node.
(() => {
  if (window.__mkFocus) return;
  window.__mkFocus = true;
  const focusable = 'input:not([disabled]),textarea:not([disabled]),select:not([disabled]),button:not([disabled]),summary,[contenteditable="true"],[tabindex="0"]';
  const scopes = '.mk-palette,[role="dialog"],[role="alertdialog"]';
  const modal = scope => scope.matches('.mk-palette,.mk-panel-settings,[aria-modal="true"],[role="alertdialog"]');
  const visible = el => el?.isConnected && el.getClientRects().length > 0;
  const workbenchPanel = panel => panel?.matches('.wb-panel-content[role="tabpanel"]') &&
    panel.parentElement?.matches('.wb-panel-frame') &&
    !panel.parentElement.closest('.wb-panel-content') ? panel : null;
  const ref = el => {
    // A menu item disappears when it runs; its persistent menu button is
    // the return target. Toolbar triggers remain their own return target.
    if (el?.matches?.('.mk-menu-item')) el = el.closest('.mk-menu')?.querySelector('.mk-menu-button') || el;
    if (el?.closest?.('.mk-toolbar-more-items')) el = el.closest('.mk-toolbar-more')?.querySelector('summary') || el;
    if (el?.closest?.('.mk-rail-overflow')) el = document.getElementById('mk-rail-more') || el;
    let panel = el?.closest?.('.wb-panel-content');
    let outer;
    while ((outer = panel?.parentElement?.closest('.wb-panel-content'))) panel = outer;
    panel = workbenchPanel(panel);
    return {node: el, id: el?.id, panel: panel?.id,
      index: panel ? [...panel.querySelectorAll(focusable)].indexOf(el) : -1};
  };
  const resolve = saved => {
    if (visible(saved?.node)) return saved.node;
    const exact = saved?.id && document.getElementById(saved.id);
    if (visible(exact)) return exact;
    const panel = workbenchPanel(saved?.panel && document.getElementById(saved.panel));
    return panel?.querySelectorAll(focusable)[saved.index] || panel;
  };
  const focus = el => { if (visible(el)) { el.focus({preventScroll: true}); return document.activeElement === el; } return false; };
  let pendingPanelFocus = 0;
  const restore = saved => {
    const panel = workbenchPanel(saved?.panel && document.getElementById(saved.panel));
    if (panel && !visible(panel)) {
      // Closing a tab can select another retained tab. Reactivate the opener's
      // panel through the workbench, then restore its actual control.
      const tab = document.getElementById(panel.getAttribute('aria-labelledby'));
      if (tab?.matches('button.wb-tab[role="tab"]') && !tab.closest('.wb-panel-content') &&
          tab.getAttribute('aria-controls') === panel.id) tab.click();
    }
    let attempts = 0;
    const attempt = () => {
      if (focus(resolve(saved))) return;
      if (++attempts < 30) requestAnimationFrame(attempt);
    };
    attempt();
  };
  const returnFocus = saved => {
    let attempts = 0;
    const attempt = () => {
      // A command opening another panel gets the first chance to focus it.
      // Cancellation/closure returns only when focus is otherwise stranded.
      if (document.activeElement !== document.body) return;
      if (pendingPanelFocus && ++attempts < 30) requestAnimationFrame(attempt);
      else restore(saved);
    };
    requestAnimationFrame(attempt);
  };
  let last = ref(document.activeElement), active = [];
  document.addEventListener('focusin', e => { last = ref(e.target); }, true);
  // Capture the opener before autofocus or removal can replace it.
  let opener = last;
  for (const type of ['pointerdown','keydown']) document.addEventListener(type, () => { opener = last; }, true);
  document.addEventListener('click', () => { opener = last; }, true);
  const update = () => {
    const current = [...document.querySelectorAll(scopes)].filter(visible);
    for (const scope of current) {
      if (active.some(item => item.scope === scope)) continue;
      const previous = active.find(item => item.scope.contains(opener.node));
      const saved = previous && !current.includes(previous.scope) ? previous.saved : opener;
      active.push({scope, saved});
      if (!scope.contains(document.activeElement)) focus(scope.querySelector('[autofocus]') || scope.querySelector('input:not([disabled]),textarea:not([disabled])') || scope.querySelector(focusable));
    }
    const removed = active.filter(item => !current.includes(item.scope));
    active = active.filter(item => current.includes(item.scope));
    for (const item of removed.reverse()) {
      if (item.scope.contains(opener.node)) opener = item.saved;
      if (active.length && modal(active.at(-1).scope)) {
        const target = resolve(item.saved);
        if (active.at(-1).scope.contains(target)) focus(target);
        continue;
      }
      // A command may already have focused a newly opened panel/editor.
      const element = document.activeElement;
      if (element === document.body || item.scope.contains(element)) returnFocus(item.saved);
    }
  };
  new MutationObserver(update).observe(document.documentElement, {subtree: true, childList: true});
  document.addEventListener('keydown', e => {
    const scope = active.at(-1)?.scope;
    if (!scope || !modal(scope) || e.isComposing || e.key !== 'Tab' || !scope.contains(e.target)) return;
    const items = [...scope.querySelectorAll(focusable)].filter(visible);
    const index = items.indexOf(document.activeElement);
    if (e.shiftKey && index <= 0) { e.preventDefault(); focus(items.at(-1)); }
    else if (!e.shiftKey && index === items.length - 1) { e.preventDefault(); focus(items[0]); }
  }, true);
  const panelReturns = new Map();
  window.moonkaleFocusPanel = (id, editor = false) => {
    const saved = active[0]?.saved || opener;
    pendingPanelFocus++;
    // Layout reconciliation is asynchronous; wait for the selected tab.
    let attempts = 0;
    const attempt = () => {
      const safeId = id.replace(/[^a-zA-Z0-9_-]/g, '-');
      const tab = document.getElementById(`wb-tab-${safeId}`);
      const panel = tab && document.getElementById(tab.getAttribute('aria-controls'));
      if (visible(panel) && tab.getAttribute('aria-selected') === 'true') {
        if (!editor || !panelReturns.has(id)) panelReturns.set(id, {saved});
        if (!panel.contains(document.activeElement)) {
          const preferred = editor && panel.querySelector('.mk-editor-core-input-sink,.cm-content,[contenteditable="true"]');
          focus(preferred || panel.querySelector(focusable)) || focus(panel) || focus(tab);
        }
        pendingPanelFocus--;
      } else if (++attempts < 30) requestAnimationFrame(attempt);
      else pendingPanelFocus--;
    };
    requestAnimationFrame(attempt);
  };
  new MutationObserver(() => {
    for (const [id, item] of panelReturns) {
      if (document.getElementById(`wb-tab-${id.replace(/[^a-zA-Z0-9_-]/g, '-')}`)) continue;
      panelReturns.delete(id);
      if (document.activeElement === document.body) returnFocus(item.saved);
    }
  }).observe(document.documentElement, {subtree: true, childList: true});
})();
