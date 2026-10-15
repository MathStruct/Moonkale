// Fixture-only native WebKit DOM input with an installed live language server.
// This checks the production panel and native adapter, not OS/IME delivery or FPS.
try {
    const wait = async (predicate, label, timeout = 30000) => {
        const deadline = Date.now() + timeout;
        while (Date.now() < deadline) {
            if (predicate()) return;
            await new Promise(resolve => setTimeout(resolve, 50));
        }
        throw Error(label);
    };
    const root = () => document.querySelector('.primary [data-editor=rust]');
    const text = () => document.querySelector('.canonical')?.textContent;
    const key = async (key, code, extra = {}) => {
        const sink = root().querySelector('.mk-editor-core-input-sink');
        sink.focus();
        sink.dispatchEvent(new KeyboardEvent('keydown', {key, code, bubbles: true, cancelable: true, ...extra}));
        await new Promise(resolve => setTimeout(resolve, 75));
    };
    const at = async needle => {
        const offset = text().indexOf(needle);
        if (offset < 0) throw Error(`missing source: ${needle}`);
        await key('Home', 'Home', {ctrlKey: true});
        for (const _ of text().slice(0, offset + 1).replace(/\r\n/g, '\n')) {
            await key('ArrowRight', 'ArrowRight');
        }
    };
    await wait(() => root(), 'native editor did not mount');
    const original = text();
    await wait(() => root().querySelector('.mk-native-diagnostic-error'), 'live diagnostics missing', 120000);
    await at('add(1, 2)');
    root().querySelector('.mk-native-show-hover').click();
    await wait(() => root().querySelector('.mk-hover')?.textContent.includes('fn add'), 'live hover missing');
    await key('F12', 'F12');
    await wait(() => root().dataset.selectionHead === '3', 'live definition did not reveal declaration');
    await at('add(1, 2)');
    await key('F2', 'F2');
    await wait(() => root().querySelector('.mk-editor-rename input'), 'rename prompt missing');
    const rename = root().querySelector('.mk-editor-rename input');
    rename.value = 'plus';
    rename.dispatchEvent(new Event('input', {bubbles: true}));
    rename.dispatchEvent(new KeyboardEvent('keydown', {key: 'Enter', code: 'Enter', bubbles: true}));
    await wait(() => text().includes('fn plus(') && text().includes('plus(1, 2)'), 'live rename did not apply');
    await at('plus(1, 2)');
    await key('F12', 'F12', {shiftKey: true});
    await wait(() => Number(root().querySelector('.mk-native-references')?.dataset.count) >= 2, 'live references missing');
    root().querySelector('.mk-native-references .mk-native-tool-item').click();
    await wait(() => !root().querySelector('.mk-native-tools'), 'reference did not reveal');
    await at('a + b');
    await key('Home', 'Home');
    await key('ArrowRight', 'ArrowRight', {ctrlKey: true});
    await key('End', 'End', {shiftKey: true});
    await key('.', 'Period', {ctrlKey: true});
    await wait(() => [...root().querySelectorAll('.mk-native-actions .mk-native-tool-item')].some(x => /Extract into variable/i.test(x.textContent)), 'live code actions missing');
    [...root().querySelectorAll('.mk-native-actions .mk-native-tool-item')].find(x => /Extract into variable/i.test(x.textContent)).click();
    await wait(() => /let \w+ = a \+ b;/.test(text()), 'live code action did not apply');
    // Return to a valid call and request completion at the same live document.
    await at('plus(1, 2)');
    root().querySelector('.mk-native-complete').click();
    await wait(() => [...root().querySelectorAll('.mk-native-completion-label')].some(x => x.textContent === 'plus'), 'live completion missing');
    await dioxus.send({passed: true, checks: ['diagnostics', 'hover', 'definition', 'rename', 'references', 'codeAction', 'completion'], original, canonical: text(), userAgent: navigator.userAgent});
} catch (error) {
    await dioxus.send({passed: false, error: String(error), canonical: document.querySelector('.canonical')?.textContent});
}
// Keep the native eval owner alive until Rust has received the report.
await dioxus.recv();
