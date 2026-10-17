import { clickNativeCommand } from "./native-toolbar.mjs";
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
const preview=async()=>{await input.focus();await page.keyboard.press('Control+End');await ready();await panel.locator('.mk-editor-core-viewport').evaluate(el=>el.scrollTop=0);await ready();};
try {
  await page.goto(`http://127.0.0.1:${process.env.PORT??8099}/`,{waitUntil:'networkidle'});
  await page.click('#markdown-headings');
  await clickNativeCommand(panel.locator('.mk-native-preview'));await ready();
  const original=await canonical.textContent();
  assert.ok(original.includes('\r\n'),'Fixture retains CRLF');
  await panel.evaluate(el=>{el.style.height='1000px';});
  await page.waitForTimeout(200);
  await preview();
  assert.deepEqual(await widgets.allTextContents(),['Title 猫','Heading bold and code','Third','Fourth','Fifth','Sixth','Setext source','link']);
  assert.equal(await page.locator('.primary .mk-native-inline-widget').count(),0,'Provider is Markdown-only');
  assert.equal(await panel.locator('safe').count(),0,'Code label stays text');
  assert.equal(await widgets.nth(0).locator('.mk-markdown-fragment').evaluate(el=>getComputedStyle(el).fontStyle),'normal');
  assert.ok(Number(await widgets.nth(0).locator('.mk-markdown-fragment').evaluate(el=>getComputedStyle(el).fontWeight))>=700);
  assert.deepEqual(await widgets.evaluateAll(elements=>elements.map(el=>Number(el.dataset.headingLevel))),[1,2,3,4,5,6,2,1]);
  const sizes=await widgets.evaluateAll(elements=>elements.map(el=>Number.parseFloat(getComputedStyle(el).fontSize)));
  assert.ok(sizes[0]>sizes[1]&&sizes[1]>sizes[2]&&sizes[2]>sizes[3]&&sizes[3]>sizes[4]);
  assert.equal(await widgets.nth(1).locator('[data-code="true"]').textContent(),'code');
  const normalized=original.replaceAll('\r\n','\n');
  for(const width of [340,550,950]) {
    await panel.evaluate((el,width)=>{el.style.width=`${width}px`;el.style.height='800px';},width);
    await page.waitForTimeout(400);await preview();
    const rows=await panel.locator('.mk-editor-core-row').evaluateAll(elements=>elements.map(el=>{const r=el.getBoundingClientRect();return {top:r.top,bottom:r.bottom};}));
    assert.ok(rows.every((row,index)=>!index||Math.abs(row.top-rows[index-1].bottom)<1),'Accepted heading heights keep source rows adjacent');
    assert.ok(await widgets.first().evaluate(el=>{const row=el.closest('.mk-editor-core-row').getBoundingClientRect();return row.height>=el.getBoundingClientRect().height;}),'Heading geometry is retained in its source row height');
    for(const [token,label] of [['# Title 猫','Title 猫'],['## Heading **bold** and `code` ##','Heading bold and code'],['### Third','Third'],['#### Fourth','Fourth'],['##### Fifth','Fifth'],['###### Sixth','Sixth']]) {
      const start=Array.from(normalized.slice(0,normalized.indexOf(token))).length;
      for(const [fraction,expected] of [[.2,start],[.8,start+Array.from(token).length]]) {
        const widget=widgets.filter({hasText:label});const rect=await widget.boundingBox();
        await page.mouse.click(rect.x+rect.width*fraction,rect.y+rect.height/2);await ready();
        assert.equal(Number(await panel.locator('.mk-native-surface').getAttribute('data-cursor-offset')),expected);
        assert.equal(await widgets.count(),7,'Only active construct reveals');
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
  assert.equal(await widgets.count(),7);assert.equal(await canonical.textContent(),original);
  await input.evaluate(el=>{
    el.dispatchEvent(new CompositionEvent('compositionend',{bubbles:true,data:'你好'}));
    el.value='你好';el.dispatchEvent(new Event('input',{bubbles:true}));
  });
  const byte=Array.from(normalized).slice(0,before).join('').length;
  const rawOffset=byte+(normalized.slice(0,byte).match(/\n/g)??[]).length;
  await page.waitForFunction(text=>document.querySelector('.language-canonical').textContent===text,original.slice(0,rawOffset)+'你好'+original.slice(rawOffset));
  await page.keyboard.press('Control+z');
  await page.waitForFunction(text=>document.querySelector('.language-canonical').textContent===text,original);
  await clickNativeCommand(panel.locator('.mk-native-preview'));
  await page.waitForFunction(()=>document.querySelector('.language .mk-native-preview').getAttribute('aria-pressed')==='false');
  assert.equal(await widgets.count(),0);
  assert.equal(await canonical.textContent(),original);
  assert.deepEqual(errors,[]);
  console.log('PASS: ATX heading levels/styles, source ownership and measured height across wraps, local reveal, copy, CRLF edit/undo and composition');
} finally {await browser.close();}
