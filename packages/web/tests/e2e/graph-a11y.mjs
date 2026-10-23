// Spec 031, stage 10: the accessibility minimums on the real renderer —
// Chromium with SwiftShader, the graph3d.mjs setup. A canvas graph is
// invisible to keyboards and screen readers; the engine now takes a
// focusable overlay (arrows walk the map spatially, Enter clicks the
// focused node, Escape clears) and the panel announces the selection and
// the focus through an aria-live line (DOM checks; the focus ring is
// pixel-checked).
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
const live = () => page.$eval(".mk-graph-live", (e) => e.textContent.trim()).catch(() => "");
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
  const at = (p) => [box.x + p[0], box.y + p[1]];
  await step("a pinned three-node map settles; the overlay is focusable", async () => {
    const graph = {
      nodes: [
        { id: "start", label: "start", kind: "file", pos: [0, 0], pinned: true },
        { id: "east", label: "east", kind: "file", pos: [60, 0], pinned: true },
        { id: "west", label: "west", kind: "file", pos: [-60, 0], pinned: true },
      ],
      edges: [{ id: "ab", a: 0, b: 1 }],
    };
    await page.evaluate((json) => Object.values(window.moonkale.graphViews)[0].set_graph(json), JSON.stringify(graph));
    await page.waitForFunction(() => Object.values(window.moonkale.graphViews)[0].layout_state()[0] === 0, null, { timeout: 20000 });
    await sleep(700);
    // A canvas click focuses the overlay (the engine's capture-phase
    // listener — its own pointerdown prevents the default).
    const s = await page.evaluate(() => Object.values(window.moonkale.graphViews)[0].node_screen_position("start"));
    await page.mouse.click(...at(s));
    await sleep(200);
    const focused = await page.evaluate(() => document.activeElement.tagName + "#" + document.activeElement.id);
    if (!/CANVAS/.test(focused)) throw new Error(`the canvas did not take the focus: ${focused}`);
  });
  await step("the arrows walk the map; the focus ring follows and the live line names it", async () => {
    await page.keyboard.press("ArrowRight");
    await sleep(400);
    let text = await live();
    console.log(`\n  live: "${text}"`);
    if (!/east/.test(text)) throw new Error(`the focus did not reach east: "${text}"`);
    // The ring: ink in an annulus around the focused node.
    const e = await page.evaluate(() => Object.values(window.moonkale.graphViews)[0].node_screen_position("east"));
    const png = await page.screenshot({ clip });
    const { width, height, bpp, data } = decode(png);
    const cam = await page.evaluate(() => Object.values(window.moonkale.graphViews)[0].camera_state());
    // "east" has degree 1: its world radius is 4 + sqrt(1) = 5; the ring
    // sits at radius + 5 px with a 1.5 px stroke — sample a thin annulus.
    const base = 5.0 * cam[0] + 5.0;
    let ring = 0;
    for (const rr of [base - 1.5, base, base + 1.5]) {
      for (let a = 0; a < 6.283; a += 0.06) {
        const x = Math.round(e[0] + Math.cos(a) * rr);
        const y = Math.round(e[1] + Math.sin(a) * rr);
        if (x < 0 || y < 0 || x >= width || y >= height) continue;
        const o = (y * width + x) * bpp;
        if (data[o] + data[o + 1] + data[o + 2] > 200) ring++;
      }
    }
    if (ring < 8) throw new Error(`no focus ring around east (${ring} ink samples)`);
    console.log(`  ring ink samples: ${ring}`);
    // Back: the walk reaches west (through start, two presses).
    await page.keyboard.press("ArrowLeft");
    await sleep(300);
    await page.keyboard.press("ArrowLeft");
    await sleep(300);
    text = await live();
    if (!/west/.test(text)) throw new Error(`the walk did not reach west: "${text}"`);
    // Up: nothing above — the walk stalls, the focus stays.
    await page.keyboard.press("ArrowUp");
    await sleep(300);
    text = await live();
    if (!/west/.test(text)) throw new Error(`the walk did not stall at the edge: "${text}"`);
  });
  await step("Enter selects the focused node; Escape clears — the live line narrates", async () => {
    await page.keyboard.press("Enter");
    await sleep(300);
    const sel = await page.evaluate(() => JSON.parse(Object.values(window.moonkale.graphViews)[0].selection_state()));
    if (sel.nodes[0] !== "west") throw new Error(`Enter did not select the focused node: ${JSON.stringify(sel)}`);
    let text = await live();
    console.log(`\n  live: "${text}"`);
    if (!/selected/.test(text)) throw new Error(`the selection is not announced: "${text}"`);
    await page.keyboard.press("Escape");
    await sleep(300);
    const sel2 = await page.evaluate(() => JSON.parse(Object.values(window.moonkale.graphViews)[0].selection_state()));
    if (sel2.nodes.length !== 0) throw new Error("Escape did not clear the selection");
    text = await live();
    if (!/nothing/.test(text)) throw new Error(`the cleared selection is not announced: "${text}"`);
    await page.screenshot({ path: `${S}/s10-a11y.png`, clip });
  });
  console.log("\nGRAPH-A11Y E2E: PASS");
} catch (e) {
  console.log("\nFAIL:", e.message);
  await page.screenshot({ path: `${S}/s10-fail.png` }).catch(() => {});
  console.log(logs.filter((l) => !/WARN|Copy Value/.test(l)).slice(-15).join("\n"));
  process.exitCode = 1;
} finally {
  await browser.close();
}
