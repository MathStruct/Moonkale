// WP-14: production grid, direct writes, checked undo, and an external writer.
import { chromium } from 'playwright';
import { execFileSync } from 'node:child_process';
import assert from 'node:assert/strict';
import path from 'node:path';
const browser=await chromium.launch();
const page=await browser.newPage({viewport:{width:1600,height:1000}});
const root=process.env.M1_ROOT, repo=process.env.MOONKALE_REPO;
let step='setup';
const grid=()=>page.locator('.mk-table:visible');
const cell=()=>grid().locator('.mk-table-row[data-row="0"] .mk-table-cell').nth(1);
const open=async(file)=>{
 await page.goto(`http://127.0.0.1:${process.env.PORT}/`,{waitUntil:'networkidle'});
 await page.locator('.wb-workspace').waitFor({timeout:90000});
 // A reload before the healthy checkpoint intentionally counts as an incomplete startup.
 await page.waitForFunction(()=>JSON.parse(localStorage.getItem('moonkale.startup-health')??'{}').pending===false,null,{timeout:45000});
 await page.locator('.mk-explorer-open button[type=submit]').click();
 await page.waitForFunction(()=>document.querySelector('.wb-status-bar')?.textContent.includes('index:'));
 await page.locator('.mk-tree-db').filter({hasText:file}).first().click();
 await page.locator('.mk-explorer-source').filter({has:page.locator('.mk-explorer-source-name').filter({hasText:file})}).locator('.mk-tree-db').filter({hasText:/^edit_rows$/}).first().click();
 await grid().locator('.mk-cell-display').first().waitFor();
};
try {
 for(const file of ['data.sqlite','typed.duckdb']) {
  step=`${file}: save, Tab and Escape`;
  await open(file);
  await cell().dblclick();
  await grid().locator('.mk-cell-input').fill('saved');
  const writes=[];const record=request=>{if(request.url().includes('/api/sources/apply_rows'))writes.push(request.url());};
  page.on('request',record);
  await grid().locator('.mk-cell-input').dispatchEvent('keydown',{key:'Enter',code:'Enter',isComposing:true,bubbles:true});
  await page.waitForTimeout(100);
  assert.equal(writes.length,0,'IME candidate confirmation must not write a row');
  assert.equal(await grid().locator('.mk-cell-input').count(),1);
  page.off('request',record);
  await grid().locator('.mk-cell-input').press('Enter');
  await cell().getByRole('button',{name:'saved',exact:true}).waitFor();
  await open(file);
  assert.equal(await cell().innerText(),'saved');
  await cell().click();
  await grid().locator('.mk-cell-input').fill('discarded');
  await grid().locator('.mk-cell-input').press('Escape');
  assert.equal(await cell().innerText(),'saved');
  await grid().locator('.mk-table-grid').focus();
  await grid().locator('.mk-table-grid').press('Home');
  await grid().locator('.mk-table-grid').press('ArrowRight');
  await grid().locator('.mk-table-grid').press('Enter');
  await grid().locator('.mk-cell-input').fill('keyboard');
  await grid().locator('.mk-cell-input').press('Tab');
  await grid().locator('.mk-cell-input[type=number]').waitFor();
  await grid().locator('.mk-cell-input[type=number]').fill('4.5');
  await grid().locator('.mk-cell-input[type=number]').press('Enter');
  const score=()=>grid().locator('.mk-table-row[data-row="0"] .mk-table-cell').nth(2);
  await score().getByRole('button',{name:'4.5',exact:true}).waitFor();
  await grid().locator('.mk-table-undo').click();
  await score().getByRole('button',{name:'1.5',exact:true}).waitFor();
  await grid().locator('.mk-table-undo').click();
  await cell().getByRole('button',{name:'saved',exact:true}).waitFor();
  step=`${file}: concurrent external write shows the cell conflict`;
  await cell().dblclick();
  await grid().locator('.mk-cell-input').fill('mine');
  if(file.endsWith('.sqlite')) execFileSync('python3',['-c','import sqlite3,sys; c=sqlite3.connect(sys.argv[1]); c.execute("UPDATE edit_rows SET name=? WHERE id=1",("theirs",)); c.commit()',path.join(root,file)]);
  else {
   // A schema/linked-graph read can briefly hold DuckDB's process lock.
   // Bound retries still fail a source that retains a connection while idle.
   let last;
   for(let attempt=0;attempt<50;attempt++) {
    try { execFileSync(path.join(process.env.CARGO_TARGET_DIR??path.join(repo,'target'),'debug/examples/change_row'),[path.join(root,file),'theirs'],{encoding:'utf8',stdio:['ignore','pipe','pipe']}); last=null; break; }
    catch(error) { last=error; if(!/lock.*file|file.*lock|conflicting lock/i.test(String(error.stderr)))throw error; await page.waitForTimeout(100); }
   }
   if(last)throw last;
  }
  await grid().locator('.mk-cell-input').press('Enter');
  await grid().locator('.mk-cell-conflict').waitFor();
  assert((await grid().locator('.mk-cell-conflict').innerText()).includes('theirs'));
  assert.equal(await grid().locator('.mk-cell-input').inputValue(),'mine');
  await page.screenshot({path:`${process.env.M1_SHOTS}/table-edit-${file}-conflict.png`});
  await grid().locator('.mk-cell-keep-mine').click();
  await grid().locator('.mk-cell-save').click();
  await cell().getByRole('button',{name:'mine',exact:true}).waitFor();
  await open(file);
  assert.equal(await cell().innerText(),'mine');
 }
 console.log('TABLE EDIT E2E: PASS');
} catch(e) { console.error(`FAIL ${step}:`,e); if(e.stderr)console.error(String(e.stderr)); await page.screenshot({path:`${process.env.M1_SHOTS}/table-edit-fail.png`}); process.exitCode=1; }
finally { await browser.close(); }
