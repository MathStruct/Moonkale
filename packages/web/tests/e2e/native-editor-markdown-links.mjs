import {chromium} from 'playwright';
import assert from 'node:assert/strict';
const browser=await chromium.launch();
const page=await browser.newPage({viewport:{width:1200,height:1200}});
const errors=[];page.on('pageerror',e=>errors.push(e.message));
const panel=page.locator('.language');
const input=panel.locator('.mk-editor-core-input-sink');
const links=panel.locator('.mk-preview-link');
const canonical=page.locator('.language-canonical');
const ready=()=>page.waitForFunction(()=>{
 const runs=[...document.querySelectorAll('.language .mk-proportional-run')];
 return runs.length && runs.every(run=>run.dataset.ready==='true');
});
const preview=async()=>{await input.focus();await page.keyboard.press('Control+End');await ready();await panel.locator('.mk-editor-core-viewport').evaluate(el=>el.scrollTop=0);await ready();};
const click=async(label,modified)=>{
 const link=links.filter({hasText:label});
 const box=await link.evaluate(el=>{const range=document.createRange();range.selectNodeContents(el);const r=[...range.getClientRects()].find(r=>r.width>0&&r.height>0);return {x:r.x,y:r.y,width:r.width,height:r.height};});
 if(modified)await page.keyboard.down('Control');
 await page.mouse.click(box.x+Math.min(box.width/2,5),box.y+box.height/2);
 if(modified)await page.keyboard.up('Control');
 await ready();
};
try {
 await page.goto(`http://127.0.0.1:${process.env.PORT??8099}/`,{waitUntil:'networkidle'});
 await page.click('#markdown-links');await panel.locator('.mk-native-preview').click();
 await panel.evaluate(el=>el.style.height='950px');await preview();
 const original=await canonical.textContent();const normalized=original.replaceAll('\r\n','\n');
 assert.deepEqual(await links.allTextContents(),['Web 猫','Alias <safe>','site','local','Mail','Task alias','Reference','https://example.org/auto']);
 assert.equal(await panel.locator('safe').count(),0,'Alias is text, not HTML');
 assert.equal(await links.filter({hasText:'Web 猫'}).getAttribute('data-strong'),'true');
 for(const width of [300,800]) {
  await panel.evaluate((el,width)=>el.style.width=`${width}px`,width);await page.waitForTimeout(250);await preview();
  await click('Web 猫',true);
  await page.waitForFunction(()=>document.querySelector('.opened-links').textContent.includes('https://example.org/path?q=1#part'));
  assert.equal(await canonical.textContent(),original,'Following leaves source/history unchanged');
  await click('site',true);
  await page.waitForFunction(()=>document.querySelector('.opened-links').textContent.includes('https://example.org/heading'));
  await click('Mail',true);
  await page.waitForFunction(()=>document.querySelector('.opened-links').textContent.includes('mailto:a@example.org'));
  const before=await page.locator('.opened-links').textContent();
  await click('Alias <safe>',false);
  const start=Array.from(normalized.slice(0,normalized.indexOf('[[Note é#Heading|Alias'))).length;
  assert.equal(Number(await panel.locator('.mk-native-surface').getAttribute('data-cursor-offset')),start);
  assert.equal(await page.locator('.opened-links').textContent(),before,'Ordinary click reveals source');
  await page.keyboard.insertText('猫');
  await page.waitForFunction(text=>document.querySelector('.language-canonical').textContent===text,original.replace('[[Note é#Heading|Alias','猫[[Note é#Heading|Alias'));
  await page.keyboard.press('Control+z');await page.waitForFunction(text=>document.querySelector('.language-canonical').textContent===text,original);
  await preview();
 }
 await page.click('#wiki-delay');
 await click('Alias <safe>',true);
 await input.focus();await page.keyboard.insertText('x');
 await page.click('#wiki-release');await page.waitForTimeout(400);
 assert.equal(await page.locator('#active-key').textContent(),'nested.md','Origin edit cancels delayed preview navigation');
 await input.focus();await page.keyboard.press('Control+z');
 await page.waitForFunction(text=>document.querySelector('.language-canonical').textContent===text,original);
 await preview();
 await click('local',true);
 await page.waitForFunction(()=>document.querySelector('.language .mk-native-surface').dataset.cursorOffset===String(Array.from(document.querySelector('.language-canonical').textContent.replaceAll('\r\n','\n').split('# Heading')[0]).length));
 await preview();await input.focus();await page.keyboard.press('Control+a');await ready();
 assert.equal(await links.count(),0);
 const copied=await input.evaluate(el=>{const event=new ClipboardEvent('copy',{bubbles:true,cancelable:true,clipboardData:new DataTransfer()});el.dispatchEvent(event);return event.clipboardData.getData('text/plain');});
 assert.equal(copied,normalized);assert.equal(await canonical.textContent(),original);
 assert.deepEqual(errors,[]);console.log('NATIVE MARKDOWN LINKS E2E: PASS');
}finally{await browser.close();}
