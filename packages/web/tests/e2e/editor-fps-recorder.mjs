// Check the manual WebView recorder's transport and trusted-input accounting.
// This browser check does not substitute for release desktop frame evidence.
import {chromium} from 'playwright';
import {readFile} from 'node:fs/promises';
import {join} from 'node:path';
import assert from 'node:assert/strict';
const repo = process.env.MOONKALE_REPO;
if (!repo) throw Error('Set MOONKALE_REPO to the checkout');
const script = await readFile(join(repo, 'packages/web/tests/fixtures/native-editor/src/fps-recorder.js'), 'utf8');
const browser = await chromium.launch();
const page = await browser.newPage();
const errors = [];
page.on('pageerror', error => errors.push(error.message));
try {
  await page.goto(`http://127.0.0.1:${process.env.PORT ?? 8099}/`, {waitUntil: 'networkidle'});
  const input = page.locator('.primary .mk-editor-core-input-sink');
  await input.focus();
  const original = await page.locator('.canonical').textContent();
  const pending = page.evaluate(async script => {
    let sample;
    const bridge = {
      recv: async () => ({gesture: 'typing', durationMs: 1000, build: 'dev-desktop'}),
      send: value => { sample = value; },
    };
    await new Function('dioxus', `return (async () => {${script}})();`)(bridge);
    return sample;
  }, script);
  await page.waitForTimeout(100);
  for (let n = 0; n < 10; n++) {
    await page.keyboard.type('x');
    await page.waitForTimeout(20);
  }
  const sample = await pending;
  assert.equal(sample.events.typedKeys, 10);
  assert.equal(sample.interacted, true);
  assert.ok(sample.frames >= 30);
  assert.ok(sample.p95 > 0 && sample.max >= sample.p95);
  assert.equal(sample.clock, 'performance.now');
  assert.equal(sample.rafIntervals.length, sample.intervals.length);
  assert.equal(sample.build, 'dev-desktop');
  assert.equal(sample.passed, false, 'a development recording cannot pass release acceptance');
  assert.equal(await page.locator('.canonical').textContent(), 'x'.repeat(10) + original);
  assert.deepEqual(errors, []);
  console.log('EDITOR FPS RECORDER: PASS (transport only; desktop frames not measured)');
} finally { await browser.close(); }
