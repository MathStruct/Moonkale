// ADR-0027 / WP-64: all interaction uses keyboard events. DOM reads only observe.
import { firefox } from 'playwright';
import assert from 'node:assert/strict';
import fs from 'node:fs';
const browser = await firefox.launch();
const page = await browser.newPage({viewport: {width: 1500, height: 900}});
if (process.env.FOCUS_TRACE) await page.addInitScript(() => {
  window.focusTrace = [];
  for (const kind of ['focusin', 'keydown']) document.addEventListener(kind, event => {
    if (kind === 'keydown' && event.key !== 'Tab') return;
    window.focusTrace.push({kind, tag: event.target.tagName, id: event.target.id, cls: event.target.className, pageFocused: document.hasFocus(), ctrl: event.ctrlKey, shift: event.shiftKey});
    if (window.focusTrace.length > 80) window.focusTrace.shift();
  }, true);
});
const shots = process.env.M1_SHOTS ?? '.';
const root = process.env.M1_ROOT;
const pause = ms => new Promise(resolve => setTimeout(resolve, ms));
const tabTo = async selector => {
  for (let i = 0; i < 300; i++) {
    if (await page.evaluate(s => document.activeElement?.matches(s), selector)) return;
    // Escape immediately followed by Tab is CodeMirror's keyboard escape
    // from indentation; repeat it if traversal reaches another editor.
    if (await page.evaluate(() => document.activeElement?.matches('.cm-content'))) {
      await page.keyboard.press('Escape');
    }
    await page.keyboard.press((selector.startsWith('.wb-tab') || selector === '.mk-menu-button') ? 'Shift+Tab' : 'Tab');
  }
  console.log(JSON.stringify(await page.evaluate(() => ({focus: document.activeElement?.outerHTML.slice(0, 200), tabs: [...document.querySelectorAll('.wb-tab')].map(tab => ({text: tab.textContent, control: tab.getAttribute('aria-controls'), selected: tab.getAttribute('aria-selected')}))}))));
  throw new Error(`Tab did not reach ${selector}`);
};
const palette = async (label, id, binding = 'Control+Shift+P') => {
  await page.keyboard.press(binding);
  await page.waitForSelector('#mk-palette-input');
  await page.keyboard.type(label);
  await page.waitForFunction(key => document.querySelector('.mk-palette-item')?.dataset.key === key, id);
  await page.keyboard.press('Enter');
  await page.waitForSelector('.mk-palette', {state: 'detached'});
};
const quickOpen = async (name, binding = 'Control+P') => {
  await page.keyboard.press(binding);
  await page.waitForSelector('#mk-palette-input');
  await page.keyboard.type(name);
  await page.waitForSelector(`.mk-palette-item[data-key="${name}"]`);
  await page.keyboard.press('Enter');
  await page.waitForSelector('.mk-palette', {state: 'detached'});
  await page.waitForFunction(name => document.querySelector('.wb-tab[aria-selected="true"][aria-controls*="editor"]')?.textContent.includes(name), name);
  await page.waitForSelector('.cm-content:visible');
};
try {
  await page.goto(`http://127.0.0.1:${process.env.PORT ?? 8080}/`, {waitUntil: 'networkidle'});
  await page.waitForSelector('.wb-workspace');
  // Built-in utility tabs initially occupy the editor tile. Close them via
  // their keyboard-reachable close buttons to reveal its empty-workbench hints.
  for (let i = 0; i < 3; i++) {
    await tabTo('.wb-tab[aria-selected="true"]:is([aria-controls*="graph"], [aria-controls*="extensions"], [aria-controls*="settings"])');
    await page.keyboard.press('Tab');
    assert(await page.evaluate(() => document.activeElement?.classList.contains('wb-tab-close')));
    await page.keyboard.press('Enter');
  }
  await page.waitForFunction(() => document.querySelector('.wb-empty-keys')?.textContent.includes('Ctrl+P'));
  await page.keyboard.press('Control+O');
  await page.waitForFunction(() => document.activeElement?.id === 'mk-explorer-path');
  await page.keyboard.press('Control+A');
  await page.keyboard.type(root);
  await page.keyboard.press('Enter');
  await page.waitForFunction(() => document.querySelector('.wb-status-bar')?.textContent.includes('index:'), null, {timeout: 30000});
  await quickOpen('README.md');
  await tabTo('.cm-content');
  await page.keyboard.press('Control+End');
  await page.keyboard.type('\nKeyboard journey saved.');
  await page.keyboard.press('Control+S');
  await page.waitForFunction(() => !document.querySelector('.mk-editor-dirty'));
  assert.match(fs.readFileSync(`${root}/README.md`, 'utf8'), /Keyboard journey saved\./);

  // Modifier alone must stay quiet while an editor owns input.
  await page.keyboard.down('Control');
  await pause(650);
  assert.equal(await page.locator('.mk-key-tip').count(), 0);
  await page.keyboard.up('Control');

  await quickOpen('Alpha.md');
  await tabTo('.cm-content');
  // CodeMirror's documented Ctrl+M releases Tab from indentation until toggled.
  await page.keyboard.press('Control+M');
  await tabTo('.wb-tab[aria-selected="true"][aria-controls*="editor"]');
  await page.keyboard.press('ArrowLeft');
  await page.waitForFunction(() => document.querySelector('.wb-tab[aria-selected="true"][aria-controls*="editor"]')?.textContent.includes('README.md'));
  await page.keyboard.press('ArrowRight');
  await page.waitForFunction(() => document.querySelector('.wb-tab[aria-selected="true"][aria-controls*="editor"]')?.textContent.includes('Alpha.md'));
  // Switch retained tabs through the overview using only its shortcut/input.
  await page.keyboard.press('Control+Alt+Space');
  await page.waitForSelector('.mk-overview');
  await page.keyboard.type('README.md');
  await page.waitForFunction(() => document.querySelectorAll('.mk-overview-card').length === 1);
  await page.keyboard.press('Enter');
  await page.waitForSelector('.mk-overview', {state: 'detached'});
  await page.waitForFunction(() => document.querySelector('.wb-tab[aria-selected="true"][aria-controls*="editor"]')?.textContent.includes('README.md'));
  await palette('Show Graph', 'view.panel.graph');
  await page.waitForSelector('.mk-graph');
  await palette('Close All Editors', 'editor.closeAll');
  await page.waitForSelector('.cm-content', {state: 'detached'});
  // Graph is a static panel, so close its tab separately to expose the empty tile.
  await tabTo('.wb-tab[aria-controls*="graph"]');
  await page.keyboard.press('Tab');
  assert(await page.evaluate(() => document.activeElement?.classList.contains('wb-tab-close')));
  await page.keyboard.press('Enter');
  await page.waitForSelector('.wb-empty-keys');

  // Reopen the saved file so the key tips include the editor toolbar.
  await quickOpen('README.md');
  await tabTo('.mk-menu-button');
  await page.keyboard.down('Control');
  await pause(200);
  assert.equal(await page.locator('.mk-key-tip').count(), 0, 'tips appeared before delay');
  await page.waitForSelector('.mk-key-tip');
  assert((await page.locator('.mk-key-tip').allTextContents()).includes('Ctrl+O'));
  await page.screenshot({path: `${shots}/journey-keyboard-hold-ctrl.png`});
  await page.keyboard.up('Control');
  await page.waitForSelector('.mk-key-tips', {state: 'detached'});
  await page.keyboard.down('Control');
  await page.keyboard.press('Shift');
  await pause(650);
  assert.equal(await page.locator('.mk-key-tip').count(), 0, 'a chord should cancel tips');
  await page.keyboard.up('Control');
  await page.keyboard.press('Control+Shift+W');
  await page.waitForSelector('.cm-content', {state: 'detached'});
  console.log('JOURNEY KEYBOARD E2E: PASS');
} catch (error) {
  console.error('JOURNEY KEYBOARD E2E: FAIL', error);
  if (process.env.FOCUS_TRACE) console.error(JSON.stringify(await page.evaluate(() => window.focusTrace)));
  await page.screenshot({path: `${shots}/journey-keyboard-fail.png`}).catch(() => {});
  process.exitCode = 1;
} finally { await browser.close(); }
