// Recorder clock/gate regression without a browser. Transport and actual input
// still need editor-fps-recorder.mjs and the release desktop harness.
import {readFile} from 'node:fs/promises';
import {test} from 'node:test';
import assert from 'node:assert/strict';

const source = await readFile(new URL('../fixtures/native-editor/src/fps-recorder.js', import.meta.url), 'utf8');
const record = new Function('dioxus', 'document', 'performance', 'requestAnimationFrame',
  'cancelAnimationFrame', 'setTimeout', 'clearTimeout', 'navigator',
  `return (async () => {${source}})();`);

async function sample({step = 1000 / 60, build = 'release-desktop', stall = 0, integerClock = false, idleTicks = 0, idleStep = 16} = {}) {
  let now = 0, tick, output;
  const listeners = new Map();
  const viewport = {scrollLeft: 0, scrollTop: 0, clientWidth: 1500, clientHeight: 900,
    addEventListener() {}, removeEventListener() {}};
  const size = {dataset: {bytes: '100'}};
  const panel = {dataset: {selectionHead: '0', selectionAnchor: '0'},
    querySelector: selector => selector === '.mk-editor-core-viewport' ? viewport : null,
    addEventListener: (type, callback) => listeners.set(type, callback),
    removeEventListener() {}};
  const pending = record({
    recv: async () => ({gesture: 'typing', durationMs: 1000, build}),
    send: value => { output = value; },
  }, {querySelector: selector => selector.startsWith('.primary') ? panel : size},
  {now: () => integerClock ? Math.floor(now) : now}, callback => {tick = callback; return 1;}, () => {}, () => 1, () => {},
  {userAgent: 'clock regression'});
  await Promise.resolve();
  for (let n = 0; n < idleTicks; n++) {
    now += idleStep;
    tick(Math.round(now));
  }
  const began = now;
  for (let n = 0; n < 5; n++) listeners.get('keydown')({isTrusted: true, key: 'x'});
  size.dataset.bytes = '105';
  // The WebView's rAF clock is quantized; callback-entry time is fractional.
  for (let n = 0; now - began < 1100; n++) {
    now += n === 10 && stall ? stall : step;
    tick(Math.round(now));
  }
  await pending;
  return output;
}

test('60 Hz passes with fractional callback spacing despite integer rAF timestamps', async () => {
  const result = await sample();
  assert.equal(result.clock, 'performance.now');
  assert.equal(result.passed, true);
  assert.ok(Math.abs(result.p95 - 1000 / 60) < 1e-8);
  assert.equal(Math.max(...result.rafIntervals), 17);
  assert.equal(result.intervals.length, result.rafIntervals.length);
});

for (const step of [20, 23]) {
  test(`${step} ms typing still fails the unchanged p95 gate`, async () => {
    const result = await sample({step});
    assert.equal(result.p95, step);
    assert.equal(result.passed, false);
  });
}

test('a single frame above 50 ms still fails', async () => {
  const result = await sample({stall: 60});
  assert.ok(result.max > 50);
  assert.equal(result.passed, false);
});

test('development measurements cannot pass', async () => {
  assert.equal((await sample({build: 'dev-desktop'})).passed, false);
});


test('quantized 60 Hz uses calibrated cadence and never claims strict precision', async () => {
  const result = await sample({integerClock: true, idleTicks: 60});
  assert.equal(result.quantized, true);
  assert.equal(result.gate, 'calibrated-vsync');
  assert.equal(result.refreshInterval, 16);
  assert.equal(result.droppedFrameThreshold, 24);
  assert.equal(result.droppedFrames, 0);
  assert.equal(result.strictPassed, false);
  assert.equal(result.passed, true);
});

test('quantized double frames fail the calibrated cadence gate', async () => {
  const result = await sample({integerClock: true, idleTicks: 60, step: 32});
  assert.ok(result.droppedFrames > 0);
  assert.equal(result.passed, false);
});

test('slow idle calibration cannot relax the target', async () => {
  const result = await sample({integerClock: true, idleTicks: 60, idleStep: 32});
  assert.equal(result.refreshInterval, 32);
  assert.equal(result.droppedFrameThreshold, null);
  assert.equal(result.passed, false);
});

test('missing idle calibration cannot pass a quantized clock', async () => {
  const result = await sample({integerClock: true});
  assert.equal(result.quantized, true);
  assert.equal(result.passed, false);
});


test('quantized cadence still rejects a single frame above 50 ms', async () => {
  const result = await sample({integerClock: true, idleTicks: 60, stall: 60});
  assert.ok(result.p95 <= result.droppedFrameThreshold);
  assert.ok(result.max > 50);
  assert.equal(result.cadencePassed, false);
  assert.equal(result.passed, false);
});
