// Recorder clock/gate regression without a browser. Transport and actual input
// still need editor-fps-recorder.mjs and the release desktop harness.
import {readFile} from 'node:fs/promises';
import {test} from 'node:test';
import assert from 'node:assert/strict';

const source = await readFile(new URL('../fixtures/native-editor/src/fps-recorder.js', import.meta.url), 'utf8');
const record = new Function('dioxus', 'document', 'performance', 'requestAnimationFrame',
  'cancelAnimationFrame', 'setTimeout', 'clearTimeout', 'navigator',
  `return (async () => {${source}})();`);

async function sample({step = 1000 / 60, build = 'release-desktop', stall = 0, integerClock = false, idleTicks = 0, idleStep = 16, idleSample, gesture = 'typing', interact} = {}) {
  let now = 0, tick, output;
  const listeners = new Map();
  const viewport = {scrollLeft: 0, scrollTop: 0, clientWidth: 1500, clientHeight: 900,
    addEventListener: (type, callback) => listeners.set('viewport:' + type, callback), removeEventListener() {}};
  const size = {dataset: {bytes: '100'}};
  const panel = {dataset: {selectionHead: '0', selectionAnchor: '0'},
    querySelector: selector => selector === '.mk-editor-core-viewport' ? viewport : null,
    addEventListener: (type, callback) => listeners.set(type, callback),
    removeEventListener() {}};
  const pending = record({
    recv: async () => ({gesture, durationMs: 1000, build}),
    send: value => { output = value; },
  }, {querySelector: selector => selector.startsWith('.primary') ? panel : size},
  {now: () => integerClock ? Math.floor(now) : now}, callback => {tick = callback; return 1;}, () => {}, () => 1, () => {},
  {userAgent: 'clock regression'});
  await Promise.resolve();
  for (let n = 0; n < (idleSample?.length ?? idleTicks); n++) {
    now += idleSample?.[n] ?? idleStep;
    tick(Math.round(now));
  }
  const began = now;
  if (interact) interact({emit: (type, event = {}) => listeners.get(type)?.({isTrusted: true, ...event}), viewport, panel});
  else {
    for (let n = 0; n < 5; n++) listeners.get('keydown')({isTrusted: true, key: 'x'});
    size.dataset.bytes = '105';
  }
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

// rust.json[6], real NVIDIA/KDE Wayland, 60 Hz: short idle callbacks made
// the old median 8 ms and classified normal 16 ms gesture frames as dropped.
test('polluted real-desktop idle sample retains the 60 Hz cadence', async () => {
  const idleSample = [
    15, 2, 14, 2, 14, 3, 13, 4, 12, 4, 12, 5, 11, 6, 10, 6, 10, 7, 9, 8, 9, 8, 8, 8,
    8, 9, 7, 10, 6, 10, 6, 11, 5, 12, 5, 11, 5, 12, 5, 8, 4, 2, 16, 21, 15, 1, 11, 7,
    9, 8, 11, 6, 8, 8, 12, 5, 12, 5, 6, 10, 6, 11, 5, 12, 4, 12, 4, 13, 3, 14, 2, 14,
    2, 15, 1, 16, 16, 16, 1, 15, 2, 14, 2, 15, 2, 14, 3, 13, 3, 13, 4, 12, 5, 11, 5, 11,
    6, 10, 7, 9, 7, 9, 8, 8, 9, 7, 10, 6, 10, 6, 11, 6, 11, 5, 11, 5, 12, 4, 13, 3,
  ];
  const result = await sample({integerClock: true, idleSample});
  assert.equal(result.refreshInterval, 15);
  assert.equal(result.droppedFrames, 0);
  assert.equal(result.passed, true);
  assert.equal((await sample({integerClock: true, idleSample, step: 32})).passed, false);
});

for (const repeat of [false, true]) {
  test(`held arrows with repeat=${repeat} require traversal input and movement`, async () => {
    const interact = ({emit, panel}) => {
      for (let n = 0; n < 15; n++) emit('keydown', {key: 'ArrowRight', repeat});
      panel.dataset.selectionHead = '15';
    };
    assert.equal((await sample({gesture: 'held-arrows', interact})).interacted, true);
    assert.equal((await sample({gesture: 'held-arrows', interact: ({emit}) => {
      for (let n = 0; n < 15; n++) emit('keydown', {key: 'ArrowRight', repeat});
    }})).interacted, false);
  });
}

test('native up/down repeat pairs still demonstrate repeated traversal', async () => {
  const result = await sample({gesture: 'held-arrows', interact: ({emit, panel}) => {
    for (let n = 0; n < 15; n++) {
      emit('keydown', {key: 'ArrowRight'}); emit('keyup', {key: 'ArrowRight'});
    }
    panel.dataset.selectionHead = '15';
  }});
  assert.equal(result.interacted, true);
});

for (const axis of ['scrollLeft', 'scrollTop']) {
  test(`smooth wheel uses actual small ${axis} deltas, regardless of wheel units`, async () => {
    const result = await sample({gesture: 'small-scroll-deltas', interact: ({emit, viewport}) => {
      emit('viewport:wheel', {deltaX: axis === 'scrollLeft' ? 120 : 0, deltaY: axis === 'scrollTop' ? 120 : 0, deltaMode: 1});
      for (let n = 0; n < 10; n++) {
        viewport[axis] += 3; emit('viewport:scroll', {target: viewport});
      }
    }});
    assert.equal(result.interacted, true);
    assert.equal(result.events.smallWheel, 0);
    assert.equal(result.events.smallScroll, 10);
  });
}

test('wheel without viewport movement and input-sink scroll cannot qualify', async () => {
  for (const gesture of ['wheel', 'small-scroll-deltas']) {
    const result = await sample({gesture, interact: ({emit}) => {
      for (let n = 0; n < 20; n++) {
        emit('viewport:wheel', {deltaX: 0, deltaY: 1, deltaMode: 0});
        emit('viewport:scroll', {target: {scrollTop: n}});
      }
    }});
    assert.equal(result.interacted, false);
  }
});


test('large scroll jumps do not qualify as small deltas', async () => {
  const result = await sample({gesture: 'small-scroll-deltas', interact: ({emit, viewport}) => {
    for (let n = 0; n < 10; n++) {
      emit('viewport:wheel', {deltaX: 0, deltaY: 120, deltaMode: 0});
      viewport.scrollTop += 120; emit('viewport:scroll', {target: viewport});
    }
  }});
  assert.equal(result.interacted, false);
  assert.equal(result.events.smallScroll, 0);
});

test('untrusted keyboard and wheel events cannot start or qualify a recording', async () => {
  for (const gesture of ['held-arrows', 'wheel']) {
    const result = await sample({gesture, interact: ({emit, viewport, panel}) => {
      for (let n = 0; n < 15; n++) {
        emit('keydown', {isTrusted: false, key: 'ArrowRight'});
        emit('viewport:wheel', {isTrusted: false, deltaX: 0, deltaY: 1, deltaMode: 0});
        viewport.scrollTop++; emit('viewport:scroll', {isTrusted: false, target: viewport});
      }
      panel.dataset.selectionHead = '15';
      // The harness needs a trusted intended event to end its fake-clock run;
      // one arrow/wheel alone must remain insufficient to qualify.
      if (gesture === 'held-arrows') emit('keydown', {key: 'ArrowRight'});
      else emit('viewport:wheel', {deltaX: 0, deltaY: 1, deltaMode: 0});
    }});
    assert.equal(result.interacted, false);
  }
});
