import { clickNativeCommand } from "./native-toolbar.mjs";
import {chromium} from 'playwright';
import assert from 'node:assert/strict';
const browser=await chromium.launch();const page=await browser.newPage({viewport:{width:1400,height:1400}});
const errors=[];page.on('pageerror',e=>errors.push(e.message));
const panel=page.locator('.language'),input=panel.locator('.mk-editor-core-input-sink'),canonical=page.locator('.language-canonical');
const ready=async()=>{await page.waitForFunction(()=>{const runs=[...document.querySelectorAll('.language .mk-proportional-run')];return runs.length&&runs.every(run=>run.dataset.ready==='true');});
 // Measurements publish row heights on the next animation frame.
 await page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))));
};
const preview=async()=>{await input.focus();await page.keyboard.press('Control+End');await ready();await panel.locator('.mk-editor-core-viewport').evaluate(el=>el.scrollTop=0);await ready();};
try{
 await page.goto(`http://127.0.0.1:${process.env.PORT??8099}/`,{waitUntil:'networkidle'});await page.click('#markdown-tables');await clickNativeCommand(panel.locator('.mk-native-preview'));await panel.evaluate(el=>el.style.height='1100px');await preview();
 const original=await canonical.textContent(),normalized=original.replaceAll('\r\n','\n');assert.ok(original.includes('\r\n'));
 assert.equal(await panel.locator('[role=table]').count(),2);assert.equal(await panel.locator('[role=columnheader]').count(),5);
 assert.deepEqual(await panel.locator('[role=table]').first().locator('[role=columnheader]').evaluateAll(es=>es.map(e=>getComputedStyle(e).textAlign)),['left','center','right']);
 assert.deepEqual(await panel.locator('[role=table]').first().locator('[role=row]').first().locator('.mk-table-cell').allTextContents(),['猫','a|b','Right']);assert.equal(await panel.locator('safe').count(),0);
 const web=panel.locator('.mk-preview-link').filter({hasText:'Web'});await page.keyboard.down('Control');await ready();const link=await web.evaluate(el=>{const range=document.createRange();range.selectNodeContents(el);const r=range.getClientRects()[0];return {x:r.x,y:r.y,height:r.height};});await page.mouse.click(link.x+5,link.y+link.height/2);await page.keyboard.up('Control');await page.waitForFunction(()=>document.querySelector('.opened-links').textContent.includes('https://example.org/table'));
 const tableStart=normalized.lastIndexOf('\n',normalized.indexOf('| **猫**')-1)+1;
 const token=normalized.slice(tableStart,normalized.indexOf('\n\na | b'));
 const start=Array.from(normalized.slice(0,normalized.indexOf(token))).length;
 for(const width of [320,650,1050]){
  await panel.evaluate((el,width)=>el.style.width=`${width}px`,width);await page.waitForTimeout(200);await preview();
  const table=panel.locator('[role=table]').first();assert.ok((await table.boundingBox()).width<=width);
  const rows=await panel.locator('.mk-editor-core-row').evaluateAll(es=>es.map(e=>{const r=e.getBoundingClientRect();return{top:r.top,bottom:r.bottom};}));assert.ok(rows.every((r,i)=>!i||Math.abs(r.top-rows[i-1].bottom)<1),'Table height participates in the row index');
  for(const [fraction,offset] of [[.2,start],[.8,start+Array.from(token).length]]){
   const box=await table.boundingBox();await page.mouse.click(box.x+box.width*fraction,box.y+box.height/2);await ready();
   assert.equal(Number(await panel.locator('.mk-native-surface').getAttribute('data-cursor-offset')),offset);assert.equal(await panel.locator('[role=table]').count(),1,'Only selected table reveals');
   await page.keyboard.insertText('猫');const prefix=Array.from(normalized).slice(0,offset).join(''),raw=prefix.length+(prefix.match(/\n/g)??[]).length;await page.waitForFunction(text=>document.querySelector('.language-canonical').textContent===text,original.slice(0,raw)+'猫'+original.slice(raw));
   await page.keyboard.press('Control+z');await page.waitForFunction(text=>document.querySelector('.language-canonical').textContent===text,original);await preview();
  }
 }
 await input.focus();await page.keyboard.press('Control+Home');for(let i=0;i<start+5;i++)await page.keyboard.press('ArrowRight');await ready();assert.equal(await panel.locator('[role=table]').count(),1,'Caret inside a cell reveals table source');
 await page.keyboard.press('Control+a');await ready();assert.equal(await panel.locator('[role=table]').count(),0);
 const copied=await input.evaluate(el=>{const event=new ClipboardEvent('copy',{bubbles:true,cancelable:true,clipboardData:new DataTransfer()});el.dispatchEvent(event);return event.clipboardData.getData('text/plain');});assert.equal(copied,normalized);
 await input.focus();await page.keyboard.press('Control+Home');for(let i=0;i<start+5;i++)await page.keyboard.press('ArrowRight');await ready();
 const before=Number(await panel.locator('.mk-native-surface').getAttribute('data-cursor-offset'));
 await input.evaluate(el=>{el.dispatchEvent(new CompositionEvent('compositionstart',{bubbles:true,data:''}));el.dispatchEvent(new CompositionEvent('compositionupdate',{bubbles:true,data:'你好'}));});
 await page.waitForFunction(()=>document.querySelector('.language .mk-native-surface').dataset.composing==='true');assert.equal(await canonical.textContent(),original);
 await input.evaluate(el=>{el.dispatchEvent(new CompositionEvent('compositionend',{bubbles:true,data:'你好'}));el.value='你好';el.dispatchEvent(new Event('input',{bubbles:true}));});
 const prefix=Array.from(normalized).slice(0,before).join(''),raw=prefix.length+(prefix.match(/\n/g)??[]).length;await page.waitForFunction(text=>document.querySelector('.language-canonical').textContent===text,original.slice(0,raw)+'你好'+original.slice(raw));await page.keyboard.press('Control+z');await page.waitForFunction(text=>document.querySelector('.language-canonical').textContent===text,original);
 await preview();await panel.evaluate(el=>el.style.height='250px');await page.waitForTimeout(250);const viewport=panel.locator('.mk-editor-core-viewport');await viewport.evaluate(el=>el.scrollTop=90);await page.waitForTimeout(200);await ready();
 const clipped=await panel.locator('[role=table]').first().boundingBox(),view=await viewport.boundingBox();assert.ok(clipped.y<view.y&&clipped.y+clipped.height>view.y);await page.mouse.click(clipped.x+clipped.width*.2,view.y+4);await ready();assert.equal(Number(await panel.locator('.mk-native-surface').getAttribute('data-cursor-offset')),start);assert.equal(await panel.locator('[role=table]').count(),1);
 await panel.evaluate(el=>el.style.height='1100px');await page.waitForTimeout(200);await preview();await page.click('#layout-delay');await clickNativeCommand(panel.locator('.mk-native-preview'));await page.waitForFunction(()=>document.querySelectorAll('.language [role=table]').length===0);assert.equal(await canonical.textContent(),original);assert.deepEqual(errors,[]);console.log('NATIVE MARKDOWN TABLES E2E: PASS');
}finally{await browser.close();}
