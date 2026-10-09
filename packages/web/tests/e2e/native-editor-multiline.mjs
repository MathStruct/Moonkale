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
const preview=async()=>{await input.focus();await page.keyboard.press('Control+End');await ready();};
try {
 await page.goto(`http://127.0.0.1:${process.env.PORT??8099}/`,{waitUntil:'networkidle'});
 await page.click('#layout-multiline');
 await page.locator('.primary').evaluate(el=>{el.style.width='340px';el.style.height='1000px';});
 await page.waitForTimeout(750);await ready();
 const original=await page.locator('.canonical').textContent();
 const hidden=Array.from(original.slice(0,original.indexOf('[[hidden:'))).length;
 const start=Array.from(original.slice(0,original.indexOf('[[chip:'))).length;
 const end=start+Array.from(original.slice(original.indexOf('[[chip:'),original.indexOf(']]',original.indexOf('[[chip:'))+2)).length;
 assert.equal(await widget.count(),1);
 assert.equal(await visible(),'Wi  gapChip tail','One owner per multiline range and unchanged surrounding source');
 assert.equal(await page.locator('.primary .mk-editor-core-row[data-line="1"]').count(),0,'Blank source line inside hidden range collapses');
 await input.focus();await page.keyboard.press('Control+Home');
 for(let n=0;n<hidden;n++) await page.keyboard.press('ArrowRight');
 await ready();
 assert.equal(await offset(),hidden);
 assert.equal(await widget.count(),1,'Entering hidden range leaves multiline widget in preview');
 assert.equal(await page.locator('.primary .mk-editor-core-row[data-line="1"]').count(),1,'Reveal restores the blank logical source line');
 await preview();
 for(const width of [340,550,950]) {
  await page.locator('.primary').evaluate((el,width)=>el.style.width=`${width}px`,width);
  await page.waitForTimeout(750);await ready();
  for(const [fraction,expected] of [[.2,start],[.8,end]]) {
   const rect=await widget.boundingBox();await page.mouse.click(rect.x+rect.width*fraction,rect.y+rect.height/2);
   assert.equal(await offset(),expected,'Multiline widget maps to full canonical endpoints');await ready();
   assert.equal(await widget.count(),0);
   assert.ok((await visible()).includes('[[chip:c'));
   assert.equal(await page.locator('.primary .mk-editor-core-row[data-line="1"]').count(),0,'Other multiline construct remains hidden');
   await page.keyboard.insertText('Q');await page.keyboard.press('Control+z');
   await page.waitForFunction(text=>document.querySelector('.canonical').textContent===text,original);
   await preview();
  }
 }
 // Reveal at a pointer boundary, then compose through the shared engine sink.
 const rect=await widget.boundingBox();await page.mouse.click(rect.x+rect.width*.2,rect.y+rect.height/2);await ready();
 await input.evaluate(el=>{el.dispatchEvent(new CompositionEvent('compositionstart',{bubbles:true,data:''}));el.dispatchEvent(new CompositionEvent('compositionupdate',{bubbles:true,data:'你好'}));});
 await page.waitForFunction(()=>document.querySelector('.primary .mk-native-surface').dataset.composing==='true');
 assert.equal(await page.locator('.canonical').textContent(),original);
 assert.equal(await page.locator('.primary .mk-editor-core-row[data-line="1"]').count(),0);
 await input.evaluate(el=>{el.dispatchEvent(new CompositionEvent('compositionend',{bubbles:true,data:'你好'}));el.value='你好';el.dispatchEvent(new Event('input',{bubbles:true}));});
 const expected=Array.from(original).slice(0,start).join('')+'你好'+Array.from(original).slice(start).join('');
 await page.waitForFunction(text=>document.querySelector('.canonical').textContent===text,expected);
 await page.keyboard.press('Control+z');await ready();
 await page.keyboard.press('Control+a');await ready();
 assert.equal(await page.locator('.primary .mk-editor-core-row[data-line="1"]').count(),1);
 assert.ok((await visible()).includes('[[hidden:a'));
 assert.ok((await visible()).includes('[[chip:c'));
 await page.keyboard.insertText('R');await page.keyboard.press('Control+z');
 await page.waitForFunction(text=>document.querySelector('.canonical').textContent===text,original);
 await preview();await page.keyboard.press('ArrowUp');await page.keyboard.press('Home');await page.keyboard.press('ArrowUp');await ready();
 assert.ok(await offset()<Array.from(original).length-10,'Up crosses back into multiline source');
 await page.click('#layout-delay');await input.focus();await page.keyboard.press('Control+a');await page.keyboard.press('Control+End');await ready();
 assert.equal(await widget.count(),1,'Late revealed geometry cannot replace multiline preview');
 await page.locator('.primary').evaluate(el=>{el.style.height='190px';el.style.width='550px';});
 await page.waitForTimeout(750);await input.focus();await page.keyboard.press('Control+End');
 await page.waitForFunction(()=>Number(document.querySelector('.primary .mk-native-surface').dataset.cursorOffset)===Array.from(document.querySelector('.canonical').textContent).length);
 assert.equal(await page.locator('.canonical').textContent(),original);
 assert.deepEqual(errors,[]);
 console.log('PASS: multiline/newline ownership, empty-line collapse/reveal, pointer endpoints, resize, selection, navigation, undo, composition, scrolling and stale masks');
} finally {await browser.close();}
