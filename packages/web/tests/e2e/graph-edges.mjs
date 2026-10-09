// Spec 031, stage 1: edges and selection on the real renderer — Chromium with
// software WebGL (SwiftShader), the graph3d.mjs setup. Feeds a synthetic graph
// with known ids through set_graph and checks: parallel edges bow apart and
// are picked separately, a self-loop renders and is pickable, arrowheads are
// built for directed/`both` edges, a click selects (plain) and toggles
// (Ctrl), a drag does not select, and empty space clears.
import { chromium } from "playwright";
import { litPixels } from "./pnglit.mjs";
const S = process.env.M1_SHOTS ?? ".";
const PORT = process.env.PORT ?? 8080;
const browser = await chromium.launch({ args: ["--use-angle=swiftshader", "--enable-unsafe-swiftshader", "--ignore-gpu-blocklist"] });
const ctx = await browser.newContext({ viewport: { width: 1400, height: 900 } });
const page = await ctx.newPage();
const logs = [];
page.on("console", (m) => { const t = m.text(); if (!t.includes("session[")) logs.push(`[${m.type()}] ${t.slice(0, 300)}`); });
const step = async (n, f) => { process.stdout.write(`- ${n} … `); await f(); console.log("ok"); };
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const sel = () => page.evaluate(() => JSON.parse(Object.values(window.moonkale.graphViews)[0].selection_state()));
try {
  await step("open folder; the renderer starts on a GL backend", async () => {
    await page.goto(`http://127.0.0.1:${PORT}/`, { waitUntil: "networkidle" });
    await page.waitForSelector(".wb-workspace");
    await page.click(".mk-explorer-open button[type=submit]");
    await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
    await page.click(".wb-tab:has-text('Graph')");
    await page.waitForFunction(() => /gl|gpu/i.test(document.querySelector(".mk-graph-info")?.dataset.backend || ""), null, { timeout: 30000 });
  });
  const box = await page.$eval(".mk-graph-overlay", (e) => { const r = e.getBoundingClientRect(); return { x: r.left, y: r.top, w: r.width, h: r.height }; });
  const at = (p) => [box.x + p[0], box.y + p[1]];
  await step("a synthetic graph with parallels, a dash, a self-loop and arrowheads settles", async () => {
    const graph = {
      nodes: ["a", "b", "c", "d"].map((id) => ({ id, label: id, kind: "file" })),
      edges: [
        { id: "p0", a: 0, b: 1, directed: true },                    // bows −, target head
        { id: "p1", a: 0, b: 1 },                                     // the middle stays straight
        { id: "p2", a: 0, b: 1, dash: 8 },                            // bows +, dashed
        { id: "loop", a: 2, b: 2, directed: true },                   // self-loop with a head
        { id: "both", a: 1, b: 3, arrow: "both" },                    // heads at both ends
      ],
    };
    await page.evaluate((json) => Object.values(window.moonkale.graphViews)[0].set_graph(json), JSON.stringify(graph));
    await page.waitForFunction(() => Object.values(window.moonkale.graphViews)[0].layout_state()[0] === 0, null, { timeout: 20000 });
    await sleep(700); // settle + auto-fit
    const fr = await page.evaluate(() => Object.values(window.moonkale.graphViews)[0].frame_state());
    // 8 + 1 + 8 curve/straight segments + 16 for the loop + 1: 34;
    // heads: p0's target, the loop's, both ends of "both": 4.
    console.log("\n  frame:", JSON.stringify(fr));
    if (fr[0] !== 34 || fr[1] !== 4) throw new Error(`frame_state [segs, arrows] = ${fr}, wanted [34, 4]`);
  });
  const pos = () => page.evaluate(() => {
    const v = Object.values(window.moonkale.graphViews)[0];
    const scale = v.camera_state()[0];
    const p = {};
    for (const id of ["a", "b", "c", "d"]) p[id] = v.node_screen_position(id);
    return { p, scale };
  });
  await step("clicking the straight parallel selects it; the bowed one is picked apart", async () => {
    const { p } = await pos();
    const mid = [(p.a[0] + p.b[0]) / 2, (p.a[1] + p.b[1]) / 2];
    await page.mouse.click(...at(mid));
    await sleep(250);
    let s = await sel();
    console.log("\n  straight:", JSON.stringify(s));
    if (s.edges.length !== 1 || s.edges[0] !== "p1") throw new Error("midpoint did not select p1: " + JSON.stringify(s));
    // p2 bows +9 world units; its apex is half that off the line, on the
    // perpendicular of a→b (screen y tracks world y — no flip).
    const { p: p2, scale } = await pos();
    const d = [p2.b[0] - p2.a[0], p2.b[1] - p2.a[1]];
    const len = Math.hypot(...d);
    const perp = [-d[1] / len, d[0] / len];
    const apex = [mid[0] + perp[0] * 4.5 * scale, mid[1] + perp[1] * 4.5 * scale];
    await page.keyboard.down("Control");
    await page.mouse.click(...at(apex));
    await page.keyboard.up("Control");
    await sleep(250);
    s = await sel();
    console.log("  after ctrl+apex:", JSON.stringify(s));
    if (s.edges.length !== 2 || !s.edges.includes("p2")) throw new Error("apex did not add p2: " + JSON.stringify(s));
  });
  await step("empty space clears; nodes select, Ctrl adds; a drag selects nothing", async () => {
    await page.mouse.click(box.x + 12, box.y + 12);
    await sleep(250);
    let cleared = await sel();
    if (cleared.nodes.length || cleared.edges.length) throw new Error("empty click did not clear: " + JSON.stringify(cleared));
    const { p } = await pos();
    await page.mouse.click(...at(p.a));
    await sleep(250);
    let s = await sel();
    if (s.nodes.length !== 1 || s.nodes[0] !== "a") throw new Error("node click did not select a: " + JSON.stringify(s));
    await page.keyboard.down("Control");
    await page.mouse.click(...at(p.b));
    await page.keyboard.up("Control");
    await sleep(250);
    s = await sel();
    console.log("\n  nodes:", JSON.stringify(s));
    if (s.nodes.length !== 2 || !s.nodes.includes("b")) throw new Error("ctrl did not add b: " + JSON.stringify(s));
    // A drag of a third node leaves the selection alone.
    await page.mouse.move(...at(p.d));
    await page.mouse.down();
    await page.mouse.move(...at([p.d[0] + 60, p.d[1] + 40]), { steps: 5 });
    await page.mouse.up();
    await sleep(250);
    s = await sel();
    if (s.nodes.length !== 2 || s.nodes.includes("d")) throw new Error("a drag changed the selection: " + JSON.stringify(s));
  });
  await step("the self-loop is pickable on its ring", async () => {
    await page.mouse.click(box.x + 12, box.y + 12);
    await sleep(250);
    const { p, scale } = await pos();
    // A single loop's ring rests on the node: r = radius + 6, its top 2r
    // above c (the exact radius is the node's own, so probe up the ring's
    // vertical — a click on c itself selects the node, which the probe
    // simply walks past).
    let found = null;
    for (let off = 6; off <= 30 && !found; off += 2) {
      await page.mouse.click(...at([p.c[0], p.c[1] - off * scale]));
      await sleep(200);
      const s = await sel();
      if (s.edges.includes("loop")) found = s;
    }
    console.log("\n  loop:", JSON.stringify(found));
    if (!found) throw new Error("no point on the ring selected the loop");
    await page.screenshot({ path: `${S}/s1-edges.png` });
  });
  await step("an arrowhead's lit mass points into its node, not screen-right (B1, pixels)", async () => {
    // The "both" edge b→d carries a head at b pulled back along the edge;
    // its triangle's lit centroid must sit toward b (against the travel),
    // whatever the edge's direction — a head that always points +x on
    // screen puts the centroid the other way.
    const { p, scale } = await pos();
    const d = [p.d[0] - p.b[0], p.d[1] - p.b[1]];
    const len = Math.hypot(...d);
    const u = [d[0] / len, d[1] / len];
    // The head's centre: b pulled back along +u by (radius + 4) world
    // units; the node radius here is 4 + sqrt(degree), ≈ 5.5.
    const hc = at([p.b[0] + u[0] * 9.5 * scale, p.b[1] + u[1] * 9.5 * scale]);
    const r = Math.max(24, Math.min(3.5 * scale, 90));
    const png = await page.screenshot({ clip: { x: hc[0] - r, y: hc[1] - r, width: 2 * r, height: 2 * r } });
    const { lit } = litPixels(png);
    if (lit.length < 12) throw new Error(`no head lit around ${hc}: ${lit.length} px`);
    const cx = lit.reduce((s, q) => s + q.x, 0) / lit.length - r;
    const cy = lit.reduce((s, q) => s + q.y, 0) / lit.length - r;
    // The apex (the heavy end) points into b: −u in screen space.
    const into = [-u[0], -u[1]];
    const dot = cx * into[0] + cy * into[1];
    const mag = Math.hypot(cx, cy) || 1;
    console.log(`\n  head centroid (${cx.toFixed(1)}, ${cy.toFixed(1)}) · into-b ${dot.toFixed(1)} of ${mag.toFixed(1)}`);
    if (dot / mag < 0.25) throw new Error("the head's lit mass does not point into its node");
    await page.screenshot({ path: `${S}/s1-arrow.png` });
  });
  console.log("\nGRAPH-EDGES E2E: PASS");
} catch (e) {
  console.log("\nFAIL:", e.message);
  await page.screenshot({ path: `${S}/s1-edges-fail.png` }).catch(() => {});
  console.log(logs.filter((l) => !/WARN|Copy Value/.test(l)).slice(-15).join("\n"));
  process.exitCode = 1;
} finally {
  await browser.close();
}
