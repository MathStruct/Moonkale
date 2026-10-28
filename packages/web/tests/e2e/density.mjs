// ADR-0027 / WP-63: both densities retain commands and respect the chrome budget.
import { chromium } from 'playwright';
import assert from 'node:assert/strict';
import { control } from './chrome-controls.mjs';
const browser = await chromium.launch();
const context = await browser.newContext({viewport: {width: 1500, height: 950}});
const page = await context.newPage();
const url = `http://127.0.0.1:${process.env.PORT ?? 8080}/`;
const shots = process.env.M1_SHOTS ?? '.';
async function command(id, query, keyboard = false) {
  if (keyboard) await page.keyboard.press('Control+Shift+P');
  else await page.locator('#mk-titlebar-search').click();
  await page.locator('#mk-palette-input').fill(query);
  const row = page.locator(`.mk-palette-item[data-key="${id}"]`);
  await row.waitFor();
  if (keyboard) { await page.keyboard.press('Enter'); }
  else await row.click();
}
async function budget() {
  assert(await page.locator('.mk-main-menu').isVisible());
  assert.equal(await page.locator('.mk-titlebar .mk-menu').count(), 0);
  assert(await page.locator('.wb-rail .wb-rail-item:visible').count() <= 7);
  const statusItems = await page.locator('.wb-status-item:visible, .wb-status-message:visible').count();
  assert(statusItems >= 2 && statusItems <= 5, `${statusItems} status items`);
  for (const bar of await page.locator('.mk-editor-toolbar:visible, .mk-graph-toolbar:visible, .mk-agent-toolbar:visible, .mk-table-query:visible, .mk-git-head:visible').all()) {
    const count = await bar.locator('button:visible, summary:visible, select:visible, input:visible, textarea:visible').count();
    assert(count <= 4, `${await bar.getAttribute('class')}: ${count} controls`);
  }
}
try {
  await page.goto(url, {waitUntil: 'networkidle'});
  await page.waitForSelector('.wb-workspace');
  await budget();
  await page.keyboard.press('Alt');
  await page.locator('.mk-menu-button').filter({hasText: 'File'}).waitFor();
  await page.locator('.mk-main-menu').click();
  await page.locator('.mk-explorer-open button[type=submit]').click();
  await page.waitForFunction(() => document.querySelector('.wb-status-bar')?.textContent.includes('index:'));
  await page.locator('.mk-tree-row[title="Cargo.toml"]').dblclick();
  await page.waitForSelector('.cm-content');
  // Save is primary; Wrap is reached by keyboard through the real overflow.
  const toolbar = page.locator('.mk-editor:visible .mk-editor-toolbar');
  assert(await toolbar.locator('[data-command="file.save"]').isVisible());
  await toolbar.locator('summary').focus(); await page.keyboard.press('Enter');
  await toolbar.locator('[data-command="editor.toggleWrap"]').focus();
  await page.keyboard.press('Space');
  await toolbar.locator('summary').focus(); await page.keyboard.press('Escape');
  await budget();
  await page.screenshot({path: `${shots}/density-simple.png`});
  // Capture every registered command from the cheat sheet, including extensions
  // whose rail/toolbars have never been accepted. Search each in both densities.
  await page.keyboard.press('F1');
  const commands = await page.locator('.mk-cheatsheet-row').evaluateAll(rows => rows.map(row => ({id: row.dataset.commandId, title: row.querySelector('span').textContent})));
  assert(commands.length > 50, `only ${commands.length} registry commands`);
  await page.keyboard.press('Escape');
  for (const density of ['simple', 'detailed']) {
    await command(`view.density.${density}`, `View: ${density === 'simple' ? 'Simple' : 'Detailed'} density`, density === 'detailed');
    await page.locator(`.mk-titlebar[data-density="${density}"]`).waitFor();
    if (density === 'simple') await budget();
    else {
      assert.equal(await page.locator('.mk-main-menu').count(), 0);
      assert(await page.locator('.mk-menu-button').count() > 3);
      assert(await toolbar.locator('button:visible').count() > 2);
      await page.screenshot({path: `${shots}/density-detailed.png`});
    }
    await page.keyboard.press('Control+Shift+P');
    for (const {id, title} of commands) {
      await page.locator('#mk-palette-input').fill(title);
      await page.locator(`.mk-palette-item[data-key="${id}"]`).waitFor();
    }
    await page.keyboard.press('Escape');
    // Mouse and keyboard can both reveal all secondary code controls.
    if (density === 'simple') {
      await toolbar.locator('summary').click();
      assert(await toolbar.locator('[data-command="editor.toggleWrap"]').isVisible());
      await page.keyboard.press('Escape');
    }
  }
  // Settings owns the same density value as the palette.
  await command('view.settings', 'Settings');
  await control(page.locator('.mk-settings-density'), 'selectOption', 'simple');
  await page.locator('.mk-titlebar[data-density="simple"]').waitFor();
  await command('view.panel.extensions', 'Show Extensions');
  // Moonkale's own main panels start on the rail (ADR-0027 trims, it doesn't empty).
  assert(await page.locator('.wb-rail #mk-rail-graph').isVisible(), 'Graph missing from the Simple rail');
  const offered = page.locator('.mk-settings-ext').filter({has: page.locator('.mk-settings-ext-name', {hasText: /^Git$/})}).first();
  await offered.waitFor();
  const choice = offered.locator('.mk-chrome-choice input');
  assert.equal(await choice.isChecked(), false);
  await offered.locator('.mk-chrome-offer button').last().click();
  assert.equal(await offered.locator('.mk-chrome-offer').count(), 0);
  await choice.check();
  await page.locator('#mk-rail-git').waitFor();
  await choice.uncheck();
  await page.locator('#mk-rail-git').waitFor({state: 'detached'});
  await page.locator('#mk-rail-more').click();
  await page.locator('.mk-rail-overflow #mk-rail-git').click();
  await page.locator('.mk-git').waitFor();
  await page.reload({waitUntil: 'networkidle'});
  await command('view.panel.extensions', 'Show Extensions');
  assert.equal(await offered.locator('.mk-chrome-offer').count(), 0, 'offer repeated after declining');
  assert.equal(await choice.isChecked(), false);
  // Upgrade an existing profile once, preserving explicit declines and leaving
  // disabled/new extensions undecided. This seeds a legacy profile, not density.
  const legacy = await browser.newContext({viewport: {width: 1500, height: 950}});
  await legacy.addInitScript(() => localStorage.setItem('moonkale.settings', JSON.stringify({
    recent_folders: ['/previous-folder'],
    chrome: {'dev.moonkale.editor-agent': false},
    extensions: {enabled: ['dev.moonkale.editor-code-native']},
    editor: {implementation: 'native', markdown_rich: false},
  })));
  const old = await legacy.newPage();
  await old.goto(url, {waitUntil: 'networkidle'});
  await old.waitForFunction(() => JSON.parse(localStorage.getItem('moonkale.settings') || '{}').chrome_initialized === true);
  const saved = await old.evaluate(() => JSON.parse(localStorage.getItem('moonkale.settings')));
  assert.equal(saved.chrome['dev.moonkale.editor-graph'], true);
  assert.equal(saved.chrome['dev.moonkale.editor-code-native'], true);
  assert.equal(saved.chrome['dev.moonkale.editor-agent'], false);
  assert.equal(saved.chrome['dev.moonkale.editor-flow'], undefined);
  assert(await old.locator('.wb-rail .wb-rail-item:visible').count() <= 7);
  await old.locator('.wb-tab[aria-controls="wb-panel-extensions"]').click();
  const oldGraph = old.locator('.mk-settings-ext').filter({has: old.locator('.mk-settings-ext-name', {hasText: 'Graph'})}).first();
  assert.equal(await oldGraph.locator('.mk-chrome-choice input').isChecked(), true);
  await old.locator('.mk-explorer-open button[type=submit]').click();
  await old.waitForFunction(() => document.querySelector('.wb-status-bar')?.textContent.includes('index:'));
  await old.locator('.mk-tree-row[title="Cargo.toml"]').dblclick();
  await old.locator('.mk-native-surface').waitFor();
  await old.keyboard.press('Control+Shift+P');
  await old.locator('#mk-palette-input').fill('View: Detailed density');
  await old.keyboard.press('Enter');
  await old.locator('.mk-titlebar[data-density="detailed"]').waitFor();
  await old.setViewportSize({width: 420, height: 900});
  await old.locator('.mk-phone-bar').waitFor();
  await old.locator('.mk-phone-bar .mk-phone-btn[data-panel^="editor"]').click();
  const expanded = old.locator('.mk-crust:visible .mk-editor-toolbar');
  await expanded.waitFor();
  await old.waitForFunction(() => {
    const toolbar = [...document.querySelectorAll('.mk-crust .mk-editor-toolbar')].find(e => e.offsetParent);
    return toolbar && toolbar.scrollWidth <= toolbar.clientWidth + 1;
  });
  assert(await expanded.locator('button:visible').count() > 4);
  await old.screenshot({path: `${shots}/density-native-detailed-phone.png`});
  await legacy.close();
  console.log(`DENSITY E2E: PASS (${commands.length} commands in each density)`);
} catch (error) {
  console.error('DENSITY E2E: FAIL', error);
  await page.screenshot({path: `${shots}/density-fail.png`}).catch(() => {});
  process.exitCode = 1;
} finally { await browser.close(); }
