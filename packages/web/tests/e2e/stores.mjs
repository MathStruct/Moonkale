// Milestone 17: the embedded stores from the Sources tree — a Turso file, a redb file, a RocksDB
// directory and a HelixDB directory (fixture: stores/, seeded by seed_stores) open as sources,
// list their tables/labels, and answer in the table editor in their own dialect (sql, kv, helix).
import { firefox } from "playwright";
const S = process.env.M1_SHOTS ?? ".";
const PORT = process.env.PORT ?? 8080;
const browser = await firefox.launch();
const page = await browser.newPage({ viewport: { width: 1500, height: 900 } });
const logs = [];
page.on("console", (m) => { const t = m.text(); if (!t.includes("session[")) logs.push(`[${m.type()}] ${t.slice(0, 300)}`); });
const step = async (n, f) => { process.stdout.write(`- ${n} … `); await f(); console.log("ok"); };
const showSources = async () => { if (!(await page.isVisible(".mk-explorer"))) await page.click("#mk-rail-explorer"); };
const grid = async () => ({
  head: await page.$$eval(".mk-table:visible .mk-table-grid th", (e) => e.map((x) => x.textContent.trim())),
  rows: await page.$$eval(".mk-table:visible .mk-table-grid tbody tr", (e) => e.map((r) => [...r.querySelectorAll("td")].map((c) => c.textContent.trim()))),
});
const run = async (q, rows) => {
  await page.fill(".mk-table:visible .mk-table-sql", q);
  await page.click(".mk-table:visible button");
  await page.waitForFunction((n) => [...document.querySelectorAll(".mk-table")].some((t) => t.offsetParent && t.querySelectorAll(".mk-table-grid tbody tr").length === n), rows, { timeout: 10000 });
};
/** Click a store in the tree; wait for its source header and one child. */
const openStore = async (file, name, child) => {
  await showSources();
  await page.click(`.mk-tree-row[title='stores/${file}']`);
  await page.waitForSelector(`.mk-explorer-source-name:has-text('${name}')`, { timeout: 20000 });
  await page.waitForSelector(`.mk-tree-db >> text=${child}`, { timeout: 15000 });
};
try {
  await step("open the folder; the four stores are database rows, not folders", async () => {
    await page.goto(`http://127.0.0.1:${PORT}/`, { waitUntil: "networkidle" });
    await page.waitForSelector(".wb-workspace");
    await page.click(".mk-explorer-open button[type=submit]");
    await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
    await page.click(".mk-tree-row[title='stores']");
    for (const f of ["shop.turso", "app.redb", "events.rocksdb", "people.helix"]) {
      await page.waitForSelector(`.mk-tree-row.mk-tree-db[title='stores/${f}']`, { timeout: 10000 });
    }
  });
  await step("Turso: tables and a view; users in the grid; SQL reads, a write is refused", async () => {
    await openStore("shop.turso", "shop.turso", "users");
    await page.waitForSelector(".mk-tree-db >> text=good (view)", { timeout: 10000 });
    await page.click(".mk-tree-db >> text=users");
    await page.waitForSelector(".mk-table:visible .mk-table-grid", { timeout: 15000 });
    const g = await grid();
    if (g.head.join() !== "id,name,score" || g.rows.length !== 3) throw new Error(JSON.stringify(g));
    await run("SELECT name FROM users WHERE score > 3 ORDER BY name", 2);
    await page.fill(".mk-table:visible .mk-table-sql", "DELETE FROM users");
    await page.click(".mk-table:visible button");
    await page.waitForSelector(".mk-table:visible .mk-table-error", { timeout: 10000 });
  });
  await step("redb: typed tables with counts; kv scan and get", async () => {
    await openStore("app.redb", "app.redb", "settings · 2");
    await page.click(".mk-tree-db >> text=settings · 2");
    await page.waitForSelector(".mk-table:visible .mk-table-grid", { timeout: 15000 });
    const q = await page.inputValue(".mk-table:visible .mk-table-sql");
    if (!/^scan settings limit 200$/.test(q)) throw new Error("default query: " + q);
    const g = await grid();
    if (g.head.join() !== "key,value" || JSON.stringify(g.rows) !== JSON.stringify([["font", "Inter"], ["theme", "dark"]])) throw new Error(JSON.stringify(g));
    await run("get settings theme", 1);
    await run("scan scores", 2);
    console.log("\n  scores:", JSON.stringify((await grid()).rows));
  });
  await step("RocksDB: column families; prefix scan", async () => {
    await openStore("events.rocksdb", "events.rocksdb", "/^events · /");
    await page.click(".mk-tree-db >> text=/^events · /");
    await page.waitForSelector(".mk-table:visible .mk-table-grid", { timeout: 15000 });
    if ((await grid()).rows.length !== 3) throw new Error(JSON.stringify(await grid()));
    await run("scan events prefix e:002", 1);
  });
  await step("HelixDB: node and edge labels; a label's vertices and an edge's properties", async () => {
    await openStore("people.helix", "people.helix", "Person · 2");
    await page.waitForSelector(".mk-tree-db >> text=KNOWS (edge) · 1", { timeout: 10000 });
    await page.click(".mk-tree-db >> text=Person · 2");
    await page.waitForSelector(".mk-table:visible .mk-table-grid", { timeout: 15000 });
    const g = await grid();
    if (g.head.join() !== "$id,$label,name" || g.rows.length !== 2) throw new Error(JSON.stringify(g));
    await run("edges KNOWS", 1);
    const e = await grid();
    if (!e.head.includes("since") || !e.rows[0].includes("1843")) throw new Error(JSON.stringify(e));
    await page.screenshot({ path: `${S}/m17-stores.png` });
  });
  console.log("\nSTORES E2E: PASS");
} catch (e) { console.log("\nFAIL:", e.message); console.log(logs.slice(-10).join("\n")); await page.screenshot({ path: `${S}/m17-stores-fail.png` }); process.exitCode = 1; } finally { await browser.close(); }
