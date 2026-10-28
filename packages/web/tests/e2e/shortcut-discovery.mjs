import { control } from './chrome-controls.mjs';
import { firefox } from 'playwright';
import assert from 'node:assert/strict';
const browser = await firefox.launch();
const page = await browser.newPage({viewport: {width: 1500, height: 900}});
const menu = async (group, id) => {
  await control(page.getByRole('button', {name: group, exact: true}), 'click');
  await control(page.locator(`.mk-menu-item[data-command="${id}"]`), 'click');
};
const focused = selector => page.waitForFunction(s => document.activeElement?.matches(s), selector);
try {
  await page.goto(`http://127.0.0.1:${process.env.PORT ?? 8080}/`, {waitUntil: 'networkidle'});
  await page.waitForSelector('.wb-workspace');
  await page.locator('.mk-menu-button').first().focus();
  await page.keyboard.press('F1');
  await focused('#mk-cheatsheet-input');
  assert.match(await page.locator('[data-command-id="file.save"]').textContent(), /Ctrl\+S/);
  await control(page.locator('#mk-cheatsheet-input'), 'fill', 'Ctrl+Shift+P');
  assert.equal(await page.locator('.mk-cheatsheet-row').count(), 1);
  await page.keyboard.press('Escape');
  await focused('.mk-menu-button');
  await page.keyboard.press('Control+Shift+P');
  await control(page.locator('#mk-palette-input'), 'fill', 'Keyboard Shortcuts');
  await control(page.locator('.mk-palette-item[data-key="help.shortcuts"]'), 'click');
  await focused('#mk-cheatsheet-input');
  await page.keyboard.press('Escape');
  await focused('.mk-menu-button');
  // Printable input and slash route into the existing search palette.
  await page.keyboard.type('R');
  await focused('#mk-palette-input');
  await page.waitForFunction(() => document.querySelector('#mk-palette-input').value === 'R');
  await page.keyboard.press('Escape');
  await page.keyboard.press('/');
  await focused('#mk-palette-input');
  assert.equal(await page.locator('#mk-palette-input').inputValue(), '');
  await page.keyboard.press('Escape');
  await menu('File', 'workspace.openFolder');
  await focused('#mk-explorer-path');
  // Typing into an input must not trigger global search.
  await page.keyboard.type('x');
  assert.equal(await page.locator('#mk-palette-input').count(), 0);
  await page.keyboard.press('Backspace');
  await control(page.locator('.mk-explorer-open button[type=submit]'), 'click');
  await page.waitForFunction(() => document.querySelector('.wb-status-bar')?.textContent.includes('index:'), null, {timeout: 30000});
  await control(page.locator('#mk-titlebar-search'), 'click');
  await focused('#mk-palette-input');
  await control(page.locator('#mk-palette-input'), 'fill', 'README');
  await control(page.locator('.mk-palette-item[data-key="README.md"]'), 'click');
  await page.waitForSelector('.cm-content');
  assert.equal(await page.locator('#mk-titlebar-search').inputValue(), '');
  await control(page.locator('.mk-tree-row[title="README.md"]'), 'dblclick');
  await control(page.locator('.cm-content'), 'click');
  await page.keyboard.type('/');
  assert.equal(await page.locator('#mk-palette-input').count(), 0);
  await page.keyboard.press('Control+Z');
  // Three mouse executions show a hint once. The fourth and a reload cannot.
  for (let i = 0; i < 3; i++) await menu('View', 'view.toggleBottom');
  await page.waitForSelector('.mk-shortcut-nudge');
  assert.match(await page.locator('.mk-shortcut-nudge').textContent(), /Ctrl\+J/);
  await page.waitForSelector('.mk-shortcut-nudge', {state: 'detached'});
  await menu('View', 'view.toggleBottom');
  await page.waitForTimeout(250);
  assert.equal(await page.locator('.mk-shortcut-nudge').count(), 0);
  await page.reload({waitUntil: 'networkidle'});
  for (let i = 0; i < 3; i++) await menu('View', 'view.toggleBottom');
  await page.waitForTimeout(250);
  assert.equal(await page.locator('.mk-shortcut-nudge').count(), 0);
  // Web startup does not automatically reopen folders after reload.
  await control(page.locator('.mk-explorer-open button[type=submit]'), 'click');
  await page.waitForFunction(() => document.querySelector('.wb-status-bar')?.textContent.includes('index:'), null, {timeout: 30000});
  await control(page.locator('.mk-tree-row[title="README.md"]'), 'dblclick');
  await page.waitForSelector('.cm-content');
  await menu('File', 'view.settings');
  await focused('.mk-settings :is(input,button,select)');
  // Reopening the folder can itself earn the Open Folder hint. Let that
  // existing status expire before testing whether disabled hints can appear.
  await page.waitForSelector('.mk-shortcut-nudge', {state: 'detached'});
  await control(page.locator('.mk-settings-nudges'), 'uncheck');
  await page.waitForFunction(() => JSON.parse(localStorage.getItem('moonkale.settings') || '{}').shortcut_nudges === false);
  for (let i = 0; i < 3; i++) await menu('View', 'view.toggleSide');
  assert.equal(await page.locator('.mk-shortcut-nudge').count(), 0);
  // Rebinding and unbinding must reach the cheat sheet through the same keymap.
  await menu('File', 'view.settings');
  const binding = page.locator('.mk-settings-key input[data-command="file.save"]');
  await control(binding, 'fill', 'Ctrl+Alt+S');
  await binding.press('Tab');
  await page.keyboard.press('F1');
  await focused('#mk-cheatsheet-input');
  assert.match(await page.locator('.mk-cheatsheet-row[data-command-id="file.save"]').textContent(), /Ctrl\+Alt\+S/);
  await page.keyboard.press('Escape');
  await control(binding, 'fill', '');
  await binding.press('Tab');
  await page.keyboard.press('F1');
  await focused('#mk-cheatsheet-input');
  assert(!/Ctrl\+/.test(await page.locator('.mk-cheatsheet-row[data-command-id="file.save"]').textContent()));
  await page.keyboard.press('Escape');
  // Each command-opened utility panel must receive focus.
  for (const id of ['search','explorer','graph','agent','extensions']) {
    await menu('View', `view.panel.${id}`);
    await page.waitForFunction(id => document.activeElement?.closest('.wb-panel-content')?.id === `wb-panel-${id}`, id);
  }
  // Graph panel search works even on Firefox without an available GL backend.
  // graph-a11y additionally checks the real engine's focusable canvas.
  await menu('View', 'view.panel.graph');
  await page.locator('#wb-panel-graph').focus();
  await page.keyboard.press('ArrowRight');
  assert.equal(await page.locator('#mk-palette-input').count(), 0);
  await page.keyboard.type('g');
  await focused('#mk-palette-input');
  assert.equal(await page.locator('#mk-palette-input').inputValue(), 'g');
  await page.keyboard.press('Escape');
  await focused('#wb-panel-graph');
  await page.keyboard.press('/');
  await focused('#mk-palette-input');
  await page.keyboard.press('Escape');

  // Command dialogs focus their primary field and return to their opener.
  for (const [label, id, selector] of [
    ['Open Remote Folder', 'remote.open', '.mk-remote-host'],
    ['Connect to Server', 'server.connect', '.mk-server-url'],
  ]) {
    await page.locator('.mk-menu-button').first().focus();
    await page.keyboard.press('Control+Shift+P');
    await control(page.locator('#mk-palette-input'), 'fill', label);
    await control(page.locator(`.mk-palette-item[data-key="${id}"]`), 'click');
    await focused(selector);
    await page.keyboard.press('Shift+Tab');
    assert(await page.evaluate(() => document.activeElement.closest('[role="dialog"]') !== null));
    await page.keyboard.press('Escape');
    await focused('.mk-menu-button');
  }
  // Existing CodeMirror settings popover restores its gear button.
  await control(page.locator('.wb-tab:has-text("README.md")'), 'click');
  const gear = page.locator('.mk-editor:visible .mk-panel-settings-btn');
  await control(gear, 'click');
  await page.waitForFunction(() => document.activeElement?.closest('.mk-panel-settings'));
  await page.keyboard.press('Escape');
  await focused('.mk-editor-toolbar summary');

  // Quick open takes the new editor's focus. A command-opened panel returns it.
  await page.locator('.cm-content').focus();
  await page.keyboard.press('Control+P');
  await control(page.locator('#mk-palette-input'), 'fill', 'Alpha.md');
  await control(page.locator('.mk-palette-item[data-key="Alpha.md"]'), 'click');
  await focused('.cm-content');
  await page.keyboard.press('Control+Shift+P');
  await control(page.locator('#mk-palette-input'), 'fill', 'Show Graph');
  await control(page.locator('.mk-palette-item[data-key="view.panel.graph"]'), 'click');
  await page.waitForFunction(() => document.activeElement?.closest('#wb-panel-graph'));
  await control(page.locator('.wb-tab-item:has(.wb-tab[aria-controls="wb-panel-graph"]) .wb-tab-close'), 'click');
  await focused('.cm-content');

  // Closing the command-opened editor returns to its original editor input.
  await control(page.locator('.wb-tab-item:has(.wb-tab:has-text("Alpha.md")) .wb-tab-close'), 'click');
  await page.waitForSelector('.wb-tab:has-text("Alpha.md")', {state: 'detached'});
  await focused('.cm-content');
  assert(await page.locator('.wb-tab[aria-selected="true"]:has-text("README.md")').count());

  // Content resembling workbench markup cannot turn restoration into a click
  // on another control. The original text input is deliberately hidden.
  await page.evaluate(() => {
    window.focusTripwire = 0;
    const target = document.createElement('button');
    target.id = 'focus-tripwire';
    target.onclick = () => window.focusTripwire++;
    document.body.append(target);
    const spoof = document.createElement('div');
    spoof.id = 'focus-spoof';
    spoof.className = 'wb-panel-content';
    spoof.setAttribute('role', 'tabpanel');
    spoof.setAttribute('aria-labelledby', target.id);
    const input = document.createElement('input');
    spoof.append(input);
    document.activeElement.closest('.wb-panel-content').append(spoof);
    input.focus();
  });
  await page.keyboard.press('F1');
  await focused('#mk-cheatsheet-input');
  await page.evaluate(() => { document.getElementById('focus-spoof').style.display = 'none'; });
  await page.keyboard.press('Escape');
  await page.waitForSelector('.mk-cheatsheet', {state: 'detached'});
  await page.waitForTimeout(600);
  assert.equal(await page.evaluate(() => window.focusTripwire), 0);
  await page.evaluate(() => {
    document.getElementById('focus-spoof').remove();
    document.getElementById('focus-tripwire').remove();
  });

  // The terminal chooser's preferred control takes focus; cancelling returns.
  const context = await browser.newContext({viewport: {width: 1500, height: 900}});
  await context.addInitScript(() => localStorage.setItem('moonkale.settings', JSON.stringify({
    extensions: {enabled: ['dev.moonkale.editor-terminal-native']}, terminal: {implementation: 'ask'},
  })));
  const chooser = await context.newPage();
  await chooser.goto(`http://127.0.0.1:${process.env.PORT ?? 8080}/`, {waitUntil: 'networkidle'});
  await chooser.waitForSelector('#wb-tab-terminal-native', {state: 'attached'});
  await chooser.locator('.mk-menu-button').first().focus();
  await chooser.keyboard.press('Control+`');
  await chooser.waitForFunction(() => document.activeElement?.closest('.mk-terminal-chooser'));
  await chooser.keyboard.press('Escape');
  await chooser.waitForFunction(() => document.activeElement?.matches('.mk-menu-button'));
  await context.close();
  console.log('SHORTCUT DISCOVERY E2E: PASS');
} catch (error) {
  console.error('SHORTCUT DISCOVERY E2E: FAIL', error);
  console.error(await page.evaluate(() => document.activeElement?.outerHTML.slice(0, 350)));
  await page.screenshot({path: `${process.env.M1_SHOTS ?? '.'}/shortcut-discovery-fail.png`}).catch(() => {});
  process.exitCode = 1;
} finally { await browser.close(); }
