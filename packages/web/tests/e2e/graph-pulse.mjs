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
    // Wait for the host's initial scene before replacing it with the fixture.
    await page.waitForFunction(() => Number(document.querySelector(".mk-graph-info")?.dataset.nodes) > 0 && Object.values(window.moonkale.graphViews)[0].node_count() === Number(document.querySelector(".mk-graph-info").dataset.nodes));
  });
  const box = await page.$eval(".mk-graph-overlay", (e) => { const r = e.getBoundingClientRect(); return { x: r.left, y: r.top, w: r.width, h: r.height }; });
  const clip = { x: box.x, y: box.y, width: box.w, height: box.h };
  await step("a graph with a slow and a fast pulse settles", async () => {
    const graph = {
      nodes: ["a", "b", "c"].map((id, i) => ({ id, label: id, kind: "file", pos: [160 * i, 0], pinned: true })),
      edges: [
        { id: "slow", a: 0, b: 1, pulse_speed: SLOW },
        { id: "fast", a: 1, b: 2, pulse_speed: FAST },
      ],
    };
    await page.evaluate((json) => Object.values(window.moonkale.graphViews)[0].set_graph(json), JSON.stringify(graph));
    await page.waitForFunction(() => Object.values(window.moonkale.graphViews)[0].layout_state()[0] === 0, null, { timeout: 20000 });
    await sleep(700); // settle + auto-fit
  });
  // Sample both edges from the same frame. Absolute displacements across a
  // phase wrap over-read the slow pulse, and separate captures compare unequal
  // intervals. Ignore off-stroke peaks and backwards (wrapped) pairs instead.
  const profile = (png, pa, pb) => {
    const { width, bpp, data } = decode(png);
    const pts = [];
    for (let k = 0; k <= 256; k++) {
      const t = 0.08 + (0.84 * k) / 256;
      const x = pa[0] + (pb[0] - pa[0]) * t;
      const y = pa[1] + (pb[1] - pa[1]) * t;
      let sum = 0;
      for (let dx = -2; dx <= 2; dx++) for (let dy = -2; dy <= 2; dy++) {
        const i = ((y + dy) | 0) * width * bpp + ((x + dx) | 0) * bpp;
        sum += (data[i] + data[i + 1] + data[i + 2]) / 3;
      }
      pts.push(sum / 25);
    }
    const peak = pts.indexOf(Math.max(...pts));
    if (peak < 3 || peak > 253 || Math.max(...pts)-Math.min(...pts) < 2) return null;
    return (0.08 + 0.84*peak/256) * Math.hypot(pb[0]-pa[0], pb[1]-pa[1]);
  };
  const median = values => [...values].sort((a,b)=>a-b)[Math.floor(values.length/2)];
  await step("both pulses travel at their declared speeds in shared frames", async () => {
    const [a,b,c] = await Promise.all([pos("a"),pos("b"),pos("c")]);
    const samples = [];
    for (let i=0;i<13;i++) {
      const png = await page.screenshot({clip});
      samples.push({time:Date.now(), slow:profile(png,a,b), fast:profile(png,b,c)});
      if (i<12) await sleep(200);
    }
    const speeds = key => {
      const rates = [];
      for (let i=1;i<samples.length;i++) {
        const before=samples[i-1], after=samples[i];
        if (before[key] === null || after[key] === null) continue;
        const delta=after[key]-before[key];
        if (delta > 1) rates.push(delta*1000/(after.time-before.time));
      }
      if (rates.length<2) throw new Error(`${key} pulse has fewer than two measurable forward intervals`);
      return median(rates);
    };
    const slow=speeds('slow'), fast=speeds('fast'), scale=(await cam())[0];
    console.log(`\n  median screen speeds: slow ${slow.toFixed(1)} px/s, fast ${fast.toFixed(1)} px/s; ratio ${(fast/slow).toFixed(2)}`);
    for (const [name, actual, expected] of [['slow',slow,SLOW*scale],['fast',fast,FAST*scale]]) {
      if (actual<expected*0.55 || actual>expected*1.6) throw new Error(`${name} speed ${actual} px/s differs from ${expected}`);
    }
    if (fast/slow<2 || fast/slow>4.5) throw new Error(`pulse speed ratio ${fast/slow} differs from 3`);
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
