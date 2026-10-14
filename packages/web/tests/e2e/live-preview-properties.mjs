import {firefox} from 'playwright';
import fs from 'node:fs';
import assert from 'node:assert/strict';

const root = process.env.M1_ROOT;
if (!root) throw Error('M1_ROOT must name a disposable fixture');
const original = '---\r\n# keep comment\r\ntitle: "Properties"\r\ntags: [demo]\r\naliases:\r\n  - 猫\r\n\r\n---  \r\n\r\n# Body 😀\r\nPreserve **body** and [[Home]].\r\n';
fs.writeFileSync(`${root}/PropertiesPreview.md`, original);
fs.writeFileSync(`${root}/PropertiesOther.md`, '# Other\r\n');
const browser = await firefox.launch();
const context = await browser.newContext({viewport: {width: 1500, height: 1100}});
await context.addInitScript(() => localStorage.setItem('moonkale.settings', JSON.stringify({extensions: {enabled: ['dev.moonkale.editor-code-native']}, editor: {implementation: 'native', markdown_rich: false}})));
const page = await context.newPage(); const errors = [];
page.on('pageerror', error => errors.push(error.message));
const panel = () => page.locator('.mk-crust[data-editor="rust"]:visible');
const input = () => panel().locator('.mk-editor-core-input-sink');
const yaml = () => panel().locator('.mk-properties-yaml');
const ready = () => page.waitForFunction(() => {
  const runs = [...document.querySelectorAll('.mk-crust .mk-proportional-run')].filter(run => run.getBoundingClientRect().height > 0);
  return runs.length && runs.every(run => run.dataset.ready === 'true' && run.dataset.fontLoading === 'false');
});
const save = async () => {
  await input().focus(); await page.keyboard.press('Control+s');
  await page.waitForFunction(() => ![...document.querySelectorAll('.mk-crust')].filter(el => el.getBoundingClientRect().height > 0).some(el => el.querySelector('.mk-editor-dirty')));
};
try {
  await page.goto(`http://127.0.0.1:${process.env.PORT ?? 8094}/`, {waitUntil: 'networkidle'});
  await page.waitForSelector('.wb-workspace'); await page.click('.mk-explorer-open button[type=submit]');
  await page.waitForFunction(() => document.querySelector('.wb-status-bar')?.textContent.includes('index:'));
  await page.locator('.mk-tree-file').filter({hasText: 'PropertiesPreview.md'}).click(); await input().waitFor();
  await panel().locator('.mk-native-preview').click();
  await input().focus(); await page.keyboard.press('Control+End'); await ready();
  assert.equal(await panel().locator('.mk-editor-core-row[data-line="0"]').count(), 0);
  assert.match(await panel().locator('.mk-properties-summary').textContent(), /title: Properties.*tags: \[demo\].*aliases/);
  await panel().locator('.mk-properties-head').click();
  const initialYaml = await yaml().inputValue();
  await yaml().fill(initialYaml.replace('[demo]', '[demo, edited]'));
  assert.equal(fs.readFileSync(`${root}/PropertiesPreview.md`, 'utf8'), original);
  await yaml().focus(); await page.keyboard.press('Control+s');
  await yaml().waitFor({state: 'hidden'});
  const changed = original.replace('[demo]', '[demo, edited]');
  await page.waitForFunction(() => ![...document.querySelectorAll('.mk-crust')].some(el => el.querySelector('.mk-editor-dirty')));
  assert.equal(fs.readFileSync(`${root}/PropertiesPreview.md`, 'utf8'), changed);
  await input().focus(); await page.keyboard.press('Control+z'); await save();
  assert.equal(fs.readFileSync(`${root}/PropertiesPreview.md`, 'utf8'), original);
  await page.keyboard.press('Control+Shift+z'); await save();
  assert.equal(fs.readFileSync(`${root}/PropertiesPreview.md`, 'utf8'), changed);

  await panel().locator('.mk-properties-head').click(); await yaml().fill('title: Discarded');
  await page.keyboard.press('Escape'); await save();
  assert.equal(fs.readFileSync(`${root}/PropertiesPreview.md`, 'utf8'), changed);
  await panel().locator('.mk-native-preview').click();
  assert.equal(await panel().locator('.mk-native-properties').count(), 0);
  await input().focus(); await page.keyboard.press('Control+Home');
  await panel().locator('.mk-editor-core-row[data-line="0"]').waitFor();
  assert.equal(await panel().locator('.mk-editor-core-row[data-line="0"]').count(), 1);
  await panel().locator('.mk-native-preview').click();

  await page.locator('.mk-tree-file').filter({hasText: 'PropertiesOther.md'}).click(); await input().waitFor();
  assert.equal(await panel().locator('.mk-properties-summary').textContent(), 'none');
  await panel().locator('.mk-properties-head').click(); await yaml().fill('title: Other\naliases: [one, two]');
  await panel().locator('.mk-properties-apply').click(); await save();
  assert.equal(fs.readFileSync(`${root}/PropertiesOther.md`, 'utf8'), '---\r\ntitle: Other\r\naliases: [one, two]\r\n---\r\n# Other\r\n');
  await panel().locator('.mk-properties-head').click(); await yaml().fill('');
  await panel().locator('.mk-properties-apply').click(); await save();
  assert.equal(fs.readFileSync(`${root}/PropertiesOther.md`, 'utf8'), '# Other\r\n');
  assert.equal(fs.readFileSync(`${root}/PropertiesPreview.md`, 'utf8'), changed);
  assert.deepEqual(errors, []); console.log('LIVE PREVIEW PROPERTIES E2E: PASS');
} finally { await browser.close(); }
