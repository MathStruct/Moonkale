import { clickNativeCommand } from "./native-toolbar.mjs";
// Source-backed image blocks retain canonical CRLF text and open the asset viewer.
import {firefox} from 'playwright';
import fs from 'node:fs';
import assert from 'node:assert/strict';
const root=process.env.M1_ROOT;if(!root)throw Error('M1_ROOT must name a disposable fixture');
fs.mkdirSync(`${root}/pictures`,{recursive:true});
fs.writeFileSync(`${root}/pictures/Café.svg`,'<svg xmlns="http://www.w3.org/2000/svg" width="240" height="80"><rect width="240" height="80" fill="blue"/></svg>');
const source='---\r\ntitle: Images\r\n---\r\n\r\n![猫](pictures/Caf%C3%A9.svg "Title")\r\n\r\n![[Café.svg|Alias]]\r\n\r\n![Missing](absent.png)\r\n\r\nEnd\r\n';
fs.writeFileSync(`${root}/ImagePreview.md`,source);
const browser=await firefox.launch();const context=await browser.newContext({viewport:{width:1500,height:1100}});
await context.addInitScript(()=>localStorage.setItem('moonkale.settings',JSON.stringify({extensions:{enabled:['dev.moonkale.editor-code-native']},editor:{implementation:'native',markdown_rich:false}})));
const page=await context.newPage();const errors=[];page.on('pageerror',e=>errors.push(e.message));
const panel=()=>page.locator('.mk-crust[data-editor="rust"]:visible');
const input=()=>panel().locator('.mk-editor-core-input-sink');
const open=async()=>{await page.locator('.mk-tree-file').filter({hasText:'ImagePreview.md'}).click();await input().waitFor();};
const preview=async()=>{await input().focus();await page.keyboard.press('Control+End');await panel().locator('.mk-editor-core-viewport').evaluate(el=>el.scrollTop=0);await page.waitForFunction(()=>[...document.querySelectorAll('.mk-preview-image')].filter(i=>i.getBoundingClientRect().height>0&&i.naturalWidth===240).length===2);};
try{
 await page.goto(`http://127.0.0.1:${process.env.PORT??8094}/`,{waitUntil:'networkidle'});await page.waitForSelector('.wb-workspace');await page.click('.mk-explorer-open button[type=submit]');await page.waitForFunction(()=>document.querySelector('.wb-status-bar')?.textContent.includes('index:'));
 await open();await clickNativeCommand(panel().locator('.mk-native-preview'));await preview();
 assert.deepEqual(await panel().locator('.mk-preview-image').evaluateAll(images=>images.map(i=>i.alt)),['猫','Alias']);
 assert.ok(await panel().locator('.mk-preview-image').evaluateAll(images=>images.every(i=>i.src.startsWith('data:image/svg+xml;base64,'))));
 await panel().locator('.mk-image-placeholder').filter({hasText:/not found/i}).waitFor();
 const image=()=>panel().locator('.mk-preview-image').first();let box=await image().boundingBox();await page.mouse.click(box.x+10,box.y+10);
 assert.equal(Number(await panel().locator('.mk-native-surface').getAttribute('data-cursor-offset')),Array.from(source.replaceAll('\r\n','\n').split('![猫]')[0]).length);
 await page.keyboard.insertText('猫');await page.keyboard.press('Control+z');await preview();
 box=await image().boundingBox();await page.keyboard.down('Control');await page.mouse.click(box.x+10,box.y+10);await page.keyboard.up('Control');
 await page.locator('.mk-image:visible img').waitFor();assert.equal(await page.locator('.mk-image:visible img').evaluate(i=>i.naturalWidth),240);
 await open();await preview();await input().focus();await page.keyboard.press('Control+s');await page.waitForTimeout(300);
 assert.equal(fs.readFileSync(`${root}/ImagePreview.md`,'utf8'),source);assert.equal(fs.existsSync(`${root}/absent.png`),false);assert.deepEqual(errors,[]);console.log('LIVE PREVIEW IMAGES E2E: PASS');
}catch(error){fs.writeFileSync('/tmp/moonkale-images-dom.json',JSON.stringify(await page.locator('body').evaluate(el=>({text:el.innerText,images:[...el.querySelectorAll('.mk-image-block')].map(e=>({html:e.outerHTML,rect:e.getBoundingClientRect().toJSON()}))})),null,2));throw error;}finally{await browser.close();}
