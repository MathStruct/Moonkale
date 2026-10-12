import {chromium} from 'playwright';
import assert from 'node:assert/strict';
const browser=await chromium.launch();
const page=await browser.newPage({viewport:{width:1200,height:1200}});
const errors=[]; page.on('pageerror',e=>errors.push(e.message));
const cursor=async()=>Number(await page.locator('.primary .mk-native-surface').getAttribute('data-cursor-offset'));
const ready=()=>page.waitForFunction(()=>{
  const runs=[...document.querySelectorAll('.primary .mk-proportional-run')];
  return runs.length>0&&runs.every(run=>run.dataset.ready==='true');
});
try {
  await page.goto(`http://127.0.0.1:${process.env.PORT??8099}/`,{waitUntil:'networkidle'});
  await page.click('#layout-unicode-document');
  const original=await page.locator('.canonical').textContent();
  const firstLine=original.split('\n')[0];
  let offset=0;
  const boundaries=new Set([0]);
  for(const {segment} of new Intl.Segmenter('en',{granularity:'grapheme'}).segment(firstLine)) {
    offset+=Array.from(segment).length; boundaries.add(offset);
  }
  await page.click('#layout-proportional'); await ready();
  const measure=()=>page.locator('.primary').evaluate(root=>{
    const result=[];
    for(const run of root.querySelectorAll('.mk-proportional-run')) {
      const source=run.querySelector('.mk-proportional-text'), spans=[...source.children];
      let scalar=0;
      for(const {segment} of new Intl.Segmenter('en',{granularity:'grapheme'}).segment(source.textContent)) {
        const length=Array.from(segment).length;
        const range=document.createRange();
        range.setStart(spans[scalar].firstChild,0);
        range.setEnd(spans[scalar+length-1].firstChild,spans[scalar+length-1].textContent.length);
        const rect=range.getBoundingClientRect(), origin=source.getBoundingClientRect();
        result.push({start:Number(run.dataset.sourceStart)+scalar,end:Number(run.dataset.sourceStart)+scalar+length,
          text:segment,x:rect.x,y:rect.y,width:rect.width,height:rect.height,
          row:`${run.dataset.sourceStart}:${Math.floor((rect.y-origin.y)/32)}`});
        scalar+=length;
      }
    }
    return result;
  });
  const glyphs=await measure();
  for(const glyph of glyphs) {
    assert.ok(boundaries.has(glyph.start)&&boundaries.has(glyph.end),`Engine run splits grapheme ${JSON.stringify(glyph)}`);
  }
  for(const text of ['é','👩🏽‍💻','🇩🇪','中','\t','न','स्ते']) {
    const glyph=glyphs.find(g=>g.text===text);
    if(!glyph) { assert.ok(!['é','👩🏽‍💻','🇩🇪','中','\t'].includes(text),`Missing ${text}`); continue; }
    for(const [fraction,expected] of [[.2,glyph.start],[.8,glyph.end]]) {
      await page.mouse.click(glyph.x+glyph.width*fraction,glyph.y+glyph.height/2);
      assert.equal(await cursor(),expected,`${text} pointer hit snaps to a whole grapheme`);
    }
  }
  const emoji=glyphs.find(g=>g.text==='👩🏽‍💻');
  await page.mouse.click(emoji.x+emoji.width*.2,emoji.y+emoji.height/2);
  await page.keyboard.press('ArrowRight');
  if(await cursor()===emoji.start) await page.keyboard.press('ArrowRight');
  assert.equal(await cursor(),emoji.end,'Right traverses the whole ZWJ/skin-tone sequence');
  await page.keyboard.press('ArrowLeft');
  if(await cursor()===emoji.end) await page.keyboard.press('ArrowLeft');
  assert.equal(await cursor(),emoji.start,'Left traverses the whole ZWJ/skin-tone sequence');
  await page.keyboard.press('Shift+ArrowRight');
  if(await cursor()===emoji.start) await page.keyboard.press('Shift+ArrowRight');
  assert.equal(await cursor(),emoji.end,'Keyboard selection covers a whole grapheme');
  await page.keyboard.insertText('Z');
  const expected=Array.from(original).slice(0,emoji.start).join('')+'Z'+Array.from(original).slice(emoji.end).join('');
  await page.waitForFunction(text=>document.querySelector('.canonical').textContent===text,expected);
  await page.keyboard.press('Control+z');
  await page.waitForFunction(text=>document.querySelector('.canonical').textContent===text,original);
  await ready();
  await page.locator('.primary .mk-editor-core-input-sink').focus();
  await page.keyboard.press('Control+Home');
  const rows=[...new Set(glyphs.map(g=>g.row))];
  for(let row=0;row<rows.length;row++) {
    const rowGlyphs=glyphs.filter(g=>g.row===rows[row]);
    const current=await cursor();
    assert.ok(rowGlyphs.some(g=>g.start===current||g.end===current),'Down enters the next CSS row');
    await page.keyboard.press('Home'); assert.equal(await cursor(),rowGlyphs[0].start,'Home uses CSS row start across fallback fonts');
    await page.keyboard.press('End'); assert.equal(await cursor(),rowGlyphs.at(-1).end,'End uses CSS row end across fallback fonts');
    await page.keyboard.press('ArrowDown');
  }
  const lineLength=Array.from(firstLine).length;
  assert.equal(await cursor(),lineLength+2,'Down clamps the preferred column on a one-character row');
  await page.keyboard.press('ArrowDown');
  assert.equal(await cursor(),lineLength+3,'Down enters an empty row');
  await page.keyboard.press('ArrowDown');
  const uniform=await cursor();
  const fullBoundaries=new Set([0]); let scalar=0;
  for(const {segment} of new Intl.Segmenter('en',{granularity:'grapheme'}).segment(original)) {
    scalar+=Array.from(segment).length; fullBoundaries.add(scalar);
  }
  assert.ok(uniform>lineLength+3&&fullBoundaries.has(uniform),'Preferred-column movement on uniform tabs/CJK/combining/emoji stays at a grapheme boundary');
  await page.keyboard.press('ArrowUp');assert.equal(await cursor(),lineLength+3);
  await page.keyboard.press('ArrowUp');assert.equal(await cursor(),lineLength+2);
  await page.keyboard.press('ArrowUp');assert.ok(boundaries.has(await cursor()),'Return to measured text preserves a grapheme boundary');
  for(const width of [340,670,950]) {
    await page.locator('.primary').evaluate((panel,width)=>panel.style.width=`${width}px`,width);
    await page.waitForTimeout(150);
    await page.locator('.primary .mk-editor-core-input-sink').focus();
    await page.keyboard.press('Control+Home'); await ready();
    const reflowed=await measure();
    for(const glyph of reflowed) assert.ok(boundaries.has(glyph.start)&&boundaries.has(glyph.end),`Resize ${width} preserves grapheme source runs`);
    const firstRow=reflowed.filter(g=>g.row===reflowed[0].row);
    await page.keyboard.press('End'); assert.equal(await cursor(),firstRow.at(-1).end,`Visual End after resize ${width}`);
    await page.keyboard.press('Home'); assert.equal(await cursor(),0,`Visual Home after resize ${width}`);
  }
  assert.equal(await page.locator('.canonical').textContent(),original);
  assert.deepEqual(errors,[]);
  console.log('PASS: complex Unicode/tab pointer and keyboard targets, selection/undo, CSS edges, short/empty/uniform rows and narrow/wide reflow');
} finally {await browser.close();}
