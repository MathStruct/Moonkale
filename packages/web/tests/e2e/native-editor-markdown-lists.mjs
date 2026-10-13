import { chromium } from 'playwright';
import assert from 'node:assert/strict';
const browser = await chromium.launch();
const page = await browser.newPage({viewport:{width:1200,height:1100}});
const errors=[];page.on('pageerror',error=>errors.push(error.message));
const panel=page.locator('.language');
const input=panel.locator('.mk-editor-core-input-sink');
const lists=panel.locator('.mk-markdown-list-item');
const canonical=page.locator('.language-canonical');
const ready=()=>page.waitForFunction(()=>{
  const runs=[...document.querySelectorAll('.language .mk-proportional-run')];
  return runs.length && runs.every(run=>run.dataset.ready==='true');
});
const preview=async()=>{
  await input.focus();await page.keyboard.press('Control+End');await ready();
  await panel.locator('.mk-editor-core-viewport').evaluate(el=>el.scrollTop=0);await ready();
};
try {
  await page.goto(`http://127.0.0.1:${process.env.PORT??8099}/`,{waitUntil:'networkidle'});
  await page.click('#markdown-lists');
  await panel.locator('.mk-native-preview').click();await ready();
  const original=await canonical.textContent();
  const normalized=original.replaceAll('\r\n','\n');
  await panel.evaluate(el=>el.style.height='900px');await preview();
  assert.equal(await lists.count(),8);
  assert.deepEqual(await lists.allTextContents(),['• parent 猫','• child code','☑ completed 😀','☐ pending & safe','7. first italic','8. second','• wrapped line with words that can wrap at narrow widths continuation bold','• link']);
  assert.deepEqual(await lists.locator('.mk-markdown-list-content').evaluateAll(els=>els.map(el=>el.dataset.listDepth)),['0','1','1','0','0','0','0','0']);
  assert.deepEqual(await lists.locator('[data-task-checked]').evaluateAll(els=>els.map(el=>el.dataset.taskChecked)),['true','false']);
  assert.equal(await lists.locator('[data-strong="true"]').count(),2);
  assert.equal(await lists.locator('[data-code="true"]').count(),1);
  assert.equal(await panel.locator('.mk-markdown-list-item a, .mk-markdown-list-item safe').count(),0);
  for(const width of [340,800]) {
    await panel.evaluate((el,width)=>el.style.width=`${width}px`,width);
    await page.waitForTimeout(250);await preview();
    const rows=await panel.locator('.mk-editor-core-row').evaluateAll(els=>els.map(el=>{const r=el.getBoundingClientRect();return {top:r.top,bottom:r.bottom};}));
    assert.ok(rows.every((row,i)=>!i||Math.abs(row.top-rows[i-1].bottom)<1),'Measured item heights keep rows adjacent');
    const item=lists.filter({hasText:'completed 😀'});
    const rect=await item.boundingBox();
    await page.mouse.click(rect.x+3,rect.y+rect.height/2);await ready();
    const start=Array.from(normalized.slice(0,normalized.indexOf('  - [x] completed'))).length;
    assert.equal(Number(await panel.locator('.mk-native-surface').getAttribute('data-cursor-offset')),start);
    assert.equal(await lists.count(),7,'Only the clicked item reveals');
    await page.keyboard.insertText('猫');
    await page.waitForFunction(text=>document.querySelector('.language-canonical').textContent===text,original.replace('  - [x] completed','猫  - [x] completed'));
    await page.keyboard.press('Control+z');
    await page.waitForFunction(text=>document.querySelector('.language-canonical').textContent===text,original);
    await preview();
  }
  // Editing a task marker changes canonical source and updates presentation.
  await input.focus();await page.keyboard.press('Control+Home');
  const marker=Array.from(normalized.slice(0,normalized.indexOf('[ ] pending')+1)).length;
  for(let i=0;i<marker;i++)await page.keyboard.press('ArrowRight');
  await page.keyboard.press('Delete');await page.keyboard.insertText('x');
  const completed=original.replace('[ ] pending','[x] pending');
  await page.waitForFunction(text=>document.querySelector('.language-canonical').textContent===text,completed);
  await preview();assert.equal(await lists.filter({hasText:'☑ pending & safe'}).count(),1);
  // Undo both the insertion and deletion; redo follows the same source history.
  await page.keyboard.press('Control+z');await page.keyboard.press('Control+z');
  await page.waitForFunction(text=>document.querySelector('.language-canonical').textContent===text,original);
  await page.keyboard.press('Control+Shift+z');await page.keyboard.press('Control+Shift+z');
  await page.waitForFunction(text=>document.querySelector('.language-canonical').textContent===text,completed);
  await page.keyboard.press('Control+z');await page.keyboard.press('Control+z');
  await page.waitForFunction(text=>document.querySelector('.language-canonical').textContent===text,original);
  await input.focus();await page.keyboard.press('Control+a');await ready();
  assert.equal(await lists.count(),0,'Selection reveals complete canonical source');
  const copied=await input.evaluate(el=>{
    const event=new ClipboardEvent('copy',{bubbles:true,cancelable:true,clipboardData:new DataTransfer()});
    el.dispatchEvent(event);return event.clipboardData.getData('text/plain');
  });
  assert.equal(copied,normalized);assert.equal(await canonical.textContent(),original);
  assert.deepEqual(errors,[]);
  console.log('NATIVE MARKDOWN LISTS E2E: PASS');
} finally {await browser.close();}
