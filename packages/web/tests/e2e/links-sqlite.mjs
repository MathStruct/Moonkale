import { firefox } from "playwright";
const S = process.env.M1_SHOTS ?? ".";
const browser = await firefox.launch();
const page = await browser.newPage({ viewport: { width: 1400, height: 900 } });
const step = async (n, f) => { process.stdout.write(`- ${n} … `); await f(); console.log("ok"); };
try {
  await step("open folder", async () => {
    await page.goto("http://127.0.0.1:8080/", { waitUntil: "networkidle" });
    await page.waitForSelector(".wb-workspace");
    await page.click(".mk-explorer-open button[type=submit]");
    await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
  });
  await step("Links panel: Home.md has 1 backlink and 3 outgoing (one unresolved)", async () => {
    await page.click(".mk-tree-file >> text=Home.md");
    await page.waitForSelector(".cm-content");
    await page.click(".wb-tab[id^='wb-tab-links']");
    await page.waitForSelector(".mk-links-title");
    await page.waitForFunction(() => document.querySelectorAll(".mk-links-section")[0]?.querySelectorAll(".mk-links-item").length === 1, null, { timeout: 10000 });
    const out = await page.evaluate(() => [...document.querySelectorAll(".mk-links-section")[1].querySelectorAll(".mk-links-item .mk-links-label")].map((e) => e.textContent));
    if (out.length !== 3 || !out.includes("Missing")) throw new Error(`outgoing: ${JSON.stringify(out)}`);
    const phantom = await page.$$eval(".mk-links-phantom", (els) => els.length);
    if (phantom !== 1) throw new Error(`phantom count ${phantom}`);
    await page.screenshot({ path: `${S}/m2-links.png` });
  });
  await step("clicking a backlink opens that file", async () => {
    await page.click(".mk-links-section >> nth=0 >> .mk-links-item");
    await page.waitForFunction(() => document.querySelector(".mk-titlebar-title").textContent.startsWith("Alpha.md"), null, { timeout: 10000 });
  });
  await step("click data.sqlite in the explorer → database source with tables", async () => {
    await page.click(".wb-tab[id^='wb-tab-explorer']");
    await page.click(".mk-tree-db >> text=data.sqlite");
    await page.waitForSelector(".mk-explorer-source-name:has-text('data.sqlite')", { timeout: 15000 });
    await page.waitForSelector(".mk-tree-db >> text=users", { timeout: 10000 });
  });
  await step("click the users table → table panel with 3 rows", async () => {
    await page.click(".mk-tree-db >> text=users");
    await page.waitForSelector(".mk-table-grid", { timeout: 15000 });
    const rows = await page.$$eval(".mk-table-grid tbody tr", (els) => els.length);
    const head = await page.$$eval(".mk-table-grid th", (els) => els.map((e) => e.textContent));
    if (rows !== 3 || head.join() !== "id,name,score") throw new Error(`rows=${rows} head=${head}`);
    const nulls = await page.$$eval(".mk-cell-null", (els) => els.length);
    if (nulls !== 1) throw new Error(`nulls ${nulls}`);
  });
  await step("custom SQL runs; a write is refused with a clear message", async () => {
    await page.fill(".mk-table-sql", "SELECT name FROM users WHERE score > 2");
    await page.click(".mk-table button");
    await page.waitForFunction(() => document.querySelectorAll(".mk-table-grid tbody tr").length === 1, null, { timeout: 10000 });
    await page.fill(".mk-table-sql", "DELETE FROM users");
    await page.click(".mk-table button");
    await page.waitForSelector(".mk-table-error", { timeout: 10000 });
    console.log("\n  refusal:", await page.$eval(".mk-table-error", (e) => e.textContent.trim()));
    await page.screenshot({ path: `${S}/m2-table.png` });
  });
  console.log("\nLINKS+SQLITE E2E: PASS");
} catch (e) { console.log("\nFAIL:", e.message); await page.screenshot({ path: `${S}/m2-fail.png` }); process.exitCode = 1; } finally { await browser.close(); }
