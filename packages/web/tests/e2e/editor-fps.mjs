// CED-R22 measurement, deliberately separate from run-all's acceptance list.
// Use a disposable full-app fixture. FPS_BUILD records the actual build used;
// a dev measurement is diagnostic evidence, never release acceptance.
import { chromium } from 'playwright';
import fs from 'node:fs';
import assert from 'node:assert/strict';

const root = process.env.M1_ROOT;
const shots = process.env.M1_SHOTS;
const build = process.env.FPS_BUILD;
if (!root || !shots || !build) throw Error('Set M1_ROOT, M1_SHOTS and FPS_BUILD (dev-web or release-web)');
if (!['dev-web', 'release-web'].includes(build)) throw Error('This harness measures the web build; desktop needs webview evidence');
if (process.env.FPS_STRICT === '1' && build !== 'release-web') throw Error('Strict acceptance requires a release build');
const files = {
  'FpsSource.rs': Array.from({ length: 1000 }, (_, n) =>
    `/// Function ${n}: **documentation** and [web](https://example.org/${n}).\nfn sample_${n}(value: usize) -> usize {\n    let result = value + ${n};\n    result\n}\n`).join(''),
  'FpsThreeMb.txt': 'x'.repeat(3_000_000),
};
for (const [name, source] of Object.entries(files)) fs.writeFileSync(`${root}/${name}`, source);
const browser = await chromium.launch({ ignoreDefaultArgs: ["--hide-scrollbars"] });
const context = await browser.newContext({ viewport: { width: 1500, height: 1100 } });
await context.addInitScript(() => localStorage.setItem('moonkale.settings', JSON.stringify({
  extensions: { enabled: ['dev.moonkale.editor-code-native'] },
  editor: { implementation: 'native', wrap: false, doc_comment_preview: false },
})));
const page = await context.newPage();
const errors = [];
page.on('pageerror', error => errors.push(error.message));
const panel = () => page.locator('.mk-crust[data-editor=rust]:visible');
const input = () => panel().locator('.mk-editor-core-input-sink');
const viewport = () => panel().locator('.mk-editor-core-viewport');
const results = [];
let completed = false;

async function measure(interact) {
  await page.evaluate(() => {
    const sample = { intervals: [], longFrames: [], supported: [], observers: [], active: true };
    window.editorFrameSample = sample;
    let previous;
    const tick = timestamp => {
      if (!sample.active) return;
      if (previous !== undefined) sample.intervals.push(timestamp - previous);
      previous = timestamp;
      sample.raf = requestAnimationFrame(tick);
    };
    sample.raf = requestAnimationFrame(tick);
    for (const type of ['long-animation-frame', 'longtask']) {
      if (!PerformanceObserver.supportedEntryTypes.includes(type)) continue;
      sample.supported.push(type);
      const observer = new PerformanceObserver(list => {
        for (const entry of list.getEntries()) sample.longFrames.push({
          type, startTime: entry.startTime, duration: entry.duration,
          scripts: Array.from(entry.scripts ?? [], script => ({
            sourceURL: script.sourceURL, duration: script.duration,
            forcedStyleAndLayoutDuration: script.forcedStyleAndLayoutDuration,
          })),
        });
      });
      observer.observe({ type });
      sample.observers.push(observer);
    }
  });
  await interact();
  // Include the frame that completes the final interaction.
  await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  return page.evaluate(() => {
    const sample = window.editorFrameSample;
    sample.active = false;
    cancelAnimationFrame(sample.raf);
    for (const observer of sample.observers) observer.disconnect();
    const values = sample.intervals.toSorted((a, b) => a - b);
    const quantile = q => values[Math.max(0, Math.ceil(values.length * q) - 1)];
    return {
      frames: values.length, p50: quantile(.5), p95: quantile(.95), max: values.at(-1),
      intervals: sample.intervals, longFrames: sample.longFrames, supported: sample.supported,
      passed: values.length >= 30 && quantile(.95) <= 16.7 && values.at(-1) <= 50,
    };
  });
}

try {
  await page.goto(`http://127.0.0.1:${process.env.PORT ?? 8094}/`, { waitUntil: 'networkidle' });
  await page.waitForSelector('.wb-workspace');
  await page.click('.mk-explorer-open button[type=submit]');
  await page.waitForFunction(() => document.querySelector('.wb-status-bar')?.textContent.includes('index:'));
  for (const name of Object.keys(files)) {
    await page.locator('.mk-tree-file').filter({ hasText: name }).click();
    await page.waitForFunction(name => [...document.querySelectorAll('.mk-crust[data-editor=rust]')]
      .some(el => el.getBoundingClientRect().height > 0 && el.querySelector('.mk-editor-path')?.textContent.endsWith(name)), name);
    await input().waitFor();
    for (const preview of name.endsWith('.rs') ? [false, true] : [false]) {
      if (name.endsWith('.rs')) {
        const toggle = panel().locator('.mk-native-doc-preview');
        if ((await toggle.getAttribute('aria-pressed') === 'true') !== preview) await toggle.click();
      }
      const horizontal = name.endsWith(".txt");
      let box = await viewport().boundingBox();
      assert.ok(box);
      let client = await viewport().evaluate(el => ({ width: el.clientWidth, height: el.clientHeight }));
      let trackX = box.x + client.width + (box.width - client.width) / 2;
      let trackY = box.y + client.height + (box.height - client.height) / 2;
      const gestures = {
        typing: async () => {
          for (let n = 0; n < 60; n++) { await page.keyboard.insertText('x'); await page.waitForTimeout(20); }
        },
        'held-arrows': async () => {
          for (let n = 0; n < 100; n++) { await page.keyboard.down('ArrowRight'); await page.waitForTimeout(20); }
          await page.keyboard.up('ArrowRight');
        },
        wheel: async () => {
          await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
          for (let n = 0; n < 60; n++) { await page.mouse.wheel(horizontal ? 80 : 0, horizontal ? 0 : 80); await page.waitForTimeout(20); }
        },
        'small-scroll-deltas': async () => {
          await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
          for (let n = 0; n < 100; n++) { await page.mouse.wheel(horizontal ? 8 : 0, horizontal ? 0 : 8); await page.waitForTimeout(20); }
        },
        'scrollbar-drag': async () => {
          await page.mouse.move(horizontal ? box.x + 30 : trackX,
            horizontal ? trackY : box.y + 30);
          await page.mouse.down();
          await page.mouse.move(horizontal ? box.x + box.width - 10 : trackX,
            horizontal ? trackY : box.y + box.height - 10, { steps: 100 });
          await page.mouse.up();
        },
        'selection-drag': async () => {
          await page.mouse.move(box.x + 150, horizontal ? box.y + 10 : box.y + 30);
          await page.mouse.down();
          await page.mouse.move(box.x + box.width - 40, horizontal ? box.y + 10 : box.y + box.height - 30, { steps: 100 });
          await page.mouse.up();
        },
        'caret-movement': async () => {
          for (let n = 0; n < 60; n++) {
            await page.mouse.click(box.x + 100 + n % 10 * 12, box.y + 30 + n % 15 * 22);
            await page.waitForTimeout(20);
          }
        },
      };
      for (const [gesture, interact] of Object.entries(gestures)) {
        await input().focus();
        await page.keyboard.press('Control+Home');
        await viewport().evaluate(el => { el.scrollLeft = 0; el.scrollTop = 0; });
        await page.waitForTimeout(1000);
        box = await viewport().boundingBox();
        assert.ok(box);
        client = await viewport().evaluate(el => ({ width: el.clientWidth, height: el.clientHeight }));
        trackX = box.x + client.width + (box.width - client.width) / 2;
        trackY = box.y + client.height + (box.height - client.height) / 2;
        const before = await viewport().evaluate(el => ({ left: el.scrollLeft, top: el.scrollTop }));
        const measured = await measure(interact);
        const after = await viewport().evaluate(el => ({ left: el.scrollLeft, top: el.scrollTop }));
        if (['wheel', 'small-scroll-deltas', 'scrollbar-drag'].includes(gesture)) {
          measured.interacted = before.left !== after.left || before.top !== after.top;
          if (!measured.interacted) { measured.passed = false; measured.failure = 'Scroll gesture did not move the viewport'; }
        }
        if (gesture === 'selection-drag') {
          measured.interacted = await panel().locator('.mk-editor-core-selected').count() > 0;
          if (!measured.interacted) { measured.passed = false; measured.failure = 'Drag did not select source'; }
        }
        results.push({ name, bytes: Buffer.byteLength(files[name]), preview, gesture, ...measured });
        console.log(JSON.stringify({ name, preview, gesture, frames: measured.frames,
          p50: measured.p50, p95: measured.p95, max: measured.max, passed: measured.passed, failure: measured.failure }));
        if (gesture === 'typing') {
          for (let n = 0; n < 60; n++) await page.keyboard.press('Control+z');
        }
      }
    }
  }
  assert.deepEqual(errors, []);
  completed = true;
  const passed = results.every(result => result.passed);
  console.log(`EDITOR FPS: ${passed ? 'PASS' : 'GATE NOT MET'} (${build}; desktop not measured)`);
  if (!passed && process.env.FPS_STRICT === '1') process.exitCode = 1;
} finally {
  fs.writeFileSync(`${shots}/editor-fps.json`, JSON.stringify({ build, completed, browser: browser.version(), errors, results }, null, 2));
  await browser.close();
}
