import {chromium} from 'playwright';
import assert from 'node:assert/strict';
const browser=await chromium.launch();
const page=await browser.newPage({viewport:{width:1200,height:1400}});
const errors=[];page.on('pageerror',error=>errors.push(error.message));
const input=page.locator('.primary .mk-editor-core-input-sink');
const widget=page.locator('.primary .mk-native-inline-widget').filter({hasText:'Chip'});
const offset=async()=>Number(await page.locator('.primary .mk-native-surface').getAttribute('data-cursor-offset'));
const visible=async()=>(await page.locator('.primary .mk-proportional-text').allTextContents()).join('');
const ready=()=>page.waitForFunction(()=>{
  const runs=[...document.querySelectorAll('.primary .mk-proportional-run')];
  return runs.length&&runs.every(run=>run.dataset.ready==='true');
});
const move=async(target)=>{
  await input.focus();await page.keyboard.press('Control+Home');
  for(let attempts=0;await offset()<target&&attempts<target+30;attempts++) await page.keyboard.press('ArrowRight');
  assert.equal(await offset(),target);await ready();
};
try {
  await page.goto(`http://127.0.0.1:${process.env.PORT??8099}/`,{waitUntil:'networkidle'});
  await page.click('#layout-presentation');await ready();
  const original=await page.locator('.canonical').textContent();
  const hidden=Array.from(original.slice(0,original.indexOf('[[hidden]]'))).length;
  const chip=Array.from(original.slice(0,original.indexOf('[[chip]]'))).length;
  assert.equal(await widget.count(),1,'Caret before constructs keeps preview');
  for(const width of [340,550,950]) {
    await page.locator('.primary').evaluate((el,width)=>{el.style.width=`${width}px`;el.style.height='1000px';},width);
    await page.waitForTimeout(750);await ready();
    await move(hidden);
    assert.ok((await visible()).includes('[[hidden]]'),'Hidden range reveals at its source boundary');
    assert.equal(await widget.count(),1,'Other widget remains in preview');
    await page.keyboard.press('ArrowRight');await page.keyboard.press('Shift+ArrowRight');await ready();
    assert.equal(await widget.count(),1,'Local selection leaves unrelated widget rendered');
    await move(chip);
    assert.equal(await widget.count(),0,'Entered widget reveals its source');
    assert.ok((await visible()).includes('[[chip]]'));
    assert.ok(!(await visible()).includes('[[hidden]]'),'Other range remains hidden');
    await page.keyboard.press('Control+a');await ready();
    assert.equal(await visible(),original.split('\n')[0],'Selection crossing both reveals both');
    await move(chip);
  }
  await input.evaluate(el=>{
    el.dispatchEvent(new CompositionEvent('compositionstart',{bubbles:true,data:''}));
    el.dispatchEvent(new CompositionEvent('compositionupdate',{bubbles:true,data:'你好'}));
  });
  await page.waitForFunction(()=>document.querySelector('.primary .mk-native-surface').dataset.composing==='true');
  assert.ok((await visible()).includes('[[chip]]'));
  assert.ok(!(await visible()).includes('[[hidden]]'),'Composition preserves unrelated hidden preview');
  assert.equal(await page.locator('.canonical').textContent(),original);
  await input.evaluate(el=>{
    el.dispatchEvent(new CompositionEvent('compositionend',{bubbles:true,data:'你好'}));
    el.value='你好';el.dispatchEvent(new Event('input',{bubbles:true}));
  });
  const expected=Array.from(original).slice(0,chip).join('')+'你好'+Array.from(original).slice(chip).join('');
  await page.waitForFunction(text=>document.querySelector('.canonical').textContent===text,expected);
  await page.keyboard.press('Control+z');await ready();
  assert.equal(await page.locator('.canonical').textContent(),original);
  await page.click('#layout-delay');await move(hidden);await move(chip);await ready();
  assert.equal(await widget.count(),0);
  assert.ok(!(await visible()).includes('[[hidden]]'),'Late measurements do not restore a previous reveal set');
  await page.keyboard.press('Control+End');await ready();assert.equal(await widget.count(),1);
  assert.equal(await page.locator('.canonical').textContent(),original);
  assert.deepEqual(errors,[]);
  console.log('PASS: local caret/selection reveal, cross-run resize, unrelated preview during IME, single commit/undo and stale reveal invalidation');
} finally {await browser.close();}
