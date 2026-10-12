// Fixture-only controls and observations. No editor state is inferred from DOM.
await dioxus.recv();
try {
    const pause = ms => new Promise(resolve => setTimeout(resolve, ms));
    for (let i = 0; i < 50 && !document.querySelector('.primary .mk-editor-core-input-sink'); i++) await pause(100);
    document.getElementById('layout-navigation-document').click();
    await pause(300);
    document.getElementById('layout-proportional').click();
    await pause(5000);
    const panel = document.querySelector('.primary');
    const reference = document.createElement('textarea');
    reference.className = 'ime-reference';
    reference.placeholder = 'Plain native WebKit IME control';
    reference.style.cssText = 'position:fixed;right:10px;top:40px;width:280px;height:80px;z-index:99999';
    document.body.append(reference);
    const events = [];
    let referenceComposing = false;
    const geometry = () => [...panel.querySelectorAll('.mk-proportional-run')].map(run => {
        const source = run.querySelector('.mk-proportional-text');
        const rect = source.getBoundingClientRect();
        const range = document.createRange();
        range.selectNodeContents(source);
        return {...run.dataset, geometryResult:source.dataset.geometryResult, textLength: source.textContent.length, connected: source.isConnected,
            width: rect.width, height: rect.height, rectangles: range.getClientRects().length};
    });
    const state = () => ({reference: reference.value, referenceComposing,
        text: document.querySelector('.canonical')?.textContent,
        composing: panel.querySelector('.mk-native-surface')?.dataset.composing === 'true',
        focused: document.activeElement?.className, geometry: geometry(), events: [...events]});
    let pending = false;
    const publish = () => {
        if (pending) return;
        pending = true;
        requestAnimationFrame(() => { pending = false; dioxus.send({state: state()}); });
    };
    for (const type of ['keydown', 'input', 'compositionstart', 'compositionupdate', 'compositionend']) {
        document.addEventListener(type, event => {
            if (!event.isTrusted) return;
            if (event.target === reference) {
                if (type === 'compositionstart') referenceComposing = true;
                if (type === 'compositionend') referenceComposing = false;
            }
            events.push({type, key: event.key, data: event.data, target: event.target?.className});
            if (events.length > 80) events.shift();
            publish();
        }, true);
    }
    new MutationObserver(publish).observe(panel, {subtree:true, childList:true, attributes:true, characterData:true});
    const box = node => {
        const rect = node.getBoundingClientRect();
        return {x: rect.x, y: rect.y, width: rect.width, height: rect.height};
    };
    const source = panel.querySelector('.mk-proportional-text');
    dioxus.send({ok:true, geometry:box(source.firstElementChild), reference:box(reference), initial:state()});
    publish();
} catch (error) {
    dioxus.send({ok:false,error:String(error)});
}
