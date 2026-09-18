// Milestone 4: Search panel (Ctrl+Shift+F, opens the file at the line) and traces drawn as graphs.
import { firefox } from "playwright";
const S = process.env.M1_SHOTS ?? ".";
const PORT = process.env.PORT ?? 8080;
const browser = await firefox.launch();
const page = await browser.newPage({ viewport: { width: 1500, height: 900 } });
const logs = [];
page.on("console", (m) => { const t = m.text(); if (!t.includes("session[")) logs.push(`[${m.type()}] ${t.slice(0, 300)}`); });
const step = async (n, f) => { process.stdout.write(`- ${n} … `); await f(); console.log("ok"); };
try {
  await step("open folder", async () => {
    await page.goto(`http://127.0.0.1:${PORT}/`, { waitUntil: "networkidle" });
    await page.waitForSelector(".wb-workspace");
    await page.click(".mk-explorer-open button[type=submit]");
    await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
  });
  await step("Ctrl+Shift+F brings the Search tab forward and focuses the box", async () => {
    await page.click(".mk-explorer");
    await page.keyboard.press("Control+Shift+F");
    await page.waitForFunction(() => [...document.querySelectorAll(".wb-tab[aria-selected=true]")].some((t) => t.textContent.includes("Search")), null, { timeout: 10000 });
    await page.waitForFunction(() => document.activeElement?.id === "mk-search-input", null, { timeout: 5000 });
  });
  await step("searching 'wrong String' lists src/main.rs at line 7; clicking opens the file there", async () => {
    await page.keyboard.type("wrong String");
    await page.keyboard.press("Enter");
    await page.waitForSelector(".mk-search-hit", { timeout: 15000 });
    const first = await page.$eval(".mk-search-hit", (e) => e.getAttribute("title"));
    console.log("\n  first hit:", first);
    if (first !== "src/main.rs:7") throw new Error("expected src/main.rs:7");
    await page.click(".mk-search-hit");
    await page.waitForFunction(() => document.querySelector(".mk-titlebar-title")?.textContent.startsWith("src/main.rs"), null, { timeout: 15000 });
    await page.waitForFunction(() => /let wrong: String/.test(document.querySelector(".cm-activeLine")?.textContent || ""), null, { timeout: 15000 });
    await page.screenshot({ path: `${S}/m4-search.png` });
  });
  await step("Graph → Trace… → paste a Rust panic → a trace source is drawn", async () => {
    await page.click(".wb-tab:has-text('Graph')");
    await page.click(".mk-graph-toolbar button:has-text('Trace')");
    await page.fill(".mk-graph-paste-text", "thread 'main' panicked at src/main.rs:7:5:\nboom\nstack backtrace:\n   0: m2root::add\n             at ./src/main.rs:1:1\n   1: m2root::main\n             at ./src/main.rs:6:22\n");
    await page.click(".mk-graph-paste-actions button:has-text('Draw')");
    await page.waitForFunction(() => /^trace:/.test(document.querySelector(".mk-graph-source")?.selectedOptions[0]?.textContent || ""), null, { timeout: 10000 });
    // root + 2 files (src/main.rs, ./src/main.rs) + 3 frames = 6 nodes; 2 root→file + 3 file→frame + 2 calls = 7 edges
    await page.waitForFunction(() => document.querySelector(".mk-graph-info")?.getAttribute("data-nodes") === "6", null, { timeout: 15000 });
    console.log("\n  info:", await page.$eval(".mk-graph-info", (e) => e.textContent.trim()));
    if (!/6 nodes · 7 edges/.test(await page.$eval(".mk-graph-info", (e) => e.textContent))) throw new Error("edge count");
  });
  console.log("\nSEARCH+TRACE E2E: PASS");
} catch (e) { console.log("\nFAIL:", e.message); console.log(logs.slice(-10).join("\n")); await page.screenshot({ path: `${S}/m4-fail.png` }); process.exitCode = 1; } finally { await browser.close(); }
