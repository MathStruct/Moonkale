// Fixture-only DOM input; the external runner separately inspects AT-SPI.
try {
  const wait = async predicate => {
    for (let attempt = 0; attempt < 200; attempt++) {
      if (predicate()) return;
      await new Promise(resolve => setTimeout(resolve, 50));
    }
    throw Error('native accessible source context did not become ready');
  };
  const root = () => document.querySelector('.primary [data-editor=rust]');
  await wait(() => root()?.querySelector('.mk-editor-core-input-sink'));
  root().querySelector('.mk-editor-core-input-sink').focus();
  const key = async (key, code, extra = {}) => {
    root().querySelector('.mk-editor-core-input-sink').dispatchEvent(new KeyboardEvent('keydown', {key, code, bubbles:true, cancelable:true, ...extra}));
    await new Promise(resolve => setTimeout(resolve, 100));
  };
  await key('ArrowDown', 'ArrowDown');
  await key('End', 'End');
  await key('ArrowLeft', 'ArrowLeft', {shiftKey:true});
  await wait(() => root().querySelector('[role=status]').textContent.includes('Selection from line 2'));
  await dioxus.send({passed:true, context:root().querySelector('[role=status]').textContent});
} catch (error) {
  await dioxus.send({passed:false, error:String(error)});
}
await dioxus.recv();
