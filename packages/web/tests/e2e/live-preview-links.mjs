// Full-app acceptance: native link navigation reads canonical, unsaved documents.
import {firefox} from 'playwright';
import fs from 'node:fs';
import assert from 'node:assert/strict';
const root=process.env.M1_ROOT;
if(!root)throw Error('M1_ROOT must name a disposable fixture');
const source='---\r\ntitle: Links\r\n---\r\n\r\n# Local **猫**\r\n\r\n[Web](https://example.org/path) [[#local-猫|Self]]\r\n\r\n# Links [File](Caf%C3%A9.md#unsaved-%E7%8C%AB)\r\n\r\n- [[Café#unsaved-猫|Wiki]]\r\n- [Missing file](Absent.md)\r\n- [[Created.md|Create]]\r\n- [Missing heading](Caf%C3%A9.md#absent)\r\n\r\nEnd\r\n';
fs.writeFileSync(`${root}/LinkPreview.md`,source);
fs.writeFileSync(`${root}/Café.md`,'---\r\ntitle: Target\r\n---\r\n\r\n# Saved\r\n\r\nTarget end\r\n');
const browser=await firefox.launch();
const context=await browser.newContext({viewport:{width:1500,height:1100}});
await context.addInitScript(()=>{
 localStorage.setItem('moonkale.settings',JSON.stringify({extensions:{enabled:['dev.moonkale.editor-code-native']},editor:{implementation:'native',markdown_rich:false}}));
 window.openedLinks=[];window.open=(...args)=>{window.openedLinks.push(args);return null;};
});
const page=await context.newPage();const errors=[];page.on('pageerror',e=>errors.push(e.message));
const panel=()=>page.locator('.mk-crust[data-editor="rust"]:visible');
const input=()=>panel().locator('.mk-editor-core-input-sink');
const surface=()=>panel().locator('.mk-native-surface');
const ready=()=>page.waitForFunction(()=>{
 const panel=[...document.querySelectorAll('.mk-crust[data-editor="rust"]')].find(el=>el.getBoundingClientRect().height>0&&getComputedStyle(el).visibility!=='hidden');
 const runs=panel?.querySelectorAll('.mk-proportional-run');return runs?.length&&[...runs].every(el=>el.dataset.ready==='true');
});
const active=async name=>{await panel().locator('.mk-editor-path').filter({hasText:name}).waitFor();await input().waitFor();};
const open=async name=>{await page.locator('.mk-tree-file').filter({hasText:name}).click();await active(name);};
const preview=async()=>{await input().focus();await page.keyboard.press('Control+End');await ready();await panel().locator('.mk-editor-core-viewport').evaluate(el=>el.scrollTop=0);await ready();};
const follow=async label=>{
 await preview();const link=panel().locator('.mk-preview-link').filter({hasText:new RegExp('^'+label+'$')});
 const box=await link.evaluate(el=>{const range=document.createRange();range.selectNodeContents(el);const r=[...range.getClientRects()].find(r=>r.width>0&&r.height>0);return{x:r.x,y:r.y,width:r.width,height:r.height};});
 await page.keyboard.down('Control');await page.mouse.click(box.x+Math.min(5,box.width/2),box.y+box.height/2);await page.keyboard.up('Control');
};
try{
 await page.goto(`http://127.0.0.1:${process.env.PORT??8094}/`,{waitUntil:'networkidle'});await page.waitForSelector('.wb-workspace');
 await page.click('.mk-explorer-open button[type=submit]');await page.waitForFunction(()=>document.querySelector('.wb-status-bar')?.textContent.includes('index:'));
 await open('Café.md');await input().focus();await page.keyboard.press('Control+Home');await page.keyboard.press('Control+a');
 const unsaved='---\r\ntitle: Target\r\n---\r\n\r\n# Unsaved 猫\r\n\r\n## Duplicate\r\n\r\n## Duplicate\r\n';
 await page.keyboard.insertText(unsaved);await panel().locator('.mk-editor-dirty').waitFor();
 await open('LinkPreview.md');await panel().locator('.mk-native-preview').click();await preview();
 assert.equal(await panel().locator('.mk-preview-link').count(),7);
 await follow('Web');await page.waitForFunction(()=>window.openedLinks.length===1);
 assert.deepEqual(await page.evaluate(()=>window.openedLinks[0]),['https://example.org/path','_blank','noopener,noreferrer']);
 await follow('Self');await ready();assert.equal(Number(await surface().getAttribute('data-cursor-offset')),Array.from(source.replaceAll('\r\n','\n').split('# Local')[0]).length);
 for(const label of ['File','Wiki']){
  await follow(label);await active('Café.md');await ready();
  assert.equal(Number(await surface().getAttribute('data-cursor-offset')),Array.from(unsaved.replaceAll('\r\n','\n').split('# Unsaved')[0]).length);
  await panel().locator('.mk-editor-dirty').waitFor();assert.ok(fs.readFileSync(`${root}/Café.md`,'utf8').includes('# Saved'));
  await open('LinkPreview.md');
 }
 await follow('Missing file');await page.waitForFunction(()=>document.querySelector('.wb-status-bar').textContent.toLowerCase().includes('not found'));
 await active('LinkPreview.md');assert.equal(fs.existsSync(`${root}/Absent.md`),false);
 await follow('Missing heading');await active('Café.md');await ready();assert.equal(Number(await surface().getAttribute('data-cursor-offset')),0);
 await page.waitForFunction(()=>document.querySelector('.wb-status-bar').textContent.includes('absent'));
 await open('LinkPreview.md');await follow('Create');await active('Created.md');assert.equal(fs.existsSync(`${root}/Created.md`),true);assert.equal(fs.existsSync(`${root}/Created.md.md`),false);
 assert.equal(fs.readFileSync(`${root}/LinkPreview.md`,'utf8'),source);assert.deepEqual(errors,[]);console.log('LIVE PREVIEW LINKS E2E: PASS');
}finally{await browser.close();}
