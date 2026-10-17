import { clickNativeCommand } from "./native-toolbar.mjs";
// Native table preview edits and saves the original Markdown, including CRLF.
import {firefox} from 'playwright';import fs from 'node:fs';import assert from 'node:assert/strict';
const root=process.env.M1_ROOT;if(!root)throw Error('M1_ROOT must name a disposable fixture');
const source='---\r\ntitle: Tables\r\n---\r\n\r\n| **猫** | Center | Right |\r\n| :--- | :---: | ---: |\r\n| &amp; | `a\\|b` | [Web](https://example.org/table) |\r\n| wrapped words that remain editable | 中文 | [[#tables|Self]] |\r\n\r\n# Tables\r\n\r\nEnd\r\n';fs.writeFileSync(`${root}/TablePreview.md`,source);
const browser=await firefox.launch();const context=await browser.newContext({viewport:{width:1500,height:1100}});
await context.addInitScript(()=>{localStorage.setItem('moonkale.settings',JSON.stringify({extensions:{enabled:['dev.moonkale.editor-code-native']},editor:{implementation:'native',markdown_rich:false}}));window.openedLinks=[];window.open=(...args)=>{window.openedLinks.push(args);return null;};});
const page=await context.newPage();const errors=[];page.on('pageerror',e=>errors.push(e.message));
const panel=()=>page.locator('.mk-crust[data-editor="rust"]:visible'),input=()=>panel().locator('.mk-editor-core-input-sink');
const ready=()=>page.waitForFunction(()=>{const runs=[...document.querySelectorAll('.mk-crust[data-editor="rust"] .mk-proportional-run')].filter(e=>e.getBoundingClientRect().height>0);return runs.length&&runs.every(e=>e.dataset.ready==='true');});
const preview=async()=>{await input().focus();await page.keyboard.press('Control+End');await ready();await panel().locator('.mk-editor-core-viewport').evaluate(el=>el.scrollTop=0);await ready();};
const save=async()=>{await page.keyboard.press('Control+s');await page.waitForFunction(()=>![...document.querySelectorAll('.mk-crust')].filter(el=>el.getBoundingClientRect().height>0).some(el=>el.querySelector('.mk-editor-dirty')));};
try{
 await page.goto(`http://127.0.0.1:${process.env.PORT??8094}/`,{waitUntil:'networkidle'});await page.waitForSelector('.wb-workspace');await page.click('.mk-explorer-open button[type=submit]');await page.waitForFunction(()=>document.querySelector('.wb-status-bar')?.textContent.includes('index:'));
 await page.locator('.mk-tree-file').filter({hasText:'TablePreview.md'}).click();await input().waitFor();await clickNativeCommand(panel().locator('.mk-native-preview'));await preview();
 assert.equal(await panel().locator('[role=table]').count(),1);assert.deepEqual(await panel().locator('[role=columnheader]').evaluateAll(es=>es.map(e=>getComputedStyle(e).textAlign)),['left','center','right']);
 const web=panel().locator('.mk-preview-link').filter({hasText:'Web'});let box=await web.boundingBox();await page.keyboard.down('Control');await page.mouse.click(box.x+5,box.y+box.height/2);await page.keyboard.up('Control');await page.waitForFunction(()=>window.openedLinks.length===1);assert.equal(await page.evaluate(()=>window.openedLinks[0][0]),'https://example.org/table');
 box=await panel().locator('[role=table]').boundingBox();await page.mouse.click(box.x+10,box.y+box.height/2);await ready();
 const offset=Array.from(source.replaceAll('\r\n','\n').split('| **猫**')[0]).length;assert.equal(Number(await panel().locator('.mk-native-surface').getAttribute('data-cursor-offset')),offset);assert.equal(await panel().locator('[role=table]').count(),0);
 await page.keyboard.insertText('猫');await panel().locator('.mk-editor-dirty').waitFor();await save();assert.equal(fs.readFileSync(`${root}/TablePreview.md`,'utf8'),source.replace('| **猫**','猫| **猫**'));
 await page.keyboard.press('Control+z');await save();assert.equal(fs.readFileSync(`${root}/TablePreview.md`,'utf8'),source);await page.keyboard.press('Control+Shift+z');await save();assert.equal(fs.readFileSync(`${root}/TablePreview.md`,'utf8'),source.replace('| **猫**','猫| **猫**'));await page.keyboard.press('Control+z');await save();
 await preview();await input().focus();await page.keyboard.press('Control+a');await ready();assert.equal(await panel().locator('[role=table]').count(),0);
 const copied=await input().evaluate(el=>{const event=new ClipboardEvent('copy',{bubbles:true,cancelable:true,clipboardData:new DataTransfer()});el.dispatchEvent(event);return event.clipboardData.getData('text/plain');});assert.equal(copied,source.replaceAll('\r\n','\n'));assert.equal(fs.readFileSync(`${root}/TablePreview.md`,'utf8'),source);assert.deepEqual(errors,[]);console.log('LIVE PREVIEW TABLES E2E: PASS');
}finally{await browser.close();}
