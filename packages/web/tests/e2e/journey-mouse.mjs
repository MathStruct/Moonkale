import { control } from './chrome-controls.mjs';
// ADR-0027: mouse-only counterpart of journey-keyboard. The only keyboard
// operation types document text; menus/tree/buttons perform every command.
import { firefox } from 'playwright';
import assert from 'node:assert/strict';
import fs from 'node:fs';
const browser = await firefox.launch();
const page = await browser.newPage({viewport: {width: 1500, height: 900}});
await page.addInitScript(() => localStorage.setItem('moonkale.settings', JSON.stringify({language: 'en', editor: {implementation: 'codemirror', markdown_rich: false}, keymap: 'left-hand'})));
const shots = process.env.M1_SHOTS ?? '.';
const menu = async (group, command) => {
  await control(page.getByRole('button', {name: group, exact: true}), 'click');
  await control(page.locator(`.mk-menu-item[data-command="${command}"]`), 'click');
};
try {
  await page.goto(`http://127.0.0.1:${process.env.PORT ?? 8080}/`, {waitUntil: 'networkidle'});
  await page.waitForSelector('.wb-workspace');
  for (const id of ['graph', 'extensions', 'settings']) {
    await control(page.locator(`.wb-tab-item:has(.wb-tab[aria-controls="wb-panel-${id}"]) .wb-tab-close`), 'click');
  }
  await page.waitForSelector('.wb-empty-keys');
  // The server supplies the default folder path: opening needs only a click.
  await menu('File', 'workspace.openFolder');
  await control(page.locator('.mk-explorer-open button[type=submit]'), 'click');
  await page.waitForFunction(() => document.querySelector('.wb-status-bar')?.textContent.includes('index:'), null, {timeout: 30000});
  await control(page.locator('.mk-tree-row[title="README.md"]'), 'dblclick');
  await control(page.locator('.cm-content'), 'click');
  // Place the caret with the mouse; no navigation or save keys.
  await page.keyboard.type('Mouse journey saved.\n');
  await control(page.locator('.mk-editor-toolbar button[data-command="file.save"]'), 'click');
  await page.waitForFunction(() => !document.querySelector('.mk-editor-dirty'));
  assert.match(fs.readFileSync(`${process.env.M1_ROOT}/README.md`, 'utf8'), /Mouse journey saved\./);
  await control(page.locator('.mk-tree-row[title="Alpha.md"]'), 'dblclick');
  await page.waitForSelector('.wb-tab[aria-selected="true"]:has-text("Alpha.md")');
  await control(page.locator('.wb-tab:has-text("README.md")'), 'click');
  await page.waitForSelector('.wb-tab[aria-selected="true"]:has-text("README.md")');
  await control(page.locator('.wb-tab:has-text("Alpha.md")'), 'click');
  await page.waitForSelector('.wb-tab[aria-selected="true"]:has-text("Alpha.md")');
  // F13 is the programmable mouse overview binding; cards need no typing.
  await page.evaluate(() => document.activeElement.dispatchEvent(new KeyboardEvent('keydown', {key: 'F13', code: 'F13', bubbles: true, cancelable: true})));
  await page.waitForSelector('.mk-overview');
  await control(page.locator('.mk-overview-card').filter({hasText: 'README.md'}), 'click');
  await page.waitForSelector('.mk-overview', {state: 'detached'});
  await page.waitForSelector('.wb-tab[aria-selected="true"]:has-text("README.md")');
  await menu('View', 'view.panel.graph');
  await page.waitForSelector('.mk-graph');
  await menu('File', 'editor.closeAll');
  await page.waitForSelector('.cm-content', {state: 'detached'});
  await control(page.locator('.wb-tab-item:has(.wb-tab[aria-controls*="graph"]) .wb-tab-close'), 'click');
  await page.waitForSelector('.wb-empty-keys');
  await control(page.locator('.mk-tree-row[title="README.md"]'), 'dblclick');
  await page.waitForSelector('.cm-content');
  await menu('File', 'editor.closeAll');
  await page.waitForSelector('.cm-content', {state: 'detached'});
  await page.screenshot({path: `${shots}/journey-mouse.png`});
  console.log('JOURNEY MOUSE E2E: PASS');
} catch (error) {
  console.error('JOURNEY MOUSE E2E: FAIL', error);
  await page.screenshot({path: `${shots}/journey-mouse-fail.png`}).catch(() => {});
  process.exitCode = 1;
} finally { await browser.close(); }
