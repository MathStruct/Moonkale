import {chromium} from 'playwright';
import assert from 'node:assert/strict';
const browser=await chromium.launch();
const page=await browser.newPage({viewport:{width:1200,height:1400}});
const errors=[];page.on('pageerror',error=>errors.push(error.message));
const input=page.locator('.primary .mk-editor-core-input-sink');
const ready=()=>page.waitForFunction(()=>{
  const runs=[...document.querySelectorAll('.primary .mk-proportional-run')];
  return runs.length&&runs.every(run=>run.dataset.ready==='true');
});
const offset=async()=>Number(await page.locator('.primary .mk-native-surface').getAttribute('data-cursor-offset'));
const preview=async()=>{await input.focus();await page.keyboard.press('Control+End');await ready();};
try {
  await page.goto(`http://127.0.0.1:${process.env.PORT??8099}/`,{waitUntil:'networkidle'});
  await page.click('#layout-consumed-runs');
  await page.locator('.primary').evaluate(el=>{el.style.width='340px';el.style.height='1000px';});
  await page.waitForTimeout(750);await ready();
  const original=await page.locator('.canonical').textContent();
  const start=Array.from(original.slice(0,original.indexOf('[[chip:'))).length;
  const end=start+Array.from(original.slice(original.indexOf('[[chip:'),original.indexOf(']]',original.indexOf('[[chip:'))+2)).length;
  await input.focus();await page.keyboard.press('Control+a');await ready();
  const sourceRows=await page.locator('.primary .mk-editor-core-row[data-line="0"]').count();
  assert.ok(sourceRows>5,'Long tokens span several complete engine runs');
  const sourceNext=(await page.locator('.primary .mk-editor-core-row[data-line="1"]').boundingBox()).y;
  await preview();
  const widget=page.locator('.primary .mk-native-inline-widget').filter({hasText:'Chip'});
  assert.equal(await widget.count(),1);
  const previewRows=page.locator('.primary .mk-editor-core-row[data-line="0"]');
  assert.ok(await previewRows.count()<sourceRows-3,'Consumed source runs leave no DOM rows');
  assert.equal((await page.locator('.primary .mk-proportional-text').allTextContents()).join(''),'Wi  Chip tail');
  const geometry=await page.locator('.primary').evaluate(root=>{
    const block=root.querySelector('.mk-native-block-widget').getBoundingClientRect();
    const rows=[...root.querySelectorAll('.mk-editor-core-row[data-line="0"]')].map(el=>el.getBoundingClientRect());
    const next=root.querySelector('.mk-editor-core-row[data-line="1"]').getBoundingClientRect();
    return {expected:block.bottom+rows.reduce((sum,row)=>sum+row.height,0),actual:next.top};
  });
  assert.ok(Math.abs(geometry.expected-geometry.actual)<1,'Collapsed rows leave no pixel gaps');
  assert.ok((await page.locator('.primary .mk-editor-core-row[data-line="1"]').boundingBox()).y<sourceNext-96);
  for(const [fraction,expected] of [[.2,start],[.8,end]]) {
    const rect=await widget.boundingBox();await page.mouse.click(rect.x+rect.width*fraction,rect.y+rect.height/2);
    assert.equal(await offset(),expected);await ready();
    const revealedRows=await page.locator('.primary .mk-editor-core-row[data-line="0"]').count();
    assert.ok(revealedRows<sourceRows && revealedRows>2,'Only the entered construct restores its source rows');
    assert.ok(!(await page.locator('.primary .mk-proportional-text').allTextContents()).join('').includes('[[hidden:'));
    await page.keyboard.insertText('Q');await page.keyboard.press('Control+z');
    await page.waitForFunction(text=>document.querySelector('.canonical').textContent===text,original);
    await preview();
  }
  await page.keyboard.press('ArrowUp');await page.keyboard.press('Home');await page.keyboard.press('ArrowUp');await ready();
  assert.ok(await offset()<Array.from(original.split('\n')[0]).length,'Up enters source across compacted runs');
  assert.equal(await widget.count(),0,'Keyboard entry at the widget boundary reveals it');
  assert.ok(!(await page.locator('.primary .mk-proportional-text').allTextContents()).join('').includes('[[hidden:'),'Keyboard entry keeps the unrelated range hidden');
  await preview();await page.click('#layout-delay');
  await input.focus();await page.keyboard.press('Control+a');await ready();
  assert.equal(await page.locator('.primary .mk-editor-core-row[data-line="0"]').count(),sourceRows);
  await page.keyboard.press('Control+a');await page.keyboard.insertText('R');await page.keyboard.press('Control+z');
  await page.waitForFunction(text=>document.querySelector('.canonical').textContent===text,original);
  assert.deepEqual(errors,[]);
  console.log('PASS: consumed rows have zero spacing, unique widgets, source-boundary hits, reveal/undo, navigation and stale height invalidation');
} finally {await browser.close();}
