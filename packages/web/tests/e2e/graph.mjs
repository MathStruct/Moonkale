// Graph panel data path. Headless Firefox here has no WebGL/WebGPU, so the
// renderer is expected to report that clearly; on a real browser the same
// script sees a backend name instead and probes for a hover popup.
import { firefox } from "playwright";
const S = process.env.M1_SHOTS ?? ".";
const browser = await firefox.launch();
const page = await browser.newPage({ viewport: { width: 1400, height: 900 } });
const step = async (n, f) => { process.stdout.write(`- ${n} … `); await f(); console.log("ok"); };
try {
  await step("open folder → index is built and reported", async () => {
    await page.goto(`http://127.0.0.1:${process.env.PORT ?? 8080}/`, { waitUntil: "networkidle" });
    await page.waitForSelector(".wb-workspace");
    await page.click(".mk-explorer-open button[type=submit]");
    await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
  });
  await step("Graph tab: host queries the index and hands nodes/edges to the renderer", async () => {
    await page.click(".wb-tab[id^='wb-tab-graph']");
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
    await page.click(".mk-graph-check:has-text('unresolved') input");
    await page.waitForFunction((b) => Number(document.querySelector(".mk-graph-info").getAttribute("data-nodes")) === b - 1, before, { timeout: 10000 });
  });
  await step("save a markdown edit → index refresh → graph re-queried", async () => {
    // Phantoms are currently off (previous step). Turning them back on adds "Missing";
    // the saved [[Another]] link adds one more phantom: +2 in total.
    const before = Number(await page.$eval(".mk-graph-info", (e) => e.getAttribute("data-nodes")));
    await page.click(".mk-tree-file >> text=Home.md");
    await page.waitForSelector(".cm-content");
    await page.click(".cm-content"); await page.keyboard.press("End"); await page.keyboard.type(" and [[Another]]");
    await page.keyboard.press("Control+s");
    await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("Saved Home.md"), null, { timeout: 10000 });
    await page.click(".wb-tab[id^='wb-tab-graph']");
    await page.click(".mk-graph-check:has-text('unresolved') input");
    await page.waitForFunction((b) => Number(document.querySelector(".mk-graph-info").getAttribute("data-nodes")) === b + 2, before, { timeout: 15000 });
  });
  console.log("\nGRAPH E2E: PASS");
} catch (e) { console.log("\nFAIL:", e.message); process.exitCode = 1; } finally { await browser.close(); }
