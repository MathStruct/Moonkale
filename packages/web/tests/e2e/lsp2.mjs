// Milestone 7: LSP second half with rust-analyzer — completion popup, F2 rename across the file
// (unsaved edit, cursor kept), Shift+F12 references, Ctrl+. code actions.
import { firefox } from "playwright";
import { lspEditor } from "./lsp-editor.mjs";
import fs from "node:fs";
const S = process.env.M1_SHOTS ?? ".";
const PORT = process.env.PORT ?? 8080;
const ROOT = process.env.M1_ROOT;
const browser = await firefox.launch();
const ctx = await browser.newContext({ viewport: { width: 1500, height: 900 } });
const page = await ctx.newPage();
const editor = lspEditor(page);
await editor.configure();
const logs = [];
page.on("console", (m) => { const t = m.text(); if (!t.includes("session[")) logs.push(`[${m.type()}] ${t.slice(0, 300)}`); });
const step = async (n, f) => { process.stdout.write(`- ${n} … `); await f(); console.log("ok"); };
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const clickAt = (needle, off) => editor.clickAt(needle, off);
try {
  await step("open src/main.rs; rust-analyzer reports diagnostics (it is ready)", async () => {
    await page.goto(`http://127.0.0.1:${PORT}/`, { waitUntil: "networkidle" });
    await page.waitForSelector(".wb-workspace");
    await page.click(".mk-explorer-open button[type=submit]");
    await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
    await page.click(".mk-tree-dir >> text=src");
    await page.waitForSelector("text=main.rs", { timeout: 10000 });
    await page.click(".mk-tree-file >> text=main.rs");
    await page.waitForSelector(editor.input, { timeout: 15000 });
    await page.waitForSelector(editor.error, { timeout: 120000 });
  });
  await step("typing `ad` inside main() opens a completion popup that offers `add`", async () => {
    const before = await editor.text();
    await clickAt("let total", 0);
    await page.keyboard.press("Home");
    await page.keyboard.type("    ad");
    await page.waitForSelector(editor.completion, { timeout: 20000 });
    await page.waitForFunction(selector => [...document.querySelectorAll(selector)].some(l => l.textContent === "add"), editor.completionLabel, { timeout: 20000 });
    const first = await page.$$eval(editor.completionItem, (l) => l.slice(0, 5).map((x) => x.textContent));
    console.log("\n  completions:", first.join(" | "));
    await page.screenshot({ path: `${S}/m7-lsp-completion.png` });
    await page.keyboard.press("Escape");
    // Undo the typed characters.
    await editor.undoTo(before);
    await page.waitForFunction(selector => !document.querySelector(selector), editor.completion, { timeout: 5000 });
  });
  await step("F2 on `add` → prompt → 'plus' → definition and call renamed, document dirty, not saved", async () => {
    await clickAt("add(1, 2)", 1);
    await page.keyboard.press("F2");
    await page.waitForSelector(".mk-editor-rename input", { timeout: 5000 });
    await page.fill(".mk-editor-rename input", "plus");
    await page.locator(".mk-editor-rename input").evaluate(input => input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", code: "Enter", isComposing: true, bubbles: true })));
    await sleep(150);
    if (!await page.locator(".mk-editor-rename input").isVisible()) throw Error("IME Enter committed rename");
    await page.keyboard.press("Enter");
    await editor.waitText("fn plus\\(");
    if (!(await editor.text()).includes("plus(1, 2)") || (await editor.text()).includes("add")) throw Error("rename did not update both occurrences");
    if (!(await page.$(".mk-tab-dirty"))) throw new Error("should be dirty");
    if (/plus/.test(fs.readFileSync(`${ROOT}/src/main.rs`, "utf8"))) throw new Error("written without save");
    await page.screenshot({ path: `${S}/m7-lsp-rename.png` });
  });
  await step("editing while the rename prompt is open rejects its stale position", async () => {
    const before = await editor.text();
    await clickAt("plus(1, 2)", 1);
    await page.keyboard.press("F2");
    await page.waitForSelector(".mk-editor-rename input");
    await page.fill(".mk-editor-rename input", "staleName");
    await editor.focus();
    await page.keyboard.press("Control+Home");
    await page.keyboard.insertText("/* shifted */ ");
    if (editor.native) {
      await page.locator('.mk-editor-rename input').waitFor({ state: 'detached' });
    } else {
      await page.locator('.mk-editor-rename input').press('Enter');
      await page.waitForFunction(() => document.querySelector('.wb-status-bar')?.textContent.includes('Document changed'));
    }
    if ((await editor.text()).includes('staleName')) throw Error('stale rename applied');
    // Remove only the inserted prefix, independent of undo transaction grouping.
    await editor.focus();
    await page.keyboard.press('Control+Home');
    for (let index = 0; index < '/* shifted */ '.length; index++) await page.keyboard.press('Shift+ArrowRight');
    await page.keyboard.press('Backspace');
    await editor.waitText('^' + before.replace(/[.*+?^${}()|[\]\\]/g, '\\$&') + '$');
  });
  await step("Shift+F12 on `plus` lists 2 references; clicking one moves the cursor", async () => {
    await clickAt("plus(1, 2)", 1);
    await page.keyboard.press("Shift+F12");
    await page.waitForFunction(selector => Number(document.querySelector(selector)?.dataset.count) >= 2, editor.references, { timeout: 20000 });
    const n = await page.$eval(editor.references, (e) => e.dataset.count);
    const rows = await page.$$eval(editor.reference, (l) => l.map((x) => x.textContent));
    console.log("\n  references:", n, rows.join(" | "));
    if (+n < 2) throw new Error("expected 2 references");
    await page.click(editor.reference);
    await sleep(300);
    const line = await editor.activeLine();
    console.log("  active line:", line);
    if (!/plus/.test(line)) throw new Error("cursor not on a reference");
  });
  await step("select `a + b`, Ctrl+. offers assists; 'Extract into variable' applies as an unsaved edit", async () => {
    await clickAt("a + b", 0);
    await page.keyboard.press("Home");
    await page.keyboard.press("Control+ArrowRight");
    await page.keyboard.press("Shift+End");
    await page.keyboard.press("Control+.");
    await page.waitForFunction(selector => Number(document.querySelector(selector)?.dataset.count) > 0, editor.actions, { timeout: 20000 });
    const titles = await page.$$eval(editor.action, (b) => b.map((x) => x.textContent).filter((t) => t !== "✕"));
    console.log("\n  actions:", titles.join(" | "));
    const extract = titles.find((t) => /Extract into variable/i.test(t));
    if (!extract) throw new Error("no extract action");
    await page.locator(editor.action).filter({ hasText: extract }).click();
    await editor.waitText("let \\w+ = a \\+ b;");
    console.log("  line:", (await editor.text()).split("\n").find(t => /= a \+ b/.test(t)));
    await page.screenshot({ path: `${S}/m7-lsp-actions.png` });
  });
  console.log("LSP2 E2E: PASS");
} catch (e) {
  console.log("\nFAIL:", e.message);
  await page.screenshot({ path: `${S}/m7-lsp2-fail.png` }).catch(() => {});
  console.log(logs.filter((l) => !/WARN/.test(l)).slice(-20).join("\n"));
  process.exitCode = 1;
} finally {
  await browser.close();
}
