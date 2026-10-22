import { firefox } from 'playwright';
import assert from 'node:assert/strict';
const browser = await firefox.launch();
const page = await browser.newPage({viewport:{width:1500,height:1000}});
const selected = () => page.locator('.wb-tab[aria-selected=true]').allTextContents();
page.setDefaultTimeout(15000);
try {
 await page.goto(`http://127.0.0.1:${process.env.PORT}/`,{waitUntil:'networkidle'});
 await page.click('.mk-explorer-open button[type=submit]');
 await page.locator('.mk-tree-file').filter({hasText:'README.md'}).click();
 await page.locator('.cm-content:visible').waitFor();
 await page.locator('.wb-tab').filter({hasText:/^Graph$/}).click();
 const before = (await selected()).map(x=>x.trim());
 assert.ok(before.includes('Graph'));
 for (let i=0;i<2;i++) {
  await page.setViewportSize({width:420,height:820});
  await page.locator('.mk-shell.mk-narrow').waitFor();
  await page.setViewportSize({width:1500,height:1000});
  await page.locator('.mk-shell:not(.mk-narrow)').waitFor();
  await page.waitForTimeout(200);
  assert.deepEqual((await selected()).map(x=>x.trim()),before);
 }
 console.log('RESPONSIVE TABS E2E: PASS');
} finally {await browser.close();}
