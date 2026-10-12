import {chromium} from 'playwright';
import assert from 'node:assert/strict';
const browser=await chromium.launch();
const page=await browser.newPage({viewport:{width:1200,height:1000}});
const errors=[];page.on('pageerror',e=>errors.push(e.message));
try {
  await page.goto(`http://127.0.0.1:${process.env.PORT??8099}/`,{waitUntil:'networkidle'});
  await page.click('#layout-navigation-document');
  const original=await page.locator('.canonical').textContent();
  await page.click('#layout-proportional');
  await page.waitForFunction(()=>{const runs=[...document.querySelectorAll('.primary .mk-proportional-run')];return runs.length>1&&runs.every(run=>run.dataset.ready==='true');});
  const points=await page.locator('.primary').evaluate(root=>{
    const values=[];
    for(const run of root.querySelectorAll('.mk-proportional-run')) {
      const spans=[...run.querySelectorAll('.mk-proportional-text > span')];
      let scalar=0;
      for(const {segment} of new Intl.Segmenter('en',{granularity:'grapheme'}).segment(spans.map(s=>s.textContent).join(''))) {
        const length=Array.from(segment).length;
        const range=document.createRange();range.setStart(spans[scalar].firstChild,0);range.setEnd(spans[scalar+length-1].firstChild,spans[scalar+length-1].textContent.length);
        const r=range.getBoundingClientRect();
        values.push({start:Number(run.dataset.sourceStart)+scalar,end:Number(run.dataset.sourceStart)+scalar+length,x:r.x,right:r.right,y:r.y,height:r.height});scalar+=length;
      }
    }
    return values;
  });
  const cursor=async()=>Number(await page.locator('.primary .mk-native-surface').getAttribute('data-cursor-offset'));
  const input=page.locator('.primary .mk-editor-core-input-sink');
  await input.focus();await page.keyboard.press('Control+Home');
  const first=points[0], last=points.filter(p=>Math.abs(p.y-first.y)<.5).at(-1);
  await page.keyboard.press('End');assert.equal(await cursor(),last.end);
  const caret=page.locator('.primary .mk-proportional-run .mk-editor-core-caret');
  assert.equal(await caret.count(),1,'End displays one caret at the preceding row edge');
  const before=await caret.boundingBox();
  assert.ok(Math.abs(before.x-last.right)<2 && Math.abs(before.y-last.y)<2,'End keeps backward wrap affinity');
  await page.keyboard.press('ArrowRight');assert.equal(await cursor(),last.end,'Right crosses affinity without skipping a character');
  const after=await caret.boundingBox();assert.ok(after.y>before.y+10);
  await page.keyboard.press('ArrowLeft');assert.equal(await cursor(),last.end,'Left returns to the previous display edge');
  assert.ok(Math.abs((await caret.boundingBox()).y-before.y)<2);
  await page.keyboard.press('Home');assert.equal(await cursor(),first.start);
  await page.keyboard.press('Shift+End');assert.equal(await cursor(),last.end);
  await page.keyboard.insertText('Z');
  const expected='Z'+Array.from(original).slice(last.end).join('');
  await page.waitForFunction(expected=>document.querySelector('.canonical').textContent===expected,expected);
  await page.keyboard.press('Control+z');await page.waitForFunction(expected=>document.querySelector('.canonical').textContent===expected,original);
  await page.waitForFunction(()=>[...document.querySelectorAll('.primary .mk-proportional-run')].every(run=>run.dataset.ready==='true'));
  const tail=points.at(-1);
  await page.mouse.click(tail.x+(tail.right-tail.x)*.2,tail.y+tail.height/2);
  // Enter/leave the measured line at the retained pixel column. Ordinary rows
  // expose their actual monospaced source cells to the same nearest-boundary rule.
  await page.keyboard.press('ArrowDown');
  const ordinary=await cursor();assert.ok(ordinary>Array.from(original.split('\n')[0]).length);
  await page.keyboard.press('ArrowUp');assert.equal(await cursor(),tail.start,'Mixed-layout round trip retains the pixel column');
  await page.keyboard.press('ArrowDown');
  await page.locator('.primary [data-line="1"] .mk-editor-core-cell').first().click({position:{x:1,y:5}});
  await page.keyboard.press('ArrowUp');
  assert.equal(await cursor(),points.filter(p=>Math.abs(p.y-tail.y)<.5)[0].start,'Ordinary-to-measured entry uses the ordinary pixel column');
  assert.equal(await page.locator('.canonical').textContent(),original);
  assert.deepEqual(errors,[]);
  console.log('PASS: visual Home/End, wrap affinity, bidirectional boundary arrows, Shift replacement/undo and mixed-layout pixel column');
} finally {await browser.close();}
