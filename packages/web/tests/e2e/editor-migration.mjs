import { clickNativeCommand } from "./native-toolbar.mjs";
// One backend setting migrates live/restored documents without writing their source.
import { chromium } from 'playwright';
import fs from 'node:fs';
const root = process.env.M1_ROOT;
if (!root) throw Error('M1_ROOT must be a disposable fixture');
fs.writeFileSync(`${root}/src/main.rs`, fs.readFileSync(`${root}/src/main.rs`, 'utf8').replace(/\r?\n/g, '\r\n'));
fs.writeFileSync(`${root}/second.txt`, '😀中 second\r\nnext\r\n');
const originals = Object.fromEntries(['src/main.rs', 'second.txt'].map(name => [name, fs.readFileSync(`${root}/${name}`, 'utf8')]));
const browser = await chromium.launch();
const context = await browser.newContext({ viewport: { width: 1500, height: 1000 } });
// No native extension enable list: selecting the implementation alone must work.
await context.addInitScript(() => {
  if (!localStorage.getItem('moonkale.settings')) localStorage.setItem('moonkale.settings', JSON.stringify({ editor: { markdown_rich: false, implementation: 'codemirror' } }));
});
const page = await context.newPage();
// The Explorer form is server-rendered before its submit handler is hydrated.
// Wait for the client startup request on both first load and restoration.
const load = async navigate => {
  const hydrated = page.waitForResponse(response => response.url().endsWith('/api/llm/info') && response.status() === 200);
  await navigate();
  await hydrated;
  await page.locator('.wb-workspace').waitFor();
};
const native = () => page.locator('.mk-crust[data-editor=rust]:visible');
const tab = name => page.locator('.wb-tab').filter({ hasText: name }).first();
const setting = async value => {
  await page.keyboard.press('Control+,');
  const select = page.locator('.mk-settings-editor-impl');
  await select.waitFor();
  await select.selectOption(value);
  await page.waitForFunction(value => JSON.parse(localStorage.getItem('moonkale.settings')).editor.implementation === value, value);
};
const open = async name => {
  await tab(name).click();
  await page.waitForFunction(name => [...document.querySelectorAll('.mk-crust')].some(el => el.getBoundingClientRect().height && el.querySelector('.mk-editor-path')?.textContent.endsWith(name) && el.querySelector('.mk-editor-core-input-sink')), name);
};
const save = async (name, expected) => {
  await native().locator('.mk-editor-core-input-sink').focus();
  await page.keyboard.press('Control+s');
  await page.waitForFunction(() => { const panel = [...document.querySelectorAll('.mk-crust')].find(el => el.getBoundingClientRect().height); return panel && !panel.querySelector('.mk-editor-dirty'); });
  if (fs.readFileSync(`${root}/${name}`, 'utf8') !== expected) throw Error(`changed bytes in ${name}`);
};
try {
  await load(() => page.goto(`http://127.0.0.1:${process.env.PORT ?? 8094}/`));
  await page.locator('.mk-explorer-open button[type=submit]').click();
  await page.waitForFunction(() => document.querySelector('.wb-status-bar')?.textContent.includes('index:'));
  await page.locator('.mk-tree-dir').filter({ hasText:'src' }).click();
  await page.locator('.mk-tree-file').filter({ hasText:'main.rs' }).click();
  await page.locator('.cm-content:visible').waitFor();
  await page.locator('.cm-content:visible').focus();
  await page.keyboard.press('Control+Home');
  await page.keyboard.insertText('/* λ😀 */');
  await page.locator('.mk-tab-dirty').waitFor();
  await page.locator('.mk-tree-file').filter({ hasText:'second.txt' }).click();
  await page.waitForFunction(() => [...document.querySelectorAll('[data-editor=codemirror]')].some(el => el.getBoundingClientRect().height && el.querySelector('.mk-editor-path')?.textContent.endsWith('second.txt') && el.querySelector('.cm-content')));
  await page.locator('.cm-content:visible').focus();
  await page.keyboard.press('Control+Home');
  await page.keyboard.insertText('追加');
  await page.keyboard.press('Control+Home');
  for (let n=0; n<3; n++) await page.keyboard.press('ArrowRight');
  await page.keyboard.press('Shift+ArrowLeft');
  await page.waitForFunction(() => { const panel=[...document.querySelectorAll('[data-editor=codemirror]')].find(el=>el.getBoundingClientRect().height); return panel?.dataset.selectionAnchor==='4' && panel.dataset.selectionHead==='2'; });
  console.log('select native');
  await setting('native');
  await open('second.txt');
  await page.waitForFunction(() => {
    const panel=[...document.querySelectorAll('.mk-crust')].find(el=>el.getBoundingClientRect().height);
    return panel?.dataset.selectionAnchor==='4' && panel.dataset.selectionHead==='2';
  });
  if (fs.readFileSync(`${root}/second.txt`,'utf8') !== originals['second.txt'] || fs.readFileSync(`${root}/src/main.rs`,'utf8') !== originals['src/main.rs']) throw Error('switch saved unsaved edits');
  await native().locator('.mk-editor-core-input-sink').focus();
  await page.keyboard.press('Control+End');
  await page.keyboard.type('Z');
  await page.keyboard.press('Control+z');
  console.log('save migrated second');
  await save('second.txt', '追加' + originals['second.txt']);
  console.log('save migrated main');
  await open('main.rs');
  await save('src/main.rs', '/* λ😀 */' + originals['src/main.rs']);
  // An explicit per-document fallback choice outranks subsequent global changes.
  await clickNativeCommand(native().locator('.mk-editor-switch'));
  await page.locator('.cm-content:visible').waitFor();
  await setting('codemirror');
  await tab('second.txt').click();
  await page.locator('.cm-content:visible').waitFor();
  await setting('native');
  await tab('main.rs').click();
  await page.locator('.cm-content:visible').waitFor();
  await open('second.txt');
  await page.waitForFunction(() => [...Array(localStorage.length)].some((_,i)=>localStorage.key(i).startsWith('moonkale.state/layout/')));
  console.log('restore saved tabs');
  await load(() => page.reload());
  await page.locator('.mk-explorer-open button[type=submit]').click();
  await page.waitForFunction(() => document.querySelector('.wb-status-bar')?.textContent.includes('index:'));
  // Restoration remaps saved panel IDs to the currently selected backend.
  await open('second.txt');
  await save('second.txt','追加'+originals['second.txt']);
  await open('main.rs');
  await save('src/main.rs','/* λ😀 */'+originals['src/main.rs']);
  console.log('EDITOR MIGRATION: PASS (single setting, unsaved live buffers/caret, per-document fallback, saved tab restoration, CRLF/Unicode)');
} catch (error) {
  await page.screenshot({ path: `${process.env.M1_SHOTS ?? '.'}/editor-migration-fail.png` });
  console.log('failure:', error.stack, await page.locator('.wb-status-bar').textContent().catch(() => '<status bar unmounted>'));
  throw error;
} finally { await browser.close(); }
