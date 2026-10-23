// Spec 031, stage 4: groups and LOD on the real renderer — Chromium with
// software WebGL (SwiftShader), the graph3d.mjs setup. Checks: nested and
// overlapping groups tint as hue-distinct regions (pixel checks — a hull's
// tint is dimmer than any lit threshold), a collapse/expand driven through
// apply_delta keeps the external edges, the camera and the id-keyed
// selection and re-shapes the hulls, focus() centres on a subset, and the
// LOD gates land: hulls and ports fade out at low zoom, a dashed curve goes
// solid when its sub-segments shrink below the dash's reach.
import { chromium } from "playwright";
import { decode } from "./pnglit.mjs";
const S = process.env.M1_SHOTS ?? ".";
const PORT = process.env.PORT ?? 8080;
const browser = await chromium.launch({ args: ["--use-angle=swiftshader", "--enable-unsafe-swiftshader", "--ignore-gpu-blocklist"] });
const ctx = await browser.newContext({ viewport: { width: 1400, height: 900 } });
const page = await ctx.newPage();
const logs = [];
page.on("console", (m) => { const t = m.text(); if (!t.includes("session[")) logs.push(`[${m.type()}] ${t.slice(0, 300)}`); });
const step = async (n, f) => { process.stdout.write(`- ${n} … `); await f(); console.log("ok"); };
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const cam = () => page.evaluate(() => Array.from(Object.values(window.moonkale.graphViews)[0].camera_state()));
const pos = (id) => page.evaluate((id) => Object.values(window.moonkale.graphViews)[0].node_screen_position(id), id);
const sel = () => page.evaluate(() => JSON.parse(Object.values(window.moonkale.graphViews)[0].selection_state()));
const frame = () => page.evaluate(() => Array.from(Object.values(window.moonkale.graphViews)[0].frame_state()));
const counts = () => page.evaluate(() => [Object.values(window.moonkale.graphViews)[0].node_count(), Object.values(window.moonkale.graphViews)[0].edge_count()]);
// The view reports canvas-relative positions; clicks need viewport ones.
const at = async (p) => {
  const r = await page.$eval(".mk-graph-overlay", (e) => e.getBoundingClientRect());
  return [r.left + p[0], r.top + p[1]];
};
// Pixel hue counts: a hull's tint never passes a brightness threshold, so
// the checks tell colours apart by channel dominance instead.
const hues = (png, match) => {
  const { width, height, bpp, data } = decode(png);
  let n = 0;
  for (let i = 0; i < width * height; i++) {
    const o = i * bpp;
    if (match(data[o], data[o + 1], data[o + 2])) n++;
  }
  return n;
};
const GREEN = (r, g, b) => g > r + 15 && g > b + 10;
const RED = (r, g, b) => r > g + 15 && r > b + 15;
const YELLOW = (r, g, b) => r > 150 && g > 150 && b < r - 40;
const zoomTo = async (fn) => {
  for (let i = 0; i < 80; i++) {
    if (await fn((await cam())[0])) return;
    const r = await page.$eval(".mk-graph-overlay", (e) => e.getBoundingClientRect());
    await page.mouse.move(r.left + r.width / 2, r.top + r.height / 2);
    await page.mouse.wheel(0, 300); // scroll down zooms out (1/1.12)
    await sleep(90);
  }
  throw new Error("zoom target not reached");
};
try {
  await step("open folder; the renderer starts on a GL backend", async () => {
    await page.goto(`http://127.0.0.1:${PORT}/`, { waitUntil: "networkidle" });
    await page.waitForSelector(".wb-workspace");
    await page.click(".mk-explorer-open button[type=submit]");
    await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
    await page.click(".wb-tab:has-text('Graph')");
    await page.waitForFunction(() => /gl|gpu/i.test(document.querySelector(".mk-graph-info")?.dataset.backend || ""), null, { timeout: 30000 });
    await page.waitForFunction(() => Number(document.querySelector(".mk-graph-info")?.dataset.nodes) > 0 && Object.values(window.moonkale.graphViews)[0].node_count() === Number(document.querySelector(".mk-graph-info").dataset.nodes));
  });
  const box = await page.$eval(".mk-graph-overlay", (e) => { const r = e.getBoundingClientRect(); return { x: r.left, y: r.top, w: r.width, h: r.height }; });
  const world = async (p) => {
    const c = await cam();
    return [c[1] + (p[0] - box.w / 2) / c[0], c[2] + (p[1] - box.h / 2) / c[0]];
  };
  const GRAPH = {
    nodes: ["a", "b", "c", "d", "e"].map((id) => ({ id, label: id, kind: "file" })),
    edges: [
      { id: "ab", a: 0, b: 1, dash: 6, route: { kind: "bezier", offset: 15 } }, // curved + dashed: the dash-LOD probe
      { id: "bc", a: 1, b: 2 },
      { id: "ad", a: 0, b: 3, directed: true },
      { id: "ce", a: 2, b: 4 },
    ],
    ports: [
      { node: 3, name: "out", side: "right", color: "#ffe459" },
      { node: 4, name: "in", side: "left", color: "#ffe459" },
    ],
    groups: [
      { id: "outer", label: "module", members: ["a", "b", "c"], color: "#59ff8c" },
      { id: "pair", label: "pair", members: ["a", "b"], color: "#ff5959" }, // nested in outer
    ],
  };
  await step("a five-node graph with nested groups settles; two hulls are built", async () => {
    await page.evaluate((json) => Object.values(window.moonkale.graphViews)[0].set_graph(json), JSON.stringify(GRAPH));
    await page.waitForFunction(() => Object.values(window.moonkale.graphViews)[0].layout_state()[0] === 0, null, { timeout: 20000 });
    await sleep(700);
    const fr = await frame();
    console.log("\n  frame [segs, arrows, ports, hulls]:", JSON.stringify(fr));
    if (fr[3] !== 2) throw new Error(`wanted 2 hulls, built ${fr[3]}`);
  });
  await step("the nested and overlapping tints read as distinct hues", async () => {
    // A user navigation first: with auto-fit still on, a warmed layout
    // re-fits by design (P-100) — the camera check below is about the
    // delta, which must never move a navigated view.
    await page.mouse.move(box.x + box.w / 2, box.y + box.h / 2);
    await page.mouse.down();
    await page.mouse.move(box.x + box.w / 2 - 60, box.y + box.h / 2 - 20, { steps: 5 });
    await page.mouse.up();
    await sleep(400);
    const png = await page.screenshot({ clip: { x: box.x, y: box.y, width: box.w, height: box.h } });
    const green = hues(png, GREEN), red = hues(png, RED);
    console.log(`\n  green px: ${green}, red px: ${red}`);
    if (green < 500) throw new Error("the outer hull does not tint");
    if (red < 200) throw new Error("the nested pair hull does not tint");
    await page.screenshot({ path: `${S}/s4b-hulls.png` });
  });
  await step("a collapse through apply_delta keeps the view; the expand restores it", async () => {
    await page.mouse.click(...await at(await pos("d")));
    await sleep(300);
    if ((await sel()).nodes.join() !== "d") throw new Error("d not selected");
    const before = { c: await cam(), pos: await Promise.all(["a", "b", "c"].map(pos)), fr: await frame() };
    const centre = [0, 1].map((k) => before.pos.reduce((s, p) => s + p[k], 0) / 3);
    const wcentre = await world(centre);
    await page.evaluate((json) => Object.values(window.moonkale.graphViews)[0].apply_delta(json), JSON.stringify({
      remove_nodes: ["a", "b", "c"],
      add_nodes: [{ id: "agg", label: "module", kind: "directory", pos: wcentre, pinned: true }],
      add_edges: [
        { id: "aggd", a: "agg", b: "d", directed: true },
        { id: "agge", a: "agg", b: "e" },
      ],
      add_groups: [{ id: "outer", label: "module", members: ["agg"], color: "#59ff8c" }],
      remove_groups: ["pair"],
    }));
    await sleep(500);
    const after = { c: await cam(), n: await counts(), fr: await frame(), s: await sel() };
    console.log("\n  collapsed:", JSON.stringify(after.n), "hulls:", after.fr[3], "selection:", after.s.nodes.join());
    if (after.n[0] !== 3 || after.n[1] !== 2) throw new Error(`counts drifted: ${JSON.stringify(after.n)}`);
    if (after.fr[3] !== 1) throw new Error(`the collapse left ${after.fr[3]} hulls, wanted 1`);
    if (JSON.stringify(after.c) !== JSON.stringify(before.c)) throw new Error("the delta moved the camera");
    if (after.s.nodes.join() !== "d") throw new Error("selection lost across the collapse");
    const png = await page.screenshot({ clip: { x: box.x, y: box.y, width: box.w, height: box.h } });
    if (hues(png, GREEN) < 200) throw new Error("the collapsed group's hull does not tint");
    // The expand: members back where they were, edges and groups restored.
    const back = await Promise.all(before.pos.map((p) => world(p)));
    await page.evaluate((json) => Object.values(window.moonkale.graphViews)[0].apply_delta(json), JSON.stringify({
      remove_nodes: ["agg"],
      remove_edges: ["aggd", "agge"],
      add_nodes: ["a", "b", "c"].map((id, i) => ({ id, label: id, kind: "file", pos: back[i] })),
      add_edges: [
        { id: "ab", a: "a", b: "b", dash: 6, route: { kind: "bezier", offset: 15 } },
        { id: "bc", a: "b", b: "c" },
        { id: "ad", a: "a", b: "d", directed: true },
        { id: "ce", a: "c", b: "e" },
      ],
      add_groups: [
        { id: "outer", label: "module", members: ["a", "b", "c"], color: "#59ff8c" },
        { id: "pair", label: "pair", members: ["a", "b"], color: "#ff5959" },
      ],
    }));
    await sleep(500);
    const done = { n: await counts(), fr: await frame(), s: await sel() };
    console.log("  expanded:", JSON.stringify(done.n), "hulls:", done.fr[3], "selection:", done.s.nodes.join());
    if (done.n[0] !== 5 || done.n[1] !== 4) throw new Error(`the expand did not restore: ${JSON.stringify(done.n)}`);
    if (done.fr[3] !== 2) throw new Error(`the expand left ${done.fr[3]} hulls, wanted 2`);
    if (done.s.nodes.join() !== "d") throw new Error("selection lost across the expand");
  });
  await step("focus centres on a subset; Fit brings the whole graph back", async () => {
    const before = await cam();
    await page.evaluate(() => Object.values(window.moonkale.graphViews)[0].focus('["a","b"]'));
    await sleep(300);
    const after = await cam();
    const pa = await pos("a"), pb = await pos("b"), pe = await pos("e");
    const mid = [(pa[0] + pb[0]) / 2, (pa[1] + pb[1]) / 2];
    const offMid = Math.hypot(mid[0] - box.w / 2, mid[1] - box.h / 2);
    const offE = Math.hypot(pe[0] - box.w / 2, pe[1] - box.h / 2);
    console.log(`\n  scale ${before[0].toFixed(2)} → ${after[0].toFixed(2)}, the a-b midpoint ${offMid.toFixed(0)} px from centre, e ${offE.toFixed(0)} px`);
    if (JSON.stringify(after) === JSON.stringify(before)) throw new Error("focus changed nothing");
    if (offMid > 120) throw new Error(`the focal pair's midpoint is ${offMid.toFixed(0)} px from the centre`);
    for (const [id, p] of [["a", pa], ["b", pb]]) {
      if (p[0] < -20 || p[1] < -20 || p[0] > box.w + 20 || p[1] > box.h + 20) {
        throw new Error(`${id} is off-screen after focus: ${p.map((v) => v.toFixed(0))}`);
      }
    }
    if (offE < 200) throw new Error(`e (${offE.toFixed(0)} px) is not outside the focus`);
    await page.click("button:has-text('Fit')");
    await sleep(400);
  });
  await step("zoomed out, the hulls fade away (LOD §6)", async () => {
    await zoomTo((s) => s < 0.05);
    const c = await cam();
    const png = await page.screenshot({ clip: { x: box.x, y: box.y, width: box.w, height: box.h } });
    const green = hues(png, GREEN), red = hues(png, RED);
    console.log(`\n  scale ${c[0].toFixed(3)}: green px ${green}, red px ${red}`);
    if (green > 10 || red > 10) throw new Error("a hull still tints below the pixel threshold");
    await page.screenshot({ path: `${S}/s4b-hulls-lod.png` });
    await page.click("button:has-text('Fit')");
    await sleep(400);
    const png2 = await page.screenshot({ clip: { x: box.x, y: box.y, width: box.w, height: box.h } });
    if (hues(png2, GREEN) < 500) throw new Error("the hull did not come back with the zoom");
  });
  await step("ports stop drawing once their size falls under the threshold", async () => {
    await page.evaluate(() => Object.values(window.moonkale.graphViews)[0].set_interaction("edit"));
    await sleep(300);
    let png = await page.screenshot({ clip: { x: box.x, y: box.y, width: box.w, height: box.h } });
    const litFit = hues(png, YELLOW);
    console.log(`\n  port px at fit: ${litFit}`);
    if (litFit < 20) throw new Error("the ports do not draw in edit mode");
    await zoomTo((s) => s < 0.8);
    const c = await cam();
    png = await page.screenshot({ clip: { x: box.x, y: box.y, width: box.w, height: box.h } });
    const litFar = hues(png, YELLOW);
    console.log(`  port px at scale ${c[0].toFixed(2)}: ${litFar}`);
    if (litFar > 0) throw new Error("a port still draws below the pixel threshold");
    await page.click("button:has-text('Fit')");
    await sleep(400);
    png = await page.screenshot({ clip: { x: box.x, y: box.y, width: box.w, height: box.h } });
    if (hues(png, YELLOW) < 20) throw new Error("the ports did not come back with the zoom");
    await page.evaluate(() => Object.values(window.moonkale.graphViews)[0].set_interaction("explore"));
    await sleep(200);
  });
  await step("a dashed curve keeps its gaps while it reads — even when its sub-segments shrink", async () => {
    // The curve ab's apex sits half its control offset (7.5 world) off the
    // endpoints' line; a clip there sees the stroke and nothing else.
    const apexClip = async () => {
      const [pa, pb] = await Promise.all([pos("a"), pos("b")]);
      const mid = [(pa[0] + pb[0]) / 2, (pa[1] + pb[1]) / 2];
      const d = [pb[0] - pa[0], pb[1] - pa[1]];
      const len = Math.hypot(d[0], d[1]);
      const n = [-d[1] / len, d[0] / len];
      const c = await cam();
      const apex = [mid[0] + n[0] * 7.5 * c[0], mid[1] + n[1] * 7.5 * c[0]];
      return { x: box.x + apex[0] - 22, y: box.y + apex[1] - 22, width: 44, height: 44 };
    };
    const coverage = async () => {
      const png = await page.screenshot({ clip: await apexClip() });
      const { width, height, bpp, data } = decode(png);
      const cols = new Set();
      for (let y = 0; y < height; y++) {
        for (let x = 0; x < width; x++) {
          const i = (y * width + x) * bpp;
          if ((data[i] + data[i + 1] + data[i + 2]) / 3 > 60) cols.add(x);
        }
      }
      // The longest run of consecutive covered columns: dashes cut the
      // stroke into short runs; a solid stroke spans the clip.
      const xs = [...cols].sort((a, b) => a - b);
      let best = 0, run = 0, prev = -2;
      for (const x of xs) { run = x === prev + 1 ? run + 1 : 1; prev = x; best = Math.max(best, run); }
      return best;
    };
    // A dedicated two-node graph: nothing else draws near the curve (at
    // low zoom the group hulls' rims are lit and sit in the apex clip).
    await page.evaluate((json) => Object.values(window.moonkale.graphViews)[0].set_graph(json), JSON.stringify({
      nodes: ["a", "b"].map((id) => ({ id, label: id, kind: "file" })),
      edges: [{ id: "ab", a: 0, b: 1, dash: 6, route: { kind: "bezier", offset: 15 } }],
    }));
    await page.waitForFunction(() => Object.values(window.moonkale.graphViews)[0].layout_state()[0] === 0, null, { timeout: 20000 });
    await sleep(700); // settle + auto-fit
    const dashed = await coverage();
    // Zoom out until the curve's sub-segments (≈ its screen length / 8)
    // fall under 6 px — exactly where the LOD used to gate per sub-segment
    // and the curve drew solid while its straight twin stayed dashed
    // (the P-166 review's #5). The whole stroke still spans ~40 px, so
    // the gaps must keep reading: the longest lit run stays a dash long,
    // it does not become the whole curve.
    await zoomTo(async () => {
      const [pa, pb] = await Promise.all([pos("a"), pos("b")]);
      return Math.hypot(pb[0] - pa[0], pb[1] - pa[1]) < 45;
    });
    const stillDashed = await coverage();
    const [pa, pb] = await Promise.all([pos("a"), pos("b")]);
    console.log(`\n  longest lit run: ${dashed}px at fit, ${stillDashed}px at a ${Math.hypot(pb[0] - pa[0], pb[1] - pa[1]).toFixed(0)}px curve`);
    if (dashed > 12) throw new Error(`the dash pattern does not read at fit (run ${dashed}px)`);
    if (stillDashed > 14) throw new Error(`the curve drew solid while its stroke spans ${(Math.hypot(pb[0] - pa[0], pb[1] - pa[1])).toFixed(0)}px (run ${stillDashed}px)`);
    await page.screenshot({ path: `${S}/s4b-dash-kept.png` });
  });
  console.log("\nGRAPH-GROUPS E2E: PASS");
} catch (e) {
  console.log("\nFAIL:", e.message);
  await page.screenshot({ path: `${S}/s4b-fail.png` }).catch(() => {});
  console.log(logs.filter((l) => !/WARN|Copy Value/.test(l)).slice(-15).join("\n"));
  process.exitCode = 1;
} finally {
  await browser.close();
}
