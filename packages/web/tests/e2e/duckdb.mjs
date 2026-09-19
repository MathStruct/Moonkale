// Milestone 9: DuckDB — a CSV in the folder opens its directory as a database of views; the table
// editor shows the rows; SQL joins two files; writes are refused.
import { firefox } from "playwright";
const S = process.env.M1_SHOTS ?? ".";
const PORT = process.env.PORT ?? 8080;
const browser = await firefox.launch();
const ctx = await browser.newContext({ viewport: { width: 1500, height: 900 } });
const page = await ctx.newPage();
const step = async (n, f) => { process.stdout.write(`- ${n} … `); await f(); console.log("ok"); };
try {
  await step("open folder; expand data/; people.csv is a database path", async () => {
    await page.goto(`http://127.0.0.1:${PORT}/`, { waitUntil: "networkidle" });
    await page.waitForSelector(".wb-workspace");
    await page.click(".mk-explorer-open button[type=submit]");
    await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
    await page.click(".mk-tree-dir >> text=data");
    await page.waitForSelector(".mk-tree-db >> text=people.csv", { timeout: 10000 });
  });
  await step("click people.csv → 'data/ (data files)' source with people and orders views", async () => {
    await page.click(".mk-tree-db >> text=people.csv");
    await page.waitForSelector(".mk-explorer-source-name:has-text('data/ (data files)')", { timeout: 20000 });
    await page.waitForSelector(".mk-tree-db >> text=people (view)", { timeout: 10000 });
    await page.waitForSelector(".mk-tree-db >> text=orders (view)", { timeout: 10000 });
  });
  await step("the people table shows 3 rows; a join across the two files runs; a write is refused", async () => {
    await page.click(".mk-tree-db >> text=people (view)");
    await page.waitForSelector(".mk-table-grid", { timeout: 15000 });
    const rows = await page.$$eval(".mk-table-grid tbody tr", (els) => els.length);
    const head = await page.$$eval(".mk-table-grid th", (els) => els.map((e) => e.textContent));
    console.log("\n  columns:", head.join(","), "rows:", rows);
    if (rows !== 3 || head.join() !== "name,age") throw new Error("grid");
    await page.fill(".mk-table-sql", "SELECT p.name, sum(o.qty) AS qty FROM people p JOIN orders o ON o.person = p.name GROUP BY p.name ORDER BY qty DESC");
    await page.click(".mk-table button");
    await page.waitForFunction(() => document.querySelectorAll(".mk-table-grid tbody tr").length === 2, null, { timeout: 10000 });
    const first = await page.$eval(".mk-table-grid tbody tr", (r) => r.textContent);
    console.log("  top:", first.trim());
    if (!first.includes("Ada")) throw new Error("join result");
    await page.fill(".mk-table-sql", "DELETE FROM people");
    await page.click(".mk-table button");
    await page.waitForSelector(".mk-table-error", { timeout: 10000 });
    await page.screenshot({ path: `${S}/m9-duckdb.png` });
  });
  console.log("DUCKDB E2E: PASS");
} catch (e) {
  console.log("\nFAIL:", e.message);
  await page.screenshot({ path: `${S}/m9-duckdb-fail.png` }).catch(() => {});
  process.exitCode = 1;
} finally {
  await browser.close();
}
