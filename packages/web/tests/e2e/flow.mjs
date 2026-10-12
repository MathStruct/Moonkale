// Milestone 6 + spec 031 stages 7b/7c: the flow editor on the graph
// engine (ADR-0018) — Chromium with SwiftShader, like the graph suites,
// because the canvas is wgpu now. Covers: the extension toggle and New
// Flow, the palette, wiring by dragging ports (typed: a mismatch is
// refused with a notice), the parameter editor floating beside the
// selected block (one HTML editor, spec 031 §7 — and spec 001's
// keyboard guard), undo/redo of a wire, a drag, a parameter and a
// delete (a pinned block moves back to the document's coordinates),
// the layered Layout, Save and Lux codegen, unknown-kind issues when
// the library is disabled, and the broken-file guard (#11).
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
// The engine view behind this panel: positions and ports are its to report.
const canvasRect = () => page.$eval(".mk-flow-canvas", (e) => { const r = e.getBoundingClientRect(); return { x: r.left, y: r.top }; });
const idOfKind = async (prefix) => {
  const pos = JSON.parse(await page.evaluate(() => Object.values(window.moonkale.flowViews)[0].node_positions()));
  // The palette shows the block's name ("Dense"); its id is the kind's
  // ("dense-…").
  const id = Object.keys(pos).find((k) => k.toLowerCase().startsWith(prefix.toLowerCase() + "-"));
  if (!id) throw new Error(`no block of kind ${prefix}`);
  return id;
};
const screen = (id) => page.evaluate((id) => Object.values(window.moonkale.flowViews)[0].node_screen_position(id), id);
const worldPos = async (id) => (JSON.parse(await page.evaluate(() => Object.values(window.moonkale.flowViews)[0].node_positions())))[id];
// A block's position before/after, for the keyboard guard (no DOM transform
// exists any more — the canvas owns the pixels, the document the numbers).
const posOf = async (id) => JSON.stringify(await worldPos(id));
const dragWire = async (fromId, fromPort, toId, toPort) => {
  const box = await canvasRect();
  const a = await page.evaluate(({ id, port }) => Object.values(window.moonkale.flowViews)[0].port_screen_position(id, port), { id: fromId, port: fromPort });
  const b = await page.evaluate(({ id, port }) => Object.values(window.moonkale.flowViews)[0].port_screen_position(id, port), { id: toId, port: toPort });
  if (!a || !b) throw new Error(`ports not found for ${fromId}.${fromPort} → ${toId}.${toPort}`);
  await page.mouse.move(box.x + a[0], box.y + a[1]);
  await page.mouse.down();
  await page.mouse.move(box.x + b[0], box.y + b[1], { steps: 20 });
  await sleep(120);
  await page.mouse.up();
  await sleep(300);
};
try {
  await step("open folder; File menu has no 'New Flow…' while the flow extension is off", async () => {
    await page.goto(`http://127.0.0.1:${PORT}/`, { waitUntil: "networkidle" });
    await page.waitForSelector(".wb-workspace");
    await page.click(".mk-explorer-open button[type=submit]");
    await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
    await page.click(".mk-menu-button:has-text('File')");
    if (await page.$(".mk-menu-item:has-text('New Flow')")) throw new Error("New Flow visible while disabled");
    await page.click(".mk-menu-backdrop");
  });
  await step("Extensions panel: enable Flow editor and Lux.jl (user scope)", async () => {
    await page.click("#mk-rail-extensions");
    await page.waitForSelector(".mk-settings-ext", { timeout: 10000 });
    const names = await page.$$eval(".mk-settings-ext-name", (e) => e.map((x) => x.textContent));
    console.log("\n  catalog:", names.join(", "));
    await page.click(".mk-settings-ext:has(.mk-settings-ext-name:text-is('Flow editor')) > label input");
    await page.click(".mk-settings-ext:has(.mk-settings-ext-name:text-is('Lux.jl library')) > label input");
    await page.waitForFunction(() => /editor-flow/.test(localStorage.getItem("moonkale.settings") || "") && /ext-lux/.test(localStorage.getItem("moonkale.settings") || ""), null, { timeout: 10000 });
  });
  await step("File → New Flow… creates untitled.flow.json and opens the flow editor", async () => {
    await page.click(".mk-menu-button:has-text('File')");
    await page.click(".mk-menu-item:has-text('New Flow')");
    await page.waitForSelector(".mk-flow", { timeout: 15000 });
    await page.waitForFunction(() => /untitled\.flow\.json/.test(document.querySelector(".mk-titlebar-title")?.textContent || ""), null, { timeout: 15000 });
    if (!fs.existsSync(`${ROOT}/untitled.flow.json`)) throw new Error("file not created");
  });
  await step("palette adds Input → Conv → MaxPool → Flatten → Dense → Loss → Optimiser; the canvas draws them", async () => {
    await page.waitForSelector(".mk-flow-palette-block", { timeout: 10000 });
    for (const b of ["Input", "Conv", "MaxPool", "Flatten", "Dense", "Loss", "Optimiser"]) {
      await page.click(`.mk-flow-palette-block:text-is('${b}')`);
    }
    await page.waitForFunction(() => document.querySelector(".mk-flow-status")?.dataset.blocks === "7", null, { timeout: 10000 });
    const s = await status();
    console.log("\n  status:", JSON.stringify(s));
    if (s.issues === 0) throw new Error("unwired required inputs should be issues");
    // The engine has the view: seven blocks, positioned where the palette
    // put them (the panel pins them — the document owns the coordinates).
    const pos = JSON.parse(await page.evaluate(() => Object.values(window.moonkale.flowViews)[0].node_positions()));
    if (Object.keys(pos).length !== 7) throw new Error("the canvas does not hold seven blocks");
    await page.screenshot({ path: `${S}/s7b-flow.png` });
  });
  await step("editing a parameter: Backspace/Delete/arrows in the field do not delete or move the block (spec 001)", async () => {
    // Select the Dense block by clicking it where the engine drew it; the
    // side editor (one editor for the selected node, spec 031 §7) appears.
    const dense = await idOfKind("Dense");
    const box = await canvasRect();
    const p = await screen(dense);
    const before = await posOf(dense);
    await page.mouse.click(box.x + p[0], box.y + p[1]);
    await page.waitForSelector(".mk-flow-props", { timeout: 10000 });
    const field = page.locator(".mk-flow-props .mk-flow-param:has-text('out') input");
    await field.click();
    await page.keyboard.press("End");
    for (let i = 0; i < 6; i++) await page.keyboard.press("Backspace");
    await page.keyboard.press("Delete");
    await page.keyboard.press("ArrowLeft");
    await page.keyboard.type("42");
    await page.keyboard.press("Enter");
    await page.keyboard.press("Tab");
    await page.waitForTimeout(300);
    const blocks = await page.$eval(".mk-flow-status:visible", (e) => e.dataset.blocks);
    const after = await posOf(dense);
    const value = await field.inputValue();
    console.log("\n  blocks:", blocks, "out:", value, "position held:", before === after);
    if (blocks !== "7") throw new Error("a block was deleted while editing a field");
    if (value !== "42") throw new Error(`out = ${value}`);
    if (after !== before) throw new Error(`the block moved while editing a field: ${before} → ${after}`);
    // Deselect: the floating editor sits where the wire gestures go —
    // an open editor over a port swallows the drop (it is a real panel).
    await page.mouse.click(box.x + 6, box.y + 6);
    await sleep(200);
  });
  await step("wiring by dragging ports (typed): 6 wires, no issues", async () => {
    const id = async (p) => idOfKind(p);
    await dragWire(await id("Input"), "out", await id("Conv"), "in");
    await dragWire(await id("Conv"), "out", await id("MaxPool"), "in");
    await dragWire(await id("MaxPool"), "out", await id("Flatten"), "in");
    await dragWire(await id("Flatten"), "out", await id("Dense"), "in");
    await dragWire(await id("Dense"), "out", await id("Loss"), "prediction");
    await dragWire(await id("Loss"), "loss", await id("Optimiser"), "loss");
    const s = await status();
    console.log("\n  status:", JSON.stringify(s));
    if (s.wires !== 6 || s.issues !== 0) throw new Error("wiring incomplete");
    await page.screenshot({ path: `${S}/s7b-wired.png` });
  });
  await step("a type mismatch is refused (Conv image → Dense vector)", async () => {
    await dragWire(await idOfKind("Conv"), "out", await idOfKind("Dense"), "in");
    await page.waitForTimeout(300);
    const s = await status();
    const notice = await page.$eval(".mk-flow .mk-flow-notice", (e) => e.textContent).catch(() => "");
    console.log("\n  status:", JSON.stringify(s), "notice:", notice.slice(0, 60));
    if (s.wires !== 6 || s.issues !== 0) throw new Error("mismatched wire was accepted: " + JSON.stringify(s));
    if (!/does not fit/.test(notice)) throw new Error("the refusal has no notice");
  });
  await step("the parameter editor floats beside its block (spec 031 §7)", async () => {
    // Select the Dense block; the one HTML editor opens beside it — its
    // position comes from the engine's screen report.
    const dense = await idOfKind("Dense");
    const box = await canvasRect();
    const p = await screen(dense);
    await page.mouse.click(box.x + p[0], box.y + p[1]);
    await page.waitForSelector(".mk-flow-props", { timeout: 10000 });
    const rect = await page.$eval(".mk-flow-props", (e) => { const r = e.getBoundingClientRect(); return { x: r.left, y: r.top }; });
    // The editor sits at the node's screen position + the panel's offset
    // (60 right, 18 up) — canvas-relative engine coords against the page.
    console.log(`\n  editor at ${rect.x.toFixed(0)},${rect.y.toFixed(0)} — node at ${(box.x + p[0] + 60).toFixed(0)},${(box.y + p[1] - 18).toFixed(0)}`);
    if (Math.abs(rect.x - (box.x + p[0] + 60)) > 4 || Math.abs(rect.y - (box.y + p[1] - 18)) > 4) {
      throw new Error("the editor does not sit beside its block");
    }
    await page.keyboard.press("Escape"); // nothing bound; deselect below
    await page.mouse.click(box.x + 6, box.y + 6); // empty canvas: deselect    await sleep(200);
    if (await page.$(".mk-flow-props")) throw new Error("the editor survived the deselection");
  });
  await step("undo and redo: a wire, a drag, a parameter, a delete (Ctrl+Z / Ctrl+Y)", async () => {
    // A wire: the last wire goes, then comes back.
    await page.keyboard.press("Control+z");
    await sleep(250);
    let s = await status();
    if (s.wires !== 5) throw new Error("undo did not drop the wire: " + JSON.stringify(s));
    await page.keyboard.press("Control+y");
    await sleep(250);
    s = await status();
    if (s.wires !== 6) throw new Error("redo did not restore the wire: " + JSON.stringify(s));
    // A drag: the block moved, undo puts it back — the engine moves a
    // pinned block to its restored coordinates (the document owns them).
    const dense = await idOfKind("Dense");
    const box = await canvasRect();
    const before = await worldPos(dense);
    const p = await screen(dense);
    await page.mouse.move(box.x + p[0], box.y + p[1]);
    await page.mouse.down();
    await page.mouse.move(box.x + p[0] + 120, box.y + p[1] + 40, { steps: 10 });
    await sleep(150);
    await page.mouse.up();
    await sleep(300);
    const after = await worldPos(dense);
    console.log(`\n  dense ${before[0].toFixed(1)},${before[1].toFixed(1)} → ${after[0].toFixed(1)},${after[1].toFixed(1)}`);
    if (Math.abs(after[0] - before[0]) < 20 || Math.abs(after[1] - before[1]) < 10) throw new Error("the drag did not move the block");
    await page.keyboard.press("Control+z");
    await sleep(300);
    const undone = await worldPos(dense);
    if (Math.abs(undone[0] - before[0]) > 1 || Math.abs(undone[1] - before[1]) > 1) {
      throw new Error(`undo did not restore the position: ${undone} vs ${before}`);
    }
    await page.keyboard.press("Control+y");
    await sleep(300);
    const redone = await worldPos(dense);
    if (Math.abs(redone[0] - after[0]) > 1 || Math.abs(redone[1] - after[1]) > 1) {
      throw new Error(`redo did not restore the drag: ${redone} vs ${after}`);
    }
    // A parameter: Ctrl+Z in the field is the field's own text undo, not
    // the flow's (spec 001); the flow's undo restores the last edit.
    const pd = await screen(dense);
    await page.mouse.click(box.x + pd[0], box.y + pd[1]);
    await page.waitForSelector(".mk-flow-props", { timeout: 10000 });
    const field = page.locator(".mk-flow-props .mk-flow-param:has-text('out') input");
    await field.click();
    await page.keyboard.press("Control+z");
    await sleep(200);
    s = await status();
    if (s.wires !== 6) throw new Error("a field's Ctrl+Z undid the flow: " + JSON.stringify(s));
    await field.fill("24");
    await page.keyboard.press("Enter");
    await sleep(200);
    await page.mouse.click(box.x + 6, box.y + 6); // deselect; the editor goes
    await sleep(200);
    await page.keyboard.press("Control+z"); // the flow's undo: out back to 42
    await sleep(300);
    s = await status();
    if (s.wires !== 6 || s.blocks !== 7) throw new Error("the param undo broke the flow: " + JSON.stringify(s));
    const pd2 = await screen(dense);
    await page.mouse.click(box.x + pd2[0], box.y + pd2[1]);
    await page.waitForSelector(".mk-flow-props", { timeout: 10000 });
    const value = await page.$eval(".mk-flow-props .mk-flow-param:has-text('out') input", (e) => e.value);
    console.log("\n  out after undo:", value);
    if (value !== "42") throw new Error(`undo did not restore the parameter: ${value}`);
    // A delete: the block and its wires come back.
    await page.keyboard.press("Delete");
    await sleep(300);
    s = await status();
    if (s.blocks !== 6 || s.wires !== 4) throw new Error("delete removed the wrong things: " + JSON.stringify(s));
    await page.keyboard.press("Control+z");
    await sleep(300);
    s = await status();
    if (s.blocks !== 7 || s.wires !== 6 || s.issues !== 0) throw new Error("undo did not restore the deleted block: " + JSON.stringify(s));
    await page.screenshot({ path: `${S}/s7c-undo.png` });
  });
  await step("Layout lines the flow up; Save writes the flow JSON with the coordinates", async () => {
    await page.click(".mk-flow button:has-text('Layout')");
    await sleep(400);
    const dense = await idOfKind("Dense");
    const loss = await idOfKind("Loss");
    const wd = await worldPos(dense);
    const wl = await worldPos(loss);
    console.log(`\n  dense x ${wd[0].toFixed(1)}, loss x ${wl[0].toFixed(1)}`);
    if (!(wl[0] > wd[0] + 50)) throw new Error("the layout did not order the chain");
    await page.click(".mk-flow button:has-text('Save')");
    await page.waitForFunction(() => !document.querySelector(".mk-flow .mk-editor-dirty"), null, { timeout: 15000 });
    const flow = JSON.parse(fs.readFileSync(`${ROOT}/untitled.flow.json`, "utf8"));
    if (flow.blocks.length !== 7 || flow.wires.length !== 6) throw new Error("flow json wrong");
    const xs = flow.blocks.map((b) => b.x);
    if (!xs.every((x) => Number.isFinite(x))) throw new Error("positions did not reach the document");
    // The Layout's harvest must match the engine exactly (the review's
    // #4): the reply is parsed into a map now, so the document's
    // coordinates are the engine's — not the pre-layout ones.
    const eng = JSON.parse(await page.evaluate(() => Object.values(window.moonkale.flowViews)[0].node_positions()));
    for (const b of flow.blocks) {
      const p = eng[b.id];
      if (!p) throw new Error(`block ${b.id} is missing from the engine`);
      if (Math.abs(p[0] - b.x) > 0.01 || Math.abs(p[1] - b.y) > 0.01) {
        throw new Error(`document and engine disagree on ${b.id}: ${b.x},${b.y} vs ${p[0]},${p[1]}`);
      }
    }
  });
  await step("Generate julia writes model.jl and opens it", async () => {
    await page.click(".mk-flow-generate");
    await page.waitForFunction(() => /model\.jl/.test(document.querySelector(".mk-titlebar-title")?.textContent || ""), null, { timeout: 15000 });
    const jl = fs.readFileSync(`${ROOT}/model.jl`, "utf8");
    console.log("\n  model.jl:", jl.split("\n").find((l) => l.includes("Conv(")).trim());
    if (!/Conv\(\(3, 3\), 1 => 16, relu; pad=1\)/.test(jl)) throw new Error("unexpected codegen");
  });
  await step("disabling Lux hides its blocks: the flow reports unknown kinds", async () => {
    await page.click(".wb-tab:has-text('Extensions')");
    await page.click(".mk-settings-ext:has(.mk-settings-ext-name:text-is('Lux.jl library')) > label input");
    await page.click(".wb-tab:has-text('untitled.flow.json')");
    await page.waitForFunction(() => +document.querySelector(".mk-flow-status")?.dataset.issues >= 7, null, { timeout: 10000 });
    await page.click(".wb-tab:has-text('Extensions')");
    await page.click(".mk-settings-ext:has(.mk-settings-ext-name:text-is('Lux.jl library')) > label input");
    await page.click(".wb-tab:has-text('untitled.flow.json')");
    await sleep(500);
  });
  await step("an unparseable .flow.json shows an error and is never written (#11); fixed on disk, Reload loads it", async () => {
    const bad = '{"version": 1, "blocks": [ {"id": "b1", "kind": "lux/Dense", "x": 0, "y": 0, "params": {} } ';
    fs.writeFileSync(`${ROOT}/broken.flow.json`, bad);
    if (!(await page.isVisible(".mk-explorer"))) await page.click("#mk-rail-explorer");
    await page.click(".mk-tree-row[title='broken.flow.json']");
    await page.waitForSelector(".mk-flow-broken", { timeout: 15000 });
    await page.click(".mk-flow-reload:visible");
    await page.waitForSelector(".mk-flow-broken", { timeout: 15000 });
    const untouched = fs.readFileSync(`${ROOT}/broken.flow.json`, "utf8");
    if (untouched !== bad) throw new Error("the broken file was written");
    fs.writeFileSync(`${ROOT}/broken.flow.json`, '{"version":1,"blocks":[{"id":"b1","kind":"lux/Dense","x":10,"y":10,"params":{"out":"7"}}],"wires":[]}');
    await page.click(".mk-flow-reload:visible");
    await sleep(500);
    const s = await status();
    console.log("\n  reloaded:", JSON.stringify(s));
    if (s.blocks !== "1" && s.blocks !== 1) throw new Error("Reload did not load the fixed file: " + JSON.stringify(s));
  });
  await step("the #11 guard: a stale selection against a broken file never writes (#3)", async () => {
    // Select the block — then the file goes broken on disk. Reload shows
    // the error; Backspace/Delete against the stale selection must not
    // write the old flow over the user's broken file.
    const box = await canvasRect();
    await page.waitForFunction(() => {
      const v = Object.values(window.moonkale.flowViews || {})[0];
      return v && Object.keys(JSON.parse(v.node_positions())).includes("b1");
    }, null, { timeout: 15000 });
    const p = await screen("b1");
    await page.mouse.click(box.x + p[0], box.y + p[1]);
    await page.waitForSelector(".mk-flow-props", { timeout: 10000 });
    const bad = '{ broken — not a flow';
    fs.writeFileSync(`${ROOT}/broken.flow.json`, bad);
    await page.click(".mk-flow-reload:visible");
    await page.waitForSelector(".mk-flow-broken", { timeout: 15000 });
    // The toolbar offers nothing but Reload while broken.
    for (const label of ["Layout", "Fit", "Save"]) {
      const disabled = await page.$eval(`.mk-flow button:has-text('${label}')`, (e) => e.disabled);
      if (!disabled) throw new Error(`${label} is clickable while the document is broken`);
    }
    await page.keyboard.press("Backspace");
    await page.keyboard.press("Delete");
    await sleep(300);
    const untouched = fs.readFileSync(`${ROOT}/broken.flow.json`, "utf8");
    console.log("\n  broken file held:", untouched === bad);
    if (untouched !== bad) throw new Error("Delete wrote to a broken file (#11)");
  });
  await step("a valid file after a broken spell draws again (the canvas re-mounts, #5)", async () => {
    fs.writeFileSync(`${ROOT}/broken.flow.json`, '{"version":1,"blocks":[{"id":"b1","kind":"lux/Dense","x":10,"y":10,"params":{"out":"7"}}],"wires":[]}');
    await page.click(".mk-flow-reload:visible");
    await sleep(300);
    // The old renderer was torn down; a fresh one must have mounted on
    // the new canvas div — a blank canvas here means the mount guard
    // refused the second init (the review's #5).
    await page.waitForFunction(() => {
      const views = Object.values(window.moonkale.flowViews || {});
      if (views.length !== 1) return false;
      const pos = JSON.parse(views[0].node_positions());
      return Object.keys(pos).length === 1 && "b1" in pos;
    }, null, { timeout: 15000 });
    const s = await status();
    console.log("\n  reloaded:", JSON.stringify(s));
    if (+s.blocks !== 1) throw new Error("the canvas did not re-mount: " + JSON.stringify(s));
  });
  console.log("\nFLOW E2E: PASS");
} catch (e) {
  console.log("\nFAIL:", e.message);
  await page.screenshot({ path: `${S}/s7b-fail.png` }).catch(() => {});
  console.log(logs.filter((l) => !/WARN|Copy Value/.test(l)).slice(-15).join("\n"));
  process.exitCode = 1;
} finally {
  await browser.close();
}
