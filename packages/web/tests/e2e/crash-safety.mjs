// WP-57: requires --features crash-test (test extension also requires debug assertions).
import { chromium, firefox } from 'playwright';
import assert from 'node:assert/strict';
const engine = process.env.CRASH_BROWSER ?? 'firefox';
const browser = await ({ chromium, firefox }[engine]).launch();
const page = await browser.newPage({ viewport: { width: 1500, height: 900 } });
let leakedPayload = false;
page.on('request', request => { if (request.postData()?.includes('test-only')) leakedPayload = true; });
page.on('websocket', socket => socket.on('framesent', frame => { if (String(frame.payload).includes('test-only')) leakedPayload = true; }));
const base = `http://127.0.0.1:${process.env.PORT ?? 8080}/`;
const boot = async (query = '?crash-test', recovery = false) => {
  await page.goto(base + query, {waitUntil: 'networkidle'});
  if (!recovery) await page.waitForSelector('.wb-workspace');
};
const showTest = async () => {
  await page.keyboard.press('Control+Shift+P');
  await page.fill('.mk-palette-input', 'Crash test');
  await page.locator('.mk-palette-item[data-key="view.panel.crash-test"]').click();
  await page.waitForSelector('#mk-crash-test');
};
try {
  await boot();
  await showTest();
  await page.click('#mk-crash-test-disable');
  await page.waitForSelector('.mk-crash-notice');
  assert.match(await page.locator('.mk-crash-notice').innerText(), /dev.moonkale.crash-test/);
  await page.waitForSelector('#mk-crash-test', {state: 'detached'});
  await page.click('#mk-rail-search');
  await page.waitForSelector('#mk-search-input');
  assert.equal(await page.locator('.mk-crash-notice').count(), 1);

  await page.waitForFunction(() => window.moonkale?.wasmHost?.available());
  const failures = await page.evaluate(async () => {
    // memory + alloc + a run export that executes unreachable (a real wasm trap).
    const bytes = new Uint8Array([
      0,97,115,109,1,0,0,0, 1,12,2,96,1,127,1,127,96,2,127,127,1,126,
      3,3,2,0,1, 5,3,1,0,1,
      7,24,3,6,109,101,109,111,114,121,2,0,5,97,108,108,111,99,0,0,3,114,117,110,0,1,
      10,10,2,4,0,65,0,11,3,0,0,11,
    ]);
    const url = URL.createObjectURL(new Blob([bytes]));
    const errors = [];
    try {
      for (let i = 0; i < 5; ++i) {
        try { await window.moonkale.wasmHost.run(url, 'go', {}, async () => '{}'); errors.push('unexpected success'); }
        catch (error) { errors.push(String(error)); }
      }
    } finally { URL.revokeObjectURL(url); }
    return errors;
  });
  assert(failures.slice(0, 4).every(error => /retry is limited/.test(error)), failures.join('\n'));
  assert.match(failures[4], /disabled after repeated crashes/);

  // Both paths execute real Rust panics. Abort builds must retain independent
  // DOM buttons, since the wasm client can no longer service callbacks.
  for (const action of ['event', 'task']) {
    await boot();
    await showTest();
    await page.evaluate(() => {
      Object.defineProperty(navigator, 'clipboard', {configurable: true, value: {writeText: async text => { globalThis.copiedCrashDetails = text; }}});
    });
    if (action === 'event') await page.evaluate(() => { delete globalThis.moonkaleCrashWords; });
    await page.click(`#mk-crash-test-${action}`);
    await page.waitForSelector('#mk-crash-overlay');
    assert.match(await page.locator('#mk-crash-overlay').innerText(), /Reload/);
    assert.doesNotMatch(await page.locator('#mk-crash-overlay').innerText(), /test-only/);
    assert.equal(await page.locator('#mk-crash-overlay button').count(), 3);
    await page.getByRole('button', {name: 'Copy details', exact: true}).click();
    assert.match(await page.evaluate(() => globalThis.copiedCrashDetails), /Moonkale wasm client stopped/);
    assert.equal(leakedPayload, false, 'panic payload must not be sent to the dev server');
    await page.evaluate(() => localStorage.removeItem('moonkale.startup-health'));
    await page.locator('#mk-crash-overlay button').first().click();
    await page.waitForSelector('#mk-crash-overlay', {state: 'detached'});
    await page.waitForSelector('.wb-workspace');
  }

  await page.evaluate(() => {
    localStorage.setItem('moonkale.startup-health', JSON.stringify({pending: true, failures: 1}));
    localStorage.setItem('moonkale.settings', JSON.stringify({language: 'en', extensions: {enabled: ['dev.moonkale.editor-flow']}}));
  });
  await boot('?crash-test', true);
  await page.waitForSelector('.mk-crash-notice');
  assert.match(await page.locator('.mk-crash-notice').innerText(), /startup/);
  await page.getByRole('button', {name: 'Start in safe mode', exact: true}).click();
  await page.waitForSelector('#mk-rail-explorer', {state: 'detached'});
  const saved = await page.evaluate(() => localStorage.getItem('moonkale.settings'));
  await boot('?safe');
  await page.waitForFunction(() => document.querySelector('.mk-crash-notice')?.textContent.includes('Safe mode'));
  assert.equal(await page.locator('#mk-rail-explorer').count(), 0);
  assert.equal(await page.evaluate(() => localStorage.getItem('moonkale.settings')), saved);
  assert.equal(await page.locator('#mk-crash-test').count(), 0);
  console.log(`CRASH SAFETY E2E: PASS (${engine})`);
} catch (error) {
  console.error('CRASH SAFETY E2E: FAIL', error);
  await page.screenshot({path: `${process.env.M1_SHOTS ?? '.'}/crash-safety-fail.png`});
  process.exitCode = 1;
} finally {
  // Restore the fixture's default local preferences for subsequent suites.
  await page.evaluate(() => { localStorage.removeItem('moonkale.startup-health'); localStorage.removeItem('moonkale.settings'); }).catch(() => {});
  await browser.close();
}
