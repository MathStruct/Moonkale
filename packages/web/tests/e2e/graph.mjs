import { control } from './chrome-controls.mjs';
// Graph panel: production payloads and the renderer using CPU SwiftShader.
import { chromium } from "playwright";
const S = process.env.M1_SHOTS ?? ".";
const browser = await chromium.launch({args:["--use-angle=swiftshader","--enable-unsafe-swiftshader","--ignore-gpu-blocklist"]});
const page = await browser.newPage({ viewport: { width: 1400, height: 900 } });
const step = async (n, f) => { process.stdout.write(`- ${n} … `); await f(); console.log("ok"); };
let unresolvedHidden = 0;
try {
  await step("open folder → index is built and reported", async () => {
    await page.goto(`http://127.0.0.1:${process.env.PORT ?? 8080}/`, { waitUntil: "networkidle" });
    await page.waitForSelector(".wb-workspace");
    await control(page, 'click', ".mk-explorer-open button[type=submit]");
    await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
  });
  await step("Graph tab: host queries the index and hands nodes/edges to the renderer", async () => {
    await control(page, 'click', ".wb-tab[id^='wb-tab-graph']");
    await page.waitForFunction(() => Number(document.querySelector(".mk-graph-info")?.getAttribute("data-nodes")) > 0, null, { timeout: 15000 });
    console.log("\n  info:", await page.$eval(".mk-graph-info", (e) => e.textContent.trim()));
  });
  await step("renderer module (JS + wasm assets) loads", async () => {
    await page.waitForFunction(() => document.querySelector(".mk-graph-info")?.getAttribute("data-module") === "loaded", null, { timeout: 30000 });
  });
  await step("renderer either starts (backend) or fails legibly (no GPU API)", async () => {
    await page.waitForFunction(() => (document.querySelector(".mk-graph-info")?.getAttribute("data-backend") || "").length > 0 || !!document.querySelector(".mk-graph-error"), null, { timeout: 20000 });
    const backend = await page.$eval(".mk-graph-info", (e) => e.getAttribute("data-backend"));
    const err = await page.$eval(".mk-graph-error", (e) => e.textContent).catch(() => null);
    console.log("\n  backend:", backend || "-", "| error:", err || "-");
    if (backend) {
      const box = await (await page.$(".mk-graph-host")).boundingBox();
      let found = false;
      outer: for (let gy = 0.15; gy <= 0.85; gy += 0.05) for (let gx = 0.15; gx <= 0.85; gx += 0.05) {
        await page.mouse.move(box.x + box.width * gx, box.y + box.height * gy); await page.waitForTimeout(25);
        if (await page.$(".mk-graph-popup")) { found = true; break outer; }
      }
      console.log("  hover popup:", found ? await page.$eval(".mk-graph-popup-title", (e) => e.textContent) : "not found");
    }
    await page.screenshot({ path: `${S}/m2-graph.png` });
  });
  await step("unresolved toggle changes the node count (filters reach the query)", async () => {
    const before = Number(await page.$eval(".mk-graph-info", (e) => e.getAttribute("data-nodes")));
    await control(page, 'click', ".mk-graph-check:has-text('unresolved') input");
    await page.waitForFunction((b) => Number(document.querySelector(".mk-graph-info").getAttribute("data-nodes")) < b, before, { timeout: 10000 });
    unresolvedHidden = before - Number(await page.$eval(".mk-graph-info", (e) => e.getAttribute("data-nodes")));
  });
  await step("save a markdown edit → index refresh → graph re-queried", async () => {
    // Restore the previous unresolved nodes plus the newly saved wiki target.
    const before = Number(await page.$eval(".mk-graph-info", (e) => e.getAttribute("data-nodes")));
    await control(page, 'click', ".mk-tree-file >> text=Home.md");
    await page.waitForSelector(".cm-content");
    await control(page, 'click', ".cm-content"); await page.keyboard.press("End"); await page.keyboard.type(" and [[Another]]");
    await page.keyboard.press("Control+s");
    await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("Saved Home.md"), null, { timeout: 10000 });
    await control(page, 'click', ".wb-tab[id^='wb-tab-graph']");
    await control(page, 'click', ".mk-graph-check:has-text('unresolved') input");
    await page.waitForFunction(({b, hidden}) => Number(document.querySelector(".mk-graph-info").getAttribute("data-nodes")) === b + hidden + 1, { b: before, hidden: unresolvedHidden }, { timeout: 15000 });
  });
  await step("spec 020: a second folder joins the same graph, coloured, and the picker narrows to one", async () => {
    const before = Number(await page.$eval(".mk-graph-info", (e) => e.getAttribute("data-nodes")));
    await control(page, 'fill', ".mk-explorer-open input.mk-input", "notes");
    await control(page, 'click', ".mk-explorer-open button[type=submit]");
    await page.waitForFunction((b) => Number(document.querySelector(".mk-graph-info")?.getAttribute("data-nodes")) > b, before, { timeout: 30000 });
    const options = await page.$$eval(".mk-graph-source option", (o) => o.map((x) => x.textContent.trim()));
    const legend = await page.$$eval(".mk-graph-legend-item", (l) => l.map((x) => x.textContent.trim()));
    console.log("\n  picker:", options.join(" | "), "· legend:", legend.join(", "));
    if (options[0] !== "all folders" || !options.some((o) => o === "folder: notes")) throw new Error("picker lacks the folders");
    if (legend.length !== 2) throw new Error("expected one legend entry per folder");
    const all = Number(await page.$eval(".mk-graph-info", (e) => e.getAttribute("data-nodes")));
    await control(page, 'selectOption', ".mk-graph-source", { label: "folder: notes" });
    await page.waitForFunction((n) => { const v = Number(document.querySelector(".mk-graph-info")?.getAttribute("data-nodes")); return v > 0 && v < n; }, all, { timeout: 15000 });
    console.log("  all folders:", all, "· notes only:", await page.$eval(".mk-graph-info", (e) => e.getAttribute("data-nodes")));
  });
  console.log("\nGRAPH E2E: PASS");
} catch (e) { console.log("\nFAIL:", e.message); process.exitCode = 1; } finally { await browser.close(); }
