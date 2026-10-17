import { clickNativeCommand } from "./native-toolbar.mjs";
import {firefox} from 'playwright';import fs from 'node:fs';import assert from 'node:assert/strict';
const root=process.env.M1_ROOT,shots=process.env.M1_SHOTS;if(!root||!shots)throw Error('M1_ROOT and M1_SHOTS must name disposable fixture directories');
const source=Array.from({length:240},(_,n)=>`/// # Section ${n} 猫\r\n///\r\n/// Documentation **bold** and [web](https://example.org/${n}).\r\n///\r\n/// - first item\r\n/// - second item\r\n///\r\n/// \`\`\`rust\r\n/// fn example_${n}() {}\r\n/// \`\`\`\r\nfn section_${n}() {\r\n    // ${'ordinary code comment '.repeat(40)}\r\n}\r\n\r\n`).join('')+'fn main() {}\r\n';
fs.writeFileSync(`${root}/LongDocPreview.rs`,source);assert.ok(Buffer.byteLength(source)>200_000);
const browser=await firefox.launch(),context=await browser.newContext({viewport:{width:1500,height:1100}});
await context.addInitScript(()=>localStorage.setItem('moonkale.settings',JSON.stringify({extensions:{enabled:['dev.moonkale.editor-code-native']},editor:{implementation:'native',wrap:false,doc_comment_preview:false}})));
const page=await context.newPage(),errors=[];page.on('pageerror',error=>errors.push(error.message));
const panel=()=>page.locator('.mk-crust[data-editor=rust]:visible'),input=()=>panel().locator('.mk-editor-core-input-sink'),surface=()=>panel().locator('.mk-native-surface');
const ready=()=>page.waitForFunction(()=>{const runs=[...document.querySelectorAll('.mk-crust .mk-proportional-run')].filter(run=>run.getBoundingClientRect().height>0);return runs.length&&runs.every(run=>run.dataset.ready==='true'&&run.dataset.fontLoading==='false');});
const bench=async()=>{
 await input().focus();await page.keyboard.press('Control+End');await page.waitForTimeout(1000);const samples=[];
 for(let n=0;n<10;n++){const offset=Number(await surface().getAttribute('data-cursor-offset'));await page.evaluate(()=>window.docBenchStart=performance.now());await page.keyboard.insertText('x');await page.waitForFunction(offset=>[...document.querySelectorAll('.mk-crust')].find(el=>el.getBoundingClientRect().height>0)?.querySelector('.mk-native-surface')?.dataset.cursorOffset===String(offset+1),offset);const elapsed=await page.evaluate(()=>performance.now()-window.docBenchStart);if(n>=3)samples.push(elapsed);}
 samples.sort((a,b)=>a-b);const result={median:samples[3],p95:samples.at(-1),samples};
 for(let n=0;n<10;n++)await page.keyboard.press('Control+z');
 return result;
};
try{
 await page.goto(`http://127.0.0.1:${process.env.PORT??8094}/`,{waitUntil:'networkidle'});await page.waitForSelector('.wb-workspace');await page.click('.mk-explorer-open button[type=submit]');await page.waitForFunction(()=>document.querySelector('.wb-status-bar')?.textContent.includes('index:'));await page.locator('.mk-tree-file').filter({hasText:'LongDocPreview.rs'}).click();await input().waitFor();
 const toggle=panel().locator('.mk-native-doc-preview');if(await toggle.getAttribute('aria-pressed')==='true')await clickNativeCommand(toggle);
 const off=await bench();console.log('Same-file Source typing:',JSON.stringify(off));await clickNativeCommand(toggle);
 const viewport=panel().locator('.mk-editor-core-viewport');
 for(const section of [0,60,120,180,239]){
  await input().focus();await page.keyboard.press('Control+f');await panel().locator('.mk-search-query').fill(`fn section_${section}()`);await panel().getByRole('button',{name:'Next',exact:true}).click();await panel().locator('.mk-search-query').press('Escape');await input().focus();await page.keyboard.press('ArrowRight');await viewport.evaluate(el=>el.scrollTop=Math.max(0,el.scrollTop-el.clientHeight/2));await ready();
  const doc=panel().locator('.mk-doc-body').filter({hasText:`Section ${section} 猫`});for(let step=0;step<20&&await doc.count()===0;step++){await viewport.evaluate((el,line)=>{const first=Number(el.querySelector(".mk-editor-core-row")?.dataset.line??0);el.scrollTop+=(first>line?-1:1)*el.clientHeight/4;},section*14);await ready();await page.waitForTimeout(100);}await doc.waitFor();
  if(section>0)assert.ok(await doc.evaluate(el=>Number(el.closest('.mk-proportional-run').dataset.sourceStart)+Number(el.parentElement.dataset.sourceStart))>4096);
  assert.ok(await panel().locator('.mk-editor-core-row').count()<150);assert.equal(await panel().locator('.mk-native-wrap').getAttribute('aria-pressed'),'false');
 }
 await input().focus();await page.keyboard.press('Control+a');await page.waitForFunction(()=>document.querySelectorAll('.mk-documentation-preview').length===0);
 const copied=await input().evaluate(el=>{const e=new ClipboardEvent('copy',{bubbles:true,cancelable:true,clipboardData:new DataTransfer()});el.dispatchEvent(e);return e.clipboardData.getData('text/plain');});assert.equal(copied,source.replaceAll('\r\n','\n'));
 const on=await bench();console.log('Same-file Doc preview typing:',JSON.stringify(on));
 fs.writeFileSync(`${shots}/doc-preview-long-latency.json`,JSON.stringify({bytes:Buffer.byteLength(source),off,on},null,2));
 assert.ok(on.median<=Math.max(off.median*1.25,off.median+25),`Doc preview median ${on.median} exceeds Source ${off.median}`);
 assert.ok(on.p95<=Math.max(off.p95*1.5,off.p95+50),`Doc preview p95 ${on.p95} exceeds Source ${off.p95}`);
 assert.equal(fs.readFileSync(`${root}/LongDocPreview.rs`,'utf8'),source);assert.deepEqual(errors,[]);console.log('DOC PREVIEW LONG E2E: PASS');
}catch(error){console.log("viewport",JSON.stringify(await panel().locator(".mk-editor-core-viewport").evaluate(el=>({top:el.scrollTop,height:el.scrollHeight,client:el.clientHeight,rows:[...el.querySelectorAll(".mk-editor-core-row")].map(row=>row.dataset.line),docs:[...el.querySelectorAll(".mk-doc-body")].map(doc=>doc.textContent.slice(0,35)),cursor:el.closest(".mk-native-surface")?.dataset.cursorOffset})).catch(()=>null)));await page.screenshot({path:`${shots}/doc-preview-long-fail.png`}).catch(()=>{});console.log(error.stack);process.exitCode=1;}finally{await browser.close();}
