// Milestone 6: opt-in extensions (Settings → Extensions), the flow editor and the Lux.jl library.
import { firefox } from "playwright";
import fs from "node:fs";
const S = process.env.M1_SHOTS ?? ".";
const PORT = process.env.PORT ?? 8080;
const ROOT = process.env.M1_ROOT;
const browser = await firefox.launch();
const page = await browser.newPage({ viewport: { width: 1500, height: 900 } });
const logs = [];
page.on("console", (m) => { const t = m.text(); if (!t.includes("session[")) logs.push(`[${m.type()}] ${t.slice(0, 300)}`); });
const step = async (n, f) => { process.stdout.write(`- ${n} … `); await f(); console.log("ok"); };
const status = () => page.$eval(".mk-flow-status", (e) => ({ blocks: +e.dataset.blocks, wires: +e.dataset.wires, issues: +e.dataset.issues }));
const handle = (block, port) => page.$(`.mk-flow-block:has(.mk-flow-block-title:text-is('${block}')) .mk-port-${port}`);
const drag = async (fromBlock, fromPort, toBlock, toPort) => {
  const from = await handle(fromBlock, fromPort); const to = await handle(toBlock, toPort);
  if (!from || !to) throw new Error(`handles not found for ${fromBlock}.${fromPort} → ${toBlock}.${toPort}`);
  const a = await from.boundingBox(); const b = await to.boundingBox();
  // A click on empty canvas first: settles any pending gesture (P-073).
  await page.mouse.click(700, 600);
  await page.waitForTimeout(150);
  await page.mouse.move(a.x + a.width / 2, a.y + a.height / 2);
  await page.mouse.down();
  await page.waitForTimeout(100);
  await page.mouse.move(b.x + b.width / 2, b.y + b.height / 2, { steps: 20 });
  await page.waitForTimeout(150);
  await page.mouse.up();
  await page.waitForTimeout(300);
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
  await step("Settings → Extensions: enable Flow editor and Lux.jl (user scope)", async () => {
    await page.click(".wb-status-bar");
    await page.keyboard.press("Control+,");
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
  await step("palette shows Lux blocks; add Input → Conv → MaxPool → Flatten → Dense → Loss → Optimiser", async () => {
    await page.waitForSelector(".mk-flow-palette-block", { timeout: 10000 });
    for (const b of ["Input", "Conv", "MaxPool", "Flatten", "Dense", "Loss", "Optimiser"]) {
      await page.click(`.mk-flow-palette-block:text-is('${b}')`);
    }
    await page.waitForFunction(() => document.querySelector(".mk-flow-status")?.dataset.blocks === "7", null, { timeout: 10000 });
    const s = await status();
    console.log("\n  status:", JSON.stringify(s));
    if (s.issues === 0) throw new Error("unwired required inputs should be issues");
  });
  await step("editing a parameter: Backspace/Delete/arrows in the field do not delete or move the block (spec 001)", async () => {
    // The Dense block's `units` field: select it, clear it with Backspace, type a value.
    const field = page.locator(".df-node:has(.mk-flow-block-title:text-is('Dense')) .mk-flow-param:has-text('out') input");
    // The node's `transform` is its position (its rect also changes with the selection scale effect).
    const pos = () => page.$eval(".df-node:has(.mk-flow-block-title:text-is('Dense'))", (n) => n.style.transform);
    const before = await pos();
    await field.click();
    await page.keyboard.press("End");
    for (let i = 0; i < 6; i++) await page.keyboard.press("Backspace");
    await page.keyboard.press("Delete");
    await page.keyboard.press("ArrowLeft");
    await page.keyboard.type("42");
    await page.keyboard.press("Enter");
    await page.keyboard.press("Tab");
    await page.waitForTimeout(300);
    const blocks = await page.$eval(".mk-flow-status", (e) => e.dataset.blocks);
    const after = await pos();
    const value = await field.inputValue();
    console.log("\n  blocks:", blocks, "out:", value, "position:", before, "→", after);
    if (blocks !== "7") throw new Error("a block was deleted while editing a field");
    if (value !== "42") throw new Error(`out = ${value}`);
    if (after !== before) throw new Error("block moved while editing a field");
    await page.mouse.click(700, 600);
  });
  await step("wiring by dragging handles (typed): 6 wires, no issues", async () => {
    // No Layout/Fit before wiring: dioxus-flow's handle geometry lags a layout
    // animation (P-073); new blocks land in a grid that fits the canvas.
    await drag("Input", "out", "Conv", "in");
    await drag("Conv", "out", "MaxPool", "in");
    await drag("MaxPool", "out", "Flatten", "in");
    await drag("Flatten", "out", "Dense", "in");
    await drag("Dense", "out", "Loss", "prediction");
    await drag("Loss", "loss", "Optimiser", "loss");
    const s = await status();
    console.log("\n  status:", JSON.stringify(s), "positions:", await page.$$eval(".df-node", (ns) => ns.map((n) => n.querySelector(".mk-flow-block-title")?.textContent + "@" + Math.round(n.getBoundingClientRect().x) + "," + Math.round(n.getBoundingClientRect().y)).join(" ")));
    if (s.wires !== 6 || s.issues !== 0) throw new Error("wiring incomplete");
    await page.screenshot({ path: `${S}/m6-flow.png` });
  });
  await step("a type mismatch is refused (Conv image → Dense vector)", async () => {
    await drag("Conv", "out", "Dense", "in");
    await page.waitForTimeout(300);
    const s = await status();
    if (s.wires !== 6 || s.issues !== 0) throw new Error("mismatched wire was accepted: " + JSON.stringify(s));
  });
  await step("Save writes the flow JSON; Generate julia writes model.jl and opens it", async () => {
    await page.click(".mk-flow button:has-text('Save')");
    await page.waitForFunction(() => !document.querySelector(".mk-flow .mk-editor-dirty"), null, { timeout: 15000 });
    const flow = JSON.parse(fs.readFileSync(`${ROOT}/untitled.flow.json`, "utf8"));
    if (flow.blocks.length !== 7 || flow.wires.length !== 6) throw new Error("flow json wrong");
    await page.click(".mk-flow-generate");
    await page.waitForFunction(() => /model\.jl/.test(document.querySelector(".mk-titlebar-title")?.textContent || ""), null, { timeout: 15000 });
    const jl = fs.readFileSync(`${ROOT}/model.jl`, "utf8");
    console.log("\n  model.jl:", jl.split("\n").find((l) => l.includes("Conv(")).trim());
    if (!/Conv\(\(3, 3\), 1 => 16, relu; pad=1\)/.test(jl)) throw new Error("unexpected codegen");
  });
  await step("disabling Lux hides its blocks: the flow reports unknown kinds", async () => {
    await page.click(".wb-tab:has-text('Settings')");
    await page.click(".mk-settings-ext:has(.mk-settings-ext-name:text-is('Lux.jl library')) > label input");
    await page.click(".wb-tab:has-text('untitled.flow.json')");
    await page.waitForFunction(() => +document.querySelector(".mk-flow-status")?.dataset.issues >= 7, null, { timeout: 10000 });
  });
  console.log("\nFLOW E2E: PASS");
} catch (e) { console.log("\nFAIL:", e.message); console.log(logs.slice(-10).join("\n")); await page.screenshot({ path: `${S}/m6-fail.png` }); process.exitCode = 1; } finally { await browser.close(); }
