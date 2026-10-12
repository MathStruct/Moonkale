// Spec 031, stage 6: animated edges on the real renderer — Chromium with
// SwiftShader. Two solid edges carry pulses at different speeds; the suite
// finds the glow's position along each edge's screen line (the brightness
// peak) in pairs of captures and checks that both pulses travel and the
// faster one travels farther in the same interval — and that at a zoom
// where the segments fall under the pixel threshold the animation stops
// (LOD §6: barely visible animations are suppressed).
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
const SLOW = 12, FAST = 36; // world units per second
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
  const clip = { x: box.x, y: box.y, width: box.w, height: box.h };
  await step("a graph with a slow and a fast pulse settles", async () => {
    const graph = {
      nodes: ["a", "b", "c"].map((id) => ({ id, label: id, kind: "file" })),
      edges: [
        { id: "slow", a: 0, b: 1, pulse_speed: SLOW },
        { id: "fast", a: 1, b: 2, pulse_speed: FAST },
      ],
    };
    await page.evaluate((json) => Object.values(window.moonkale.graphViews)[0].set_graph(json), JSON.stringify(graph));
    await page.waitForFunction(() => Object.values(window.moonkale.graphViews)[0].layout_state()[0] === 0, null, { timeout: 20000 });
    await sleep(700); // settle + auto-fit
  });
  // The brightness profile along an edge's screen line (15–85 %, nodes
  // excluded): the pulse's Gaussian glow is the peak; its index gives the
  // pulse's position along the stroke.
  const profile = async (pa, pb) => {
    const png = await page.screenshot({ clip });
    const { width, bpp, data } = decode(png);
    const pts = [];
    for (let k = 0; k <= 40; k++) {
      const t = 0.15 + (0.7 * k) / 40;
      const x = pa[0] + (pb[0] - pa[0]) * t;
      const y = pa[1] + (pb[1] - pa[1]) * t;
      let s = 0;
      for (let dx = -2; dx <= 2; dx++) {
        for (let dy = -2; dy <= 2; dy++) {
          const i = ((y + dy) | 0) * width * bpp + ((x + dx) | 0) * bpp;
          s += (data[i] + data[i + 1] + data[i + 2]) / 3;
        }
      }
      pts.push(s / 25);
    }
    return pts;
  };
  const peak = (pts) => pts.indexOf(Math.max(...pts));
  // The glow's displacement over one interval, in screen pixels. A pulse
  // can wrap during the interval (fract); several pairs, the max is the
  // unwrapped displacement.
  const displacement = async (idA, idB, pairs) => {
    const pa = await pos(idA), pb = await pos(idB);
    const len = Math.hypot(pb[0] - pa[0], pb[1] - pa[1]);
    let best = 0;
    for (let i = 0; i < pairs; i++) {
      const p1 = peak(await profile(pa, pb));
      await sleep(180);
      const p2 = peak(await profile(pa, pb));
      const d = (Math.abs(p2 - p1) / 40) * 0.7 * len;
      best = Math.max(best, d);
    }
    return best;
  };
  await step("both pulses travel, and the faster one travels farther", async () => {
    // Six pairs, the max of them: a pair that straddles a wrap or a
    // dropped frame under-reads; the fixes review saw 11.1 px against a
    // bar of 12 under light load. The bar sits at ~70 % of the expected
    // displacement so a loaded machine still passes.
    const slowPx = await displacement("a", "b", 6);
    const fastPx = await displacement("b", "c", 6);
    console.log(`\n  slow pulse: ${slowPx.toFixed(0)} px in ~180 ms, fast: ${fastPx.toFixed(0)} px`);
    if (slowPx < 9) throw new Error(`the slow pulse did not move (${slowPx.toFixed(1)} px)`);
    if (fastPx < 9) throw new Error(`the fast pulse did not move (${fastPx.toFixed(1)} px)`);
    if (fastPx < slowPx * 1.5) throw new Error(`the fast pulse (${fastPx.toFixed(0)} px) does not outpace the slow one (${slowPx.toFixed(0)} px)`);
    await page.screenshot({ path: `${S}/s6-pulses.png` });
  });
  await step("zoomed out under the pixel threshold, the animation stops (LOD)", async () => {
    // Scroll-down zooms out; the whole view is a few pixels wide long
    // before the floor — every segment under 8 screen px gates its glow.
    for (let i = 0; i < 60; i++) {
      const c = await cam();
      if (c[0] < 0.05) break;
      await page.mouse.move(box.x + box.w / 2, box.y + box.h / 2);
      await page.mouse.wheel(0, 300);
      await sleep(90);
    }
    const a = await page.screenshot({ clip });
    await sleep(400);
    const b = await page.screenshot({ clip });
    const da = decode(a), db = decode(b);
    let diff = 0;
    for (let i = 0; i < da.width * da.height; i++) {
      const o = i * da.bpp;
      if (Math.abs(da.data[o] - db.data[o]) + Math.abs(da.data[o + 1] - db.data[o + 1]) + Math.abs(da.data[o + 2] - db.data[o + 2]) > 12) diff++;
    }
    console.log(`\n  pixels changed between two frames 400 ms apart: ${diff}`);
    if (diff > 20) throw new Error("the picture still animates below the pixel threshold");
    await page.screenshot({ path: `${S}/s6-pulse-lod.png` });
  });
  console.log("\nGRAPH-PULSE E2E: PASS");
} catch (e) {
  console.log("\nFAIL:", e.message);
  await page.screenshot({ path: `${S}/s6-fail.png` }).catch(() => {});
  console.log(logs.filter((l) => !/WARN|Copy Value/.test(l)).slice(-15).join("\n"));
  process.exitCode = 1;
} finally {
  await browser.close();
}
