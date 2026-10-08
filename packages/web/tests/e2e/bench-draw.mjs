// Spec 031, stage 0 measurement: the renderer's draw cost on a synthetic graph,
// in Chromium with software WebGL (SwiftShader) — the same setup as graph3d.mjs.
// Not part of the pass/fail suites; prints ms per drawn frame for a settling
// 100k-node graph and for the hover of a settled one. Uses the module's
// draw_stats() instrumentation (deltas), so the layout and label times are not
// included — only Renderer::draw.
import { chromium } from "playwright";
const PORT = process.env.PORT ?? 8080;
const N = Number(process.env.BENCH_N ?? 100000);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const browser = await chromium.launch({ args: ["--use-angle=swiftshader", "--enable-unsafe-swiftshader", "--ignore-gpu-blocklist"] });
const ctx = await browser.newContext({ viewport: { width: 1400, height: 900 } });
const page = await ctx.newPage();
try {
  await page.goto(`http://127.0.0.1:${PORT}/`, { waitUntil: "networkidle" });
  await page.waitForSelector(".wb-workspace");
  await page.click(".mk-explorer-open button[type=submit]");
  await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
  await page.click(".wb-tab:has-text('Graph')");
  await page.waitForFunction(() => /gl|gpu/i.test(document.querySelector(".mk-graph-info")?.dataset.backend || ""), null, { timeout: 30000 });
  const settle = await page.evaluate(async (n) => {
    const src = performance.getEntriesByType("resource").map((r) => r.name).find((u) => /graph_render.*\.js$/.test(u));
    const mod = await import(src);
    const v = Object.values(window.moonkale.graphViews)[0];
    // A deterministic synthetic graph: a tree (i -> i/7) plus every 5th node linked onward.
    const nodes = [], edges = [];
    for (let i = 0; i < n; i++) nodes.push({ id: "n" + i, label: "n" + i, kind: "file" });
    for (let i = 1; i < n; i++) {
      edges.push({ a: i, b: (i / 7) | 0 });
      if (i % 5 === 0) edges.push({ a: i, b: (i * 7919) % n });
    }
    const s0 = mod.draw_stats();
    v.set_graph(JSON.stringify({ nodes, edges }));
    const t0 = Date.now();
    while (v.layout_state()[0] > 0 && Date.now() - t0 < 180000) await new Promise((r) => setTimeout(r, 250));
    await new Promise((r) => setTimeout(r, 500));
    const s1 = mod.draw_stats();
    return { src, frames: s1[1] - s0[1], ms: s1[0] - s0[0], edges: edges.length };
  }, N);
  // Settled: hover moves redraw only the affected instances.
  const h0 = await page.evaluate(() => (async () => {
    const mod = await import(performance.getEntriesByType("resource").map((r) => r.name).find((u) => /graph_render.*\.js$/.test(u)));
    return mod.draw_stats();
  })());
  const box = await page.$eval(".mk-graph-overlay", (e) => { const r = e.getBoundingClientRect(); return { x: r.left, y: r.top, w: r.width, h: r.height }; });
  for (let i = 0; i < 60; i++) {
    await page.mouse.move(box.x + box.w * (0.2 + 0.6 * (i % 30) / 30), box.y + box.h * (0.3 + 0.4 * (i % 17) / 17));
    await sleep(33);
  }
  const h1 = await page.evaluate(() => (async () => {
    const mod = await import(performance.getEntriesByType("resource").map((r) => r.name).find((u) => /graph_render.*\.js$/.test(u)));
    return mod.draw_stats();
  })());
  console.log("draw bench (" + N + " nodes, " + settle.edges + " edges, SwiftShader):", JSON.stringify({
    settle_frames: settle.frames, settle_ms: +settle.ms.toFixed(0),
    settle_ms_per_frame: +(settle.ms / Math.max(1, settle.frames)).toFixed(2),
    hover_frames: h1[1] - h0[1], hover_ms: +(h1[0] - h0[0]).toFixed(1),
    hover_ms_per_frame: +((h1[0] - h0[0]) / Math.max(1, h1[1] - h0[1])).toFixed(2),
  }));
} catch (e) {
  console.log("FAIL:", e.message);
  process.exitCode = 1;
} finally {
  await browser.close();
}
