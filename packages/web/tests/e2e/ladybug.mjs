// Milestone 3 step 5: a LadybugDB database in the folder → source, schema in the
// explorer, Cypher in the table editor, "Show in Graph" hands the result to the Graph panel.
import { firefox } from "playwright";
const S = process.env.M1_SHOTS ?? ".";
const browser = await firefox.launch();
const page = await browser.newPage({ viewport: { width: 1400, height: 900 } });
const logs = [];
page.on("console", (m) => { const t = m.text(); if (!t.includes("session[")) logs.push(`[${m.type()}] ${t.slice(0, 300)}`); });
const step = async (n, f) => { process.stdout.write(`- ${n} … `); await f(); console.log("ok"); };
try {
  await step("open folder; people.lbug is listed as a database", async () => {
    await page.goto(`http://127.0.0.1:${process.env.PORT ?? 8080}/`, { waitUntil: "networkidle" });
    await page.waitForSelector(".wb-workspace");
    await page.click(".mk-explorer-open button[type=submit]");
    await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
    await page.waitForSelector(".mk-tree-db >> text=people.lbug", { timeout: 10000 });
  });
  await step("clicking it opens a Ladybug source with node + rel tables", async () => {
    await page.click(".mk-tree-db >> text=people.lbug");
    await page.waitForSelector(".mk-explorer-source-name >> text=people.lbug", { timeout: 20000 });
    await page.waitForSelector(".mk-tree-row >> text=Person", { timeout: 10000 });
    const rows = await page.$$eval(".mk-explorer-source:has(.mk-explorer-source-name:has-text('people.lbug')) .mk-tree-row", (r) => r.map((x) => x.textContent.trim()));
    console.log("\n  tables:", rows.join(", "));
    if (!rows.includes("Knows (rel)")) throw new Error("rel table missing");
  });
  await step("Graph tab switched itself to the database's data (5 nodes, 4 relations, legend)", async () => {
    await page.click(".wb-tab:has-text('Graph')");
    await page.waitForSelector(".mk-graph-source", { timeout: 10000 });
    await page.waitForFunction(() => document.querySelector(".mk-graph-info")?.getAttribute("data-nodes") === "5", null, { timeout: 15000 });
    const sel = await page.$eval(".mk-graph-source", (s) => s.options[s.selectedIndex].textContent);
    if (sel !== "people.lbug") throw new Error(`picker is ${sel}`);
    const legend = await page.$$eval(".mk-graph-legend-item", (l) => l.map((x) => x.textContent.trim()));
    console.log("\n  info:", await page.$eval(".mk-graph-info", (e) => e.textContent.trim()), "| legend:", legend.join(", "));
    if (legend.join() !== "City,Person") throw new Error("legend");
  });
  await step("Schema mode draws tables and properties (6 nodes, 7 edges)", async () => {
    await page.click(".mk-graph-modes button:has-text('Schema')");
    await page.waitForFunction(() => document.querySelector(".mk-graph-info")?.getAttribute("data-nodes") === "6", null, { timeout: 15000 });
  });
  await step("Person opens the table editor with a Cypher default query", async () => {
    await page.click(".mk-tree-row >> text=Person");
    await page.waitForSelector(".mk-table-sql", { timeout: 15000 });
    const q = await page.$eval(".mk-table-sql", (t) => t.value);
    console.log("\n  query:", q);
    if (!/MATCH \(n:Person\)/.test(q)) throw new Error("expected Cypher default");
    await page.waitForSelector(".mk-table-grid tbody tr", { timeout: 15000 });
    const n = await page.$$eval(".mk-table-grid tbody tr", (r) => r.length);
    if (n !== 3) throw new Error(`expected 3 people, got ${n}`);
  });
  await step("a path query returns nodes+edges; Show in Graph appears", async () => {
    await page.fill(".mk-table-sql", "MATCH (a:Person)-[k]->(b) RETURN a, k, b");
    await page.click("button:has-text('Run')");
    await page.waitForFunction(() => /nodes · \d+ edges/.test(document.querySelector(".mk-table-meta")?.textContent || ""), null, { timeout: 15000 });
    console.log("\n  meta:", await page.$eval(".mk-table-meta", (e) => e.textContent.trim()));
    await page.waitForSelector(".mk-table-graph", { timeout: 5000 });
    await page.screenshot({ path: `${S}/m3-ladybug-table.png` });
  });
  await step("Show in Graph brings the Graph tab forward and draws the query (5 nodes, 4 edges)", async () => {
    await page.click(".mk-table-graph");
    await page.waitForFunction(() => [...document.querySelectorAll(".wb-tab[aria-selected=true]")].some((t) => t.textContent.includes("Graph")), null, { timeout: 10000 });
    await page.waitForSelector(".mk-graph-source", { timeout: 10000 });
    await page.waitForFunction(() => document.querySelector(".mk-graph-info")?.getAttribute("data-nodes") === "5", null, { timeout: 15000 });
    const sel = await page.$eval(".mk-graph-source", (s) => s.options[s.selectedIndex].textContent);
    const info = await page.$eval(".mk-graph-info", (e) => e.textContent.trim());
    console.log("\n  picker:", sel, "| info:", info);
    if (sel !== "people.lbug") throw new Error("picker not switched");
    await page.screenshot({ path: `${S}/m3-ladybug-graph.png` });
  });
  await step("Query mode is selected; Schema still available", async () => {
    const on = await page.$eval(".mk-graph-modes .mk-btn-on", (b) => b.textContent.trim());
    if (on !== "Query") throw new Error(`mode is ${on}`);
    await page.click(".mk-graph-modes button:has-text('Schema')");
    await page.waitForFunction(() => document.querySelector(".mk-graph-info")?.getAttribute("data-nodes") === "6", null, { timeout: 15000 });
  });
  await step("back to 'index' restores the folder graph", async () => {
    await page.selectOption(".mk-graph-source", "");
    await page.waitForFunction(() => document.querySelector(".mk-graph-check:has(input) ") && [...document.querySelectorAll(".mk-graph-check")].some((l) => l.textContent.includes("files")), null, { timeout: 10000 });
  });
  console.log("\nLADYBUG E2E: PASS");
} catch (e) { console.log("\nFAIL:", e.message); console.log(logs.slice(-10).join("\n")); await page.screenshot({ path: `${S}/m3-fail.png` }); process.exitCode = 1; } finally { await browser.close(); }
