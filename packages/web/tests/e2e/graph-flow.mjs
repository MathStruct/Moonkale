// Spec 031, stage 7a: the flow shapes on the real renderer — Chromium with
// SwiftShader, the graph3d.mjs setup. A tiny flow of rect blocks with side
// ports and ortho wires: the blocks draw as wide rectangles (the lit
// bounding box is wider than tall — a circle would be square), the ports
// ride the sides in edit mode, the wires step in three square-cornered
// segments, hier_layout lines the chain up left-to-right and pins it, and
// the host's harvest (node_positions) names every block.
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
const pos = (id) => page.evaluate((id) => Object.values(window.moonkale.graphViews)[0].node_screen_position(id), id);
const frame = () => page.evaluate(() => Array.from(Object.values(window.moonkale.graphViews)[0].frame_state()));
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
  await step("a three-block flow settles: rect blocks, side ports, ortho wires", async () => {
    const flow = {
      nodes: [
        { id: "src", label: "Input", kind: "file", shape: "rect", width: 46, height: 22 },
        { id: "dense", label: "Dense", kind: "file", shape: "rect", width: 46, height: 22 },
        { id: "loss", label: "Loss", kind: "file", shape: "rect", width: 46, height: 22 },
      ],
      edges: [
        { id: "w1", a: 0, b: 1, directed: true, a_port: "out", b_port: "in", route: { kind: "ortho" } },
        { id: "w2", a: 1, b: 2, directed: true, a_port: "out", b_port: "in", route: { kind: "ortho" } },
      ],
      ports: [
        { node: 0, name: "out", side: "right", direction: "out", color: "#ffe459" },
        { node: 1, name: "in", side: "left", direction: "in", color: "#ffe459" },
        { node: 1, name: "out", side: "right", direction: "out", color: "#ffe459" },
        { node: 2, name: "in", side: "left", direction: "in", color: "#ffe459" },
      ],
    };
    await page.evaluate((json) => Object.values(window.moonkale.graphViews)[0].set_graph(json), JSON.stringify(flow));
    await page.waitForFunction(() => Object.values(window.moonkale.graphViews)[0].layout_state()[0] === 0, null, { timeout: 20000 });
    await sleep(700);
    // Two ortho wires; two heads. The settled layout may leave a block
    // on the far side of its neighbour, and the route then detours round
    // it (ADR-0024): three segments when the ports face each other, up
    // to five when they face away.
    const fr = await frame();
    console.log("\n  frame [segs, arrows, ports, …]:", JSON.stringify(fr));
    if (fr[0] < 6 || fr[0] > 10) throw new Error(`wanted 3–5 segments per ortho wire, got ${fr[0]}`);
    if (fr[1] !== 2) throw new Error(`wanted 2 heads, got ${fr[1]}`);
  });
  await step("hier_layout lines the chain up left-to-right and pins it", async () => {
    await page.evaluate(() => Object.values(window.moonkale.graphViews)[0].hier_layout());
    await sleep(500);
    const [src, dense, loss] = await Promise.all([pos("src"), pos("dense"), pos("loss")]);
    console.log(`\n  columns: src ${src[0].toFixed(0)}, dense ${dense[0].toFixed(0)}, loss ${loss[0].toFixed(0)}`);
    if (!(src[0] < dense[0] - 50 && dense[0] < loss[0] - 50)) throw new Error("the chain is not left-to-right");
    if (Math.abs(src[1] - dense[1]) > 4 || Math.abs(dense[1] - loss[1]) > 4) throw new Error("the rows do not line up");
    // Lined up, every pair of ports faces each other: no detours.
    const lined = await frame();
    if (lined[0] > 6) throw new Error(`a lined-up chain still detours: ${lined[0]} segments`);
    // The host's harvest: every block named with its world position.
    const wp = JSON.parse(await page.evaluate(() => Object.values(window.moonkale.graphViews)[0].node_positions()));
    const ids = Object.keys(wp).sort();
    console.log("  node_positions:", JSON.stringify(wp));
    if (ids.join() !== "dense,loss,src") throw new Error("the harvest misses a block: " + ids.join());
    if (wp.dense[0] <= wp.src[0] || wp.loss[0] <= wp.dense[0]) throw new Error("the world columns disagree with the screen");
    // A click on the middle block selects it (the flow editing surface).
    await page.mouse.click(box.x + dense[0], box.y + dense[1]);
    await sleep(300);
    const sel = JSON.parse(await page.evaluate(() => Object.values(window.moonkale.graphViews)[0].selection_state()));
    if (sel.nodes.join() !== "dense") throw new Error("the block is not selectable: " + JSON.stringify(sel));
    await page.screenshot({ path: `${S}/s7a-hier.png` });
  });
  await step("the blocks draw as wide rectangles, their ports on the sides", async () => {
    // After the arrangement the blocks sit alone in their columns: the
    // wires' corners run mid-gap, outside a window cropped to the block
    // (half-width 46, half-height 22 world — the camera's scale says the
    // pixels). A rect is twice as wide as tall; a circle would be square.
    const s = (await page.evaluate(() => Array.from(Object.values(window.moonkale.graphViews)[0].camera_state())))[0];
    const p = await pos("dense");
    const { width, height, bpp, data } = decode(await page.screenshot({
      clip: { x: box.x, y: box.y, width: box.w, height: box.h },
    }));
    const half = { w: 46 * s + 2, h: 22 * s + 3 };
    let x0 = 1e9, y0 = 1e9, x1 = -1e9, y1 = -1e9, lit = 0;
    for (let y = Math.max(0, p[1] - half.h) | 0; y < Math.min(height, p[1] + half.h); y++) {
      for (let x = Math.max(0, p[0] - half.w) | 0; x < Math.min(width, p[0] + half.w); x++) {
        const i = (y * width + x) * bpp;
        if ((data[i] + data[i + 1] + data[i + 2]) / 3 > 60) {
          lit++;
          x0 = Math.min(x0, x); x1 = Math.max(x1, x);
          y0 = Math.min(y0, y); y1 = Math.max(y1, y);
        }
      }
    }
    const w = x1 - x0 + 1, h = y1 - y0 + 1;
    console.log(`\n  dense block at scale ${s.toFixed(2)}: ${lit} lit px, ${w}×${h}`);
    if (lit < (92 * s) * (44 * s) * 0.5) throw new Error("the block does not fill its rectangle");
    if (w < h * 1.6) throw new Error(`a rect must be wider than tall: ${w}×${h}`);
    // The ports ride the sides (edit mode): in on the left, out on the right.
    await page.evaluate(() => Object.values(window.moonkale.graphViews)[0].set_interaction("edit"));
    await sleep(300);
    const c = await pos("dense");
    const pin = await page.evaluate(() => Object.values(window.moonkale.graphViews)[0].port_screen_position("dense", "in"));
    const pout = await page.evaluate(() => Object.values(window.moonkale.graphViews)[0].port_screen_position("dense", "out"));
    console.log(`  ports: in ${pin[0].toFixed(0)} < centre ${c[0].toFixed(0)} < out ${pout[0].toFixed(0)}`);
    if (!(pin[0] < c[0] - 5 && pout[0] > c[0] + 5)) throw new Error("the ports are not on their sides");
    await page.evaluate(() => Object.values(window.moonkale.graphViews)[0].set_interaction("explore"));
    await sleep(200);
    await page.screenshot({ path: `${S}/s7a-flow.png` });
  });
  console.log("\nGRAPH-FLOW E2E: PASS");
} catch (e) {
  console.log("\nFAIL:", e.message);
  await page.screenshot({ path: `${S}/s7a-fail.png` }).catch(() => {});
  console.log(logs.filter((l) => !/WARN|Copy Value/.test(l)).slice(-15).join("\n"));
  process.exitCode = 1;
} finally {
  await browser.close();
}
