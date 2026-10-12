import { chromium } from 'playwright';
import assert from 'node:assert/strict';
const browser=await chromium.launch();
const page=await browser.newPage({viewport:{width:1200,height:1000}});
const errors=[]; page.on('pageerror',error=>errors.push(error.message));
try {
  await page.goto(`http://127.0.0.1:${process.env.PORT??8099}/`,{waitUntil:'networkidle'});
  await page.click('#layout-navigation-document');
  const original=await page.locator('.canonical').textContent();
  await page.click('#layout-proportional');
  await page.waitForFunction(()=>{
    const runs=[...document.querySelectorAll('.primary .mk-proportional-run')];
    return runs.length>1 && runs.every(run=>run.dataset.ready==='true');
  });
  const positions=await page.locator('.primary').evaluate(root=>{
    const values=[];
    for(const run of root.querySelectorAll('.mk-proportional-run')) {
      const spans=[...run.querySelectorAll('.mk-proportional-text > span')];
      const text=spans.map(span=>span.textContent).join('');
      let scalar=0;
      for(const {segment} of new Intl.Segmenter('en',{granularity:'grapheme'}).segment(text)) {
        const range=document.createRange();
        range.setStart(spans[scalar].firstChild,0);
        const length=Array.from(segment).length;
        range.setEnd(spans[scalar+length-1].firstChild,spans[scalar+length-1].textContent.length);
        const rect=range.getBoundingClientRect();
        values.push({offset:Number(run.dataset.sourceStart)+scalar,x:rect.x,y:rect.y,width:rect.width,height:rect.height});
        scalar+=length;
      }
    }
    return values;
  });
  const start=positions.find(p=>p.offset===3);
  await page.mouse.click(start.x+start.width*.2,start.y+start.height/2);
  const cursor=async()=>Number(await page.locator('.primary .mk-native-surface').getAttribute('data-cursor-offset'));
  assert.equal(await cursor(),start.offset);
  const next=(origin,x,down)=>{
    const rows=positions.filter(p=>down?p.y>origin.y+.5:p.y<origin.y-.5);
    const y=rows.reduce((best,p)=>Math.abs(p.y-origin.y)<Math.abs(best-origin.y)?p.y:best,rows[0].y);
    return rows.filter(p=>Math.abs(p.y-y)<.5).sort((a,b)=>Math.abs(a.x-x)-Math.abs(b.x-x))[0];
  };
  let current=start;
  for(let i=0;i<3;i++) {
    current=next(current,start.x,true);
    await page.keyboard.press('ArrowDown');
    assert.equal(await cursor(),current.offset,'Down follows measured wraps and retains the pixel column');
  }
  for(let i=0;i<3;i++) {
    current=next(current,start.x,false);
    await page.keyboard.press('ArrowUp');
    assert.equal(await cursor(),current.offset,'Up returns along the retained pixel column');
  }
  assert.equal(current.offset,start.offset);
  const input=page.locator('.primary .mk-editor-core-input-sink');
  await input.evaluate(el=>{
    el.dispatchEvent(new CompositionEvent('compositionstart',{bubbles:true,data:''}));
    el.dispatchEvent(new CompositionEvent('compositionupdate',{bubbles:true,data:'你好'}));
  });
  await page.waitForFunction(()=>document.querySelector('.primary .mk-native-surface').dataset.composing==='true');
  await page.keyboard.press('ArrowDown');
  assert.equal(await cursor(),start.offset,'Composition keeps the caret stable');
  assert.equal(await page.locator('.canonical').textContent(),original);
  await input.evaluate(el=>{
    el.dispatchEvent(new CompositionEvent('compositionend',{bubbles:true,data:'你好'}));
    el.value='你好'; el.dispatchEvent(new Event('input',{bubbles:true}));
  });
  const composed=Array.from(original).slice(0,start.offset).join('')+'你好'+Array.from(original).slice(start.offset).join('');
  await page.waitForFunction(expected=>document.querySelector('.canonical').textContent===expected,composed);
  await page.waitForTimeout(100);
  assert.equal(await page.locator('.canonical').textContent(),composed,'Repeated post-composition input commits once');
  await page.keyboard.press('Control+z');
  await page.waitForFunction(expected=>document.querySelector('.canonical').textContent===expected,original);
  await input.evaluate(el=>{
    el.value='你好'; el.dispatchEvent(new Event('input',{bubbles:true}));
    el.dispatchEvent(new CompositionEvent('compositionend',{bubbles:true,data:'你好'}));
  });
  await page.waitForFunction(expected=>document.querySelector('.canonical').textContent===expected,composed);
  await page.waitForTimeout(100);
  assert.equal(await page.locator('.canonical').textContent(),composed,'Native input-before-compositionend commits once without start/update');
  await page.keyboard.press('Control+z');
  await page.waitForFunction(expected=>document.querySelector('.canonical').textContent===expected,original);
  await page.waitForFunction(()=>[...document.querySelectorAll('.primary .mk-proportional-run')].every(run=>run.dataset.ready==='true'));
  await page.mouse.click(start.x+start.width*.2,start.y+start.height/2);
  const target=next(start,start.x,true);
  await page.keyboard.press('Shift+ArrowDown');
  assert.equal(await cursor(),target.offset);
  assert.ok(await page.locator('.primary .mk-proportional-run .mk-editor-core-selected').count()>0);
  await page.keyboard.insertText('Z');
  const chars=Array.from(original);
  const expected=chars.slice(0,start.offset).join('')+'Z'+chars.slice(target.offset).join('');
  await page.waitForFunction(expected=>document.querySelector('.canonical').textContent===expected,expected);
  await page.keyboard.press('Control+z');
  await page.waitForFunction(expected=>document.querySelector('.canonical').textContent===expected,original);
  await page.keyboard.press('Control+End');
  await page.keyboard.press('ArrowUp');
  await page.keyboard.press('ArrowUp');
  await page.keyboard.press('End');
  await page.keyboard.press('ArrowDown');
  assert.ok(await cursor()>Array.from(original.split('\n')[0]).length,'Leaving the measured line uses normal engine movement');
  assert.deepEqual(errors,[]);
  console.log('PASS: measured wrap Up/Down, retained pixel column, grapheme boundaries, Shift selection, Workspace undo and synthetic composition');
} finally {await browser.close();}
