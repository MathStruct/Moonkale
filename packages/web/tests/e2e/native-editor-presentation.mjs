import {chromium} from 'playwright';
import assert from 'node:assert/strict';
const browser=await chromium.launch();
const page=await browser.newPage({viewport:{width:1200,height:1200}});
const errors=[]; page.on('pageerror',e=>errors.push(e.message));
const cursor=async()=>Number(await page.locator('.primary .mk-native-surface').getAttribute('data-cursor-offset'));
const ready=()=>page.waitForFunction(()=>{
  const runs=[...document.querySelectorAll('.primary .mk-proportional-run')];
  return runs.length>0&&runs.every(run=>run.dataset.ready==='true');
});
const input=page.locator('.primary .mk-editor-core-input-sink');
const preview=async()=>{await input.focus();await page.keyboard.press('Control+End');await ready();};
try {
  await page.goto(`http://127.0.0.1:${process.env.PORT??8099}/`,{waitUntil:'networkidle'});
  await page.click('#layout-presentation'); await ready();
  const original=await page.locator('.canonical').textContent();
  const start=Array.from(original.slice(0,original.indexOf('[[chip]]'))).length,end=start+8;
  assert.equal(await page.locator('.primary .mk-native-inline-widget').filter({hasText:'Chip'}).count(),1,'Unrelated caret keeps both constructs in preview');
  const block=page.locator('.primary .mk-native-block-widget');
  assert.equal(await block.textContent(),'Source-anchored block widget');
  assert.equal(await block.getAttribute('data-source-anchor'),'0');
  assert.ok(Math.abs((await block.boundingBox()).height-72)<1);
  await preview();
  const widget=page.locator('.primary .mk-native-inline-widget').filter({hasText:'Chip'});
  assert.equal(await widget.count(),1);
  assert.ok(!(await page.locator('.primary .mk-proportional-text').allTextContents()).join('').includes('[[hidden]]'));
  assert.equal(await page.locator('.canonical').textContent(),original,'Presentation never edits canonical source');
  for(const [fraction,expected] of [[.2,start],[.8,end]]) {
    const rect=await widget.boundingBox();
    await page.mouse.click(rect.x+rect.width*fraction,rect.y+rect.height/2);
    assert.equal(await cursor(),expected,'Widget pointer chooses a source boundary');
    await ready();
    assert.equal(await widget.count(),0,'Entering widget reveals its source');
    assert.ok(!(await page.locator('.primary .mk-proportional-text').allTextContents()).join('').includes('[[hidden]]'),'Other construct remains hidden');
    await page.keyboard.insertText('Q');
    const text=Array.from(original).slice(0,expected).join('')+'Q'+Array.from(original).slice(expected).join('');
    await page.waitForFunction(text=>document.querySelector('.canonical').textContent===text,text);
    await page.keyboard.press('Control+z');
    await page.waitForFunction(text=>document.querySelector('.canonical').textContent===text,original);
    await preview();
  }
  const crossed=new Set();
  for(const width of [340,480,500,520,550,670,950]) {
    await input.focus();await page.keyboard.press('Control+Home');await page.keyboard.press('Control+a');
    await page.locator('.primary').evaluate((el,width)=>{el.style.width=`${width}px`;el.style.height='1000px';},width);
    await page.waitForTimeout(750);await ready();
    const boundaries=await page.locator('.primary .mk-proportional-run').evaluateAll(runs=>runs.map(run=>Number(run.dataset.sourceStart)));
    for(const token of ['[[hidden]]','[[chip]]']) {
      const a=Array.from(original.slice(0,original.indexOf(token))).length,b=a+token.length;
      if(boundaries.some(offset=>a<offset&&offset<b)) crossed.add(token);
    }
    await preview();
    assert.equal(await widget.count(),1,`Exactly one widget after resize to ${width}`);
    const visible=(await page.locator('.primary .mk-proportional-text').allTextContents()).join('');
    assert.equal(visible,original.split('\n')[0].replace('[[hidden]]','').replace('[[chip]]','Chip'),'Mapped runs preserve all remaining source in order');
    for(const [fraction,expected] of [[.2,start],[.8,end]]) {
      const rect=await widget.boundingBox();
      await page.mouse.click(rect.x+rect.width*fraction,rect.y+rect.height/2);
      assert.equal(await cursor(),expected,'Cross-run widget hits preserve full source endpoints');
      await ready();await preview();
    }
  }
  assert.deepEqual([...crossed].sort(),['[[chip]]','[[hidden]]'],'Both replacements actually cross an engine wrap boundary');
  await block.click();assert.equal(await cursor(),0,'Block pointer maps to its source anchor');
  await ready();
  await page.keyboard.press('Control+a');await ready();
  assert.equal(await page.locator('.primary .mk-native-inline-widget').count(),0,'Canonical selection reveals hidden ranges');
  await page.keyboard.insertText('R');
  await page.waitForFunction(()=>document.querySelector('.canonical').textContent==='R');
  await page.keyboard.press('Control+z');
  await page.waitForFunction(text=>document.querySelector('.canonical').textContent===text,original);
  await input.focus();await page.keyboard.press('Control+Home');await ready();
  await input.evaluate(el=>{
    el.dispatchEvent(new CompositionEvent('compositionstart',{bubbles:true,data:''}));
    el.dispatchEvent(new CompositionEvent('compositionupdate',{bubbles:true,data:'你好'}));
  });
  await page.waitForFunction(()=>document.querySelector('.primary .mk-native-surface').dataset.composing==='true');
  assert.equal(await widget.count(),1,'Composition outside replacements keeps unrelated preview');
  assert.equal(await page.locator('.canonical').textContent(),original);
  await input.evaluate(el=>{
    el.dispatchEvent(new CompositionEvent('compositionend',{bubbles:true,data:'你好'}));
    el.value='你好';el.dispatchEvent(new Event('input',{bubbles:true}));
  });
  await page.waitForFunction(text=>document.querySelector('.canonical').textContent==='你好'+text,original);
  await page.keyboard.press('Control+z');
  await page.waitForFunction(text=>document.querySelector('.canonical').textContent===text,original);
  await page.click('#layout-delay');
  await input.focus();await page.keyboard.press('Control+End');await page.keyboard.press('Control+a');
  await ready();assert.equal(await page.locator('.primary .mk-native-inline-widget').count(),0,'Late preview geometry cannot re-hide active source');
  await page.click('#layout-navigation-document');await ready();
  await preview();assert.equal(await page.locator('.primary .mk-native-inline-widget').count(),0,'Provider output is rebuilt for the new revision');
  assert.deepEqual(errors,[]);
  console.log('PASS: hidden/inline/block source mapping, source reveal, selection/replacement/undo, composition and stale preview invalidation');
} finally {await browser.close();}
