import { control } from './chrome-controls.mjs';
import { clickNativeCommand } from "./native-toolbar.mjs";
import {firefox} from 'playwright';import fs from 'node:fs';import assert from 'node:assert/strict';
const root=process.env.M1_ROOT;if(!root)throw Error('M1_ROOT must name a disposable fixture');
const sources={
 'DocPreview.rs':'/// # Rust docs 猫\r\n///\r\n/// **Bold** [Web](https://example.org/docs).\r\n///\r\n/// ```rust\r\n/// fn example() {}\r\n/// ```\r\nfn sample() {}\r\n\r\n/** Second documentation. */\r\nfn another() {}\r\n',
 'DocPreview.py':'"""# Python docs 猫\r\n\r\nModule **documentation**.\r\n"""\r\nvalue = 1\r\n\r\ndef sample():\r\n    """Function `documentation`."""\r\n    return 1\r\n',
 'DocPreview.jl':'"""# Julia docs 猫\r\n\r\nFunction **documentation**.\r\n"""\r\nfunction sample(x)\r\n    return x\r\nend\r\n\r\nraw"""Type documentation."""\r\nstruct Example\r\nend\r\n',
};for(const [name,text]of Object.entries(sources))fs.writeFileSync(`${root}/${name}`,text);
const browser=await firefox.launch(),context=await browser.newContext({viewport:{width:1500,height:1200}});
await context.addInitScript(()=>{localStorage.setItem('moonkale.settings',JSON.stringify({extensions:{enabled:['dev.moonkale.editor-code-native']},editor:{implementation:'native',wrap:false}}));window.openedLinks=[];window.open=(...args)=>{window.openedLinks.push(args);return null;};});
const page=await context.newPage(),errors=[];page.on('pageerror',error=>errors.push(error.message));
const panel=()=>page.locator('.mk-crust[data-editor=rust]:visible'),input=()=>panel().locator('.mk-editor-core-input-sink');
const ready=()=>page.waitForFunction(()=>{const runs=[...document.querySelectorAll('.mk-crust .mk-proportional-run')].filter(run=>run.getBoundingClientRect().height>0);return runs.length&&runs.every(run=>run.dataset.ready==='true'&&run.dataset.fontLoading==='false');});
const save=async()=>{await input().focus();await page.keyboard.press('Control+s');await page.waitForFunction(()=>![...document.querySelectorAll('.mk-crust')].filter(el=>el.getBoundingClientRect().height>0).some(el=>el.querySelector('.mk-editor-dirty')));};
try{
 await page.goto(`http://127.0.0.1:${process.env.PORT??8094}/`,{waitUntil:'networkidle'});await page.waitForSelector('.wb-workspace');await control(page, 'click', '.mk-explorer-open button[type=submit]');await page.waitForFunction(()=>document.querySelector('.wb-status-bar')?.textContent.includes('index:'));
 for(const [name,original]of Object.entries(sources)){
  await control(page.locator('.mk-tree-file').filter({hasText:name}), 'click');await input().waitFor();
  const toggle=panel().locator('.mk-native-doc-preview');if(await toggle.getAttribute('aria-pressed')==='false')await clickNativeCommand(toggle);
  await input().focus();await page.keyboard.press('Control+End');await panel().locator('.mk-editor-core-viewport').evaluate(el=>el.scrollTop=0);await ready();
  assert.equal(await panel().locator('.mk-native-wrap').getAttribute('aria-pressed'),'false');assert.equal(await panel().locator('.mk-documentation-preview').count(),2);
  assert.equal(fs.readFileSync(`${root}/${name}`,'utf8'),original);await save();assert.equal(fs.readFileSync(`${root}/${name}`,'utf8'),original);
  if(name.endsWith('.rs')){const link=panel().locator('.mk-doc-body .mk-preview-link');await link.scrollIntoViewIfNeeded();await ready();const box=await link.boundingBox();await page.keyboard.down('Control');await page.mouse.click(box.x+3,box.y+box.height/2);await page.keyboard.up('Control');await page.waitForFunction(()=>window.openedLinks.length===1);assert.equal((await page.evaluate(()=>window.openedLinks))[0][0],'https://example.org/docs');}
  const body=panel().locator('.mk-doc-body').first();await body.scrollIntoViewIfNeeded();await ready();const box=await body.boundingBox();await page.mouse.click(box.x+8,box.y+10);await ready();assert.equal(await panel().locator('.mk-documentation-preview').count(),1);
  await page.keyboard.press('End');await page.keyboard.insertText(' edited');await save();const end=original.indexOf('\r\n'),changed=original.slice(0,end)+' edited'+original.slice(end);assert.equal(fs.readFileSync(`${root}/${name}`,'utf8'),changed);
  await page.keyboard.press('Control+z');await save();assert.equal(fs.readFileSync(`${root}/${name}`,'utf8'),original);await page.keyboard.press('Control+Shift+z');await save();assert.equal(fs.readFileSync(`${root}/${name}`,'utf8'),changed);await page.keyboard.press('Control+z');await save();
  await clickNativeCommand(toggle);assert.equal(await panel().locator('.mk-documentation-preview').count(),0);await save();assert.equal(fs.readFileSync(`${root}/${name}`,'utf8'),original);await clickNativeCommand(toggle);
 }
 assert.deepEqual(errors,[]);console.log('DOC PREVIEW E2E: PASS');
}finally{await browser.close();}
