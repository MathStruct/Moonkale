import { control } from './chrome-controls.mjs';
// WP-61: presets are below user overrides; punctuation uses physical positions.
import { chromium } from 'playwright';
import assert from 'node:assert/strict';
const browser = await chromium.launch({args: ['--use-angle=swiftshader','--enable-unsafe-swiftshader','--ignore-gpu-blocklist']});
const page = await browser.newPage({viewport: {width: 1500, height: 900}});
const device = await page.context().newCDPSession(page);
const macroKey = async n => {
  const key = `F${n}`;
  await device.send('Input.dispatchKeyEvent', {type:'rawKeyDown',key,code:key,windowsVirtualKeyCode:111+n});
  await device.send('Input.dispatchKeyEvent', {type:'keyUp',key,code:key,windowsVirtualKeyCode:111+n});
};
await page.addInitScript(() => {
  Object.defineProperty(navigator, 'keyboard', {value: {getLayoutMap: async () => new Map([['Slash','-'],['BracketLeft','ü'],['Backquote','^']])}, configurable: true});
});
const settings = async () => {
  await page.locator('.mk-menu-button').first().focus();
  await page.keyboard.press('Control+,');
  await page.waitForSelector('.mk-keymap-preset');
};
const preset = async name => {
  await control(page.locator('.mk-keymap-preset'), 'selectOption', name);
  await page.waitForFunction(name => document.documentElement.dataset.keymap === name, name);
};
const rebind = async (id, value) => {
  const field = page.locator(`.mk-settings-key input[data-command="${id}"]`);
  await control(field, 'fill', value); await field.press('Tab');
  await page.waitForFunction(([id,value]) => JSON.parse(localStorage.getItem('moonkale.settings') || '{}').keybindings?.[id] === value, [id,value]);
};
try {
  await page.goto(`http://127.0.0.1:${process.env.PORT ?? 8080}/`, {waitUntil:'networkidle'});
  await settings();
  await rebind('file.save', 'Ctrl+Alt+K');
  await preset('vscode');
  await page.locator('.mk-menu-button').first().focus();
  await page.keyboard.press('Control+Shift+P');
  await page.waitForSelector('#mk-palette-input');
  await page.keyboard.press('Escape');
  await page.keyboard.press('F1');
  await page.waitForSelector('#mk-cheatsheet-input');
  assert.match(await page.locator('.mk-cheatsheet-preset').textContent(), /vscode/);
  assert.match(await page.locator('.mk-cheatsheet-row[data-command-id="file.save"]').textContent(), /Ctrl\+Alt\+K/);
  assert.match(await page.locator('.mk-cheatsheet-row[data-command-id="editor.toggleComment"]').textContent(), /Ctrl\+-/);
  await page.keyboard.press('Escape');
  await settings(); await preset('obsidian');
  await page.locator('.mk-menu-button').first().focus();
  await page.keyboard.press('Control+O');
  await page.waitForSelector('#mk-palette-input');
  await page.keyboard.press('Escape');
  await settings();
  assert.equal(await page.locator('input[data-command="file.save"]').inputValue(), 'Ctrl+Alt+K');
  // Extended keys can be freely rebound, including the intentionally unbound F24.
  await rebind('view.palette', 'F24');
  await page.locator('.mk-menu-button').first().focus();
  await macroKey(24); 
  await page.waitForSelector('#mk-palette-input'); await page.keyboard.press('Escape');
  await macroKey(14); 
  await page.waitForSelector('#mk-palette-input'); await page.keyboard.press('Escape');
  await settings(); await preset('left-hand');
  await control(page.locator('.mk-explorer-open button[type=submit]'), 'click');
  await page.waitForFunction(() => document.querySelector('.wb-status-bar')?.textContent.includes('index:'), null, {timeout:30000});
  await control(page.locator('.mk-tree-row[title="src"]'), 'click');
  await control(page.locator('.mk-tree-row[title="src/main.rs"]'), 'click');
  await page.waitForSelector('.cm-content');
  // German key: logical '-' at physical Slash. French and Dvorak are also
  // represented by different logical keys on the same physical position.
  for (const key of ['-', ':', 'z']) {
    const before = await page.locator('.cm-content').textContent();
    await page.locator('.cm-content').evaluate((target,key) => target.dispatchEvent(new KeyboardEvent('keydown', {key, code:'Slash', ctrlKey:true, bubbles:true, cancelable:true})), key);
    await page.waitForFunction(before => document.querySelector('.cm-content').textContent !== before, before);
  }
  const beforeIME = await page.locator('.cm-content').textContent();
  for (const modifiers of [{isComposing:true}, {altKey:true}]) {
    await page.locator('.cm-content').evaluate((target,modifiers) => target.dispatchEvent(new KeyboardEvent('keydown', {key:'-',code:'Slash',ctrlKey:true,bubbles:true,cancelable:true,...modifiers})), modifiers);
  }
  await page.waitForTimeout(150);
  assert.equal(await page.locator('.cm-content').textContent(), beforeIME);
  const tilesBefore = await page.locator('.wb-tile').count();
  await macroKey(22);
  await page.waitForFunction(count => document.querySelectorAll('.wb-tile').length > count, tilesBefore);
  await control(page.locator('.wb-tab:has-text("Graph")'), 'click');
  await page.waitForFunction(() => {const v = window.moonkale?.graphViews?.['mk-graph-panel']; return v && v.node_count()>5 && v.layout_state()[0]===0;}, null, {timeout:60000});
  const overlay = page.locator('.mk-graph-overlay');
  await overlay.focus();
  const before = await page.evaluate(() => [...window.moonkale.graphViews['mk-graph-panel'].camera_state()]);
  await page.keyboard.press('w'); await page.keyboard.press('e');
  const after = await page.evaluate(() => [...window.moonkale.graphViews['mk-graph-panel'].camera_state()]);
  assert(after[2] < before[2]); assert(after[0] > before[0]);
  assert.equal(await page.locator('#mk-palette-input').count(), 0);
  await page.waitForTimeout(300);
  // The same left-hand controls use camera-plane pan and dolly in 3D.
  await control(page.locator('.mk-graph button:text-is("3D")'), 'click');
  await page.waitForFunction(() => window.moonkale?.graphViews?.['mk-graph-panel']?.mode() === '3d');
  await overlay.focus();
  const depthBefore = await page.evaluate(() => [...window.moonkale.graphViews['mk-graph-panel'].navigation_camera_state()]);
  await page.keyboard.press('w'); await page.keyboard.press('e');
  const depthAfter = await page.evaluate(() => [...window.moonkale.graphViews['mk-graph-panel'].navigation_camera_state()]);
  assert.notEqual(depthAfter[3], depthBefore[3]);
  assert(depthAfter[6] < depthBefore[6]);
  await page.waitForTimeout(300);
  await page.keyboard.press('Alt+ArrowLeft');
  await page.waitForFunction(state => [...window.moonkale.graphViews['mk-graph-panel'].navigation_camera_state()].every((value,i) => Math.abs(value-state[i]) < 0.001), depthBefore);
  await page.keyboard.press('Alt+ArrowRight');
  await page.waitForFunction(state => [...window.moonkale.graphViews['mk-graph-panel'].navigation_camera_state()].every((value,i) => Math.abs(value-state[i]) < 0.001), depthAfter);
  await control(page.locator('.mk-graph button:text-is("3D")'), 'click');
  // Table navigation uses the same preset; F reaches the hidden column filter.
  await control(page.locator('.mk-tree-db >> text=data.sqlite'), 'click');
  await control(page.locator('.mk-tree-db >> text=users'), 'click');
  const grid = page.locator('.mk-table:visible .mk-table-grid');
  await grid.waitFor(); await grid.focus();
  await page.keyboard.press('Control+Home');
  await page.keyboard.press('d'); await page.keyboard.press('s');
  await page.waitForSelector('.mk-cell-cursor');
  assert.equal(await page.locator('.mk-cell-cursor').evaluate(el => el.closest('.mk-table-row').dataset.row), '1');
  await page.keyboard.press('f');
  await page.waitForFunction(() => document.activeElement?.tagName === 'SUMMARY' && document.activeElement.closest('.mk-column-menu'));
  await page.keyboard.press('Enter');
  await page.keyboard.press('Tab');
  await page.keyboard.press('Tab');
  await page.waitForFunction(() => document.activeElement?.classList.contains('mk-filter-value'));
  await page.keyboard.type('2');
  assert.equal(await page.locator('.mk-table:visible .mk-column-menu[open] .mk-filter-value').inputValue(), '2');
  await page.keyboard.press('Escape');
  await control(page.locator('.mk-table:visible .mk-table-sql'), 'fill', 'SELECT 42 AS answer');
  await macroKey(19);
  await page.waitForFunction(() => [...document.querySelectorAll('.mk-table-hname')].some(el => el.textContent === 'answer'));
  // Letters in fields retain their input, even with the left-hand preset.
  await settings();
  const tester = page.locator('.mk-input-test');
  await tester.focus(); await macroKey(24);
  await page.waitForFunction(() => document.querySelector('.mk-input-test')?.value.includes('F24'));
  assert.equal(await page.locator('#mk-palette-input').count(), 0);
  await page.keyboard.press('Tab');
  await page.waitForFunction(() => !document.activeElement?.classList.contains('mk-input-test'));
  await page.keyboard.press('Shift+Tab');
  await page.waitForFunction(() => document.activeElement?.classList.contains('mk-input-test'));
  await page.keyboard.press('Tab');
  const field = page.locator('input[data-command="file.save"]');
  await control(field, 'fill', 'wasd');
  assert.equal(await field.inputValue(), 'wasd');
  await control(field, 'fill', 'Ctrl+Alt+K'); await field.press('Tab');
  await page.screenshot({path:`${process.env.M1_SHOTS ?? '.'}/input-keymaps.png`});
  console.log('KEYMAPS E2E: PASS');
} catch(e) { console.error('FAIL:',e.stack); await page.screenshot({path:`${process.env.M1_SHOTS ?? '.'}/input-keymaps-fail.png`}); process.exitCode=1; }
finally { await browser.close(); }
