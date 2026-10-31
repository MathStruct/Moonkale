// WP-40: real source tables, Rust SVG, link groups, keyboard and mouse.
import { chromium } from 'playwright';
import assert from 'node:assert/strict';
import { writeFileSync } from 'node:fs';
import { control } from './chrome-controls.mjs';
const browser=await chromium.launch({args:['--use-angle=swiftshader','--enable-unsafe-swiftshader','--ignore-gpu-blocklist']});
let page,step='setup';
const plot=()=>page.locator('.mk-plot:visible');
const table=()=>page.locator('.mk-table:visible');
const tabFor=async element=>'#'+await element.evaluate(el=>el.closest('[id^="wb-panel-"]').id.replace('wb-panel-','wb-tab-'));
const action=async()=>{await control(table().locator('[data-command="plot.result"]'),'click');};
const ready=async mark=>{await plot().waitFor();await page.waitForFunction(mark=>[...document.querySelectorAll('.mk-plot')].find(p=>p.getClientRects().length)?.dataset.mark===mark,mark);};
const openTable=async(file,name)=>{
 const source=page.locator('.mk-explorer-source').filter({has:page.locator('.mk-explorer-source-name').filter({hasText:file})});
 if(!await source.count()) {await page.locator('.mk-tree-db').filter({hasText:file}).first().click();await source.waitFor();}
 await source.locator('.mk-tree-db').filter({hasText:new RegExp('^'+name+'$')}).first().click();
 await page.locator('.wb-tab[aria-selected="true"]').filter({hasText:name}).first().waitFor();
 await table().locator('.mk-table-grid').waitFor();await page.waitForFunction(()=>+[...document.querySelectorAll('.mk-table-grid')].find(p=>p.getClientRects().length)?.dataset.rows>0);
 return await tabFor(table());
};
const boot=async()=>{
 const context=await browser.newContext({viewport:{width:1600,height:1000}});page=await context.newPage();page.setDefaultTimeout(30000);
 await page.addInitScript(()=>{
  localStorage.setItem('moonkale.settings',JSON.stringify({language:'en',editor:{markdown_rich:false}}));
  window.__plotMemories=[];
  const record=r=>{for(const v of Object.values((r.instance??r).exports??{}))if(v instanceof WebAssembly.Memory&&!window.__plotMemories.includes(v))window.__plotMemories.push(v);return r;};
  for(const name of ['instantiate','instantiateStreaming']){const old=WebAssembly[name];WebAssembly[name]=async function(...args){return record(await old.apply(this,args));};}
 });
 await page.goto(`http://127.0.0.1:${process.env.PORT}/`,{waitUntil:'networkidle'});
 await page.locator('.wb-workspace').waitFor({timeout:90000});
 await page.locator('.mk-explorer-open button[type=submit]').click();
 await page.waitForFunction(()=>document.querySelector('.wb-status-bar')?.textContent.includes('index:'));
 return context;
};
try {
 for(const file of ['data.sqlite','typed.duckdb']) {
  const context=await boot();
  const errors=[];page.on('pageerror',e=>errors.push(e.message));
  for(const [name,mark] of [['plot_time','line'],['plot_scatter','scatter'],['plot_bar','bar'],['plot_hist','histogram']]) {
   step=`${file}: ${mark} in one action`;
   const tableTab=await openTable(file,name);
   if(mark==='scatter') {await table().locator('.mk-table-grid').focus();await page.keyboard.press('Control+Alt+d');}
   else await action();
   await ready(mark);
   const plotTab=await tabFor(plot());
   assert(await plot().locator('.mk-plot-point').count()>0);
   assert.equal(await page.locator(tableTab).getAttribute('data-link-group'),await page.locator(plotTab).getAttribute('data-link-group'));
   if(mark==='line') {
    assert.match(await plot().locator('.mk-plot-tick').allTextContents().then(s=>s.join(' ')),/2026-10/);
    await control(plot().locator('.mk-plot-ys'),'selectOption','log');await page.keyboard.press('Escape');
    await page.waitForFunction(()=>{const y=[...[...document.querySelectorAll('.mk-plot')].find(p=>p.getClientRects().length).querySelectorAll('.mk-plot-point circle')].map(c=>+c.getAttribute('cy'));return y.length===3&&Math.abs((y[0]-y[1])-(y[1]-y[2]))<1;});
   }
   step=`${file}: hover and linked selection ${mark}`;
   const point=plot().locator('.mk-plot-point').first();
   await point.hover();assert.match(await plot().locator('.mk-plot-tooltip').innerText(),new RegExp(mark==='line'?'instant':mark==='scatter'?'x':mark==='bar'?'category':'value'));
   await point.click();
   await page.locator(tableTab).click();
   await table().locator('.mk-cell-cursor').waitFor();
   const row=table().locator('.mk-table-row[data-row="1"] .mk-table-cell').first();await row.click();
   await page.locator(plotTab).click();
   await plot().locator(mark==='bar'?'.mk-plot-point[data-row="0"][data-selected="true"]':'.mk-plot-point[data-row="1"][data-selected="true"]').waitFor();
   step=`${file}: keyboard inspection, zoom and pan ${mark}`;
   const canvas=plot().locator('.mk-plot-canvas');await canvas.focus();await canvas.press('Home');await canvas.press('Enter');
   await plot().locator('.mk-plot-point[data-selected="true"]').first().waitFor();
   await canvas.press('+');await page.waitForFunction(()=>+[...document.querySelectorAll('.mk-plot')].find(p=>p.getClientRects().length)?.dataset.zoom>1);
   await canvas.press('Shift+ArrowRight');
   await control(plot().locator('.mk-plot-fit'),'click');
   await page.waitForFunction(()=>+[...document.querySelectorAll('.mk-plot')].find(p=>p.getClientRects().length)?.dataset.zoom===1);
   await canvas.hover();await page.mouse.wheel(0,-100);
   await page.waitForFunction(()=>+[...document.querySelectorAll('.mk-plot')].find(p=>p.getClientRects().length)?.dataset.zoom>1);
   const box=await canvas.boundingBox();await page.mouse.move(box.x+box.width/2,box.y+box.height/2);await page.mouse.down();await page.mouse.move(box.x+box.width/2+50,box.y+box.height/2+20);await page.mouse.up();
   if(mark==='scatter') {
    await control(plot().locator('.mk-plot-x'),'selectOption','1');await control(plot().locator('.mk-plot-y'),'selectOption','0');
    await control(plot().locator('.mk-plot-xs'),'selectOption','log');await control(plot().locator('.mk-plot-equal'),'check');await page.keyboard.press('Escape');
    assert.equal(await plot().locator('.mk-plot-label').first().textContent(),'y');
   }
   await page.screenshot({path:`${process.env.M1_SHOTS}/plot-${file}-${mark}.png`});
  }
  assert.deepEqual(errors,[]);await context.close();
 }
 step='million-row DuckDB: bounded loaded-page histogram';
 const context=await boot();const loadStarted=performance.now();const tableTab=await openTable('million.duckdb','million');const tableLoadMs=performance.now()-loadStarted;
 const requests=[];page.on('request',r=>{if(/\/query_rows|\/read_rows|\/query\?/.test(r.url()))requests.push(r.url());});
 const cdp=await context.newCDPSession(page);await cdp.send('HeapProfiler.collectGarbage');
 const before=await page.evaluate(()=>({heap:performance.memory?.usedJSHeapSize??null,wasm:window.__plotMemories.reduce((sum,m)=>sum+m.buffer.byteLength,0)}));
 const started=performance.now();await action();await ready('scatter');await control(plot().locator('.mk-plot-mark'),'selectOption','histogram');await page.keyboard.press('Escape');
 await page.waitForFunction(()=>[...document.querySelectorAll('.mk-plot')].find(p=>p.getClientRects().length)?.dataset.mark==='histogram');
 await plot().locator('.mk-plot-point rect').first().waitFor();await page.evaluate(()=>new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r))));
 const elapsed=performance.now()-started;
 const counts=await plot().evaluate(el=>({points:+el.dataset.points,partial:el.dataset.partial,dom:el.querySelectorAll('*').length,summary:el.querySelector('.mk-plot-summary').textContent}));
 assert(counts.points<=40);assert.equal(counts.partial,'true');assert(elapsed<10000);assert.equal(requests.length,0,'plot must not fetch the rest of a million-row table');
 await plot().locator('.mk-plot-partial').waitFor();
 await cdp.send('HeapProfiler.collectGarbage');
 const after=await page.evaluate(()=>({heap:performance.memory?.usedJSHeapSize??null,wasm:window.__plotMemories.reduce((sum,m)=>sum+m.buffer.byteLength,0)}));
 assert(after.wasm<128*1024*1024);
 const measurements={elapsedMs:elapsed,tableLoadMs,totalMs:elapsed+tableLoadMs,before,after,counts,requests:requests.length};writeFileSync(`${process.env.M1_SHOTS}/plot-million.json`,JSON.stringify(measurements,null,2));console.log('million:',JSON.stringify(measurements));
 await page.screenshot({path:`${process.env.M1_SHOTS}/plot-million.png`});
 await page.locator(tableTab).click();
 step='query results and decimation';
 await table().locator('.mk-table-sql').fill('SELECT id AS x, bucket AS y FROM million LIMIT 50000');await control(table().locator('.mk-table-query button').first(),'click');
 await page.waitForFunction(()=>+[...document.querySelectorAll('.mk-table-grid')].find(p=>p.getClientRects().length)?.dataset.rows===50000);
 await action();await ready('scatter');await plot().locator('.mk-plot-sampled').waitFor();
 assert.equal(+await plot().getAttribute('data-points'),5000);assert.equal(await plot().locator('.mk-plot-point').count(),5000);
 await page.locator(tableTab).click();await table().locator('.mk-table-row[data-row="1"] .mk-table-cell').first().click();
 const resultTableTab=await tabFor(table());await action();await ready('scatter');
 await plot().locator('.mk-plot-point[data-row="1"][data-selected="true"]').waitFor();
 assert.equal(await plot().locator('.mk-plot-point').count(),5000);
 assert.equal(await plot().locator('.mk-plot-point[data-row="0"]').count(),1);
 assert.equal(await plot().locator('.mk-plot-point[data-row="49999"]').count(),1);
 // Mouse-only controls remain available through the actual disclosure.
 await page.locator(resultTableTab).click();await action();await ready('scatter');
 await control(plot().locator('.mk-plot-mark'),'selectOption','histogram');
 const more=plot().locator('.mk-toolbar-more summary');await more.click();
 await plot().locator('.mk-plot-point').first().click();
 await context.close();
 console.log('PLOT E2E: PASS');
} catch(e) {console.error(`FAIL ${step}:`,e);await page?.screenshot({path:`${process.env.M1_SHOTS}/plot-fail.png`}).catch(()=>{});process.exitCode=1;}
finally {await browser.close();}
