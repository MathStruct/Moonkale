// PLT-09 (WP-40 live plots): a training run appends rows to its metrics CSV;
// the table over the run's data folder and the plot linked to it update in
// place, without a click, and the plot keeps its zoom.
import { chromium } from 'playwright';
import assert from 'node:assert/strict';
import { appendFileSync, mkdirSync, writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { control } from './chrome-controls.mjs';
const ROOT=process.env.M1_ROOT,SHOTS=process.env.M1_SHOTS??'.';
const browser=await chromium.launch({args:['--use-angle=swiftshader','--enable-unsafe-swiftshader','--ignore-gpu-blocklist']});
let page,step='setup';
const visible=sel=>`[...document.querySelectorAll('${sel}')].find(e=>e.getClientRects().length)`;
const points=()=>page.evaluate(`+(${visible('.mk-plot')}?.dataset.points??-1)`);
const rows=()=>page.evaluate(`+(${visible('.mk-table-grid')}?.dataset.rows??-1)`);
try {
 const csv=`${ROOT}/runs/demo/metrics.csv`;
 mkdirSync(`${ROOT}/runs/demo`,{recursive:true});
 writeFileSync(csv,'step,loss\n1,0.9\n2,0.7\n3,0.6\n');
 const context=await browser.newContext({viewport:{width:1500,height:950}});page=await context.newPage();page.setDefaultTimeout(30000);
 const errors=[];page.on('pageerror',e=>errors.push(e.message));
 await page.addInitScript(()=>localStorage.setItem('moonkale.settings',JSON.stringify({language:'en',editor:{markdown_rich:false}})));
 await page.goto(`http://127.0.0.1:${process.env.PORT}/`,{waitUntil:'networkidle'});
 await page.locator('.wb-workspace').waitFor({timeout:90000});
 await page.locator('.mk-explorer-open button[type=submit]').click();
 await page.waitForFunction(()=>document.querySelector('.wb-status-bar')?.textContent.includes('index:'));

 step='open the run folder as a data source and plot its metrics';
 await page.locator('.mk-tree-dir').filter({hasText:'runs'}).first().click();
 await page.locator('.mk-tree-dir').filter({hasText:'demo'}).first().click();
 await page.locator('.mk-tree-db').filter({hasText:'metrics.csv'}).first().click();
 await page.locator('.mk-explorer-source-name').filter({hasText:'demo/ (data files)'}).waitFor();
 await page.locator('.mk-tree-db').filter({hasText:'metrics (view)'}).first().click();
 await page.waitForFunction(`+(${visible('.mk-table-grid')}?.dataset.rows)===3`);
 await control(page.locator('.mk-table:visible [data-command="plot.result"]'),'click');
 await page.locator('.mk-plot:visible').waitFor();
 await page.waitForFunction(`+(${visible('.mk-plot')}?.dataset.points)===3`);
 const plotTab='#'+await page.locator('.mk-plot:visible').evaluate(el=>el.closest('[id^="wb-panel-"]').id.replace('wb-panel-','wb-tab-'));

 step='the run appends rows: the plot grows by itself';
 appendFileSync(csv,'4,0.5\n5,0.45\n');
 await page.waitForFunction(`+(${visible('.mk-plot')}?.dataset.points)===5`,null,{timeout:15000});

 step='a zoomed plot stays zoomed while rows arrive';
 const canvas=page.locator('.mk-plot:visible .mk-plot-canvas');await canvas.focus();await canvas.press('+');
 await page.waitForFunction(`+(${visible('.mk-plot')}?.dataset.zoom)>1`);
 const zoom=await page.evaluate(`+${visible('.mk-plot')}.dataset.zoom`);
 appendFileSync(csv,'6,0.4\n');
 await page.waitForFunction(`+(${visible('.mk-plot')}?.dataset.points)===6`,null,{timeout:15000});
 assert.equal(await page.evaluate(`+${visible('.mk-plot')}.dataset.zoom`),zoom);
 assert.equal(await page.locator(plotTab).getAttribute('aria-selected'),'true','the plot stays the active tab');

 step='a histogram over the loaded rows follows the file (data-folder views are not pushed down)';
 await control(page.locator('.mk-plot:visible .mk-plot-mark'),'selectOption','histogram');await page.keyboard.press('Escape');
 await page.waitForFunction(`${visible('.mk-plot')}?.dataset.mark==='histogram'&&+${visible('.mk-plot')}.dataset.totalRows===6`);
 appendFileSync(csv,'7,0.38\n8,0.37\n');
 await page.waitForFunction(`+(${visible('.mk-plot')}?.dataset.totalRows)===8`,null,{timeout:15000});

 step='the table followed as well';
 await page.locator('.wb-tab').filter({hasText:'metrics'}).first().click();
 await page.waitForFunction(`+(${visible('.mk-table-grid')}?.dataset.rows)===8`);
 assert.equal(await rows(),8);

 step='SQLite run database: the pushed-down grid reloads in place and the source aggregate follows';
 const db=`${ROOT}/runs/demo/run.sqlite`;
 const sql=q=>execFileSync('sqlite3',[db,q]);
 sql('PRAGMA journal_mode=WAL; CREATE TABLE m(step INTEGER PRIMARY KEY, loss REAL); INSERT INTO m VALUES (1,0.9),(2,0.7),(3,0.6);');
 await page.locator('.mk-tree-db').filter({hasText:'run.sqlite'}).first().click();
 const runSource=page.locator('.mk-explorer-source').filter({has:page.locator('.mk-explorer-source-name').filter({hasText:'run.sqlite'})});
 await runSource.waitFor();
 await runSource.locator('.mk-tree-db').filter({hasText:/^m$/}).first().click();
 await page.waitForFunction(`+(${visible('.mk-table-grid')}?.dataset.rows)===3`);
 assert.equal(await page.locator('.mk-table:visible .mk-table-next').count(),1,'the grid is pushed down');
 const runTableTab='#'+await page.locator('.mk-table:visible').evaluate(el=>el.closest('[id^="wb-panel-"]').id.replace('wb-panel-','wb-tab-'));
 await control(page.locator('.mk-table:visible [data-command="plot.result"]'),'click');
 await page.locator('.mk-plot:visible').waitFor();
 await control(page.locator('.mk-plot:visible .mk-plot-mark'),'selectOption','histogram');await page.keyboard.press('Escape');
 await page.waitForFunction(`${visible('.mk-plot')}?.dataset.mark==='histogram'&&+${visible('.mk-plot')}.dataset.totalRows===3&&Boolean(${visible('.mk-plot')}.dataset.aggMs)`);
 sql('INSERT INTO m VALUES (4,0.5),(5,0.45);');
 await page.waitForFunction(`+(${visible('.mk-plot')}?.dataset.totalRows)===5&&Boolean(${visible('.mk-plot')}.dataset.aggMs)`,null,{timeout:15000});
 assert.equal(await page.evaluate(`${visible('.mk-plot')}.dataset.partial`),'false');
 await page.locator(runTableTab).click();
 await page.waitForFunction(`+(${visible('.mk-table-grid')}?.dataset.rows)===5`);
 await page.screenshot({path:`${SHOTS}/plot-live.png`});
 assert.deepEqual(errors,[]);
 await context.close();
 console.log('PLOT LIVE E2E: PASS');
} catch(e) {console.error(`FAIL ${step}:`,e,'plot',await page.evaluate(`JSON.stringify(${visible('.mk-plot')}?.dataset)`).catch(()=>'?'));await page?.screenshot({path:`${SHOTS}/plot-live-fail.png`}).catch(()=>{});process.exitCode=1;}
finally {await browser.close();}
