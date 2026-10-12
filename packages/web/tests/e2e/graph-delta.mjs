// Spec 031, stage 4a: delta updates on the real renderer — the graph3d.mjs
// setup (Chromium + SwiftShader). Checks: apply_delta removes and adds by
// instance id (a collapse: node b out, aggregate in, proxies to the
// outside), the camera and the id-keyed selection survive, an edge naming a
// missing node waits, and the reverse delta (an expand) restores the scene.
import { chromium } from "playwright";
import { decode } from "./pnglit.mjs";
const S = process.env.M1_SHOTS ?? ".";
const PORT = process.env.PORT ?? 8080;
const browser = await chromium.launch({ args: ["--use-angle=swiftshader", "--enable-unsafe-swiftshader", "--ignore-gpu-blocklist"] });
const ctx = await browser.newContext({ viewport: { width: 1400, height: 900 } });
const page = await ctx.newPage();
// Page-side shorthands over the one graph view; every helper exists only for
// this suite, defined before any page script runs.
await page.addInitScript(() => {
  const v = () => Object.values(window.moonkale.graphViews)[0];
  window.view_set = (json) => v().set_graph(json);
  window.view_delta = (json) => v().apply_delta(json);
  window.view_sel = () => v().selection_state();
  window.view_cam = () => Array.from(v().camera_state());
  window.view_count = () => [v().node_count(), v().edge_count()];
  window.view_pos = (id) => v().node_screen_position(id);
  window.view_layout_settled = () => v().layout_state()[0] === 0;
});
// The hue of a drawn kind — the pixel probe for "did the delta reach the
// screen" (the P-166 review's #1: counts and ids held, the picture did not).
const box = () => page.$eval(".mk-graph-overlay", (e) => { const r = e.getBoundingClientRect(); return { x: r.left, y: r.top, w: r.width, h: r.height }; });
const bluePx = async () => {
  // Park the pointer off the canvas first: a hovered node draws white
  // (radius ×1.4), and where the mouse happens to rest must not decide
  // which nodes count as their kind's colour.
  await page.mouse.move(2, 2);
  await sleep(200);
  const b = await box();
  const { width, height, bpp, data } = decode(await page.screenshot({
    clip: { x: b.x, y: b.y, width: b.w, height: b.h },
  }));
  let n = 0;
  for (let i = 0; i < width * height; i++) {
    const o = i * bpp;
    if (data[o + 2] > data[o] + 40 && data[o + 2] > data[o + 1] + 40) n++; // file blue
  }
  return n;
};
const logs = [];
page.on("console", (m) => { const t = m.text(); if (!t.includes("session[")) logs.push(`[${m.type()}] ${t.slice(0, 300)}`); });
const step = async (n, f) => { process.stdout.write(`- ${n} … `); await f(); console.log("ok"); };
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
// The view reports canvas-relative positions; clicks need viewport ones.
const at = async (p) => {
  const r = await page.$eval(".mk-graph-overlay", (e) => e.getBoundingClientRect());
  return [r.left + p[0], r.top + p[1]];
};
try {
  await step("open folder; the renderer starts on a GL backend", async () => {
    await page.goto(`http://127.0.0.1:${PORT}/`, { waitUntil: "networkidle" });
    await page.waitForSelector(".wb-workspace");
    await page.click(".mk-explorer-open button[type=submit]");
    await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
    await page.click(".wb-tab:has-text('Graph')");
    await page.waitForFunction(() => /gl|gpu/i.test(document.querySelector(".mk-graph-info")?.dataset.backend || ""), null, { timeout: 30000 });
  });
  await step("a three-node graph settles; a is selected", async () => {
    const graph = {
      nodes: ["a", "b", "c"].map((id) => ({ id, label: id, kind: "file" })),
      edges: [{ id: "ab", a: 0, b: 1 }, { id: "bc", a: 1, b: 2 }],
    };
    await page.evaluate((json) => view_set(json), JSON.stringify(graph));
    await page.waitForFunction(() => view_layout_settled(), null, { timeout: 20000 });
    await sleep(700);
    await page.mouse.click(...await at(await page.evaluate(() => view_pos("a"))));
    await sleep(300);
    const s = await page.evaluate(() => JSON.parse(view_sel()));
    if (s.nodes.join() !== "a") throw new Error("a not selected: " + JSON.stringify(s));
    // A user navigation first: with auto-fit still on, a warmed layout
    // re-fits by design while it runs (P-160) — the camera check below is
    // about the delta, which must never move a navigated view. The pan
    // starts in a corner, on empty space — the canvas centre holds a node,
    // and a drag there moves the node, not the view.
    const r = await page.$eval(".mk-graph-overlay", (e) => e.getBoundingClientRect());
    await page.mouse.move(r.left + 12, r.top + 12);
    await page.mouse.down();
    await page.mouse.move(r.left + 52, r.top + 32, { steps: 4 });
    await page.mouse.up();
    await sleep(300);
  });
  let blueAfter = 0;
  await step("a collapse delta replaces b with an aggregate and keeps the view", async () => {
    const blueBefore = await bluePx();
    const before = await page.evaluate(() => [view_count(), Array.from(view_cam()), view_pos("b")]);
    // b's position is where the aggregate will sit — in world
    // coordinates: `pos` is world, and b's screen point is pixels off the
    // overlay's centre.
    const o = await page.evaluate(() => {
      const r = document.querySelector(".mk-graph-overlay").getBoundingClientRect();
      return [r.width, r.height];
    });
    const [bx, by] = [
      before[1][1] + (before[2][0] - o[0] / 2) / before[1][0],
      before[1][2] + (before[2][1] - o[1] / 2) / before[1][0],
    ];
    await page.evaluate((json) => view_delta(json), JSON.stringify({
      remove_nodes: ["b"],
      add_nodes: [{ id: "agg", label: "group", kind: "directory", pos: [bx, by], pinned: true }],
      add_edges: [
        { id: "a-agg", a: "a", b: "agg" },
        { id: "agg-c", a: "agg", b: "c" },
      ],
    }));
    await sleep(400);
    const after = await page.evaluate(() => [view_count(), Array.from(view_cam()), view_pos("agg"), JSON.parse(view_sel())]);
    console.log("\n  counts:", JSON.stringify(after[0]), "selection:", after[3].nodes.join());
    // 3 nodes before, 3 after (b out, agg in); edges likewise (2 out, 2 in).
    if (after[0][0] !== before[0][0] || after[0][1] !== before[0][1]) {
      throw new Error("counts drifted: " + JSON.stringify({ before: before[0], after: after[0] }));
    }
    if (JSON.stringify(after[1]) !== JSON.stringify(before[1])) throw new Error("the delta moved the camera");
    if (!after[2]) throw new Error("the aggregate did not arrive");
    // The selection is by id: a survives the collapse untouched.
    if (after[3].nodes.join() !== "a") throw new Error("selection lost across the delta: " + JSON.stringify(after[3]));
    // The picture changed: b drew blue (a file), the aggregate draws grey
    // (a directory) — the blue count must drop by a node (the counts alone
    // proved nothing; the collapse keeps them).
    blueAfter = await bluePx();
    console.log("\n  blue px:", blueBefore, "→", blueAfter);
    if (blueAfter > blueBefore - 1500) throw new Error(`the collapse did not redraw: ${blueBefore} → ${blueAfter} blue px`);
    await page.screenshot({ path: `${S}/s4a-collapse.png` });
  });
  await step("an edge naming a missing node waits for it", async () => {
    const before = await page.evaluate(() => view_count());
    await page.evaluate((json) => view_delta(json), JSON.stringify({
      add_edges: [{ id: "early", a: "a", b: "ghost" }],
    }));
    await sleep(250);
    const after = await page.evaluate(() => view_count());
    if (after[1] !== before[1]) throw new Error("an edge to a missing node was added: " + JSON.stringify({ before, after }));
  });
  await step("the expand delta restores the scene", async () => {
    // Where the aggregate sits, in world coordinates — `pos` is world, and
    // the screen point alone put b far off-canvas (nothing redrew to hide
    // it, before the review's #1 fix).
    const p = await page.evaluate(() => view_pos("agg"));
    const c = await page.evaluate(() => Array.from(view_cam()));
    const b = await page.evaluate(() => { const r = document.querySelector(".mk-graph-overlay").getBoundingClientRect(); return [r.width, r.height]; });
    const wpos = [c[1] + (p[0] - b[0] / 2) / c[0], c[2] + (p[1] - b[1] / 2) / c[0]];
    await page.evaluate((json) => view_delta(json), JSON.stringify({
      remove_nodes: ["agg"],
      remove_edges: ["a-agg", "agg-c"],
      add_nodes: [{ id: "b", label: "b", kind: "file", pos: wpos }],
      add_edges: [{ id: "ab", a: "a", b: "b" }, { id: "bc", a: "b", b: "c" }],
    }));
    await sleep(400);
    const after = await page.evaluate(() => [view_count(), view_pos("b"), JSON.parse(view_sel())]);
    console.log("\n  counts:", JSON.stringify(after[0]), "selection:", after[2].nodes.join());
    if (after[0][0] !== 3 || after[0][1] !== 2) throw new Error("the expand did not restore: " + JSON.stringify(after[0]));
    if (!after[1]) throw new Error("b did not come back");
    if (after[2].nodes.join() !== "a") throw new Error("selection lost across the expand");
    const blueBack = await bluePx();
    if (blueBack < blueAfter + 1500) throw new Error(`the expand did not redraw: ${blueAfter} → ${blueBack} blue px`);
  });
  console.log("\nGRAPH-DELTA E2E: PASS");
} catch (e) {
  console.log("\nFAIL:", e.message);
  await page.screenshot({ path: `${S}/s4a-fail.png` }).catch(() => {});
  console.log(logs.filter((l) => !/WARN|Copy Value/.test(l)).slice(-15).join("\n"));
  process.exitCode = 1;
} finally {
  await browser.close();
}
