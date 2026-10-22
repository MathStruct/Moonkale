import { firefox } from 'playwright';
import assert from 'node:assert/strict';
const browser=await firefox.launch();const page=await browser.newPage({viewport:{width:1500,height:1000}});
await page.addInitScript(()=>localStorage.setItem('moonkale.settings',JSON.stringify({language:'de',editor:{markdown_rich:false}})));
page.setDefaultTimeout(15000);
try {
 await page.goto(`http://127.0.0.1:${process.env.PORT}/`,{waitUntil:'networkidle'});
 await page.click('.mk-explorer-open button[type=submit]');
 await page.locator('.mk-tree-file').filter({hasText:'README.md'}).click();
 const content=page.locator('.cm-content:visible');await content.waitFor();await content.click();await page.keyboard.press('Control+f');
 const panel=page.locator('.cm-search:visible');await panel.waitFor();
 await panel.locator('input[placeholder="Suchen"]').waitFor();
 assert.ok((await panel.textContent()).includes('Groß-/Kleinschreibung'));
 const styles=await panel.locator('.cm-button').first().evaluate(el=>({image:getComputedStyle(el).backgroundImage,radius:getComputedStyle(el).borderRadius}));
 assert.deepEqual(styles,{image:'none',radius:'4px'});
 await panel.locator('input[placeholder="Suchen"]').fill('Sample');await panel.locator('button[name=next]').click();
 assert.equal(await page.locator('.cm-searchMatch-selected').count(),1);
 console.log('CODEMIRROR SEARCH E2E: PASS');
} finally {await browser.close();}
