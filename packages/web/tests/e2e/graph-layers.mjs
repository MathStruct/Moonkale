// Spec 031, stage 3: layers and traces on the real renderer — the graph3d.mjs
// setup (Chromium + SwiftShader). Acceptance §8.3: a blue call-graph layer and
// a yellow stack-trace layer toggle independently; the edge they share draws
// both (offset parallel strokes); the trace is an ordered walk whose recursion
// keeps occurrence order; hiding a layer removes its strokes and its walk.
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
const v = () => page.evaluate(() => Object.values(window.moonkale.graphViews)[0]);
try {
  await step("open folder; the renderer starts on a GL backend", async () => {
    await page.goto(`http://127.0.0.1:${PORT}/`, { waitUntil: "networkidle" });
    await page.waitForSelector(".wb-workspace");
    await page.click(".mk-explorer-open button[type=submit]");
    await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
    await page.click(".wb-tab:has-text('Graph')");
    await page.waitForFunction(() => /gl|gpu/i.test(document.querySelector(".mk-graph-info")?.dataset.backend || ""), null, { timeout: 30000 });
  });
  await step("a call graph with a trace settles; both layers draw", async () => {
    const graph = {
      nodes: ["f", "g", "h"].map((id) => ({ id, label: id, kind: "symbol" })),
      edges: [
        { id: "fg", a: 0, b: 1, layers: ["calls", "trace"] }, // shared: both layers
        { id: "gh", a: 1, b: 2, layers: ["calls"] },
      ],
      layers: [
        { id: "calls", name: "Function calls", color: "#7aa2f7", width: 1.4 },
        { id: "trace", name: "Stack trace", color: "#e0af68", overlay: true, width: 2.0 },
      ],
      traces: [
        // f→g→h→g→f: h recurses nothing, g and f are revisited on the way
        // back — the walk's order survives (spec 031 §3).
        { id: "stack", layer: "trace", steps: ["f", "g", "h", "g", "f"] },
      ],
    };
    await page.evaluate((json) => Object.values(window.moonkale.graphViews)[0].set_graph(json), JSON.stringify(graph));
    await page.waitForFunction(() => Object.values(window.moonkale.graphViews)[0].layout_state()[0] === 0, null, { timeout: 20000 });
    await sleep(700);
    const st = await page.evaluate(() => [
      JSON.parse(Object.values(window.moonkale.graphViews)[0].layer_state()),
      Object.values(window.moonkale.graphViews)[0].frame_state(),
    ]);
    console.log("\n  layers:", JSON.stringify(st[0]), "frame:", JSON.stringify(st[1]));
    // The shared edge draws twice (8 curve segments per bowed stroke), the
    // plain call edge once, and the walk adds its 4 hops with 4 heads.
    if (st[1][0] !== 2 * 8 + 1 + 4) throw new Error("segment count wrong: " + JSON.stringify(st[1]));
    if (st[1][1] !== 4) throw new Error("the trace's hop heads are missing: " + JSON.stringify(st[1]));
    if (st[0].length !== 2 || !st[0].every((l) => l.visible)) throw new Error("layer state: " + JSON.stringify(st[0]));
    await page.screenshot({ path: `${S}/s3-layers.png` });
  });
  await step("hiding the trace layer removes its strokes and its walk", async () => {
    await page.evaluate(() => Object.values(window.moonkale.graphViews)[0].set_layer_visibility("trace", false));
    await sleep(300);
    const st = await page.evaluate(() => [
      JSON.parse(Object.values(window.moonkale.graphViews)[0].layer_state()),
      Object.values(window.moonkale.graphViews)[0].frame_state(),
    ]);
    console.log("\n  without the trace:", JSON.stringify(st[1]));
    // The shared edge keeps its calls stroke (straight now — one stroke
    // needs no bow), the call edge stays, the walk and its heads are gone.
    if (st[1][0] !== 2 || st[1][1] !== 0) throw new Error("hiding the layer left too much: " + JSON.stringify(st[1]));
    const trace = st[0].find((l) => l.id === "trace");
    if (trace?.visible !== false) throw new Error("layer_state did not record it: " + JSON.stringify(st[0]));
  });
  await step("the calls layer hides independently; topology is untouched", async () => {
    await page.evaluate(() => Object.values(window.moonkale.graphViews)[0].set_layer_visibility("calls", false));
    await sleep(300);
    const fr = await page.evaluate(() => Object.values(window.moonkale.graphViews)[0].frame_state());
    console.log("\n  everything hidden:", JSON.stringify(fr));
    // Both edges live in (only) these layers — nothing draws; the graph
    // itself still holds two edges and three nodes.
    if (fr[0] !== 0) throw new Error("edges remain: " + JSON.stringify(fr));
    // Back on: both layers again.
    await page.evaluate(() => {
      const v0 = Object.values(window.moonkale.graphViews)[0];
      v0.set_layer_visibility("calls", true);
      v0.set_layer_visibility("trace", true);
    });
    await sleep(300);
    const fr2 = await page.evaluate(() => Object.values(window.moonkale.graphViews)[0].frame_state());
    if (fr2[0] !== 21) throw new Error("restoring the layers changed the count: " + JSON.stringify(fr2));
  });
  console.log("\nGRAPH-LAYERS E2E: PASS");
} catch (e) {
  console.log("\nFAIL:", e.message);
  await page.screenshot({ path: `${S}/s3-layers-fail.png` }).catch(() => {});
  console.log(logs.filter((l) => !/WARN|Copy Value/.test(l)).slice(-15).join("\n"));
  process.exitCode = 1;
} finally {
  await browser.close();
}
