// Passive diagnostic observer, enabled only by MOONKALE_FPS_INPUT_REPORT.
const events = [], frames = [];
const publish = () => {
    const panel = document.querySelector('.primary .mk-crust');
    const viewport = panel?.querySelector('.mk-editor-core-viewport');
    const rect = viewport?.getBoundingClientRect();
    const rectOf = selector => {
        const r = document.querySelector(selector)?.getBoundingClientRect();
        return r && {x: r.x + r.width / 2, y: r.y + r.height / 2};
    };
    dioxus.send({events, frames, innerHeight: window.innerHeight, innerWidth: window.innerWidth, controls: {gesture: rectOf('.fps-recorder select'), record: rectOf('.fps-recorder button')}, viewport: rect && {x: rect.x, y: rect.y, width: rect.width, height: rect.height},
        preview: panel?.querySelector('.mk-native-doc-preview')?.getAttribute('aria-pressed'),
        head: panel?.dataset.selectionHead, anchor: panel?.dataset.selectionAnchor});
};
for (const type of ['keydown', 'keyup', 'wheel', 'scroll']) {
    document.addEventListener(type, event => {
        if (!event.isTrusted) return;
        events.push({type, key: event.key, code: event.code, repeat: event.repeat,
            ctrl: event.ctrlKey, deltaX: event.deltaX, deltaY: event.deltaY, deltaMode: event.deltaMode,
            target: event.target.className, left: event.target.scrollLeft, top: event.target.scrollTop});
        if (events.length > 300) events.shift();
        setTimeout(publish, 100);
    }, {capture: true, passive: true});
}
let previous;
const tick = timestamp => {
    const now = performance.now();
    if (previous !== undefined) frames.push({now, delta: now - previous, timestamp});
    previous = now;
    if (frames.length < 120) requestAnimationFrame(tick);
    else publish();
};
requestAnimationFrame(tick);
