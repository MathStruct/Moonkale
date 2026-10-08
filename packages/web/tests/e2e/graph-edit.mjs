// Spec 031, stage 2: ports and edit mode on the real renderer — the
// graph3d.mjs setup (Chromium + SwiftShader). Checks: ports appear only in
// edit mode, the camera survives the mode switch, a wire dragged from one
// port to another emits `connect` with both endpoints, and a wire dropped on
// empty space cancels. The renderer never adds the edge — that is the host's
// job (validation stays outside, spec 031 §7).
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
const view = () => page.evaluate(() => Object.values(window.moonkale.graphViews)[0]);
try {
  await step("open folder; the renderer starts on a GL backend", async () => {
    await page.goto(`http://127.0.0.1:${PORT}/`, { waitUntil: "networkidle" });
    await page.waitForSelector(".wb-workspace");
    await page.click(".mk-explorer-open button[type=submit]");
    await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
    await page.click(".wb-tab:has-text('Graph')");
    await page.waitForFunction(() => /gl|gpu/i.test(document.querySelector(".mk-graph-info")?.dataset.backend || ""), null, { timeout: 30000 });
  });
  await step("a graph with ports settles; explore shows no ports", async () => {
    const graph = {
      nodes: ["a", "b", "c"].map((id) => ({ id, label: id, kind: "file" })),
      edges: [{ id: "e0", a: 0, b: 1, directed: true, a_port: "out", b_port: "in" }],
      ports: [
        { node: 0, name: "out", side: "right", direction: "out", color: "#7aa2f7" },
        { node: 1, name: "in", side: "left", direction: "in", color: "#f7768e" },
        { node: 2, name: "in", side: "left", direction: "in", color: "#f7768e" },
      ],
    };
    await page.evaluate((json) => Object.values(window.moonkale.graphViews)[0].set_graph(json), JSON.stringify(graph));
    await page.waitForFunction(() => Object.values(window.moonkale.graphViews)[0].layout_state()[0] === 0, null, { timeout: 20000 });
    await sleep(700);
    const st = await page.evaluate(() => [
      Object.values(window.moonkale.graphViews)[0].interaction(),
      Object.values(window.moonkale.graphViews)[0].frame_state(),
    ]);
    console.log("\n  explore:", JSON.stringify(st));
    if (st[0] !== "explore") throw new Error("not exploring");
    if (st[1][2] !== 0) throw new Error("ports drawn in explore mode");
  });
  const box = await page.$eval(".mk-graph-overlay", (e) => { const r = e.getBoundingClientRect(); return { x: r.left, y: r.top, w: r.width, h: r.height }; });
  const at = (p) => [box.x + p[0], box.y + p[1]];
  await step("edit mode draws the ports and keeps the camera", async () => {
    const before = await page.evaluate(() => Array.from(Object.values(window.moonkale.graphViews)[0].camera_state()));
    await page.evaluate(() => Object.values(window.moonkale.graphViews)[0].set_interaction("edit"));
    await sleep(300);
    const after = await page.evaluate(() => [
      Object.values(window.moonkale.graphViews)[0].interaction(),
      Object.values(window.moonkale.graphViews)[0].frame_state(),
      Array.from(Object.values(window.moonkale.graphViews)[0].camera_state()),
    ]);
    console.log("\n  edit:", after[0], "frame:", JSON.stringify(after[1]));
    if (after[0] !== "edit") throw new Error("mode did not switch");
    if (after[1][2] !== 3) throw new Error("ports not drawn in edit mode: " + JSON.stringify(after[1]));
    if (JSON.stringify(after[2]) !== JSON.stringify(before)) throw new Error("the mode switch moved the camera");
  });
  await step("a wire dropped on empty space cancels", async () => {
    const out = await page.evaluate(() => Object.values(window.moonkale.graphViews)[0].port_screen_position("a", "out"));
    await page.mouse.move(...at(out));
    await page.mouse.down();
    await page.mouse.move(box.x + box.w * 0.5, box.y + box.h * 0.05, { steps: 10 });
    await page.mouse.up();
    await sleep(300);
    const c = JSON.parse(await page.evaluate(() => Object.values(window.moonkale.graphViews)[0].connect_state()));
    if (c.from !== null) throw new Error("a cancelled wire connected: " + JSON.stringify(c));
  });
  await step("a wire from a:out to b:in emits connect with both endpoints", async () => {
    const out = await page.evaluate(() => Object.values(window.moonkale.graphViews)[0].port_screen_position("a", "out"));
    const bin = await page.evaluate(() => Object.values(window.moonkale.graphViews)[0].port_screen_position("b", "in"));
    await page.mouse.move(...at(out));
    await page.mouse.down();
    await page.mouse.move(...at(bin), { steps: 15 });
    await sleep(150);
    await page.mouse.up();
    await sleep(300);
    const c = JSON.parse(await page.evaluate(() => Object.values(window.moonkale.graphViews)[0].connect_state()));
    console.log("\n  connect:", JSON.stringify(c));
    if (c.from?.node !== "a" || c.from?.port !== "out") throw new Error("wrong source: " + JSON.stringify(c));
    if (c.to?.node !== "b" || c.to?.port !== "in") throw new Error("wrong target: " + JSON.stringify(c));
    // The pending wire is gone; the graph still has one edge — the renderer
    // does not add connections itself.
    const fr = await page.evaluate(() => Object.values(window.moonkale.graphViews)[0].frame_state());
    if (fr[0] !== 1) throw new Error("the renderer added the edge itself: " + JSON.stringify(fr));
    await page.screenshot({ path: `${S}/s2-edit.png` });
  });
  await step("back to explore hides the ports again", async () => {
    await page.evaluate(() => Object.values(window.moonkale.graphViews)[0].set_interaction("explore"));
    await sleep(300);
    const st = await page.evaluate(() => [
      Object.values(window.moonkale.graphViews)[0].interaction(),
      Object.values(window.moonkale.graphViews)[0].frame_state(),
    ]);
    if (st[0] !== "explore" || st[1][2] !== 0) throw new Error("ports stayed visible: " + JSON.stringify(st));
  });
  console.log("\nGRAPH-EDIT E2E: PASS");
} catch (e) {
  console.log("\nFAIL:", e.message);
  await page.screenshot({ path: `${S}/s2-edit-fail.png` }).catch(() => {});
  console.log(logs.filter((l) => !/WARN|Copy Value/.test(l)).slice(-15).join("\n"));
  process.exitCode = 1;
} finally {
  await browser.close();
}
