// ADR-0021 stage A: a subflow block in the browser. A fixture flow holds
// one subflow (an in, an out and an electrical pin) and six blocks — the
// subflow block, three ModelingToolkit components and two Lux layers —
// so the panel must resolve the subflow block through its declaration,
// take pin wires onto the declared pin (a bus, like any pin), refuse a
// wrong domain through the boundary, take dataflow wires through the
// declared in and out, and keep the whole thing in the file on Save.
// Opening the fixture from the tree is deliberate: P-183 found here was
// the shell replaying ShowPanel on the first edit (fixed on master).
import { chromium } from "playwright";
import fs from "node:fs";
const S = process.env.M1_SHOTS ?? ".";
const PORT = process.env.PORT ?? 8080;
const ROOT = process.env.M1_ROOT;
const browser = await chromium.launch({ args: ["--use-angle=swiftshader", "--enable-unsafe-swiftshader", "--ignore-gpu-blocklist"] });
const page = await browser.newPage({ viewport: { width: 1500, height: 900 } });
const logs = [];
page.on("console", (m) => { const t = m.text(); if (!t.includes("session[")) logs.push(`[${m.type()}] ${t.slice(0, 300)}`); });
const step = async (n, f) => { process.stdout.write(`- ${n} … `); await f(); console.log("ok"); };
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const status = () => page.$eval(".mk-flow-status:visible", (e) => ({ blocks: +e.dataset.blocks, wires: +e.dataset.wires, issues: +e.dataset.issues }));
const canvasRect = () => page.$eval(".mk-flow-canvas:visible", (e) => { const r = e.getBoundingClientRect(); return { x: r.left, y: r.top }; });
const screenOf = (id) => page.evaluate((id) => Object.values(window.moonkale.flowViews)[0].node_screen_position(id), id);
const portOf = (id, port) => page.evaluate(({ id, port }) => Object.values(window.moonkale.flowViews)[0].port_screen_position(id, port), { id, port });
const idOfKind = async (prefix) => {
  const pos = JSON.parse(await page.evaluate(() => Object.values(window.moonkale.flowViews)[0].node_positions()));
  const id = Object.keys(pos).find((k) => k.toLowerCase().startsWith(prefix.toLowerCase() + "-"));
  if (!id) throw new Error(`no block of kind ${prefix}`);
  return id;
};
const dragWire = async (fromId, fromPort, toId, toPort) => {
  const box = await canvasRect();
  const a = await portOf(fromId, fromPort);
  const b = await portOf(toId, toPort);
  if (!a || !b) throw new Error(`ports not found for ${fromId}.${fromPort} → ${toId}.${toPort}`);
  await page.mouse.move(box.x + a[0], box.y + a[1]);
  await page.mouse.down();
  await page.mouse.move(box.x + b[0], box.y + b[1], { steps: 20 });
  await sleep(120);
  await page.mouse.up();
  await sleep(300);
};
// One flow, two libraries: the pin half needs ModelingToolkit, the
// dataflow half Lux. Validation is per block, so a mixed flow is fine;
// only codegen insists on one library per flow.
const FIXTURE = `{
  "version": 1,
  "subflows": [
    {
      "id": "amp",
      "name": "Amplifier",
      "blocks": [],
      "wires": [],
      "inputs": [{ "name": "x", "ty": { "kind": "any" } }],
      "outputs": [{ "name": "y", "ty": { "kind": "any" } }],
      "connections": [{ "name": "p", "ty": { "kind": "named", "name": "electrical" } }]
    }
  ],
  "blocks": [
    { "id": "in_1", "kind": "lux/input", "x": 60, "y": 60, "params": {} },
    { "id": "sub_1", "kind": "subflow", "subflow": "amp", "x": 250, "y": 60, "params": {} },
    { "id": "d", "kind": "lux/dense", "x": 450, "y": 60, "params": { "out": "16" } },
    { "id": "r", "kind": "mtk/resistor", "x": 60, "y": 300, "params": { "R": "1.0" } },
    { "id": "g", "kind": "mtk/ground", "x": 60, "y": 430, "params": {} },
    { "id": "m", "kind": "mtk/mass", "x": 430, "y": 430, "params": { "m": "1.0" } }
  ],
  "wires": []
}`;
try {
  await step("open folder", async () => {
    await page.goto(`http://127.0.0.1:${PORT}/`, { waitUntil: "networkidle" });
    await page.waitForSelector(".wb-workspace");
    await page.click(".mk-explorer-open button[type=submit]");
    await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
  });
  await step("Extensions panel: enable Flow editor, ModelingToolkit and Lux", async () => {
    await page.click("#mk-rail-extensions");
    await page.waitForSelector(".mk-settings-ext", { timeout: 10000 });
    await page.click(".mk-settings-ext:has(.mk-settings-ext-name:text-is('Flow editor')) > label input");
    await page.click(".mk-settings-ext:has(.mk-settings-ext-name:text-is('ModelingToolkit library')) > label input");
    await page.click(".mk-settings-ext:has(.mk-settings-ext-name:text-is('Lux.jl library')) > label input");
    await page.waitForFunction(() => /editor-flow/.test(localStorage.getItem("moonkale.settings") || "") && /ext-mtk/.test(localStorage.getItem("moonkale.settings") || "") && /ext-lux/.test(localStorage.getItem("moonkale.settings") || ""), null, { timeout: 10000 });
  });
  await step("the fixture flow opens from the tree: six blocks, the subflow block among them", async () => {
    fs.writeFileSync(`${ROOT}/sub.flow.json`, FIXTURE);
    await page.click(".mk-tree-row[title='sub.flow.json']");
    await page.waitForSelector(".mk-flow", { timeout: 15000 });
    await page.waitForFunction(() => {
      const v = Object.values(window.moonkale.flowViews || {})[0];
      try { return v && Object.keys(JSON.parse(v.node_positions())).includes("sub_1"); } catch { return false; }
    }, null, { timeout: 15000 });
    const s = await status();
    console.log(`\n  status: ${JSON.stringify(s)}`);
    if (s.blocks !== 6) throw new Error("the fixture did not load: " + JSON.stringify(s));
    // The subflow block itself reports nothing — resolving it through its
    // subflow is the whole point; the five issues are the others' open
    // required ports (resistor p/n, ground g, mass flange, dense in).
    if (s.issues !== 5) throw new Error("expected the five non-subflow issues: " + JSON.stringify(s));
  });
  await step("the declared boundary is on the block: x left, y right, the lone pin p left below x", async () => {
    const node = await screenOf("sub_1");
    const px = await portOf("sub_1", "x");
    const py = await portOf("sub_1", "y");
    const pp = await portOf("sub_1", "p");
    if (!px || !py || !pp) throw new Error(`declared ports missing: x=${px} y=${py} p=${pp}`);
    if (await portOf("sub_1", "nope")) throw new Error("a port the subflow does not declare exists");
    if (!(px[0] < node[0] - 10)) throw new Error("x is not on the left edge");
    if (!(py[0] > node[0] + 10)) throw new Error("y is not on the right edge");
    // ADR-0024: no pin rides the top; a lone pin takes the left, spread
    // below the input it shares the side with.
    if (!(pp[0] < node[0] - 10)) throw new Error("p is not on the left edge");
    if (!(pp[1] > px[1] + 4)) throw new Error("p does not sit below x");
    console.log(`\n  node at ${node.map((n) => n.toFixed(0))} — x ${px.map((n) => n.toFixed(0))}, y ${py.map((n) => n.toFixed(0))}, p ${pp.map((n) => n.toFixed(0))}`);
  });
  await step("a pin through the boundary: the resistor and the ground join it (a bus)", async () => {
    await dragWire("r", "p", "sub_1", "p");
    let s = await status();
    if (s.wires !== 1) throw new Error("the boundary pin did not take the equation: " + JSON.stringify(s));
    await dragWire("g", "g", "sub_1", "p");
    s = await status();
    if (s.wires !== 2) throw new Error("the boundary pin is not a bus: " + JSON.stringify(s));
    if (s.issues !== 3) throw new Error("expected the resistor's open pin, the mass's flange and the dense input: " + JSON.stringify(s));
    await page.screenshot({ path: `${S}/s-subflow-pins.png` });
  });
  await step("a wrong domain through the boundary is refused", async () => {
    await dragWire("m", "flange", "sub_1", "p");
    const s = await status();
    const notice = await page.$eval(".mk-flow .mk-flow-notice", (e) => e.textContent).catch(() => "");
    console.log(`\n  status: ${JSON.stringify(s)} notice: ${notice.slice(0, 60)}`);
    if (s.wires !== 2) throw new Error("a mechanical flange connected through an electrical boundary: " + JSON.stringify(s));
    if (!/does not connect/.test(notice)) throw new Error("the refusal has no notice");
  });
  await step("dataflow through the declared in and out", async () => {
    await dragWire("in_1", "out", "sub_1", "x");
    let s = await status();
    if (s.wires !== 3) throw new Error("the declared input did not take the wire: " + JSON.stringify(s));
    await dragWire("sub_1", "y", "d", "in");
    s = await status();
    if (s.wires !== 4) throw new Error("the declared output did not take the wire: " + JSON.stringify(s));
    if (s.issues !== 2) throw new Error("only the resistor's open pin and the refused mass flange should remain: " + JSON.stringify(s));
    await page.screenshot({ path: `${S}/s-subflow-wired.png` });
  });
  await step("Save keeps the boundary wiring and the subflow declaration", async () => {
    await page.click(".mk-flow button:has-text('Save')");
    await page.waitForFunction(() => !document.querySelector(".mk-flow .mk-editor-dirty"), null, { timeout: 15000 });
    const flow = JSON.parse(fs.readFileSync(`${ROOT}/sub.flow.json`, "utf8"));
    if (flow.subflows?.length !== 1 || flow.subflows[0].id !== "amp") throw new Error("the subflow declaration was lost on save");
    const into = flow.wires.filter((w) => w.to_block === "sub_1");
    const outOf = flow.wires.filter((w) => w.from_block === "sub_1");
    if (into.filter((w) => w.to_port === "p").length !== 2) throw new Error("the pin bus did not reach the file: " + JSON.stringify(flow.wires));
    if (!into.some((w) => w.to_port === "x")) throw new Error("the declared input wire did not reach the file");
    if (!outOf.some((w) => w.from_port === "y")) throw new Error("the declared output wire did not reach the file");
    console.log(`\n  saved: ${flow.blocks.length} blocks, ${flow.wires.length} wires, subflow "${flow.subflows[0].name}" kept`);
  });
  await step("stage B: a double-click enters the subflow — the boundary rides the edges", async () => {
    const box = await canvasRect();
    const p = await screenOf("sub_1");
    await page.dblclick(".mk-flow-canvas:visible .mk-flow-ov", { position: { x: p[0], y: p[1] } });
    await page.waitForFunction(() => {
      const v = Object.values(window.moonkale.flowViews || {})[0];
      try {
        const ids = Object.keys(JSON.parse(v.node_positions()));
        return ids.includes("boundary/x") && ids.includes("boundary/y") && ids.includes("boundary/p");
      } catch { return false; }
    }, null, { timeout: 15000 });
    const crumb = await page.$eval(".mk-flow .mk-editor-path", (e) => e.textContent);
    console.log(`\n  breadcrumb: ${crumb}`);
    if (!/Amplifier/.test(crumb)) throw new Error("the breadcrumb does not name the entered subflow");
    // An empty scope: amp declares its boundary but holds no blocks.
    let s = await status();
    if (s.blocks !== 0 || s.wires !== 0) throw new Error("the entered scope should be empty: " + JSON.stringify(s));
    await page.screenshot({ path: `${S}/s-subflow-inside.png` });
  });
  await step("stage B: fill it — a palette add lands inside; the boundary wires up", async () => {
    await page.click(".mk-flow-palette-block:text-is('Dense')");
    await page.waitForFunction(() => document.querySelector(".mk-flow-status")?.dataset.blocks === "1", null, { timeout: 10000 });
    const dense = await idOfKind("dense");
    await dragWire("boundary/x", "x", dense, "in");
    let s = await status();
    if (s.wires !== 1) throw new Error("the boundary input did not feed the inside: " + JSON.stringify(s));
    await dragWire(dense, "out", "boundary/y", "y");
    s = await status();
    if (s.wires !== 2) throw new Error("the inside did not feed the boundary output: " + JSON.stringify(s));
  });
  await step("stage B: undo and redo hold across levels (a snapshot is the whole document)", async () => {
    await page.keyboard.press("Control+z");
    await sleep(300);
    let s = await status();
    if (s.wires !== 1) throw new Error("undo did not drop the inner wire: " + JSON.stringify(s));
    await page.keyboard.press("Control+y");
    await sleep(300);
    s = await status();
    if (s.wires !== 2) throw new Error("redo did not restore the inner wire: " + JSON.stringify(s));
  });
  await step("stage B: Escape pops out; the parent view is back", async () => {
    await page.keyboard.press("Escape");
    await page.waitForFunction(() => {
      const v = Object.values(window.moonkale.flowViews || {})[0];
      try { return Object.keys(JSON.parse(v.node_positions())).includes("sub_1"); } catch { return false; }
    }, null, { timeout: 15000 });
    const s = await status();
    console.log(`\n  back at the root: ${JSON.stringify(s)}`);
    if (s.blocks !== 6) throw new Error("the parent view did not come back: " + JSON.stringify(s));
  });
  await step("stage B: an empty-canvas click pops out too (the other way back)", async () => {
    const box = await canvasRect();
    const p = await screenOf("sub_1");
    await page.dblclick(".mk-flow-canvas:visible .mk-flow-ov", { position: { x: p[0], y: p[1] } });
    await page.waitForFunction(() => {
      const v = Object.values(window.moonkale.flowViews || {})[0];
      try { return Object.keys(JSON.parse(v.node_positions())).includes("boundary/x"); } catch { return false; }
    }, null, { timeout: 15000 });
    await page.mouse.click(box.x + 6, box.y + 6);
    await page.waitForFunction(() => {
      const v = Object.values(window.moonkale.flowViews || {})[0];
      try { return Object.keys(JSON.parse(v.node_positions())).includes("sub_1"); } catch { return false; }
    }, null, { timeout: 15000 });
  });
  await step("stage B: the palette creates a subsystem you enter and fill", async () => {
    await page.click(".mk-flow-subsystem");
    await page.waitForFunction(() => document.querySelector(".mk-flow-status")?.dataset.blocks === "7", null, { timeout: 10000 });
    const sub = await idOfKind("subsystem");
    // The palette add grows the scene, so a refit may move the camera
    // under the measured position — settle, then double-click with a
    // fresh measure each try (the mtk lesson).
    let entered = false;
    for (let i = 0; i < 3 && !entered; i++) {
      await sleep(500);
      const p = await screenOf(sub);
      await page.dblclick(".mk-flow-canvas:visible .mk-flow-ov", { position: { x: p[0], y: p[1] } });
      await sleep(500);
      entered = /Subsystem \d/.test(await page.$eval(".mk-flow .mk-editor-path", (e) => e.textContent));
    }
    if (!entered) throw new Error("the double-click did not enter the created subsystem");
    // Empty: no blocks, no boundary (declaring ports is its own UX —
    // the ADR leaves it open).
    let s = await status();
    if (s.blocks !== 0) throw new Error("a fresh subsystem is not empty: " + JSON.stringify(s));
    await page.click(".mk-flow-palette-block:text-is('Input')");
    await page.waitForFunction(() => document.querySelector(".mk-flow-status")?.dataset.blocks === "1", null, { timeout: 10000 });
    await page.keyboard.press("Escape");
    await sleep(400);
    s = await status();
    if (s.blocks !== 7) throw new Error("Escape did not return to the parent: " + JSON.stringify(s));
  });
  await step("stage B: Save keeps the inside — both subflows' contents", async () => {
    await page.click(".mk-flow button:has-text('Save')");
    await page.waitForFunction(() => !document.querySelector(".mk-flow .mk-editor-dirty"), null, { timeout: 15000 });
    const flow = JSON.parse(fs.readFileSync(`${ROOT}/sub.flow.json`, "utf8"));
    if (flow.subflows?.length !== 2) throw new Error("the created subsystem's subflow was lost: " + JSON.stringify(flow.subflows?.map((x) => x.id)));
    const amp = flow.subflows.find((x) => x.id === "amp");
    if (amp.blocks.length !== 1 || amp.wires.length !== 2) throw new Error("the inside edit did not reach the file: " + JSON.stringify(amp));
    if (!amp.wires.some((w) => w.from_block === "boundary/x")) throw new Error("the boundary input wire is gone");
    const made = flow.subflows.find((x) => x.id.startsWith("subflow-"));
    if (!made || made.blocks.length !== 1) throw new Error("the filled subsystem lost its block: " + JSON.stringify(made));
    console.log(`\n  saved: ${flow.subflows.map((x) => `${x.id}(${x.blocks.length} blocks)`).join(", ")}`);
  });
  console.log("\nSUBFLOW E2E: PASS");
} catch (e) {
  console.log("\nFAIL:", e.message);
  await page.screenshot({ path: `${S}/s-subflow-fail.png` }).catch(() => {});
  console.log(logs.filter((l) => !/WARN|Copy Value/.test(l)).slice(-15).join("\n"));
  process.exitCode = 1;
} finally {
  await browser.close();
}
