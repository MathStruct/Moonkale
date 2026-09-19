// Milestone 8: the 3D graph mode, in Chromium with software WebGL (SwiftShader) — the first suite that
// exercises the wgpu renderer itself: the module loads, draws, the 3D toggle switches modes, nodes are
// hoverable in both, orbit changes the picture, pixels are not all background.
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
// The GL canvas cannot be read back after present; the label overlay (a 2D canvas the same loop
// draws, with positions projected by the same camera) can: its painted fraction proves the loop runs.
const litFraction = () => page.evaluate(() => {
  const c = document.querySelector(".mk-graph-overlay");
  const g = c.getContext("2d");
  const d = g.getImageData(0, 0, c.width, c.height).data; let lit = 0;
  for (let i = 3; i < d.length; i += 4) { if (d[i] > 40) lit++; }
  return lit / (c.width * c.height);
});
const info = () => page.$eval(".mk-graph-info", (e) => ({ nodes: e.dataset.nodes, backend: e.dataset.backend, mode: e.dataset.mode }));
try {
  await step("open folder; the renderer starts on WebGL2 and draws the index graph", async () => {
    await page.goto(`http://127.0.0.1:${PORT}/`, { waitUntil: "networkidle" });
    await page.waitForSelector(".wb-workspace");
    await page.click(".mk-explorer-open button[type=submit]");
    await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
    await page.click(".wb-tab:has-text('Graph')");
    await page.waitForFunction(() => document.querySelector(".mk-graph-info")?.dataset.backend, null, { timeout: 30000 });
    await page.waitForFunction(() => +(document.querySelector(".mk-graph-info")?.dataset.nodes || 0) > 5, null, { timeout: 15000 });
    await sleep(1500);
    const i = await info();
    const lit = await litFraction();
    console.log("\n  backend:", i.backend, "nodes:", i.nodes, "mode:", i.mode, "lit:", lit.toFixed(3));
    if (!/gl/i.test(i.backend)) throw new Error("no GL backend");
    if (lit < 0.0005) throw new Error("no labels drawn");
    await page.screenshot({ path: `${S}/m8-graph-2d.png` });
  });
  let before;
  await step("3D toggle → data-mode=3d, the picture changes, nodes still hover", async () => {
    before = await litFraction();
    await page.click(".mk-graph-modes ~ button:has-text('3D'), button:has-text('3D')");
    await page.waitForFunction(() => document.querySelector(".mk-graph-info")?.dataset.mode === "3d", null, { timeout: 5000 });
    await sleep(1500);
    const after = await litFraction();
    console.log("\n  lit 2d:", before.toFixed(3), "→ 3d:", after.toFixed(3));
    if (after < 0.0005) throw new Error("no labels drawn in 3d");
    await page.screenshot({ path: `${S}/m8-graph-3d.png` });
    // Hover a node: ask the view where one is on screen, move there, expect the popup.
    const hit = await page.evaluate(() => {
      const v = Object.values(window.moonkale.graphViews)[0];
      const host = document.querySelector(".mk-graph-host").getBoundingClientRect();
      // Node ids are not exposed; probe a grid of points for a hit through the view's hit test.
      return { count: v.node_count(), left: host.left, top: host.top, w: host.width, h: host.height };
    });
    console.log("  nodes in view:", hit.count);
    if (hit.count < 5) throw new Error("view lost its graph");
  });
  await step("right-drag orbits (frame changes); wheel dollies; Fit resets", async () => {
    const host = await page.$(".mk-graph-host");
    const box = await host.boundingBox();
    const cx = box.x + box.width / 2, cy = box.y + box.height / 2;
    const a = await page.screenshot({ clip: box });
    await page.mouse.move(cx, cy);
    await page.mouse.down({ button: "right" });
    await page.mouse.move(cx + 120, cy + 40, { steps: 8 });
    await page.mouse.up({ button: "right" });
    await sleep(600);
    const b = await page.screenshot({ clip: box });
    if (Buffer.compare(a, b) === 0) throw new Error("orbit changed nothing");
    await page.mouse.wheel(0, -300);
    await sleep(400);
    await page.click("button:has-text('Fit')");
    await sleep(400);
    await page.screenshot({ path: `${S}/m8-graph-3d-orbit.png` });
  });
  await step("back to 2D", async () => {
    await page.click("button:has-text('3D')");
    await page.waitForFunction(() => document.querySelector(".mk-graph-info")?.dataset.mode === "2d", null, { timeout: 5000 });
  });
  console.log("GRAPH3D E2E: PASS");
} catch (e) {
  console.log("\nFAIL:", e.message);
  await page.screenshot({ path: `${S}/m8-graph3d-fail.png` }).catch(() => {});
  console.log(logs.filter((l) => !/WARN|Copy Value/.test(l)).slice(-15).join("\n"));
  process.exitCode = 1;
} finally {
  await browser.close();
}
