import { clickNativeCommand } from "./native-toolbar.mjs";
import {chromium} from 'playwright';
import assert from 'node:assert/strict';
const browser=await chromium.launch();const page=await browser.newPage({viewport:{width:1400,height:1600}});
const errors=[];page.on('pageerror',e=>errors.push(e.message));
const panel=page.locator('.language');const input=panel.locator('.mk-editor-core-input-sink');const canonical=page.locator('.language-canonical');
const ready=()=>page.waitForFunction(()=>{const runs=[...document.querySelectorAll('.language .mk-proportional-run')];return runs.length&&runs.every(run=>run.dataset.ready==='true');});
const preview=async()=>{await input.focus();await page.keyboard.press('Control+End');await ready();await panel.locator('.mk-editor-core-viewport').evaluate(el=>el.scrollTop=0);await ready();};
try {
 await page.goto(`http://127.0.0.1:${process.env.PORT??8099}/`,{waitUntil:'networkidle'});
 await page.click('#image-hold');await page.click('#markdown-images');await clickNativeCommand(panel.locator('.mk-native-preview'));await panel.evaluate(el=>el.style.height='1250px');await preview();
 const original=await canonical.textContent();assert.ok(original.includes('\r\n'));
 await panel.locator('.mk-image-placeholder').first().waitFor();
 assert.equal(await panel.locator('.mk-image-block').count(),5);
 await input.focus();await page.keyboard.press('Control+a');await page.keyboard.insertText('![replacement](broken.png)\r\n');
 await page.click('#image-release');await panel.locator('.mk-image-placeholder').filter({hasText:'Cannot display'}).waitFor();
 assert.equal(await panel.locator('img.mk-preview-image').count(),0,'A removed image cannot paint after its delayed fetch');
 await input.focus();await page.keyboard.press('Control+a');await page.keyboard.insertText(original);await preview();
await page.waitForFunction(()=>document.querySelectorAll('.language img.mk-preview-image').length===2&&[...document.querySelectorAll('.language img.mk-preview-image')].every(img=>img.complete&&img.naturalWidth===240));await ready();
 assert.deepEqual(await panel.locator('img.mk-preview-image').evaluateAll(images=>images.map(img=>img.alt)),['猫','Alias <safe>']);
 assert.equal(await panel.locator('safe').count(),0);assert.ok((await panel.locator('.mk-image-placeholder').allTextContents()).some(text=>text.includes('24 MiB')));
 const log=await page.locator('.image-log').textContent();assert.equal((log.match(/pics\/Café.svg/g)??[]).length,1,'Repeated paths share bytes');assert.equal(log.includes('huge.png'),false,'Metadata limit avoids fetching');assert.equal(log.includes('decoy'),false,'Origin source owns assets');
 for(const width of [300,900]){
  await panel.evaluate((el,width)=>el.style.width=`${width}px`,width);await page.waitForTimeout(200);await preview();
  const image=panel.locator('img.mk-preview-image').first();const box=await image.boundingBox();assert.ok(box.width<=width&&box.height<=320);
  await page.mouse.click(box.x+box.width/2,box.y+box.height/2);await ready();
  const offset=Array.from(original.replaceAll('\r\n','\n').split('![**猫**]')[0]).length;
  assert.equal(Number(await panel.locator('.mk-native-surface').getAttribute('data-cursor-offset')),offset);
  await page.keyboard.insertText('猫');await page.waitForFunction(text=>document.querySelector('.language-canonical').textContent===text,original.replace('![**猫**]','猫![**猫**]'));
  await page.keyboard.press('Control+z');await page.waitForFunction(text=>document.querySelector('.language-canonical').textContent===text,original);await preview();
 }
 await page.click('#image-version');await page.waitForFunction(()=>[...document.querySelectorAll('.language img.mk-preview-image')].every(img=>img.complete&&img.naturalWidth===120)&&document.querySelectorAll('.language img.mk-preview-image').length===2);await ready();
 await input.focus();await page.keyboard.press('Control+a');await ready();assert.equal(await panel.locator('.mk-image-block').count(),0);
 const copy=await input.evaluate(el=>{const event=new ClipboardEvent('copy',{bubbles:true,cancelable:true,clipboardData:new DataTransfer()});el.dispatchEvent(event);return event.clipboardData.getData('text/plain');});assert.equal(copy,original.replaceAll('\r\n','\n'));
 assert.equal(await canonical.textContent(),original);
 await input.focus();await page.keyboard.press('Control+Home');
 for(let i=0;i<Array.from(original.replaceAll('\r\n','\n').split('![**猫**]')[0]).length+1;i++) await page.keyboard.press('ArrowRight');
 await page.keyboard.press('Control+Enter');
 await page.waitForFunction(()=>document.querySelector('#active-key').textContent==='pics/Café.svg');
 assert.deepEqual(errors,[]);console.log('NATIVE MARKDOWN IMAGES E2E: PASS');
}finally{await browser.close();}
