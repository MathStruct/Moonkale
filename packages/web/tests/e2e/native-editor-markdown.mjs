import {chromium} from 'playwright';
import assert from 'node:assert/strict';
const browser=await chromium.launch();
const page=await browser.newPage({viewport:{width:1200,height:1200}});
const errors=[];page.on('pageerror',error=>errors.push(error.message));
const panel=page.locator('.language');
const input=panel.locator('.mk-editor-core-input-sink');
const widgets=panel.locator('.mk-native-inline-widget');
const canonical=page.locator('.language-canonical');
const ready=()=>page.waitForFunction(()=>{
  const runs=[...document.querySelectorAll('.language .mk-proportional-run')];
  return runs.length>0&&runs.every(run=>run.dataset.ready==='true');
});
const preview=async()=>{await input.focus();await page.keyboard.press('Control+End');await ready();};
try {
  await page.goto(`http://127.0.0.1:${process.env.PORT??8099}/`,{waitUntil:'networkidle'});
  await page.click('#markdown-preview');await ready();
  const original=await canonical.textContent();
  assert.ok(original.includes('\r\n'),'Fixture retains CRLF');
  await preview();
  assert.deepEqual(await widgets.allTextContents(),['猫 & dog','bold 😀','code <safe>']);
  assert.equal(await page.locator('.primary .mk-native-inline-widget').count(),0,'Provider is Markdown-only');
  assert.equal(await panel.locator('safe').count(),0,'Code label stays text');
  assert.equal(await widgets.nth(0).evaluate(el=>getComputedStyle(el).fontStyle),'italic');
  assert.ok(Number(await widgets.nth(1).evaluate(el=>getComputedStyle(el).fontWeight))>=700);
  const normalized=original.replaceAll('\r\n','\n');
  for(const width of [340,550,950]) {
    await panel.evaluate((el,width)=>{el.style.width=`${width}px`;el.style.height='800px';},width);
    await page.waitForTimeout(400);await preview();
    for(const [token,label] of [['*猫 &amp; dog*','猫 & dog'],['**bold 😀**','bold 😀'],['`code <safe>`','code <safe>']]) {
      const start=Array.from(normalized.slice(0,normalized.indexOf(token))).length;
      for(const [fraction,expected] of [[.2,start],[.8,start+Array.from(token).length]]) {
        const widget=widgets.filter({hasText:label});const rect=await widget.boundingBox();
        await page.mouse.click(rect.x+rect.width*fraction,rect.y+rect.height/2);await ready();
        assert.equal(Number(await panel.locator('.mk-native-surface').getAttribute('data-cursor-offset')),expected);
        assert.equal(await widgets.count(),2,'Only active construct reveals');
        await page.keyboard.insertText('Q');
        const byte=Array.from(normalized).slice(0,expected).join('').length;
        const rawOffset=byte+(normalized.slice(0,byte).match(/\n/g)??[]).length;
        const edited=original.slice(0,rawOffset)+'Q'+original.slice(rawOffset);
        await page.waitForFunction(text=>document.querySelector('.language-canonical').textContent===text,edited);
        await page.keyboard.press('Control+z');
        await page.waitForFunction(text=>document.querySelector('.language-canonical').textContent===text,original);
        await preview();
      }
    }
  }
  await input.focus();await page.keyboard.press('Control+a');await ready();
  assert.equal(await widgets.count(),0,'Selection reveals all source');
  const copied=await input.evaluate(el=>{
    const data=new DataTransfer();
    const event=new ClipboardEvent('copy',{bubbles:true,cancelable:true,clipboardData:data});
    el.dispatchEvent(event);return data.getData('text/plain');
  });
  assert.equal(copied,normalized,'Copy preserves complete Markdown source with the engine’s LF clipboard convention');
  assert.equal(await canonical.textContent(),original,'Copy does not normalize canonical CRLF');
  await page.keyboard.insertText('replacement');
  await page.waitForFunction(()=>document.querySelector('.language-canonical').textContent==='replacement');
  await page.keyboard.press('Control+z');
  await page.waitForFunction(text=>document.querySelector('.language-canonical').textContent===text,original);
  await preview();
  const rect=await widgets.first().boundingBox();await page.mouse.click(rect.x+2,rect.y+rect.height/2);await ready();
  const before=Number(await panel.locator('.mk-native-surface').getAttribute('data-cursor-offset'));
  await input.evaluate(el=>{
    el.dispatchEvent(new CompositionEvent('compositionstart',{bubbles:true,data:''}));
    el.dispatchEvent(new CompositionEvent('compositionupdate',{bubbles:true,data:'你好'}));
  });
  await page.waitForFunction(()=>document.querySelector('.language .mk-native-surface').dataset.composing==='true');
  assert.equal(await widgets.count(),2);assert.equal(await canonical.textContent(),original);
  await input.evaluate(el=>{
    el.dispatchEvent(new CompositionEvent('compositionend',{bubbles:true,data:'你好'}));
    el.value='你好';el.dispatchEvent(new Event('input',{bubbles:true}));
  });
  const byte=Array.from(normalized).slice(0,before).join('').length;
  const rawOffset=byte+(normalized.slice(0,byte).match(/\n/g)??[]).length;
  await page.waitForFunction(text=>document.querySelector('.language-canonical').textContent===text,original.slice(0,rawOffset)+'你好'+original.slice(rawOffset));
  await page.keyboard.press('Control+z');
  await page.waitForFunction(text=>document.querySelector('.language-canonical').textContent===text,original);
  await page.click('#layout-delay');await preview();await page.click('#layout-uniform');
  await page.waitForTimeout(650);assert.equal(await widgets.count(),0,'Late measurements cannot restore disabled provider');
  assert.equal(await canonical.textContent(),original);
  // Exercise the production settings path with every layout fixture flag reset.
  const toggle=panel.locator('.mk-native-preview');
  assert.equal(await toggle.getAttribute('aria-pressed'),'false');
  await panel.locator('.mk-native-wrap').click();
  await page.waitForFunction(()=>document.querySelector('.language .mk-native-surface').dataset.wrap==='false');
  await toggle.click();await preview();
  assert.equal(await toggle.getAttribute('aria-pressed'),'true');
  assert.equal(await widgets.count(),3,'Settings enable preview without fixture flags');
  assert.equal(await panel.locator('.mk-native-wrap').isDisabled(),true);
  assert.equal(await page.locator('.primary .mk-native-preview').count(),0);
  assert.equal(await page.locator('.primary .mk-native-inline-widget').count(),0);
  await page.click('[data-language-fixture="nested.md"]');
  await page.waitForFunction(()=>!document.querySelector('.language'));
  await page.click('[data-language-fixture="nested.md"]');await preview();
  assert.equal(await toggle.getAttribute('aria-pressed'),'true','Preference survives remount');
  assert.equal(await widgets.count(),3);
  await toggle.click();
  await page.waitForFunction(()=>document.querySelector('.language .mk-native-preview').getAttribute('aria-pressed')==='false');
  assert.equal(await widgets.count(),0);
  assert.equal(await panel.locator('.mk-native-wrap').isDisabled(),false);
  await page.waitForFunction(()=>document.querySelector('.language .mk-native-surface').dataset.wrap==='false');
  assert.equal(await panel.locator('.mk-native-wrap').getAttribute('aria-pressed'),'false','Disabling preview restores the saved wrap setting');
  assert.equal(await canonical.textContent(),original);
  assert.deepEqual(errors,[]);
  console.log('PASS: Markdown styles, Unicode/entity source boundaries across wraps, canonical copy, CRLF editing/undo, composition and provider invalidation');
} finally {await browser.close();}
