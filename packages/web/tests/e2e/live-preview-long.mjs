// Full-app acceptance: preview throughout a 200 KB vault note and a Source baseline.
import { firefox } from 'playwright';
import fs from 'node:fs';
import assert from 'node:assert/strict';
const root = process.env.M1_ROOT;
const shots = process.env.M1_SHOTS ?? '.';
if (!root) throw Error('M1_ROOT must name a disposable fixture');
const note = '---\r\ntitle: Long vault note\r\n---\r\n\r\n' + Array.from({length:240}, (_,n) =>
  `# Section ${n} 猫\r\n\r\n${'plain words '.repeat(71)} [**bold ${n}**](https://example.org/${n}) and \`code ${n}\`.\r\n\r\n` +
  `- list item ${n} **bold** [[#section-${n}-猫|jump]]\r\n  - [ ] pending ${n}\r\n- [x] completed ${n} 猫\r\n\r\n` +
  ([60,180,239].includes(n) ? `| Column ${n} | Value |\r\n| :--- | ---: |\r\n| **猫** | cell ${n} |\r\n\r\n` : '') +
  ([20,60,180,239].includes(n) ? `![Image ${n}](PreviewImage.svg)\r\n\r\n` : '') +
  (n % 20 === 0 ? `\`\`\`rust\r\nfn section_${n}() {}\r\n\`\`\`\r\n\r\n` : '')).join('') + 'End.\r\n';
// Keep the byte budget near the preceding 246 KB list fixture while adding 480 links.
assert.ok(Buffer.byteLength(note)>240_000);
fs.writeFileSync(`${root}/PreviewImage.svg`,'<svg xmlns="http://www.w3.org/2000/svg" width="240" height="80"><rect width="240" height="80" fill="blue"/></svg>');
fs.writeFileSync(`${root}/LongPreview.md`,note);
fs.writeFileSync(`${root}/PreviewPerf.txt`,'x'.repeat(3_000_000));
const browser = await firefox.launch();
const context = await browser.newContext({viewport:{width:1500,height:1000}});
await context.addInitScript(() => localStorage.setItem('moonkale.settings',JSON.stringify({extensions:{enabled:['dev.moonkale.editor-code-native']},editor:{implementation:'native',markdown_rich:false}})));
const page = await context.newPage();
const errors=[];page.on('pageerror',error=>errors.push(error.message));
const panel=()=>page.locator('.mk-crust[data-editor="rust"]:visible');
const input=()=>panel().locator('.mk-editor-core-input-sink');
const surface=()=>panel().locator('.mk-native-surface');
const widgets=()=>panel().locator('.mk-native-inline-widget');
const ready=async()=>{
  await page.waitForFunction(()=>{
    const panel=[...document.querySelectorAll('.mk-crust[data-editor="rust"]')].find(el=>el.getBoundingClientRect().height>0 && el.getBoundingClientRect().width>0 && getComputedStyle(el).visibility!=='hidden');
    const runs=panel?.querySelectorAll('.mk-proportional-run');
    return runs?.length>0 && [...runs].every(run=>run.dataset.ready==='true');
  },null,{timeout:30000});
};
const open=async name=>{
  await page.locator('.mk-tree-file').filter({hasText:name}).click();
  await page.waitForFunction(name=>[...document.querySelectorAll('.mk-crust[data-editor="rust"]')].some(el=>el.getBoundingClientRect().height>0 && el.getBoundingClientRect().width>0 && getComputedStyle(el).visibility!=='hidden' && el.querySelector('.mk-editor-path')?.textContent.endsWith(name)),name);
  await input().waitFor();
};
const bench=async()=>{
  await input().focus();
  if(await panel().locator('.mk-proportional-run').count())await ready();
  await page.waitForLoadState('networkidle');
  // Let the preceding save/index and font work settle before timing input.
  await page.waitForTimeout(1000);
  const times=[];
  for(let i=0;i<10;i++){
    const before=Number(await surface().getAttribute('data-cursor-offset'));
    await page.evaluate(()=>window.previewBenchStart=performance.now());
    await page.keyboard.insertText('x');
    await page.waitForFunction(offset=>[...document.querySelectorAll('.mk-crust[data-editor="rust"]')].find(el=>el.getBoundingClientRect().height>0 && el.getBoundingClientRect().width>0 && getComputedStyle(el).visibility!=='hidden')?.querySelector('.mk-native-surface')?.dataset.cursorOffset===String(offset),before+1);
    const time=await page.evaluate(()=>performance.now()-window.previewBenchStart);
    if(i>=3)times.push(time);
  }
  times.sort((a,b)=>a-b);
  return {median:times[3],p95:times.at(-1),samples:times};
};
try {
  await page.goto(`http://127.0.0.1:${process.env.PORT??8094}/`,{waitUntil:'networkidle'});
  await page.waitForSelector('.wb-workspace');
  await page.click('.mk-explorer-open button[type=submit]');
  await page.waitForFunction(()=>document.querySelector('.wb-status-bar')?.textContent.includes('index:'));
  await open('PreviewPerf.txt');await input().focus();await page.keyboard.press('Control+Home');
  const source=await bench();
  console.log('3 MB Source typing:',JSON.stringify(source));
  await open('LongPreview.md');
  await panel().locator('.mk-native-preview').click();await ready();
  const viewport=panel().locator('.mk-editor-core-viewport');
  for(const fraction of [0,.25,.5,.75,1]){
    const before=await surface().getAttribute('data-first-row');
    await viewport.evaluate((el,fraction)=>el.scrollTop=(el.scrollHeight-el.clientHeight)*fraction,fraction);
    if(fraction>0)await page.waitForFunction(before=>[...document.querySelectorAll('.mk-crust[data-editor="rust"]')].find(el=>el.getBoundingClientRect().height>0 && el.getBoundingClientRect().width>0 && getComputedStyle(el).visibility!=='hidden')?.querySelector('.mk-native-surface')?.dataset.firstRow!==before,before);
    await ready();
    await page.waitForTimeout(150);
    const visible=await widgets().allTextContents();
    assert.ok(await panel().locator('.mk-proportional-run').count()>0,`Preview rows missing at ${fraction}`);
    assert.ok(await panel().locator('.mk-editor-core-row').count()<150,'Virtual rows must stay bounded');
    const starts=await panel().locator('.mk-proportional-run').evaluateAll(runs=>runs.map(run=>Number(run.dataset.sourceStart)));
    if(fraction>0)assert.ok(Math.max(...starts)>4096,'Preview must extend beyond the old prefix');
    console.log(`scroll ${fraction}: first row ${await surface().getAttribute('data-first-row')}; widgets ${visible.length}`);
  }
  for(const section of [60,180,239]) {
    await input().focus();await page.keyboard.press('Control+f');
    await panel().locator('.mk-search-query').fill(`list item ${section}`);
    await panel().getByRole('button',{name:'Next',exact:true}).click();
    await panel().locator('.mk-search-query').press('Escape');
    await input().focus();await page.keyboard.press('ArrowRight');await page.keyboard.press('End');await page.keyboard.press('ArrowRight');
    if(section===239) {
      await page.keyboard.press('Control+End');
      await viewport.evaluate(el=>el.scrollTop=el.scrollHeight);
    }
    await ready();
    await panel().locator('.mk-markdown-list-item').filter({hasText:`list item ${section} bold jump`}).waitFor();
    await panel().locator('.mk-markdown-list-item').filter({hasText:`☑ completed ${section} 猫`}).waitFor();
    await panel().locator('.mk-preview-link').filter({hasText:'jump'}).first().waitFor();
    if(section===239) {
      await panel().locator('.mk-preview-image[alt="Image 239"]').waitFor();
      await page.waitForFunction(()=>document.querySelector('img.mk-preview-image[alt="Image 239"]')?.naturalWidth===240);
    }
    await panel().locator('[role=table]').filter({hasText:`Column ${section}`}).waitFor();
    console.log(`deep list ${section}: preview visible`);
  }
  // Navigate to specific headings so a viewport inside a long plain paragraph
  // is not mistaken for a missing decoration.
  for(const section of [60,120,180,239]){
    await input().focus();await page.keyboard.press('Control+f');
    await panel().locator('.mk-search-query').fill(`Section ${section} 猫`);
    await panel().getByRole('button',{name:'Next',exact:true}).click();
    await panel().locator('.mk-search-query').press('Escape');
    await input().focus();await page.keyboard.press('ArrowRight');await page.keyboard.press('End');await page.keyboard.press('ArrowRight');
    await viewport.evaluate(el=>el.scrollTop=Math.max(0,el.scrollTop-el.clientHeight/2));
    await ready();
    await widgets().filter({hasText:`Section ${section} 猫`}).waitFor();
    console.log(`deep heading ${section}: preview visible`);
  }
  // Edit the final heading through keyboard navigation; its source ownership
  // must remain exact even after hundreds of earlier constructs.
  const heading=widgets().filter({hasText:'Section 239 猫'});
  await heading.waitFor();
  const offset=Array.from(note.replaceAll('\r\n','\n').split('# Section 239 猫')[0]).length;
  await input().focus();await page.keyboard.press('Control+f');
  await panel().locator('.mk-search-query').fill('Section 239 猫');
  await panel().getByRole('button',{name:'Next',exact:true}).click();
  await panel().locator('.mk-search-query').press('Escape');
  await input().focus();await page.keyboard.press('Home');
  console.log('deep edit caret:',offset,await surface().getAttribute('data-cursor-offset'));
  await page.waitForFunction(offset=>[...document.querySelectorAll('.mk-crust[data-editor="rust"]')].find(el=>el.getBoundingClientRect().height>0 && el.getBoundingClientRect().width>0 && getComputedStyle(el).visibility!=='hidden')?.querySelector('.mk-native-surface')?.dataset.cursorOffset===String(offset),offset);
  const clicked=offset;
  await page.keyboard.insertText('猫');
  await panel().locator('.mk-editor-dirty').waitFor();
  const normalized=note.replaceAll('\r\n','\n');
  const prefix=Array.from(normalized).slice(0,clicked).join('');
  const raw=prefix.length+(prefix.match(/\n/g)??[]).length;
  const edited=note.slice(0,raw)+'猫'+note.slice(raw);
  await page.keyboard.press('Control+s');
  await page.waitForFunction(()=>![...document.querySelectorAll('.mk-crust')].filter(el=>el.getBoundingClientRect().height>0 && el.getBoundingClientRect().width>0 && getComputedStyle(el).visibility!=='hidden').some(el=>el.querySelector('.mk-editor-dirty')));
  assert.equal(fs.readFileSync(`${root}/LongPreview.md`,'utf8'),edited);
  await page.keyboard.press('Control+z');await page.keyboard.press('Control+s');
  await page.waitForFunction(()=>![...document.querySelectorAll('.mk-crust')].filter(el=>el.getBoundingClientRect().height>0 && el.getBoundingClientRect().width>0 && getComputedStyle(el).visibility!=='hidden').some(el=>el.querySelector('.mk-editor-dirty')));
  assert.equal(fs.readFileSync(`${root}/LongPreview.md`,'utf8'),note);
  await input().focus();await page.keyboard.press('Control+a');await ready();
  assert.equal(await widgets().count(),0,'Selection reveals source after hundreds of constructs');
  console.log('selection:',await panel().getAttribute('data-selection-anchor'),await panel().getAttribute('data-selection-head'));
  const copied=await input().evaluate(el=>{const data=new DataTransfer();const event=new ClipboardEvent('copy',{bubbles:true,cancelable:true,clipboardData:data});el.dispatchEvent(event);return {text:event.clipboardData.getData('text/plain'),original:data.getData('text/plain'),prevented:event.defaultPrevented};});
  console.log('clipboard:',copied.text.length,copied.original.length,copied.prevented);
  assert.equal(copied.text,normalized);
  await page.keyboard.press('Control+End');await ready();
  const preview=await bench();
  console.log('200 KB Preview typing:',JSON.stringify(preview));
  fs.writeFileSync(`${shots}/live-preview-long-latency.json`,JSON.stringify({bytes:Buffer.byteLength(note),source,preview},null,2));
  assert.ok(preview.median<=Math.max(source.median*1.25,source.median+25),`Preview median ${preview.median} ms exceeds Source ${source.median} ms`);
  assert.ok(preview.p95<=Math.max(source.p95*1.5,source.p95+50),`Preview p95 ${preview.p95} ms exceeds Source ${source.p95} ms`);
  assert.equal(fs.readFileSync(`${root}/LongPreview.md`,'utf8'),note,'Benchmark edits remain unsaved');
  assert.deepEqual(errors,[]);
  console.log('LIVE PREVIEW LONG E2E: PASS');
}catch(error){console.log('PANELS:',JSON.stringify(await page.locator('.mk-crust').evaluateAll(elements=>elements.map(el=>({path:el.querySelector('.mk-editor-path')?.textContent,cursor:el.querySelector('.mk-native-surface')?.dataset.cursorOffset,visible:getComputedStyle(el).visibility,rect:el.getBoundingClientRect().height})))));console.log('LAYOUT:',JSON.stringify(await panel().locator('.mk-editor-core-row').evaluateAll(rows=>rows.map(row=>{const r=row.getBoundingClientRect(), text=row.querySelector('.mk-proportional-text'), t=text?.getBoundingClientRect();return {line:row.dataset.line,top:r.top,height:r.height,textHeight:t?.height,textTop:t?.top,label:row.textContent.slice(0,50)};}))));console.log('FAIL:',error.stack);await page.screenshot({path:`${shots}/live-preview-long-fail.png`}).catch(()=>{});process.exitCode=1;}
finally{await browser.close();}
