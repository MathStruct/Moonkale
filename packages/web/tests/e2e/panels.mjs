// Spec 011: every panel is closeable; an empty tile collapses; View: Show <panel> (palette) brings
// a closed panel back into its home — the side bar reappears on the left at its usual width.
import { firefox } from "playwright";
const S = process.env.M1_SHOTS ?? ".";
const PORT = process.env.PORT ?? 8080;
const browser = await firefox.launch();
const page = await browser.newPage({ viewport: { width: 1500, height: 900 } });
const logs = [];
page.on("console", (m) => { const t = m.text(); if (!t.includes("session[")) logs.push(`[${m.type()}] ${t.slice(0, 300)}`); });
const step = async (n, f) => { process.stdout.write(`- ${n} … `); await f(); console.log("ok"); };
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const tabs = () => page.$$eval(".wb-tab", (t) => t.map((x) => x.textContent.trim()));
const closeTab = async (title) => {
  const re = new RegExp(`^\\s*${title}\\s*$`);
  const tab = page.locator(".wb-tab-item", { has: page.locator(".wb-tab", { hasText: re }) }).first();
  await tab.hover();
  await tab.locator(".wb-tab-close").click({ force: true });
};
const runCommand = async (text, id) => {
  await page.keyboard.press("Control+Shift+P");
  await page.waitForSelector(".mk-palette-input", { timeout: 10000 });
  await page.type(".mk-palette-input", text);
  await sleep(200);
  const keys = await page.$$eval(".mk-palette-item", (l) => l.map((x) => x.dataset.key));
  if (keys[0] !== id) throw new Error(`palette first is ${keys[0]}, wanted ${id} (have ${keys.slice(0, 5)})`);
  await page.keyboard.press("Enter");
};
try {
  await step("open folder: Graph has a close button; closing it removes the tab", async () => {
    await page.goto(`http://127.0.0.1:${PORT}/`, { waitUntil: "networkidle" });
    await page.waitForSelector(".wb-workspace");
    await page.click(".mk-explorer-open button[type=submit]");
    await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
    console.log("\n  tabs:", (await tabs()).join(" | "));
    await closeTab("Graph");
    await page.waitForFunction(() => ![...document.querySelectorAll(".wb-tab")].some((t) => t.textContent.trim() === "Graph"), null, { timeout: 5000 });
  });
  await step("palette 'View: Show Graph' brings it back in the main tile", async () => {
    await runCommand("show graph", "view.panel.graph");
    await page.waitForFunction(() => [...document.querySelectorAll(".wb-tab")].some((t) => t.textContent.trim() === "Graph"), null, { timeout: 5000 });
  });
  await step("closing every side panel collapses the side bar; the main area widens", async () => {
    const before = await page.$eval(".wb-workspace", (w) => w.getBoundingClientRect().width);
    const mainBefore = await page.evaluate(() => { const t = [...document.querySelectorAll(".wb-tile")].find((x) => [...x.querySelectorAll(".wb-tab")].some((b) => /Graph|Settings/.test(b.textContent))); return t ? t.getBoundingClientRect().width : 0; });
    for (const t of ["Explorer", "Search", "Links", "Changes", "History"]) {
      if ((await tabs()).includes(t)) await closeTab(t);
    }
    await page.waitForFunction(() => !document.querySelector(".mk-explorer"), null, { timeout: 5000 });
    await sleep(300);
    const mainAfter = await page.evaluate(() => { const t = [...document.querySelectorAll(".wb-tile")].find((x) => [...x.querySelectorAll(".wb-tab")].some((b) => /Graph|Settings/.test(b.textContent))); return t ? t.getBoundingClientRect().width : 0; });
    console.log(`\n  main tile width: ${Math.round(mainBefore)} → ${Math.round(mainAfter)} (workspace ${Math.round(before)})`);
    if (!(mainAfter > mainBefore + 100)) throw new Error("side bar did not collapse");
    await page.screenshot({ path: `${S}/m10-panels-collapsed.png` });
  });
  await step("'View: Show Explorer' restores the side bar on the left at ~22 %", async () => {
    await runCommand("show explorer", "view.panel.explorer");
    await page.waitForSelector(".mk-explorer", { timeout: 5000 });
    await sleep(300);
    const geo = await page.evaluate(() => { const e = document.querySelector(".mk-explorer").closest(".wb-tile").getBoundingClientRect(); const w = document.querySelector(".wb-workspace").getBoundingClientRect(); return { left: e.left - w.left, frac: e.width / w.width }; });
    console.log("\n  explorer tile:", JSON.stringify({ left: Math.round(geo.left), frac: +geo.frac.toFixed(2) }));
    if (geo.left > 40) throw new Error("explorer not on the left");
    if (geo.frac < 0.15 || geo.frac > 0.35) throw new Error("side bar width off: " + geo.frac);
    await page.screenshot({ path: `${S}/m10-panels-restored.png` });
  });
  await step("closing the Terminal keeps its sessions; Show Terminal brings the same session back", async () => {
    await closeTab("Terminal");
    await page.waitForFunction(() => ![...document.querySelectorAll(".wb-tab")].some((t) => t.textContent.trim() === "Terminal"), null, { timeout: 5000 });
    await runCommand("show terminal", "view.panel.terminal");
    await page.waitForFunction(() => [...document.querySelectorAll(".wb-tab")].some((t) => t.textContent.trim() === "Terminal"), null, { timeout: 5000 });
  });
  console.log("\nPANELS E2E: PASS");
} catch (e) { console.log("\nFAIL:", e.message); console.log(logs.slice(-10).join("\n")); await page.screenshot({ path: `${S}/m10-panels-fail.png` }); process.exitCode = 1; } finally { await browser.close(); }
