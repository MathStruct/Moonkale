// WP-61: real five-button events stay inside the app and share session visits.
import { chromium } from 'playwright';
import assert from 'node:assert/strict';
const browser = await chromium.launch({args: ['--use-angle=swiftshader','--enable-unsafe-swiftshader','--ignore-gpu-blocklist']});
const page = await browser.newPage({viewport: {width: 1500, height: 900}});
const pointer = await page.context().newCDPSession(page);
const thumb = async button => {
  await pointer.send('Input.dispatchMouseEvent', {type:'mousePressed',x:400,y:350,button,buttons:button === 'back' ? 8 : 16,clickCount:1});
  await pointer.send('Input.dispatchMouseEvent', {type:'mouseReleased',x:400,y:350,button,buttons:0,clickCount:1});
};
const active = name => page.waitForFunction(name => [...document.querySelectorAll('.wb-tab[aria-selected="true"]')].some(t => t.textContent.includes(name)), name);
const camera = () => page.evaluate(() => [...window.moonkale.graphViews['mk-graph-panel'].camera_state()]);
try {
  await page.goto(`http://127.0.0.1:${process.env.PORT ?? 8080}/`, {waitUntil: 'networkidle'});
  await page.locator('.mk-explorer-open button[type=submit]').click();
  await page.waitForFunction(() => document.querySelector('.wb-status-bar')?.textContent.includes('index:'), null, {timeout: 30000});
  await page.locator('.mk-tree-row[title="README.md"]').click();
  await active('README.md');
  await page.locator('.mk-tree-row[title="Alpha.md"]').click({button: 'middle'});
  await page.waitForSelector('.wb-tab:has-text("Alpha.md")');
  await active('README.md');
  await page.locator('.mk-tree-row[title="Alpha.md"]').click();
  await active('Alpha.md');
  const url = page.url();
  await thumb('back');
  await active('README.md');
  await thumb('forward');
  await active('Alpha.md');
  assert.equal(page.url(), url);
  await page.locator('.wb-tab:has-text("Graph")').click();
  await page.waitForFunction(() => { const v = window.moonkale?.graphViews?.['mk-graph-panel']; return v && v.node_count() > 5 && v.layout_state()[0] === 0; }, null, {timeout: 60000});
  const overlay = page.locator('.mk-graph-overlay');
  const box = await overlay.boundingBox();
  const before = await camera();
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.wheel(0, -200);
  await page.waitForTimeout(400);
  const after = await camera();
  assert.notDeepEqual(before, after);
  await thumb('back');
  await page.waitForFunction(before => [...window.moonkale.graphViews['mk-graph-panel'].camera_state()].every((x,i) => Math.abs(x-before[i]) < 0.001), before);
  await thumb('forward');
  await page.waitForFunction(after => [...window.moonkale.graphViews['mk-graph-panel'].camera_state()].every((x,i) => Math.abs(x-after[i]) < 0.001), after);
  assert.equal(page.url(), url);
  // Middle drag pans even when it starts over a node; it cannot move the node.
  await page.mouse.move(box.x + 80, box.y + 80);
  const positions = await page.evaluate(() => window.moonkale.graphViews['mk-graph-panel'].node_positions());
  await page.mouse.down({button:'middle'});
  await page.mouse.move(box.x + 140, box.y + 110, {steps: 5});
  await page.mouse.up({button:'middle'});
  assert.equal(await page.evaluate(() => window.moonkale.graphViews['mk-graph-panel'].node_positions()), positions);
  assert.notDeepEqual(await camera(), after);
  await page.locator('.wb-tab:has-text("Alpha.md")').click({button:'middle'});
  await page.waitForSelector('.wb-tab:has-text("Alpha.md")', {state: 'detached'});
  // Mouse overrides use the same registry and survive a preset change.
  await page.locator('.mk-menu-button').first().focus();
  await page.keyboard.press('Control+,');
  const back = page.locator('select[data-mouse="back"]');
  await back.selectOption('help.shortcuts');
  await page.waitForFunction(() => window.moonkaleMouseBindings?.back === 'help.shortcuts');
  await thumb('back');
  await page.waitForSelector('#mk-cheatsheet-input');
  assert.match(await page.locator('.mk-cheatsheet-mouse-binding').first().textContent(), /Keyboard Shortcuts/);
  await page.screenshot({path:`${process.env.M1_SHOTS ?? '.'}/input-mouse.png`});
  console.log('MOUSE E2E: PASS');
} catch(e) { console.error('FAIL:', e.stack); await page.screenshot({path:`${process.env.M1_SHOTS ?? '.'}/input-mouse-fail.png`}); process.exitCode = 1; }
finally { await browser.close(); }
