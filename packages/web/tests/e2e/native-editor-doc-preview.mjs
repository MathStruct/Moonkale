import {chromium} from 'playwright';
import assert from 'node:assert/strict';
const browser=await chromium.launch();
const page=await browser.newPage({viewport:{width:1500,height:1800}});
const errors=[];page.on('pageerror',error=>errors.push(error.message));
const panel=page.locator('.language'),input=panel.locator('.mk-editor-core-input-sink');
const canonical=()=>page.locator('.language-canonical').textContent();
const ready=()=>page.waitForFunction(()=>{const runs=[...document.querySelectorAll('.language .mk-proportional-run')];return runs.every(run=>run.dataset.ready==='true'&&run.dataset.fontLoading==='false');});
const view=async()=>{await page.waitForFunction(()=>{const p=document.querySelector('.language');return p?.querySelector('.mk-native-surface')?.dataset.docPreview==='true'&&p.querySelector('.mk-native-surface').dataset.wrap===p.querySelector('.mk-native-wrap').getAttribute('aria-pressed');});await input.focus();await page.keyboard.press('Control+End');await panel.scrollIntoViewIfNeeded();await panel.locator('.mk-editor-core-viewport').evaluate(el=>el.scrollTop=0);await ready();};
try{
 await page.goto(`http://127.0.0.1:${process.env.PORT??8099}/`,{waitUntil:'networkidle'});
 for(const language of ['rust','python','julia']){
  await page.click(`#doc-${language}`);await page.waitForFunction(language=>document.querySelector('.language .mk-native-surface')?.dataset.language===language,language);await panel.evaluate(el=>el.style.height='1000px');
  const original=await canonical(),normalized=original.replaceAll('\r\n','\n');
  const wrap=panel.locator('.mk-native-wrap');if(await wrap.getAttribute('aria-pressed')==='true')await wrap.click();
  const toggle=panel.locator('.mk-native-doc-preview');if(await toggle.getAttribute('aria-pressed')==='false')await toggle.click();
  await view();assert.equal(await wrap.getAttribute('aria-pressed'),'false');
  assert.equal(await panel.locator('.mk-documentation-preview').count(),2,language);
  assert.match(await panel.locator('.mk-doc-body').first().textContent(),new RegExp(`${language[0].toUpperCase()+language.slice(1)} docs 猫`));
  assert.equal(await panel.locator('safe').count(),0);assert.equal(await panel.locator('.mk-native-properties').count(),0);
  const declaration=language==='rust'?'fn sample':language==='python'?'value =':'function sample';
  const line=normalized.slice(0,normalized.indexOf(declaration)).split('\n').length-1;
  assert.equal(await panel.locator(`.mk-editor-core-row[data-line="${line}"] .mk-proportional-run`).count(),0,'Code keeps its grid');
  if(language==='rust'){
   assert.equal(await panel.locator('.mk-doc-code[data-code-language=rust]').count(),1);
   assert.equal(await panel.locator('.mk-doc-body [role=table]').count(),1);
   assert.match(await panel.locator('.mk-doc-body').first().textContent(),/\[\[Wiki\]\] \$x\$ <safe>/);
   assert.match(await panel.locator('.mk-doc-body').first().textContent(),/parentchild/);
   const link=panel.locator('.mk-doc-body .mk-preview-link');await link.scrollIntoViewIfNeeded();await ready();const b=await link.boundingBox();
   await page.keyboard.down('Control');await page.mouse.click(b.x+3,b.y+b.height/2);await page.keyboard.up('Control');
   await page.waitForFunction(()=>document.querySelector('.opened-links').textContent.includes('https://example.org/docs'));
   assert.equal(await canonical(),original);
  }
  const first=panel.locator('.mk-doc-body').first();await first.scrollIntoViewIfNeeded();await ready();const box=await first.boundingBox();
  await page.mouse.click(box.x+8,box.y+10);await ready();
  assert.equal(Number(await panel.locator('.mk-native-surface').getAttribute('data-cursor-offset')),0);
  assert.equal(await panel.locator('.mk-documentation-preview').count(),1);
  assert.equal(await panel.locator('.mk-editor-core-row[data-line="0"] .mk-proportional-run').count(),0,'Revealed documentation uses the normal code grid');
  await page.keyboard.press('End');await page.keyboard.insertText(' edited');
  const end=original.indexOf('\r\n');const changed=original.slice(0,end)+' edited'+original.slice(end);
  await page.waitForFunction(text=>document.querySelector('.language-canonical').textContent===text,changed);
  await page.keyboard.press('Control+z');await page.waitForFunction(text=>document.querySelector('.language-canonical').textContent===text,original);
  await page.keyboard.press('Control+Shift+z');await page.waitForFunction(text=>document.querySelector('.language-canonical').textContent===text,changed);
  await page.keyboard.press('Control+z');await view();
  await input.focus();await page.keyboard.press('Control+f');await panel.locator('.mk-search-query').fill('docs 猫');await panel.getByRole('button',{name:'Next',exact:true}).click();await ready();
  assert.equal(await panel.locator('.mk-documentation-preview').count(),1);
  await panel.locator('.mk-search-query').press('Escape');await input.focus();await page.keyboard.press('Control+a');await ready();
  assert.equal(await panel.locator('.mk-documentation-preview').count(),0);
  const copied=await input.evaluate(el=>{const e=new ClipboardEvent('copy',{bubbles:true,cancelable:true,clipboardData:new DataTransfer()});el.dispatchEvent(e);return e.clipboardData.getData('text/plain');});assert.equal(copied,normalized);
  await input.evaluate(el=>el.dispatchEvent(new CompositionEvent('compositionstart',{bubbles:true})));
  await input.evaluate(el=>el.dispatchEvent(new CompositionEvent('compositionupdate',{bubbles:true,data:'临时'})));
  assert.equal(await canonical(),original);await input.evaluate(el=>el.dispatchEvent(new CompositionEvent('compositionend',{bubbles:true,data:''})));
  await toggle.click();assert.equal(await panel.locator('.mk-documentation-preview').count(),0);assert.equal(await canonical(),original);
  await toggle.click();await view();await wrap.click();await view();assert.equal(await panel.locator('.mk-documentation-preview').count(),2);
  assert.equal(await canonical(),original);await wrap.click();
 }
 assert.deepEqual(errors,[]);console.log('NATIVE DOC PREVIEW E2E: PASS');
}finally{await browser.close();}
