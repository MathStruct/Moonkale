import { control } from './chrome-controls.mjs';
// The table editor's virtualised grid: a 50 000-row result (a recursive
// query on the fixture's data.sqlite — the source is read-only, a CTE is
// a read) keeps only its window in the DOM, scrolls to the end, sorts,
// filters, moves a keyboard cursor that scrolls the cell into view,
// resizes a column, and draws only the columns under a 60-column result.
import { chromium } from "playwright";
import { writeFileSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { join } from "node:path";
const S = process.env.M1_SHOTS ?? ".";
const PORT = process.env.PORT ?? 8080;
const N = 50000;
const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1400, height: 900 } });
await page.addInitScript(() => {
  window.__tableMemories=[];
  const record=result=>{const instance=result.instance??result;for(const value of Object.values(instance.exports??{}))if(value instanceof WebAssembly.Memory&&!window.__tableMemories.includes(value))window.__tableMemories.push(value);return result;};
  for(const name of ['instantiate','instantiateStreaming']){const original=WebAssembly[name];WebAssembly[name]=async function(...args){return record(await original.apply(this,args));};}
});
const diagnostics={errors:[],failed:[],started:0,finished:0,responses:0,memory:[]};
page.on('pageerror',error=>diagnostics.errors.push(error.message));
page.on('console',message=>{if(message.type()==='error')diagnostics.errors.push(message.text());});
page.on('requestfailed',request=>{if(request.url().includes('/query_rows'))diagnostics.failed.push(request.failure());});
page.on('request',request=>{if(request.url().includes('/query_rows'))diagnostics.started++;});
page.on('requestfinished',request=>{if(request.url().includes('/query_rows'))diagnostics.finished++;});
page.on('response',response=>{if(response.url().includes('/query_rows'))diagnostics.responses++;});
const step = async (n, f) => { process.stdout.write(`- ${n} … `); await f(); console.log("ok"); };
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const grid = () => page.$eval(".mk-table:visible .mk-table-grid", (g) => ({
  rows: +g.dataset.rows, total: +g.dataset.total, first: +g.dataset.first, drawn: +g.dataset.drawn,
  colsDrawn: +g.dataset.colsDrawn, domRows: g.querySelectorAll(".mk-table-row").length,
  top: g.scrollTop, height: g.scrollHeight, client: g.clientHeight,
}));
const firstRowCells = () => page.$$eval(".mk-table:visible .mk-table-row", (rows) => {
  // The first row actually on screen (the window carries overscan above).
  const g = rows[0]?.closest(".mk-table-grid");
  if (!g) return [];
  const top = g.getBoundingClientRect().top + 26;
  const r = rows.find((x) => x.getBoundingClientRect().bottom > top + 2);
  return r ? [...r.querySelectorAll(".mk-table-cell")].map((c) => c.textContent) : [];
});
const run = async (sql) => {
  await control(page, 'fill', ".mk-table:visible .mk-table-sql", sql);
  await control(page, 'click', ".mk-table:visible .mk-table-query button");
};
try {
  await step("open folder, data.sqlite, the users table", async () => {
    execFileSync('python3',['-c',"import sqlite3,sys; c=sqlite3.connect(sys.argv[1]); c.executescript(\"CREATE TABLE schema_rows(id INTEGER PRIMARY KEY, name TEXT); WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i<1500) INSERT INTO schema_rows SELECT i,'row '||i FROM n;\"); c.commit()",join(process.env.M1_ROOT,'data.sqlite')]);
    await page.goto(`http://127.0.0.1:${PORT}/`, { waitUntil: "networkidle" });
    await page.waitForSelector(".wb-workspace");
    await control(page, 'click', ".mk-explorer-open button[type=submit]");
    await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
    await control(page, 'click', ".mk-tree-db >> text=data.sqlite");
    await page.waitForSelector(".mk-tree-db >> text=users", { timeout: 15000 });
    await control(page, 'click', ".mk-tree-db >> text=users");
    await page.waitForSelector(".mk-table:visible .mk-table-grid", { timeout: 15000 });
  });
  await step("SQLite declared dates render as dates and ordinary text stays text", async () => {
    await run("SELECT * FROM dates");
    await page.waitForFunction(() => document.querySelector(".mk-table-hname")?.textContent === "instant");
    const cells = await firstRowCells();
    if (!cells[0].includes("2026-10-23 10:30:00 +02:00") || cells[2] !== "2026-10-23") throw new Error("typed date: " + cells);
  });
  await step(`${N} rows: only the window is in the DOM`, async () => {
    await run(`WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < ${N}) SELECT i, i * 7 % 1000 AS v, 'row ' || i AS label, CASE WHEN i % 10 = 0 THEN NULL ELSE i * 0.5 END AS half FROM n`);
    await page.waitForFunction((n) => +document.querySelector(".mk-table-grid")?.dataset.total === n, N, { timeout: 30000 });
    if(!await page.locator(".mk-table:visible .mk-table-partial").isVisible())throw new Error("loaded-only query lacks partial indicator");
    const g = await grid();
    console.log(`\n  ${JSON.stringify(g)}`);
    if (g.domRows > 80) throw new Error(`${g.domRows} rows in the DOM`);
    if (g.height < N * 24) throw new Error(`the scroll box is ${g.height}px tall — not every row's height`);
    const nulls = await page.$$eval(".mk-table:visible .mk-cell-null", (e) => e.length);
    if (nulls < 1) throw new Error("every tenth half is NULL, none drawn");
  });
  await step("scrolled to the end, the last rows draw and the window stays small", async () => {
    const t0 = Date.now();
    await page.$eval(".mk-table:visible .mk-table-grid", (g) => { g.scrollTop = g.scrollHeight; });
    await page.waitForFunction((n) => [...document.querySelectorAll(".mk-table-row")].some((r) => +r.dataset.row === n - 1), N, { timeout: 10000 });
    const g = await grid();
    console.log(`\n  end drawn in ${Date.now() - t0} ms: first ${g.first}, ${g.domRows} rows in the DOM`);
    if (g.domRows > 80 || g.first < N - 80) throw new Error(`window ${JSON.stringify(g)}`);
    // The widths came from a sample across the whole result: the last
    // row's id and label fit, not only the first rows'.
    const clipped = await page.$$eval(".mk-table-row", (rows) => {
      const last = rows[rows.length - 1];
      return [...last.querySelectorAll(".mk-table-cell")].filter((c) => c.scrollWidth > c.clientWidth + 1).map((c) => c.textContent);
    });
    if (clipped.length) throw new Error(`the last row's cells are clipped: ${clipped}`);
    await page.screenshot({ path: `${S}/table-grid-end.png` });
  });
  await step("a header click sorts descending on the second click; NULLs stay last", async () => {
    await page.$eval(".mk-table:visible .mk-table-grid", (g) => { g.scrollTop = 0; });
    await control(page, 'click', ".mk-table:visible .mk-table-hcell:has(.mk-table-hname:text-is('v'))");
    await control(page, 'click', ".mk-table:visible .mk-table-hcell:has(.mk-table-hname:text-is('v'))");
    await sleep(400);
    const cells = await firstRowCells();
    console.log(`\n  first row by v desc: ${cells.join(" | ")}`);
    if (cells[1] !== "999") throw new Error(`v desc starts at ${cells[1]}`);
    const sortAttr = await page.$eval(".mk-table:visible .mk-table-hcell:has(.mk-table-hname:text-is('v'))", (h) => h.getAttribute("aria-sort"));
    if (sortAttr !== "descending") throw new Error(`aria-sort ${sortAttr}`);
    // half descending: NULL rows (every tenth) at the end, not the start.
    await control(page, 'click', ".mk-table:visible .mk-table-hcell:has(.mk-table-hname:text-is('half'))");
    await control(page, 'click', ".mk-table:visible .mk-table-hcell:has(.mk-table-hname:text-is('half'))");
    await sleep(300);
    const top = await firstRowCells();
    if (top[3] !== "24999.5") throw new Error(`half desc starts at ${top[3]}`);
  });
  await step("the filter narrows the rows and the count says so", async () => {
    await control(page, 'fill', ".mk-table:visible .mk-table-filter", "row 4999");
    await page.waitForFunction(() => +document.querySelector(".mk-table-grid").dataset.rows === 11, null, { timeout: 5000 });
    const meta = await page.$eval(".mk-table:visible .mk-table-meta", (m) => m.textContent);
    console.log(`\n  meta: ${meta.trim()}`);
    if (!/11 of 50[,. ]?000 rows/.test(meta)) throw new Error("the meta line does not count the filtered rows");
    await control(page, 'fill', ".mk-table:visible .mk-table-filter", "");
    await page.waitForFunction((n) => +document.querySelector(".mk-table-grid").dataset.rows === n, N, { timeout: 5000 });
  });
  await step("the keyboard cursor: Ctrl+End jumps to the last cell and scrolls it into view", async () => {
    await control(page, 'click', ".mk-table:visible .mk-table-row >> nth=0 >> .mk-table-cell >> nth=0");
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("ArrowRight");
    const at = await page.$eval(".mk-table:visible .mk-cell-cursor", (c) => [c.parentElement.getAttribute("aria-rowindex"), c.textContent]);
    if (at[0] !== "3") throw new Error(`after Down the cursor is on row ${at[0]}`);
    await page.keyboard.press("Control+End");
    await page.waitForFunction(() => {
      const c = document.querySelector(".mk-cell-cursor");
      if (!c) return false;
      const g = c.closest(".mk-table-grid").getBoundingClientRect();
      const r = c.getBoundingClientRect();
      return r.bottom <= g.bottom + 1 && r.top >= g.top;
    }, null, { timeout: 5000 });
    const row = await page.$eval(".mk-table:visible .mk-cell-cursor", (c) => c.parentElement.getAttribute("aria-rowindex"));
    console.log(`\n  Ctrl+End → aria-rowindex ${row}`);
    if (row !== String(N + 1)) throw new Error(`the cursor is on row ${row}`);
    await page.keyboard.press("Control+Home");
    await page.waitForFunction(() => document.querySelector(".mk-table-grid").scrollTop === 0, null, { timeout: 5000 });
  });
  await step("dragging a header's edge widens the column", async () => {
    const h = page.locator(".mk-table:visible .mk-table-hcell:has(.mk-table-hname:text-is('label'))");
    const before = (await h.boundingBox()).width;
    const handle = await h.locator(".mk-table-resize").boundingBox();
    await page.mouse.move(handle.x + 3, handle.y + 10);
    await page.mouse.down();
    await page.mouse.move(handle.x + 103, handle.y + 10, { steps: 8 });
    await page.mouse.up();
    const after = (await h.boundingBox()).width;
    const sort = await h.getAttribute("aria-sort");
    console.log(`\n  label: ${before.toFixed(0)} → ${after.toFixed(0)} px`);
    if (Math.abs(after - before - 100) > 4) throw new Error("the column did not widen by the drag");
    if (sort !== "none") throw new Error("the resize drag also sorted");
  });
  await step("60 columns: only those under the viewport draw", async () => {
    const cols = Array.from({ length: 60 }, (_, i) => `${i} * i AS column_${String(i).padStart(2, "0")}`).join(", ");
    await run(`WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 200) SELECT ${cols} FROM n`);
    await page.waitForFunction(() => document.querySelector(".mk-table-grid")?.getAttribute("aria-colcount") === "60", null, { timeout: 15000 });
    let g = await grid();
    const heads = await page.$$eval(".mk-table:visible .mk-table-hname", (e) => e.map((x) => x.textContent));
    console.log(`\n  ${g.colsDrawn} of 60 columns drawn: ${heads[0]} … ${heads[heads.length - 1]}`);
    if (g.colsDrawn >= 60 || heads.length >= 60) throw new Error("every column is in the DOM");
    await page.$eval(".mk-table:visible .mk-table-grid", (el) => { el.scrollLeft = el.scrollWidth; });
    await page.waitForFunction(() => [...document.querySelectorAll(".mk-table-hname")].some((h) => h.textContent === "column_59"), null, { timeout: 5000 });
    await page.screenshot({ path: `${S}/table-grid-wide.png` });
  });
  await step("DuckDB lists and arrays draw sparklines beside a timestamp", async () => {
    await control(page, 'click', ".mk-tree-db >> text=typed.duckdb");
    await page.waitForSelector(".mk-tree-db >> text=dates", {timeout:15000});
    await control(page.locator(".mk-explorer-source").filter({has:page.locator(".mk-explorer-source-name", {hasText:"typed.duckdb"})}).getByText("dates", {exact:true}), 'click');
    await page.waitForSelector(".mk-table:visible .mk-cell-vector", {timeout:15000});
    const lines = await page.locator(".mk-table:visible .mk-cell-vector polyline").evaluateAll(es => es.map(e => e.getAttribute("points")));
    if (lines.length !== 2 || lines.some(p => !p || p.includes("NaN"))) throw new Error("missing vector sparklines: " + lines);
    const cells = await firstRowCells();
    if (!cells[0].includes("2026-10-23 08:30:00 UTC")) throw new Error("missing DuckDB date: " + cells);
  });
  await step("million-row DuckDB: typed sorting, filtering, edits and bounded keyset traversal", async () => {
    await control(page, 'click', '.mk-tree-db >> text=million.duckdb');
    const source=page.locator('.mk-explorer-source').filter({has:page.locator('.mk-explorer-source-name',{hasText:'million.duckdb'})});
    await source.getByText('million',{exact:true}).click();
    const host=page.locator('.mk-table:visible');
    const g=host.locator('.mk-table-grid');
    await page.waitForFunction(()=>[...document.querySelectorAll('.mk-table-grid')].find(e=>e.checkVisibility())?.dataset.pushdown==='true');
    const clippedHeaders=await host.locator('.mk-table-hname').evaluateAll(es=>es.filter(e=>e.scrollWidth>e.clientWidth+1).map(e=>e.textContent));
    if(clippedHeaders.length)throw new Error('short headers clipped by column controls: '+clippedHeaders);
    const head=name=>host.locator('.mk-table-hcell').filter({has:page.locator('.mk-table-hname',{hasText:new RegExp('^'+name+'$')})});
    const settle=async()=>{await page.waitForFunction(()=>[...document.querySelectorAll('.mk-table-grid')].find(e=>e.checkVisibility())?.dataset.loading==='false');};
    // A keyboard-only header sort followed by shift+Enter for the second key.
    await head('bucket').focus(); await page.keyboard.press('Enter'); await settle();
    await head('bucket').press('Enter'); await settle();
    await head('id').press('Shift+Enter'); await settle();
    const sorts=await host.locator('[aria-sort]').evaluateAll(es=>es.filter(e=>e.getAttribute('aria-sort')!=='none').map(e=>[e.textContent,e.getAttribute('aria-sort')]));
    if(sorts.length!==2)throw new Error('secondary sort lost: '+JSON.stringify(sorts));
    const menu=head('id').locator('.mk-column-menu');
    await menu.locator('summary').focus(); await page.keyboard.press('Enter');
    await menu.locator('select').selectOption('ge');
    await menu.locator('input').fill('995000');
    await menu.locator('.mk-filter-apply').press('Enter'); await settle();
    await menu.locator('.mk-filter-apply').press('Escape');
    if(await menu.getAttribute('open')!==null)throw new Error('Escape did not close column filter');
    // Mouse filter path on a second column. This narrows to 500 rows.
    const bucketMenu=head('bucket').locator('.mk-column-menu');
    await bucketMenu.locator('summary').click();
    await bucketMenu.locator('select').selectOption('eq');
    await bucketMenu.locator('input').fill('9');
    await bucketMenu.locator('.mk-filter-apply').click(); await settle();
    await bucketMenu.locator('summary').click();
    if(await g.getAttribute('data-total')!=='500')throw new Error('AND filters did not reach source');
    const first=host.locator('.mk-table-row').first();
    const id=await first.locator('.mk-table-cell').nth(0).innerText();
    if(id!=='995009')throw new Error('multi sort/filter first id '+id);
    await first.locator('.mk-table-cell').nth(1).dblclick();
    await host.locator('.mk-cell-input').fill('8');
    await host.locator('.mk-cell-save').click();
    await host.locator('.mk-table-retained').waitFor();
    if(await first.locator('.mk-table-cell').nth(0).innerText()!==id)throw new Error('edited row vanished');
    await host.locator('.mk-table-undo').click();
    await page.waitForFunction(()=>![...document.querySelectorAll('.mk-table-retained')].find(e=>e.checkVisibility()));
    await page.screenshot({path:`${S}/table-grid-pushdown.png`});
    // Refresh the untouched default query: no filters, stable primary-key order.
    const firstPageStart=performance.now();
    await control(host.locator('.mk-table-query button').first(),'click');
    await page.waitForFunction(()=>[...document.querySelectorAll('.mk-table-grid')].find(e=>e.checkVisibility())?.dataset.total==='1000');
    const timings=[performance.now()-firstPageStart],requestTimings=[];let began;
    const start=req=>{if(req.url().includes('/api/sources/query_rows'))began=performance.now();};
    const finish=res=>{if(res.url().includes('/api/sources/query_rows')&&began!==undefined){requestTimings.push(performance.now()-began);began=undefined;}};
    page.on('request',start);page.on('response',finish);
    const cdp=await page.context().newCDPSession(page);await cdp.send('Performance.enable');
    const memory=[];let maxLoaded=0,maxDOM=0,pages=1;
    while(await g.getAttribute('data-has-next')==='true') {
      const pageStart=performance.now();
      const before=await g.evaluate(e=>+e.dataset.total + +e.dataset.skipped);
      await g.evaluate(e=>{e.scrollTop=e.scrollHeight;});
      await page.waitForFunction(before=>{const e=[...document.querySelectorAll('.mk-table-grid')].find(e=>e.checkVisibility());return e&&+e.dataset.total + +e.dataset.skipped>before&&e.dataset.loading==='false';},before,{timeout:15000});
      const state=await g.evaluate(e=>({loaded:+e.dataset.total,dom:e.querySelectorAll('.mk-table-row').length,through:+e.dataset.total + +e.dataset.skipped}));
      await page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>resolve())));
      timings.push(performance.now()-pageStart);
      maxLoaded=Math.max(maxLoaded,state.loaded);maxDOM=Math.max(maxDOM,state.dom);pages++;
      if(state.loaded>4000||state.dom>80)throw new Error('unbounded pages: '+JSON.stringify(state));
      if(pages%100===0){const metrics=await cdp.send('Performance.getMetrics');memory.push({pages,heap:metrics.metrics.find(m=>m.name==='JSHeapUsedSize')?.value,wasm:await page.evaluate(()=>window.__tableMemories.reduce((sum,m)=>sum+m.buffer.byteLength,0))});diagnostics.memory=memory;console.log(`  traversed ${state.through} rows`);}
      if(pages>1000)throw new Error('keyset did not end');
    }
    page.off('request',start);page.off('response',finish);
    const end=await g.evaluate(e=>+e.dataset.total + +e.dataset.skipped);
    if(end!==1000000||pages!==1000)throw new Error(`incomplete traversal: ${end} rows, ${pages} pages`);
    await g.evaluate(e=>{e.scrollTop=e.scrollHeight;});
    await page.waitForFunction(()=>[...document.querySelectorAll('.mk-table-row')].some(r=>r.querySelector('.mk-table-cell')?.textContent==='999999'));
    // Collect only for retained-heap measurement, after all page latency samples.
    await cdp.send('HeapProfiler.collectGarbage');
    const metrics=await cdp.send('Performance.getMetrics');
    const heap=metrics.metrics.find(m=>m.name==='JSHeapUsedSize')?.value;
    if(heap>128*1024*1024)throw new Error(`retained heap ${heap} exceeds 128 MiB`);
    timings.sort((a,b)=>a-b);requestTimings.sort((a,b)=>a-b);
    const wasm=await page.evaluate(()=>window.__tableMemories.reduce((sum,m)=>sum+m.buffer.byteLength,0));
    if(!wasm||wasm>512*1024*1024||wasm-memory[0].wasm>16*1024*1024)throw new Error('Wasm memory did not plateau: '+JSON.stringify({wasm,memory}));
    const report={requestP50:requestTimings[Math.floor(requestTimings.length*.5)],requestP95:requestTimings[Math.floor(requestTimings.length*.95)],requestTimings,wasm,rows:end,pages,maxLoaded,maxDOM,p50:timings[Math.floor(timings.length*.5)],p95:timings[Math.floor(timings.length*.95)],max:timings.at(-1),retainedHeap:heap,memory,timings};
    writeFileSync(`${S}/table-grid-million.json`,JSON.stringify(report,null,2));
    console.log('  '+JSON.stringify({...report,timings:undefined,requestTimings:undefined}));
    if(report.p95>2000||report.max>10000)throw new Error('unbounded page latency');
    await page.screenshot({path:`${S}/table-grid-million-end.png`});
  });
  await step("schema changes refuse an old-page merge and recover through Refresh", async () => {
    const source=page.locator('.mk-explorer-source').filter({has:page.locator('.mk-explorer-source-name',{hasText:'data.sqlite'})});
    await source.getByText('schema_rows',{exact:true}).click();
    const host=page.locator('.mk-table:visible');
    const g=host.locator('.mk-table-grid');
    await page.waitForFunction(()=>[...document.querySelectorAll('.mk-table-grid')].find(e=>e.checkVisibility())?.dataset.total==='1000');
    execFileSync('python3',['-c',"import sqlite3,sys; c=sqlite3.connect(sys.argv[1]); c.execute('ALTER TABLE schema_rows ADD COLUMN extra TEXT'); c.commit()",join(process.env.M1_ROOT,'data.sqlite')]);
    await host.locator('.mk-table-next').click();
    await host.getByRole('alert').filter({hasText:'The table columns changed. Refresh to load the new schema.'}).waitFor();
    if(await g.getAttribute('aria-colcount')!=='2')throw new Error('stale schema was merged');
    await page.screenshot({path:`${S}/table-grid-schema-change.png`});
    await host.locator('.mk-table-refresh').click();
    await page.waitForFunction(()=>[...document.querySelectorAll('.mk-table-grid')].find(e=>e.checkVisibility())?.getAttribute('aria-colcount')==='3');
    await host.locator('.mk-table-next').click();
    await page.waitForFunction(()=>[...document.querySelectorAll('.mk-table-grid')].find(e=>e.checkVisibility())?.dataset.total==='1500');
    if(await host.getByRole('alert').count())throw new Error('schema refresh did not clear the refusal');
  });
  console.log("\nTABLE GRID E2E: PASS");
} catch (e) {
  diagnostics.grid=await page.locator('.mk-table:visible .mk-table-grid').evaluate(e=>({dataset:{...e.dataset},meta:e.previousElementSibling?.textContent})).catch(()=>null);
  diagnostics.wasm=await page.evaluate(()=>window.__tableMemories?.reduce((sum,m)=>sum+m.buffer.byteLength,0)).catch(()=>null);
  writeFileSync(`${S}/table-grid-diagnostics.json`,JSON.stringify(diagnostics,null,2));
  console.log("\nFAIL:", e.message,JSON.stringify(diagnostics));
  await page.screenshot({ path: `${S}/table-grid-fail.png` }).catch(() => {});
  process.exitCode = 1;
} finally {
  await browser.close();
}
