import {chromium} from 'playwright';
import assert from 'node:assert/strict';
const browser=await chromium.launch();const page=await browser.newPage({viewport:{width:1400,height:1700}});
const errors=[];page.on('pageerror',e=>errors.push(e.message));
const panel=page.locator('.language'),input=panel.locator('.mk-editor-core-input-sink'),canonical=page.locator('.language-canonical');
const ready=()=>page.waitForFunction(()=>{const runs=[...document.querySelectorAll('.language .mk-proportional-run')];return runs.length&&runs.every(run=>run.dataset.ready==='true'&&run.dataset.fontLoading==='false');});
const preview=async()=>{await input.focus();await page.keyboard.press('Control+End');await ready();await panel.locator('.mk-editor-core-viewport').evaluate(el=>el.scrollTop=0);await page.waitForTimeout(200);await ready();};
const offset=()=>panel.locator('.mk-native-surface').getAttribute('data-cursor-offset').then(Number);
try{
 await page.goto(`http://127.0.0.1:${process.env.PORT??8099}/`,{waitUntil:'networkidle'});await page.click('#markdown-callouts');await panel.locator('.mk-native-preview').click();await panel.evaluate(el=>el.style.height='1050px');await preview();
 const original=await canonical.textContent(),normalized=original.replaceAll('\r\n','\n');assert.ok(original.includes('\r\n'));
 assert.equal(await panel.locator('.mk-callout').count(),4);assert.equal(await panel.locator('.mk-callout[data-callout-type=note] .mk-callout-title').textContent(),'猫 and literal');assert.equal(await panel.locator('.mk-callout[data-callout-type=proof] .mk-callout-title').textContent(),'proof');assert.equal(await panel.locator('safe').count(),0);
 const warning=panel.locator('.mk-callout[data-callout-type=warning]'),toggle=warning.locator('button'),body=warning.locator('.mk-callout-body');
 assert.equal(await toggle.getAttribute('aria-expanded'),'false');await body.waitFor({state:'hidden'});assert.equal(await panel.locator('.katex').count(),0,'Hidden math waits for visibility');
 const before=await offset(),closed=await warning.boundingBox();await toggle.click();await page.waitForFunction(()=>document.querySelector('.language .mk-callout[data-callout-type=warning] .katex'));await ready();assert.equal(await offset(),before);assert.equal(await toggle.getAttribute('aria-expanded'),'true');assert.ok((await warning.boundingBox()).height>closed.height);assert.equal(await canonical.textContent(),original);
 await toggle.focus();await page.keyboard.press('Space');await ready();assert.equal(await toggle.getAttribute('aria-expanded'),'false');assert.equal(await offset(),before);await page.keyboard.press('Enter');await ready();assert.equal(await toggle.getAttribute('aria-expanded'),'true');
 const web=panel.locator('.mk-preview-link').filter({hasText:'Web'});let box=await web.boundingBox();await page.keyboard.down('Control');await page.mouse.click(box.x+4,box.y+box.height/2);await page.keyboard.up('Control');await page.waitForFunction(()=>document.querySelector('.opened-links').textContent.includes('https://example.org/callout'));
 const start=Array.from(normalized.split('  > [!NOTE]')[0]).length;const end=start+Array.from(normalized.slice(normalized.indexOf('  > [!NOTE]'),normalized.indexOf('\n\n> [!warning]'))).length;
 for(const width of [320,650,1050]){
  await panel.evaluate((el,width)=>el.style.width=`${width}px`,width);await page.waitForTimeout(150);await preview();
  const note=panel.locator('.mk-callout[data-callout-type=note]');assert.ok((await note.boundingBox()).width<=width);
  const rows=await panel.locator('.mk-editor-core-row').evaluateAll(es=>es.map(e=>{const r=e.getBoundingClientRect();return{top:r.top,bottom:r.bottom};}));assert.ok(rows.every((r,i)=>!i||Math.abs(r.top-rows[i-1].bottom)<1),'Callout height participates in row index');
  for(const [fraction,expected] of [[.2,start],[.8,end]]){
   await note.scrollIntoViewIfNeeded();box=await note.boundingBox();await page.mouse.click(box.x+box.width*fraction,box.y+box.height/2);await ready();assert.equal(await offset(),expected);assert.equal(await panel.locator('.mk-callout').count(),3,'Only selected callout reveals');
   const prefix=Array.from(normalized).slice(0,expected).join(''),raw=prefix.length+(prefix.match(/\n/g)??[]).length;await page.keyboard.insertText('猫');await page.waitForFunction(text=>document.querySelector('.language-canonical').textContent===text,original.slice(0,raw)+'猫'+original.slice(raw));await page.keyboard.press('Control+z');await page.waitForFunction(text=>document.querySelector('.language-canonical').textContent===text,original);await preview();
  }
 }
 await input.focus();await page.keyboard.press('Control+Home');for(let i=0;i<start+8;i++)await page.keyboard.press('ArrowRight');await ready();assert.equal(await panel.locator('.mk-callout').count(),3);
 await input.evaluate(el=>{el.dispatchEvent(new CompositionEvent('compositionstart',{bubbles:true,data:''}));el.dispatchEvent(new CompositionEvent('compositionupdate',{bubbles:true,data:'你好'}));});await page.waitForFunction(()=>document.querySelector('.language .mk-native-surface').dataset.composing==='true');assert.equal(await canonical.textContent(),original);await input.evaluate(el=>{el.dispatchEvent(new CompositionEvent('compositionend',{bubbles:true,data:'你好'}));el.value='你好';el.dispatchEvent(new Event('input',{bubbles:true}));});await page.waitForFunction(()=>document.querySelector('.language-canonical').textContent.includes('你好'));await page.keyboard.press('Control+z');await page.waitForFunction(text=>document.querySelector('.language-canonical').textContent===text,original);
 await input.focus();await page.keyboard.press('Control+a');await ready();assert.equal(await panel.locator('.mk-callout').count(),0);const copied=await input.evaluate(el=>{const event=new ClipboardEvent('copy',{bubbles:true,cancelable:true,clipboardData:new DataTransfer()});el.dispatchEvent(event);return event.clipboardData.getData('text/plain');});assert.equal(copied,normalized);
 await preview();await page.click('#layout-delay');await panel.locator('.mk-native-preview').click();await page.waitForFunction(()=>document.querySelectorAll('.language .mk-callout').length===0);assert.equal(await canonical.textContent(),original);assert.deepEqual(errors,[]);console.log('NATIVE MARKDOWN CALLOUTS E2E: PASS');
}finally{await browser.close();}
