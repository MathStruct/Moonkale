// Milestone 3 step 4: rust-analyzer over the server relay → diagnostics gutter, hover, F12.
import { firefox } from "playwright";
import { lspEditor } from "./lsp-editor.mjs";
import fs from "node:fs";
const S = process.env.M1_SHOTS ?? ".";
const browser = await firefox.launch();
const page = await browser.newPage({ viewport: { width: 1400, height: 900 } });
const editor = lspEditor(page);
await editor.configure();
const logs = [];
page.on("console", (m) => { const t = m.text(); if (!t.includes("session[")) logs.push(`[${m.type()}] ${t.slice(0, 300)}`); });
const step = async (n, f) => { process.stdout.write(`- ${n} … `); await f(); console.log("ok"); };
const status = () => page.$eval(".wb-status-bar", (el) => el.textContent.trim());
try {
  await step("open folder, open src/main.rs", async () => {
    await page.goto(`http://127.0.0.1:${process.env.PORT ?? 8080}/`, { waitUntil: "networkidle" });
    await page.waitForSelector(".wb-workspace");
    await page.click(".mk-explorer-open button[type=submit]");
    await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
    await page.click(".mk-tree-dir >> text=src");
    await page.waitForSelector("text=main.rs", { timeout: 10000 });
    await page.click(".mk-tree-file >> text=main.rs");
    await page.waitForSelector(editor.input, { timeout: 15000 });
  });
  await step("language server starts (status bar shows it)", async () => {
    await page.waitForFunction(() => /rust-analyzer|initializ|indexing|ready/i.test(document.querySelector(".wb-status-bar").textContent), null, { timeout: 60000 });
    console.log("\n  status:", await status());
  });
  await step("deliberate type error shows up in the lint gutter", async () => {
    await page.waitForSelector(editor.error, { timeout: 120000 });
    const n = await page.$$eval(editor.marker, (m) => m.length);
    console.log("\n  markers:", n);
    await page.screenshot({ path: `${S}/m3-lsp-diag.png` });
  });
  await step("hover over `add` shows a tooltip from the server", async () => {
    const rect = await editor.clickAt("add(1, 2)", 1);
    await page.mouse.move(rect.x + 1, rect.y);
    await page.waitForSelector(".mk-hover", { timeout: 15000 });
    editor.assertPosition("textDocument/hover", "add(1, 2)", 1);
    console.log("\n  hover:", (await page.$eval(".mk-hover", (e) => e.textContent)).replace(/\s+/g, " ").slice(0, 120));
    await page.screenshot({ path: `${S}/m3-lsp-hover.png` });
  });
  await step("F12 on `add(` jumps to the definition (line 1)", async () => {
    await editor.clickAt("add(1, 2)", 1);
    await page.keyboard.press("F12");
    await page.waitForFunction(({ native }) => native
      ? [...document.querySelectorAll('.mk-crust')].find(x => x.getBoundingClientRect().height)?.dataset.selectionHead === '3'
      : /fn add\(/.test(document.querySelector('.cm-activeLine')?.textContent), { native: editor.native });
    editor.assertPosition("textDocument/definition", "add(1, 2)", 1);
    console.log("\n  active line:", await editor.activeLine());
  });
  await step("fixing the error and saving clears the gutter (didChange + didSave → cargo check)", async () => {
    const text = fs.readFileSync(`${process.env.M1_ROOT}/src/main.rs`, 'utf8')
      .replace("let wrong: String = total;", "let wrong: String = total.to_string();");
    await editor.clickAt("String = total;", 10);
    await page.keyboard.press("End");
    await page.keyboard.press("ArrowLeft");
    await page.keyboard.insertText('.to_string()');
    // The type error is a cargo-check diagnostic (rust-analyzer 1.98 has no native one for it), so it
    // clears on save, when the check re-runs (P-080).
    await page.waitForSelector(".mk-tab-dirty", { timeout: 5000 });
    await page.keyboard.press("Control+S");
    await page.waitForFunction(() => !document.querySelector(".mk-tab-dirty"), null, { timeout: 10000 });
    if (fs.readFileSync(`${process.env.M1_ROOT}/src/main.rs`, "utf8") !== text) throw Error("saved CRLF source differs from the intended edit");
    await page.waitForFunction(selector => !document.querySelector(selector), editor.error, { timeout: 120000 });
  });
  console.log("\nLSP E2E: PASS");
} catch (e) { console.log("\nFAIL:", e.message); console.log(logs.slice(-10).join("\n")); await page.screenshot({ path: `${S}/m3-fail.png` }); process.exitCode = 1; } finally { await browser.close(); }
