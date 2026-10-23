// Spec 031, stage 8: the §2 presentation gaps on the real renderer —
// Chromium with SwiftShader, the graph3d.mjs setup. A diamond block (its
// inscribed rhombus leaves the bounding box's corners dark), a host
// stroke (a hue only the rim band produces) and a node icon (drawn on
// the label overlay while hovered or selected — §6 keeps per-node ink
// for the few, and it must leave when the hover does).
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
const V = () => page.evaluate(() => Object.values(window.moonkale.graphViews)[0]);
const screen = (id) => page.evaluate((id) => Object.values(window.moonkale.graphViews)[0].node_screen_position(id), id);
// Count the pixels of a small patch (png coords == the overlay's own).
const patch = (png, x, y, r, match) => {
  const { width, height, bpp, data } = decode(png);
  let n = 0;
  for (let py = Math.max(0, Math.floor(y - r)); py < Math.min(height, Math.ceil(y + r)); py++) {
    for (let px = Math.max(0, Math.floor(x - r)); px < Math.min(width, Math.ceil(x + r)); px++) {
      const o = (py * width + px) * bpp;
      if (match(data[o], data[o + 1], data[o + 2])) n++;
    }
  }
  return n;
};
const ORANGE = (r, g, b) => r > g && r > b + 40 && r > 80;            // the diamond's stroke band #ff8800
const MAGENTA = (r, g, b) => r > 100 && b > 100 && g < 100;           // the circle's stroke band #9900ff
const INK = (r, g, b) => r < 100 && g < 100 && b < 100;               // the label ink (the icon)
const BG = (r, g, b) => r < 30 && g < 30 && b < 40;                   // the canvas background
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
  const clip = { x: box.x, y: box.y, width: box.w, height: box.h };
  await step("a diamond, a rect and a circle settle", async () => {
    const graph = {
      nodes: [
        { id: "dec", label: "decision", kind: "file", shape: "diamond", width: 46, height: 22, color: "#4fb3e8", stroke: "#ff8800", icon: "◆", pos: [-140, 0], pinned: true },
        { id: "blk", label: "block", kind: "file", shape: "rect", width: 40, height: 20, color: "#8ccf6a", pos: [0, 0], pinned: true },
        { id: "dot", label: "dot", kind: "file", color: "#f2a03d", stroke: "#9900ff", pos: [140, 0], pinned: true },
      ],
      edges: [],
    };
    await page.evaluate((json) => Object.values(window.moonkale.graphViews)[0].set_graph(json), JSON.stringify(graph));
    await page.waitForFunction(() => Object.values(window.moonkale.graphViews)[0].layout_state()[0] === 0, null, { timeout: 20000 });
    await sleep(700);
  });
  await step("the diamond is a rhombus: lit centre, dark bounding-box corner", async () => {
    const png = await page.screenshot({ clip });
    const d = await screen("dec");
    // The centre is lit in the fill's blue.
    const lit = patch(png, d[0], d[1], 3, (r, g, b) => b > r && b > 120 && g > 100);
    if (lit < 20) throw new Error(`the diamond's centre is not lit blue (${lit} px)`);
    // The bounding box's corner (a rhombus leaves it to the background).
    const corner = patch(png, d[0] + 55, d[1] + 25, 3, BG);
    if (corner < 20) throw new Error(`the diamond's bounding-box corner is not dark (${corner} px)`);
    // And a rect of the same make fills its corner (the shapes differ).
    const b = await screen("blk");
    const rcorner = patch(png, b[0] + 48, b[1] + 22, 3, (r, g, b) => g > r + 20 && g > 130);
    if (rcorner < 20) throw new Error(`the rect's corner is not lit green (${rcorner} px)`);
    await page.screenshot({ path: `${S}/s8-shapes.png`, clip });
  });
  await step("the rim hugs the edge, not the block's core (the fixes review's #1)", async () => {
    // Park the pointer off-canvas first: a hovered node draws white (P-166).
    await page.mouse.move(box.x - 40, box.y - 40);
    await sleep(200);
    const FILL = (r, g, b) => g > 180 && g > r + 40 && g > b + 40;   // the block's fill #8ccf6a
    const RING = (r, g, b) => r > 120 && g > 120 && b > 90;         // the selection ring's light band
    const b = await screen("blk");
    const png = await page.screenshot({ clip });
    // An unselected block's centre is the plain fill — the inverted rim
    // normalisation darkened a band across the core instead.
    let centre = patch(png, b[0], b[1], 3, FILL);
    if (centre < 20) throw new Error(`the unselected block's centre is not its fill (${centre} px)`);
    // Find the block's top rim in the pixels themselves (greenish up to
    // the boundary, background beyond), just right of the label.
    const { width, bpp, data } = decode(png);
    const xb = Math.round(b[0]) + 30;
    let top = Math.round(b[1]);
    while (top > 0 && data[(top * width + xb) * bpp + 1] > 90) top -= 1;
    // The unselected rim is the darkened fill — no ring there yet.
    const before = patch(png, b[0], top + 3, 2, RING);
    if (before > 0) throw new Error(`the unselected block already wears the ring (${before} px)`);
    // Select the block: the centre stays the fill, the edge lights up.
    await page.mouse.click(box.x + b[0], box.y + b[1]);
    await sleep(300);
    await page.mouse.move(box.x - 40, box.y - 40);
    await sleep(300);
    const png2 = await page.screenshot({ clip });
    centre = patch(png2, b[0], b[1], 3, FILL);
    if (centre < 20) throw new Error(`the selected block's centre is not its fill (${centre} px)`);
    const edge = patch(png2, b[0], top + 3, 2, RING);
    console.log(`\n  centre fill held; ring ${edge} px at the edge (unselected rim: ${before} px)`);
    if (edge < 5) throw new Error(`the selected block's edge has no light ring (${edge} px)`);
    await page.keyboard.press("Escape");
  });
  await step("the strokes: a hue only a rim band produces", async () => {
    // Park the pointer off-canvas first: a hovered node draws white (P-166).
    await page.mouse.move(box.x - 40, box.y - 40);
    await sleep(200);
    const png2 = await page.screenshot({ clip });
    const d = await screen("dec");
    const orange = patch(png2, d[0], d[1], 45, ORANGE);
    if (orange < 25) throw new Error(`the diamond's orange stroke is missing (${orange} px)`);
    const c = await screen("dot");
    const magenta = patch(png2, c[0], c[1], 25, MAGENTA);
    if (magenta < 10) throw new Error(`the circle's magenta stroke is missing (${magenta} px)`);
    console.log(`\n  orange ${orange} px around the diamond, magenta ${magenta} px around the circle`);
  });
  await step("the icon: drawn while hovered, gone when the hover leaves", async () => {
    const d = await screen("dec");
    await page.mouse.move(box.x + d[0], box.y + d[1]);
    await sleep(400);
    let png = await page.screenshot({ clip });
    let ink = patch(png, d[0], d[1], 9, INK);
    if (ink < 4) throw new Error(`no icon ink at the hovered diamond (${ink} px)`);
    await page.mouse.move(box.x - 40, box.y - 40);
    await sleep(400);
    png = await page.screenshot({ clip });
    ink = patch(png, d[0], d[1], 9, INK);
    if (ink > 0) throw new Error(`the icon outlived the hover (${ink} px)`);
    console.log(`\n  the icon leaves with the hover`);
  });
  await step("selection keeps the icon (§6: ink for the few)", async () => {
    const d = await screen("dec");
    await page.mouse.click(box.x + d[0], box.y + d[1]);
    await sleep(400);
    await page.mouse.move(box.x - 40, box.y - 40);
    await sleep(400);
    const png = await page.screenshot({ clip });
    const ink = patch(png, d[0], d[1], 9, INK);
    if (ink < 4) throw new Error(`the selected diamond lost its icon (${ink} px)`);
    await page.keyboard.press("Escape");
  });
  console.log("\nGRAPH-SHAPES E2E: PASS");
} catch (e) {
  console.log("\nFAIL:", e.message);
  await page.screenshot({ path: `${S}/s8-fail.png` }).catch(() => {});
  console.log(logs.filter((l) => !/WARN|Copy Value/.test(l)).slice(-15).join("\n"));
  process.exitCode = 1;
} finally {
  await browser.close();
}
