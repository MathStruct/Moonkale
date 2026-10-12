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
  await page.click('#markdown-fenced');
  await panel.locator('.mk-native-preview').click();await ready();
  const original=await canonical.textContent();
  assert.ok(original.includes('\r\n'),'Fixture retains CRLF');
  await panel.evaluate(el=>{el.style.height='1000px';});
  await page.waitForTimeout(200);
  await preview();
  assert.deepEqual(await widgets.allTextContents(),['fn main() {\n    let 猫 = \"<safe>\";\n    println!(\"{}\", 猫);\n}\n','*literal* <safe>\n\n']);
  assert.equal(await page.locator('.primary .mk-native-inline-widget').count(),0,'Provider is Markdown-only');
  assert.equal(await panel.locator('safe').count(),0,'Code label stays text');
  assert.deepEqual(await widgets.evaluateAll(elements=>elements.map(el=>el.dataset.codeLanguage)),['rust','unknown']);
  const spans=widgets.first().locator('.mk-markdown-code-fragment');
  assert.ok(await spans.evaluateAll(elements=>elements.some(el=>[...el.classList].some(name=>name.startsWith('a-')))),'Rust grammar supplies syntax classes');
  assert.ok(await spans.evaluateAll(elements=>new Set(elements.map(el=>getComputedStyle(el).color)).size)>=2,'Syntax highlighting uses the editor theme');
  assert.equal(await widgets.nth(1).locator('[class*="a-"]').count(),0,'Unknown languages remain plain code');
  assert.equal(await panel.locator('.mk-editor-core-row[data-line="7"]').count(),0,'Closing fence row is consumed');
  assert.equal(await panel.locator('.mk-editor-core-row[data-line="8"]').count(),1,'Blank after the code block stays visible');
  const normalized=original.replaceAll('\r\n','\n');
  for(const width of [340,550,950]) {
    await panel.evaluate((el,width)=>{el.style.width=`${width}px`;el.style.height='800px';},width);
    await page.waitForTimeout(400);await preview();
    const rows=await panel.locator('.mk-editor-core-row').evaluateAll(elements=>elements.map(el=>{const r=el.getBoundingClientRect();return {top:r.top,bottom:r.bottom};}));
    assert.ok(rows.every((row,index)=>!index||Math.abs(row.top-rows[index-1].bottom)<1),'Accepted code block heights keep source rows adjacent');
    assert.ok(await widgets.first().evaluate(el=>{const row=el.closest('.mk-editor-core-row').getBoundingClientRect();return row.height>=el.getBoundingClientRect().height;}),'Code block geometry is retained in its source row height');
    for(const [token,label] of [['```rust title=demo\nfn main() {\n    let 猫 = \"<safe>\";\n    println!(\"{}\", 猫);\n}\n```','fn main() {'],['~~~unknown\n*literal* <safe>\n\n~~~','*literal* <safe>']]) {
      const start=Array.from(normalized.slice(0,normalized.indexOf(token))).length;
      for(const [fraction,expected] of [[.2,start],[.8,start+Array.from(token).length]]) {
        const widget=widgets.filter({hasText:label});const rect=await widget.boundingBox();
        await page.mouse.click(rect.x+rect.width*fraction,rect.y+rect.height/2);await ready();
        assert.equal(Number(await panel.locator('.mk-native-surface').getAttribute('data-cursor-offset')),expected);
        assert.equal(await widgets.count(),1,'Only active construct reveals');
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
  await input.focus();await page.keyboard.press('Control+Home');
  const body=Array.from(normalized.slice(0,normalized.indexOf('let 猫'))).length;
  for(let index=0;index<body;index++)await page.keyboard.press('ArrowRight');
  await ready();
  assert.equal(Number(await panel.locator('.mk-native-surface').getAttribute('data-cursor-offset')),body);
  assert.equal(await widgets.count(),1,'Caret in code reveals the whole block');
  assert.equal(await panel.locator('.mk-editor-core-row[data-line="4"]').count(),1);
  await page.keyboard.insertText('Z');
  await page.waitForFunction(text=>document.querySelector('.language-canonical').textContent===text,original.replace('let 猫','Zlet 猫'));
  await page.keyboard.press('Control+z');
  await page.waitForFunction(text=>document.querySelector('.language-canonical').textContent===text,original);
  await page.keyboard.press('Shift+ArrowRight');await ready();
  assert.equal(await widgets.count(),1,'Body selection keeps the enclosing code block revealed');
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
  assert.equal(await widgets.count(),1);assert.equal(await canonical.textContent(),original);
  await input.evaluate(el=>{
    el.dispatchEvent(new CompositionEvent('compositionend',{bubbles:true,data:'你好'}));
    el.value='你好';el.dispatchEvent(new Event('input',{bubbles:true}));
  });
  const byte=Array.from(normalized).slice(0,before).join('').length;
  const rawOffset=byte+(normalized.slice(0,byte).match(/\n/g)??[]).length;
  await page.waitForFunction(text=>document.querySelector('.language-canonical').textContent===text,original.slice(0,rawOffset)+'你好'+original.slice(rawOffset));
  await page.keyboard.press('Control+z');
  await page.waitForFunction(text=>document.querySelector('.language-canonical').textContent===text,original);
  await preview();
  // Hit a partially clipped block using the same sparse source-row height index.
  await panel.evaluate(el=>{el.style.height='250px';});
  await page.waitForTimeout(250);
  const viewport=panel.locator('.mk-editor-core-viewport');
  await viewport.evaluate(el=>{el.scrollTop=90;});
  await page.waitForTimeout(250);await ready();
  const block=widgets.filter({hasText:'fn main() {'});
  const clipped=await block.boundingBox(),view=await viewport.boundingBox();
  assert.ok(clipped.y<view.y&&clipped.y+clipped.height>view.y,'Block is actually partially scrolled');
  await page.mouse.click(clipped.x+clipped.width*.2,view.y+4);await ready();
  const blockStart=Array.from(normalized.slice(0,normalized.indexOf('```rust'))).length;
  assert.equal(Number(await panel.locator('.mk-native-surface').getAttribute('data-cursor-offset')),blockStart);
  assert.equal(await panel.locator('.mk-markdown-fenced[data-code-language="rust"]').count(),0,'Clipped block reveals its full source');
  assert.equal(await canonical.textContent(),original);
  await page.keyboard.insertText('V');
  await page.waitForFunction(text=>document.querySelector('.language-canonical').textContent===text,original.replace('```rust','V```rust'));
  await page.keyboard.press('Control+z');
  await page.waitForFunction(text=>document.querySelector('.language-canonical').textContent===text,original);
  await panel.evaluate(el=>{el.style.height='1000px';});await page.waitForTimeout(250);await preview();
  assert.equal(await widgets.count(),2);
  await page.click('#layout-delay');
  await panel.locator('.mk-native-preview').click();
  await page.waitForFunction(()=>document.querySelector('.language .mk-native-preview').getAttribute('aria-pressed')==='false');
  await page.waitForTimeout(650);
  assert.equal(await widgets.count(),0,'Delayed code block geometry cannot restore disabled preview');
  assert.equal(await canonical.textContent(),original);
  assert.deepEqual(errors,[]);
  console.log('PASS: Fenced syntax/plain fallback, clipped scrolling, source ownership and measured height across wraps, local reveal, copy, CRLF edit/undo and composition');
} finally {await browser.close();}
