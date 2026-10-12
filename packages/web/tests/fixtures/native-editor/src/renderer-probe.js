// Fixture-only DOM events exercise the native adapter; this is not an OS/IME test.
try {
    const mode = await dioxus.recv();
    const wait = async (condition, message) => {
        for (let attempt = 0; attempt < 50; attempt++) {
            if (condition()) return;
            await new Promise(resolve => setTimeout(resolve, 100));
        }
        throw Error(message);
    };
    const canonical = () => document.querySelector('.canonical')?.textContent;
    await wait(() => document.querySelector('.primary .mk-editor-core-input-sink'), 'editor did not mount');
    const original = canonical();
    const sink = document.querySelector('.primary .mk-editor-core-input-sink');
    sink.focus();
    await wait(() => document.activeElement === sink, 'native input sink did not focus');
    const key = async (key, code, options = {}) => {
        sink.dispatchEvent(new KeyboardEvent('keydown', {key, code, bubbles: true, cancelable: true, ...options}));
        await new Promise(resolve => setTimeout(resolve, 100));
    };
    await key('Home', 'Home', {ctrlKey: true});
    await key('p', 'KeyP');
    await wait(() => canonical() === 'p' + original, 'native key adapter did not update Workspace');
    await key('Home', 'Home', {ctrlKey: true});
    await key('ArrowRight', 'ArrowRight', {shiftKey: true});
    await key('q', 'KeyQ');
    await wait(() => canonical() === 'q' + original, 'native selection replacement failed');
    await key('z', 'KeyZ', {ctrlKey: true});
    await wait(() => canonical() === 'p' + original, 'native undo failed');
    const panel = document.querySelector('.primary');
    const viewport = panel.querySelector('.mk-editor-core-viewport');
    const before = viewport.getBoundingClientRect().width;
    panel.style.width = '550px';
    panel.style.height = (mode.consumed || mode.multiline) ? '550px' : '350px';
    await wait(() => viewport.getBoundingClientRect().width < before, 'native resize did not update layout');
    await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    const first = panel.querySelector('[data-line="0"] .mk-editor-core-cell');
    if (first?.textContent !== 'p' || first.getBoundingClientRect().width <= 0) {
        throw Error('updated source did not render');
    }
    let measured = first;
    let edge = null;
    if (mode.proportional || mode.presentation) {
        document.getElementById(mode.multiline ? 'layout-multiline' : mode.consumed ? 'layout-consumed-runs' : mode.presentation ? 'layout-presentation' : 'layout-navigation-document').click();
        await wait(() => mode.presentation ? canonical()?.startsWith('Wi ') && canonical()?.includes('[[chip') : canonical()?.startsWith('Wi 😀'), 'navigation source did not mount');
        if (mode.presentation) await key('End','End',{ctrlKey:true});
        else document.getElementById('layout-proportional').click();
        await wait(() => {
            const runs = [...panel.querySelectorAll('.mk-proportional-run')];
            return runs.length > 1 && runs.every(run => run.dataset.ready === 'true');
        }, 'proportional geometry did not become ready');
        if (mode.consumed) {
            const rows = [...panel.querySelectorAll('.mk-editor-core-row[data-line="0"]')];
            const visible = [...panel.querySelectorAll('.mk-proportional-text')].map(el => el.textContent).join('');
            if (visible !== 'Wi  Chip tail' || rows.some(row => !row.querySelector('.mk-proportional-run'))) {
                throw Error('consumed source rows still render or source ownership is incomplete: '+JSON.stringify({visible,offset:panel.querySelector('.mk-native-surface')?.dataset.cursorOffset,rows:rows.map(row=>({text:row.textContent,height:row.getBoundingClientRect().height,measured:!!row.querySelector('.mk-proportional-run')}))}));
            }
            const block = panel.querySelector('.mk-native-block-widget').getBoundingClientRect();
            const next = panel.querySelector('.mk-editor-core-row[data-line="1"]').getBoundingClientRect();
            const expected = block.bottom + rows.reduce((sum, row) => sum + row.getBoundingClientRect().height, 0);
            if (Math.abs(expected - next.top) > 1) throw Error('consumed rows leave a visual gap');
        }
        const source = panel.querySelector('.mk-proportional-text');
        measured = source.firstElementChild;
        const top = measured.getBoundingClientRect().y;
        edge = 0;
        for (const span of source.children) {
            if (span.getBoundingClientRect().y > top + 10) break;
            edge = mode.presentation ? Number(span.dataset.sourceEnd) : edge + [...span.textContent].length;
        }
        if (!mode.presentation && (!edge || edge >= [...source.textContent].length)) throw Error('expected a CSS wrap boundary');
    }
    const events = {pointer: 0, key: 0, input: 0};
    const recent = [];
    let compositions = 0;
    let publishState = () => {};
    for (const type of ['pointerdown', 'keydown', 'input', 'compositionstart', 'compositionupdate', 'compositionend']) {
        document.addEventListener(type, event => {
            if (!event.isTrusted) return;
            if (type.startsWith('composition')) {
                if (type === 'compositionend') compositions++;
            } else events[type === 'pointerdown' ? 'pointer' : type === 'keydown' ? 'key' : 'input']++;
            recent.push({type, key: event.key, ctrl: event.ctrlKey, shift: event.shiftKey,
                repeat: event.repeat, data: event.data, target: event.target?.className,
                text: canonical()?.slice(0, 80), length: canonical()?.length,
                offset: panel.querySelector('.mk-native-surface')?.dataset.cursorOffset});
            if (recent.length > 80) recent.shift();
            dioxus.send({events: {...events}, last: type,
                target: event.target?.className, focused: document.activeElement?.className,
                recent: [...recent]});
            publishState();
        }, true);
    }
    const state = () => {
        const caret = panel.querySelector('.mk-proportional-run .mk-editor-core-caret');
        const rect = caret?.getBoundingClientRect();
        const widgets = [...panel.querySelectorAll('.mk-native-inline-widget')].filter(node=>node.textContent).map(node=>{
            const rect=node.getBoundingClientRect();
            const start=Number(node.closest('.mk-proportional-run').dataset.sourceStart);
            return {start:start+Number(node.dataset.sourceStart),end:start+Number(node.dataset.sourceEnd),x:rect.x,y:rect.y,width:rect.width,height:rect.height};
        });
        return {text: canonical(), offset: Number(panel.querySelector('.mk-native-surface')?.dataset.cursorOffset),
            widgets,
            compositions,
            composing: panel.querySelector('.mk-native-surface')?.dataset.composing === 'true',
            caret: rect ? {x: rect.x, y: rect.y} : null,
            visible: [...panel.querySelectorAll('.mk-proportional-text')].map(el=>el.textContent).join(''),
            ready: [...panel.querySelectorAll('.mk-proportional-run')].every(run => run.dataset.ready === 'true')};
    };
    dioxus.send({ok: true, text: canonical(), edge, focused: document.activeElement === sink,
        geometry: (() => {
            const rect = measured.getBoundingClientRect();
            return {x: rect.x, y: rect.y, width: rect.width, height: rect.height};
        })(),
        width: viewport.getBoundingClientRect().width,
        note: 'Synthetic DOM events through native WebKit; verify visible painting separately.'});
    dioxus.send({events: {...events}, last: 'listener-ready',
        focused: document.activeElement?.className});
    if (mode.proportional || mode.presentation) {
        let pending = false;
        const publish = () => {
            if (pending) return;
            pending = true;
            requestAnimationFrame(() => { pending = false; dioxus.send({state: state()}); });
        };
        new MutationObserver(publish).observe(panel, {subtree: true, attributes: true, childList: true, characterData: true});
        publishState = publish;
        publish();
    }
} catch (error) {
    dioxus.send({ok: false, error: String(error)});
}
