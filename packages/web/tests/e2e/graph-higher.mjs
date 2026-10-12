// Spec 031, stage 5: higher-order connections on the real renderer —
// Chromium with SwiftShader, the graph3d.mjs setup. A graph shows all
// three classes at once, and the checks tell them apart by hue counts:
// a plain group's green wash, a highlighted clique's magenta tint+rim,
// a hyperedge's purple junction and spokes, and a filled 2-simplex's
// orange face — plus the counts (frame_state) and a delta round-trip
// (remove/re-add by id), and the LOD fade of the face at low zoom.
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
const frame = () => page.evaluate(() => Array.from(Object.values(window.moonkale.graphViews)[0].frame_state()));
// Hue counts: each class owns a colour no other class, node, edge or port
// produces (the tint and rim differ by channel dominance, not brightness).
const hues = (png, match) => {
  const { width, height, bpp, data } = decode(png);
  let n = 0;
  for (let i = 0; i < width * height; i++) {
    const o = i * bpp;
    if (match(data[o], data[o + 1], data[o + 2])) n++;
  }
  return n;
};
const GREEN = (r, g, b) => g > r + 15 && g > b + 10;                       // the group's wash
const MAGENTA = (r, g, b) => r > g + 20 && r > b;                          // the clique's tint + rim
const PURPLE = (r, g, b) => b > r && r > 125 && b > g + 40;               // the hyperedge's spokes + marker
const ORANGE = (r, g, b) => r > 150 && b < 100 && g > 80;                 // the simplex's face
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
  await step("a graph with a group, a clique, a hyperedge and a simplex settles", async () => {
    const graph = {
      nodes: ["a", "b", "c", "d", "e"].map((id) => ({ id, label: id, kind: "file" })),
      edges: [{ id: "ab", a: 0, b: 1 }, { id: "bc", a: 1, b: 2 }],
      // Four classes over the same five nodes (spec 031 §2): a group, a
      // highlighted clique overlapping it, a hyperedge and a 2-simplex.
      groups: [
        { id: "mod", label: "module", members: ["a", "b", "c"], color: "#59ff8c" },
        { id: "clique", label: "clique", members: ["b", "c", "d"], color: "#ff59c8", highlight: true },
      ],
      hyperedges: [{ id: "h", nodes: ["c", "d", "e"], color: "#bb77ee" }],
      simplexes: [{ id: "s", nodes: ["a", "b", "c"], color: "#ff9933" }],
    };
    await page.evaluate((json) => Object.values(window.moonkale.graphViews)[0].set_graph(json), JSON.stringify(graph));
    await page.waitForFunction(() => Object.values(window.moonkale.graphViews)[0].layout_state()[0] === 0, null, { timeout: 20000 });
    await sleep(700);
    const fr = await frame();
    console.log("\n  frame [segs, arrows, ports, hulls, simplexes, junctions]:", JSON.stringify(fr));
    if (fr[3] !== 2) throw new Error(`wanted 2 hulls, built ${fr[3]}`);
    if (fr[4] !== 1) throw new Error(`wanted 1 simplex triangle, built ${fr[4]}`);
    if (fr[5] !== 1) throw new Error(`wanted 1 junction, built ${fr[5]}`);
  });
  await step("the four classes read as four hues over the same graph", async () => {
    const png = await page.screenshot({ clip });
    const counts = {
      group: hues(png, GREEN),
      clique: hues(png, MAGENTA),
      hyperedge: hues(png, PURPLE),
      simplex: hues(png, ORANGE),
    };
    console.log("\n  px by class:", JSON.stringify(counts));
    if (counts.group < 500) throw new Error("the plain group does not tint");
    if (counts.clique < 100) throw new Error("the highlighted clique does not read");
    if (counts.hyperedge < 40) throw new Error("the hyperedge's spokes and marker do not read");
    if (counts.simplex < 500) throw new Error("the 2-simplex does not fill");
    await page.screenshot({ path: `${S}/s5-classes.png` });
  });
  await step("a delta removes and re-adds the hyperedge and the simplex by id", async () => {
    await page.evaluate((json) => Object.values(window.moonkale.graphViews)[0].apply_delta(json), JSON.stringify({
      remove_hyperedges: ["h"],
      remove_simplexes: ["s"],
    }));
    await sleep(400);
    let fr = await frame();
    if (fr[4] !== 0 || fr[5] !== 0) throw new Error(`removal left faces/junctions: ${JSON.stringify(fr)}`);
    let png = await page.screenshot({ clip });
    if (hues(png, PURPLE) > 10) throw new Error("a spoke or marker survived the removal");
    if (hues(png, ORANGE) > 10) throw new Error("a face survived the removal");
    // The group and the clique are untouched — the classes are independent.
    if (hues(png, GREEN) < 500 || hues(png, MAGENTA) < 100) throw new Error("the removal bled into other classes");
    await page.evaluate((json) => Object.values(window.moonkale.graphViews)[0].apply_delta(json), JSON.stringify({
      add_hyperedges: [{ id: "h", nodes: ["c", "d", "e"], color: "#bb77ee" }],
      add_simplexes: [{ id: "s", nodes: ["a", "b", "c"], color: "#ff9933" }],
    }));
    await sleep(400);
    fr = await frame();
    png = await page.screenshot({ clip });
    console.log("\n  re-added:", JSON.stringify(fr), "purple px:", hues(png, PURPLE), "orange px:", hues(png, ORANGE));
    if (fr[4] !== 1 || fr[5] !== 1) throw new Error(`the re-add did not restore: ${JSON.stringify(fr)}`);
    if (hues(png, PURPLE) < 40) throw new Error("the hyperedge did not come back");
    if (hues(png, ORANGE) < 500) throw new Error("the simplex did not come back");
  });
  await step("zoomed out, the face fades (LOD §6); the junction stays", async () => {
    // Scroll-down zooms out (1/1.12 a notch); the triangle's smallest
    // edge falls under the 6 px fade threshold long before the floor.
    for (let i = 0; i < 60; i++) {
      const c = await cam();
      if (c[0] < 0.05) break;
      await page.mouse.move(box.x + box.w / 2, box.y + box.h / 2);
      await page.mouse.wheel(0, 300);
      await sleep(90);
    }
    const png = await page.screenshot({ clip });
    const orange = hues(png, ORANGE);
    const purple = hues(png, PURPLE);
    console.log(`\n  orange px: ${orange}, purple px: ${purple}`);
    if (orange > 10) throw new Error("the face still fills below the pixel threshold");
    if (purple === 0) throw new Error("the junction marker vanished (structural, must stay)");
    await page.screenshot({ path: `${S}/s5-lod.png` });
  });
  console.log("\nGRAPH-HIGHER E2E: PASS");
} catch (e) {
  console.log("\nFAIL:", e.message);
  await page.screenshot({ path: `${S}/s5-fail.png` }).catch(() => {});
  console.log(logs.filter((l) => !/WARN|Copy Value/.test(l)).slice(-15).join("\n"));
  process.exitCode = 1;
} finally {
  await browser.close();
}
