// The table editor's virtualised grid: a 50 000-row result (a recursive
// query on the fixture's data.sqlite — the source is read-only, a CTE is
// a read) keeps only its window in the DOM, scrolls to the end, sorts,
// filters, moves a keyboard cursor that scrolls the cell into view,
// resizes a column, and draws only the columns under a 60-column result.
import { chromium } from "playwright";
const S = process.env.M1_SHOTS ?? ".";
const PORT = process.env.PORT ?? 8080;
const N = 50000;
const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1400, height: 900 } });
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
  await page.fill(".mk-table:visible .mk-table-sql", sql);
  await page.click(".mk-table:visible .mk-table-query button");
};
try {
  await step("open folder, data.sqlite, the users table", async () => {
    await page.goto(`http://127.0.0.1:${PORT}/`, { waitUntil: "networkidle" });
    await page.waitForSelector(".wb-workspace");
    await page.click(".mk-explorer-open button[type=submit]");
    await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
    await page.click(".mk-tree-db >> text=data.sqlite");
    await page.waitForSelector(".mk-tree-db >> text=users", { timeout: 15000 });
    await page.click(".mk-tree-db >> text=users");
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
    await page.click(".mk-table:visible .mk-table-hcell:has(.mk-table-hname:text-is('v'))");
    await page.click(".mk-table:visible .mk-table-hcell:has(.mk-table-hname:text-is('v'))");
    await sleep(400);
    const cells = await firstRowCells();
    console.log(`\n  first row by v desc: ${cells.join(" | ")}`);
    if (cells[1] !== "999") throw new Error(`v desc starts at ${cells[1]}`);
    const sortAttr = await page.$eval(".mk-table:visible .mk-table-hcell:has(.mk-table-hname:text-is('v'))", (h) => h.getAttribute("aria-sort"));
    if (sortAttr !== "descending") throw new Error(`aria-sort ${sortAttr}`);
    // half descending: NULL rows (every tenth) at the end, not the start.
    await page.click(".mk-table:visible .mk-table-hcell:has(.mk-table-hname:text-is('half'))");
    await page.click(".mk-table:visible .mk-table-hcell:has(.mk-table-hname:text-is('half'))");
    await sleep(300);
    const top = await firstRowCells();
    if (top[3] !== "24999.5") throw new Error(`half desc starts at ${top[3]}`);
  });
  await step("the filter narrows the rows and the count says so", async () => {
    await page.fill(".mk-table:visible .mk-table-filter", "row 4999");
    await page.waitForFunction(() => +document.querySelector(".mk-table-grid").dataset.rows === 11, null, { timeout: 5000 });
    const meta = await page.$eval(".mk-table:visible .mk-table-meta", (m) => m.textContent);
    console.log(`\n  meta: ${meta.trim()}`);
    if (!/11 of 50[,. ]?000 rows/.test(meta)) throw new Error("the meta line does not count the filtered rows");
    await page.fill(".mk-table:visible .mk-table-filter", "");
    await page.waitForFunction((n) => +document.querySelector(".mk-table-grid").dataset.rows === n, N, { timeout: 5000 });
  });
  await step("the keyboard cursor: Ctrl+End jumps to the last cell and scrolls it into view", async () => {
    await page.click(".mk-table:visible .mk-table-row >> nth=0 >> .mk-table-cell >> nth=0");
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
    await page.click(".mk-tree-db >> text=typed.duckdb");
    await page.waitForSelector(".mk-tree-db >> text=dates", {timeout:15000});
    await page.locator(".mk-explorer-source").filter({has:page.locator(".mk-explorer-source-name", {hasText:"typed.duckdb"})}).getByText("dates", {exact:true}).click();
    await page.waitForSelector(".mk-table:visible .mk-cell-vector", {timeout:15000});
    const lines = await page.locator(".mk-table:visible .mk-cell-vector polyline").evaluateAll(es => es.map(e => e.getAttribute("points")));
    if (lines.length !== 2 || lines.some(p => !p || p.includes("NaN"))) throw new Error("missing vector sparklines: " + lines);
    const cells = await firstRowCells();
    if (!cells[0].includes("2026-10-23 08:30:00 UTC")) throw new Error("missing DuckDB date: " + cells);
  });
  console.log("\nTABLE GRID E2E: PASS");
} catch (e) {
  console.log("\nFAIL:", e.message);
  await page.screenshot({ path: `${S}/table-grid-fail.png` }).catch(() => {});
  process.exitCode = 1;
} finally {
  await browser.close();
}
