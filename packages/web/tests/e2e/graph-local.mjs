// Milestone 16 (Prompt26): the graph's Local view lays its neighbourhood out on its own instead
// of keeping the Whole layout's scattered positions, and switching back gives the Whole view
// back exactly — camera and node positions — as it was. Chromium with software WebGL (the
// renderer's state is read through window.moonkale.graphViews, like graph3d.mjs).
import { chromium } from "playwright";
const S = process.env.M1_SHOTS ?? ".";
const PORT = process.env.PORT ?? 8080;
const browser = await chromium.launch({ args: ["--use-angle=swiftshader", "--enable-unsafe-swiftshader", "--ignore-gpu-blocklist"] });
const ctx = await browser.newContext({ viewport: { width: 1400, height: 900 } });
const page = await ctx.newPage();
const logs = [];
page.on("console", (m) => { const t = m.text(); if (!t.includes("session[")) logs.push(`[${m.type()}] ${t.slice(0, 300)}`); });
const step = async (n, f) => { process.stdout.write(`- ${n} … `); await f(); console.log("ok"); };
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const view = (fn) => page.evaluate((fn) => { const v = window.moonkale.graphViews["mk-graph-panel"]; return fn === "layout" ? [...v.layout_state()] : fn === "camera" ? [...v.camera_state()] : v.node_count(); }, fn);
const nodes = () => page.$eval(".mk-graph-info", (e) => +e.dataset.nodes);
const settled = () => page.waitForFunction(() => { const v = window.moonkale?.graphViews?.["mk-graph-panel"]; return v && v.layout_state()[0] === 0; }, null, { timeout: 60000 });
const same = (a, b) => a.length === b.length && a.every((x, i) => Math.abs(x - b[i]) < 1e-3);
try {
  let wholeCount, wholeCamera, wholeLayout;
  await step("Whole view settles; the user zooms (the camera is theirs now)", async () => {
    await page.goto(`http://127.0.0.1:${PORT}/`, { waitUntil: "networkidle" });
    await page.waitForSelector(".wb-workspace");
    await page.click(".mk-explorer-open button[type=submit]");
    await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
    await page.click(".wb-tab:has-text('Graph')");
    await page.waitForFunction(() => +(document.querySelector(".mk-graph-info")?.dataset.nodes || 0) > 5, null, { timeout: 30000 });
    await settled();
    const box = await page.$eval(".mk-graph-overlay", (c) => { const r = c.getBoundingClientRect(); return { x: r.x + r.width / 2, y: r.y + r.height / 2 }; });
    await page.mouse.move(box.x, box.y);
    await page.mouse.wheel(0, -240);
    await sleep(600);
    wholeCount = await nodes();
    wholeCamera = await view("camera");
    wholeLayout = await view("layout");
    console.log("\n  whole:", wholeCount, "nodes, camera", wholeCamera.map((x) => x.toFixed(2)).join(","));
    await page.screenshot({ path: `${S}/m16-graph-whole.png` });
  });
  await step("Local around Home.md: a small graph, laid out from scratch", async () => {
    if (!(await page.isVisible(".mk-explorer"))) await page.click("#mk-rail-explorer"); // the rail entry toggles
    await page.click(".mk-tree-file >> text=Home.md");
    await page.waitForSelector(".mk-md", { timeout: 15000 });
    await page.click(".wb-tab:has-text('Graph')");
    await page.click(".mk-graph button:text-is('Local')");
    await page.waitForFunction((n) => { const x = +(document.querySelector(".mk-graph-info")?.dataset.nodes || 0); return x > 0 && x < n; }, wholeCount, { timeout: 15000 });
    const l = await view("layout");
    console.log("\n  local:", await nodes(), "nodes, temperature", l[1].toFixed(2), "iterations", l[2]);
    // A warm (incremental) reload starts at temperature 3; a fresh layout far above it.
    if (!(l[1] > 3.5 || l[2] > 0)) throw new Error("the local graph was not laid out fresh: " + l.join(","));
    await settled();
    const cam = await view("camera");
    if (same(cam, wholeCamera)) throw new Error("the camera was not fitted to the neighbourhood");
    await page.screenshot({ path: `${S}/m16-graph-local.png` });
  });
  await step("back to Whole: the same camera and the same positions, still settled", async () => {
    await page.click(".mk-graph button:text-is('Whole')");
    await page.waitForFunction((n) => +(document.querySelector(".mk-graph-info")?.dataset.nodes || 0) === n, wholeCount, { timeout: 15000 });
    await sleep(800);
    const cam = await view("camera");
    const lay = await view("layout");
    console.log("\n  camera", cam.map((x) => x.toFixed(2)).join(","), "layout", lay.map((x) => x.toFixed(1)).join(","));
    if (!same(cam, wholeCamera)) throw new Error(`camera not restored: ${cam} vs ${wholeCamera}`);
    if (!same(lay.slice(3), wholeLayout.slice(3))) throw new Error(`positions not restored: ${lay} vs ${wholeLayout}`);
    if (lay[0] !== 0) throw new Error("the restored layout started moving again");
    await page.screenshot({ path: `${S}/m16-graph-whole-again.png` });
  });
  console.log("\nGRAPH LOCAL E2E: PASS");
} catch (e) { console.log("\nFAIL:", e.message); console.log(logs.slice(-10).join("\n")); await page.screenshot({ path: `${S}/m16-graph-fail.png` }); process.exitCode = 1; } finally { await browser.close(); }
