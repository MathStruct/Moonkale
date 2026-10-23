// Production Graph panel: paste plain engine JSON and use translated controls.
import assert from 'node:assert/strict';
import {chromium} from 'playwright';
import {readFile} from 'node:fs/promises';
const browser=await chromium.launch({args:['--use-angle=swiftshader','--enable-unsafe-swiftshader','--ignore-gpu-blocklist']});
const page=await browser.newPage({viewport:{width:1600,height:1000}});
try {
 await page.goto(`http://127.0.0.1:${process.env.PORT||8080}/`,{waitUntil:'networkidle'});
 await page.waitForSelector('.wb-workspace');
 await page.click('.mk-explorer-open button[type=submit]');
 await page.waitForFunction(()=>document.querySelector('.wb-status-bar').textContent.includes('index:'));
 await page.click('.wb-tab:has-text("Graph")');
 await page.waitForFunction(()=>/gl|gpu/i.test(document.querySelector('.mk-graph-info')?.dataset.backend||''));
 await page.click('button[title="Paste a stack trace or compiler output and draw it"]');
 await page.fill('.mk-graph-paste-text',await readFile(`${process.env.MOONKALE_REPO}/packages/graph-render/fixtures/egraph.json`,'utf8'));
 await page.click('.mk-egraph-load');
 await page.waitForSelector('.mk-eclass option',{state:'attached'});
 assert.equal(await page.locator('.mk-eclass option').count(),4);
 assert.ok(await page.$eval('.mk-graph-toolbar',e=>e.scrollWidth<=e.clientWidth+1),'e-graph controls must fit the panel');
 await page.screenshot({path:`${process.env.M1_SHOTS||'.'}/egraph-expanded.png`});
 await page.selectOption('.mk-eclass','result');
 await page.getByRole('button',{name:'Collapse class',exact:true}).click();
 await page.waitForFunction(()=>Object.values(window.moonkale.graphViews)[0].node_count()===9);
 await page.screenshot({path:`${process.env.M1_SHOTS||'.'}/egraph-collapsed.png`});
 await page.getByRole('button',{name:'Expand class',exact:true}).click();
 await page.waitForFunction(()=>Object.values(window.moonkale.graphViews)[0].node_count()===13);
 await page.getByRole('button',{name:'Collapse all',exact:true}).click();
 await page.waitForFunction(()=>Object.values(window.moonkale.graphViews)[0].node_count()===4);
 await page.getByRole('button',{name:'Expand all',exact:true}).click();
 await page.getByLabel('Rewrite provenance').uncheck();
 assert.equal(await page.evaluate(()=>JSON.parse(Object.values(window.moonkale.graphViews)[0].layer_state()).find(l=>l.id==='provenance').visible),false);
 await page.getByLabel('Rewrite provenance').check();
 await page.getByRole('button',{name:'Back to sources',exact:true}).click();
 await page.waitForFunction(()=>!document.querySelector('.mk-eclass'));
 // Invalid JSON reports a fixed local message and leaves source loading usable.
 await page.click('button[title="Paste a stack trace or compiler output and draw it"]');
 await page.fill('.mk-graph-paste-text','{"nodes":[]}');
 await page.click('.mk-egraph-load');
 await page.waitForFunction(()=>document.querySelector('.wb-status-bar').textContent.includes('Invalid e-graph'));
 await page.waitForFunction(()=>!document.querySelector('.mk-eclass'));
 console.log('PASS graph-egraph-ui');
} catch(error) {
 await page.screenshot({path:`${process.env.M1_SHOTS||'.'}/egraph-ui-failure.png`}).catch(()=>{});
 throw error;
} finally {await browser.close();}
