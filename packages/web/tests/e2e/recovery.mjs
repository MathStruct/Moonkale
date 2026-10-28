// WP-56: real dirty-buffer reload, explicit restore/discard and stale base.
import { firefox } from 'playwright';
import assert from 'node:assert/strict';
import fs from 'node:fs';
const browser = await firefox.launch();
const page = await browser.newPage({viewport: {width:1500,height:1000}});
page.setDefaultTimeout(30000);
const root = process.env.M1_ROOT;
const url = `http://127.0.0.1:${process.env.PORT ?? 8080}/`;
const snapshots = () => page.evaluate(() => Object.keys(localStorage).filter(k => k.startsWith('moonkale.state/recovery/')).map(k => JSON.parse(new TextDecoder().decode(Uint8Array.from(localStorage[k].match(/../g), x => parseInt(x,16)))) .data));
const waitSnapshot = text => page.waitForFunction(text => Object.keys(localStorage).filter(k => k.startsWith('moonkale.state/recovery/')).some(k => {
  const s=JSON.parse(new TextDecoder().decode(Uint8Array.from(localStorage[k].match(/../g), x=>parseInt(x,16)))).data;
  return s.text.includes(text);
}),text);
const open = async () => {
  await page.keyboard.press('Control+P');
  await page.locator('#mk-palette-input').fill('README.md');
  await page.locator('.mk-palette-item[data-key="README.md"]').click();
  await page.locator('.cm-content:visible').waitFor();
};
const edit = async text => {
  await page.locator('.cm-content:visible').click();
  await page.keyboard.press('Control+End');
  await page.keyboard.type(text);
  await waitSnapshot(text);
};
const reload = async () => {
  await page.reload({waitUntil:'networkidle'});
  // Several deliberate reloads within startup's 30-second health window
  // exercise the existing repeated-start notice too. Continue normally.
  await page.waitForFunction(() => document.querySelector('.wb-workspace') || [...document.querySelectorAll('button')].some(b => b.textContent === 'Dismiss'));
  if (await page.getByRole('button',{name:'Dismiss',exact:true}).isVisible()) {
    await page.getByRole('button',{name:'Dismiss',exact:true}).click();
  }
};
const restore = async () => {
  await page.locator('.mk-recovery summary').click();
  await page.locator('.mk-recovery button', {hasText:'Restore'}).click();
};
try {
  await page.goto(url, {waitUntil:'networkidle'});
  await page.keyboard.press('Control+O');
  await page.locator('#mk-explorer-path').fill(root);
  await page.locator('#mk-explorer-path').press('Enter');
  await page.waitForFunction(() => document.querySelector('.wb-status-bar')?.textContent.includes('index:'), null, {timeout:30000});
  await open();
  console.log('STEP: opened document');
  await edit('\nRecovered after reload.');
  console.log('STEP: snapshot durable');
  assert(!fs.readFileSync(`${root}/README.md`,'utf8').includes('Recovered after reload.'));
  await reload();
  console.log('STEP: reloaded, restoring');
  await restore();
  await page.waitForFunction(() => document.querySelector('.cm-content')?.textContent.includes('Recovered after reload.'));
  await page.locator('.cm-content:visible').click();
  await page.keyboard.press('Control+S');
  await page.waitForFunction(() => !Object.keys(localStorage).some(k=>k.startsWith('moonkale.state/recovery/')));
  assert(fs.readFileSync(`${root}/README.md`,'utf8').includes('Recovered after reload.'));
  await edit('\nStale recovery buffer.');
  fs.appendFileSync(`${root}/README.md`,'\nExternal change.');
  await reload();
  console.log('STEP: reloaded, restoring');
  await restore();
  await page.getByRole('dialog',{name:'The source changed — compare recovery'}).waitFor();
  assert((await page.locator('.mk-recovery-conflict').textContent()).includes('Stale recovery buffer.'));
  assert((await page.locator('.mk-recovery-conflict').textContent()).includes('External change.'));
  assert(!fs.readFileSync(`${root}/README.md`,'utf8').includes('Stale recovery buffer.'));
  await page.getByRole('button',{name:'Close',exact:true}).click();
  await page.locator('.mk-recovery button',{hasText:'Discard'}).click();
  await page.waitForFunction(() => !Object.keys(localStorage).some(k=>k.startsWith('moonkale.state/recovery/')));
  assert.equal((await snapshots()).length,0);
  await reload();
  await page.keyboard.press('Control+O');
  await page.locator('#mk-explorer-path').fill(root);
  await page.locator('#mk-explorer-path').press('Enter');
  await page.waitForFunction(() => document.querySelector('.wb-status-bar')?.textContent.includes('index:'));
  await open();
  await page.waitForTimeout(2500);
  assert.equal(await page.locator('.mk-recovery summary').count(),0);
  console.log('PASS: recovery reload, explicit restore, save cleanup, stale comparison and discard');
} catch(e) {
  console.error('FAIL:',e.stack);
  await page.screenshot({path:`${process.env.M1_SHOTS ?? '.'}/recovery-fail.png`}).catch(()=>{});
  process.exitCode=1;
} finally { await browser.close(); }
