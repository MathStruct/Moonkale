// Benchmark instrumentation only: observe actual WebView input and geometry.
// No synthetic input, document edits, language logic, or editor-state ownership.
const request = await dioxus.recv();
const panel = document.querySelector('.primary .mk-crust[data-editor=rust]');
const viewport = panel?.querySelector('.mk-editor-core-viewport');
if (!viewport) throw Error('No visible native editor');
const duration = Math.min(10000, Math.max(1000, Number(request.durationMs) || 5000));
const events = {keydown: 0, typedKeys: 0, repeats: 0, arrowKeys: 0, input: 0, wheel: 0, smallWheel: 0, smallScroll: 0, scroll: 0, pointerdown: 0, pointermove: 0};
const canonicalSize = () => Number(document.querySelector('.fps-canonical-size')?.dataset.bytes
    ?? document.querySelector('.canonical')?.textContent.length);
const idleIntervals = [], idleRafIntervals = [], intervals = [], rafIntervals = [], longFrames = [], observers = [], listeners = [];
const before = {left: viewport.scrollLeft, top: viewport.scrollTop,
    head: panel.dataset.selectionHead, anchor: panel.dataset.selectionAnchor,
    bytes: canonicalSize()};
let lastLeft = viewport.scrollLeft, lastTop = viewport.scrollTop, scrollDistance = 0;
const wheelDeltas = [];
let started, previous, previousRaf, idlePrevious, idlePreviousRaf, raf, finish;
const result = new Promise(resolve => finish = resolve);
const begin = () => { if (started === undefined) started = performance.now(); };
const listen = (target, type, callback) => {
    const listener = event => {
        if (!event.isTrusted) return;
        callback(event);
    };
    target.addEventListener(type, listener, {passive: true, capture: true});
    listeners.push(() => target.removeEventListener(type, listener, true));
};
listen(panel, 'keydown', event => {
    events.keydown++;
    if (!event.ctrlKey && !event.metaKey && !event.altKey && event.key.length === 1) {
        events.typedKeys++;
        if (request.gesture === 'typing') begin();
    }
    if (event.key.startsWith('Arrow')) {
        if (event.repeat) events.repeats++;
        // WebKitGTK can omit repeat=true (and some native backends emit
        // up/down pairs). Count traversal input itself, then require movement.
        events.arrowKeys++;
    }
    if (request.gesture === 'held-arrows' && event.key.startsWith('Arrow')) begin();
});
listen(panel, 'input', () => { events.input++; if (request.gesture === 'typing') begin(); });
listen(viewport, 'wheel', event => {
    events.wheel++;
    if (wheelDeltas.length < 120) wheelDeltas.push({x: event.deltaX, y: event.deltaY, mode: event.deltaMode});
    if (event.deltaMode === 0 && Math.max(Math.abs(event.deltaX), Math.abs(event.deltaY)) <= 10) events.smallWheel++;
    if (request.gesture === 'wheel' || request.gesture === 'small-scroll-deltas') begin();
});
listen(viewport, 'scroll', event => {
    // Capturing also observes the tiny textarea sink's own scroll events.
    // Only geometry of the document viewport demonstrates document scrolling.
    if (event.target !== viewport) return;
    const distance = Math.max(Math.abs(viewport.scrollLeft - lastLeft), Math.abs(viewport.scrollTop - lastTop));
    lastLeft = viewport.scrollLeft; lastTop = viewport.scrollTop;
    if (distance === 0) return;
    scrollDistance += distance;
    if (distance <= 10) events.smallScroll++;
    events.scroll++;
    if (request.gesture === 'scrollbar-drag') begin();
});
listen(panel, 'pointerdown', () => {
    events.pointerdown++;
    if (request.gesture === 'caret-movement' || request.gesture === 'selection-drag') begin();
});
listen(panel, 'pointermove', event => { if (event.buttons & 1) events.pointermove++; });
for (const type of ['long-animation-frame', 'longtask']) {
    if (typeof PerformanceObserver !== 'function' || !PerformanceObserver.supportedEntryTypes?.includes(type)) continue;
    const observer = new PerformanceObserver(list => {
        for (const entry of list.getEntries()) {
            if (started !== undefined && entry.startTime >= started) longFrames.push({type, duration: entry.duration});
        }
    });
    observer.observe({type}); observers.push(observer);
}
const tick = timestamp => {
    // WebKitGTK can quantize the rAF argument to integer milliseconds. Measure
    // callback spacing with the monotonic high-resolution clock instead, without
    // rounding; retain the rAF clock separately so reports expose that difference.
    const now = performance.now();
    if (started === undefined) {
        if (idlePrevious !== undefined) {
            idleIntervals.push(now - idlePrevious);
            idleRafIntervals.push(timestamp - idlePreviousRaf);
            if (idleIntervals.length > 120) { idleIntervals.shift(); idleRafIntervals.shift(); }
        }
        idlePrevious = now;
        idlePreviousRaf = timestamp;
    }
    if (started !== undefined) {
        if (previous !== undefined) {
            intervals.push(now - previous);
            rafIntervals.push(timestamp - previousRaf);
        }
        previous = now;
        previousRaf = timestamp;
        if (now - started >= duration) { finish(); return; }
    }
    raf = requestAnimationFrame(tick);
};
raf = requestAnimationFrame(tick);
const timeout = setTimeout(finish, 45000);
await result;
clearTimeout(timeout); cancelAnimationFrame(raf);
listeners.forEach(remove => remove()); observers.forEach(observer => observer.disconnect());
const scrolled = scrollDistance > 0;
const moved = before.head !== panel.dataset.selectionHead || scrolled;
const selected = panel.dataset.selectionHead !== panel.dataset.selectionAnchor;
const interacted = {
    typing: events.input + events.typedKeys >= 5 && before.bytes !== canonicalSize(),
    'held-arrows': events.arrowKeys >= 10 && moved,
    wheel: events.wheel >= 1 && events.scroll >= 5 && scrolled,
    'small-scroll-deltas': events.wheel >= 1 && events.smallScroll >= 5 && scrolled,
    'scrollbar-drag': events.scroll >= 10 && scrolled,
    'selection-drag': events.pointermove >= 5 && selected,
    'caret-movement': events.pointerdown >= 5 && before.head !== panel.dataset.selectionHead,
}[request.gesture] ?? false;
const sorted = [...intervals].sort((a,b) => a-b);
const quantile = q => sorted[Math.max(0,Math.ceil(sorted.length*q)-1)] ?? null;
const p95 = quantile(.95), max = sorted.at(-1) ?? null;
// GTK WebKit quantizes both public clocks to 1 ms without cross-origin
// isolation. Never report a fractional-clock pass from those samples.
const quantized = intervals.length > 0 && intervals.every(value => Math.abs(value - Math.round(value)) < 1e-6);
const idleSorted = [...idleIntervals].sort((a, b) => a - b);
// Idle WebKitGTK callbacks can run between display refreshes (1–16 ms on
// the same 60 Hz display). Their median estimates callback scheduling rather
// than refresh. The upper envelope resists those short callbacks; slow idle
// samples still fail the <=17 ms calibration cap, and gesture gates are unchanged.
const refreshInterval = idleSorted[Math.max(0, Math.ceil(idleSorted.length * .95) - 1)] ?? null;
const calibrated = idleSorted.length >= 30 && refreshInterval > 0 && refreshInterval <= 17;
const droppedFrameThreshold = calibrated ? refreshInterval * 1.5 : null;
const droppedFrames = calibrated ? intervals.filter(value => value > droppedFrameThreshold).length : null;
const eligible = request.build === 'release-desktop' && interacted && intervals.length >= 30;
const strictPassed = eligible && !quantized && p95 <= 16.7 && max <= 50;
const cadencePassed = eligible && calibrated && p95 <= droppedFrameThreshold && max <= 50;
dioxus.send({build: request.build, name: panel.querySelector('.mk-editor-path')?.textContent,
    preview: panel.querySelector('.mk-native-doc-preview')?.getAttribute('aria-pressed') === 'true',
    gesture: request.gesture, userAgent: navigator.userAgent,
    viewport: {width: viewport.clientWidth, height: viewport.clientHeight},
    calibration: 'idle-p95', wheelDeltas, scrollDistance,
    clock: 'performance.now', rafIntervals, quantized, idleIntervals, idleRafIntervals, refreshInterval,
    droppedFrameThreshold, droppedFrames, strictPassed, cadencePassed,
    gate: quantized ? 'calibrated-vsync' : '16.7ms',
    frames: intervals.length, p50: quantile(.5), p95, max, intervals, events, longFrames, interacted,
    passed: quantized ? cadencePassed : strictPassed});
