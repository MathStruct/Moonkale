// Milestone 7: LSP second half with rust-analyzer — completion popup, F2 rename across the file
// (unsaved edit, cursor kept), Shift+F12 references, Ctrl+. code actions.
import { firefox } from "playwright";
import fs from "node:fs";
const S = process.env.M1_SHOTS ?? ".";
const PORT = process.env.PORT ?? 8080;
const ROOT = process.env.M1_ROOT;
const browser = await firefox.launch();
const ctx = await browser.newContext({ viewport: { width: 1500, height: 900 } });
const page = await ctx.newPage();
const logs = [];
page.on("console", (m) => { const t = m.text(); if (!t.includes("session[")) logs.push(`[${m.type()}] ${t.slice(0, 300)}`); });
const step = async (n, f) => { process.stdout.write(`- ${n} … `); await f(); console.log("ok"); };
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
// Click into the text "needle" on a CodeMirror line, at character offset `off` inside it.
const clickAt = async (needle, off) => {
  const p = await page.evaluate(([needle, off]) => {
    const w = document.evaluate(`//div[contains(@class,'cm-line')]//text()[contains(., '${needle}')]`, document, null, XPathResult.FIRST_ORDERED_NODE_TYPE, null).singleNodeValue;
    const r = document.createRange(); const i = w.textContent.indexOf(needle); r.setStart(w, i + off); r.setEnd(w, i + off + 1); const b = r.getBoundingClientRect(); return { x: b.x, y: b.y + b.height / 2 };
  }, [needle, off]);
  await page.mouse.click(p.x, p.y);
};
try {
  await step("open src/main.rs; rust-analyzer reports diagnostics (it is ready)", async () => {
    await page.goto(`http://127.0.0.1:${PORT}/`, { waitUntil: "networkidle" });
    await page.waitForSelector(".wb-workspace");
    await page.click(".mk-explorer-open button[type=submit]");
    await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
    await page.click(".mk-tree-dir >> text=src");
    await page.waitForSelector("text=main.rs", { timeout: 10000 });
    await page.click(".mk-tree-file >> text=main.rs");
    await page.waitForSelector(".cm-content", { timeout: 15000 });
    await page.waitForSelector(".cm-lint-marker-error", { timeout: 120000 });
  });
  await step("typing `ad` inside main() opens a completion popup that offers `add`", async () => {
    await clickAt("let total", 0);
    await page.keyboard.press("Home");
    await page.keyboard.type("    ad");
    await page.waitForSelector(".cm-tooltip-autocomplete", { timeout: 20000 });
    await page.waitForFunction(() => [...document.querySelectorAll(".cm-tooltip-autocomplete .cm-completionLabel")].some((l) => l.textContent === "add"), null, { timeout: 20000 });
    const first = await page.$$eval(".cm-tooltip-autocomplete li", (l) => l.slice(0, 5).map((x) => x.textContent));
    console.log("\n  completions:", first.join(" | "));
    await page.screenshot({ path: `${S}/m7-lsp-completion.png` });
    await page.keyboard.press("Escape");
    // Undo the typed characters.
    for (let i = 0; i < 6; i++) await page.keyboard.press("Backspace");
    await page.waitForFunction(() => !document.querySelector(".cm-tooltip-autocomplete"), null, { timeout: 5000 });
  });
  await step("F2 on `add` → prompt → 'plus' → definition and call renamed, document dirty, not saved", async () => {
    await clickAt("add(1, 2)", 1);
    await page.keyboard.press("F2");
    await page.waitForSelector(".mk-editor-rename input", { timeout: 5000 });
    await page.fill(".mk-editor-rename input", "plus");
    await page.keyboard.press("Enter");
    await page.waitForFunction(() => { const t = [...document.querySelectorAll(".cm-content")].map((e) => e.textContent).join(""); return /fn plus\(/.test(t) && /plus\(1, 2\)/.test(t) && !/add/.test(t); }, null, { timeout: 20000 });
    if (!(await page.$(".mk-tab-dirty"))) throw new Error("should be dirty");
    if (/plus/.test(fs.readFileSync(`${ROOT}/src/main.rs`, "utf8"))) throw new Error("written without save");
    await page.screenshot({ path: `${S}/m7-lsp-rename.png` });
  });
  await step("Shift+F12 on `plus` lists 2 references; clicking one moves the cursor", async () => {
    await clickAt("plus(1, 2)", 1);
    await page.keyboard.press("Shift+F12");
    await page.waitForSelector(".mk-editor-refs[data-count]", { timeout: 20000 });
    const n = await page.$eval(".mk-editor-refs", (e) => e.dataset.count);
    const rows = await page.$$eval(".mk-editor-ref", (l) => l.map((x) => x.textContent));
    console.log("\n  references:", n, rows.join(" | "));
    if (+n < 2) throw new Error("expected 2 references");
    await page.click(".mk-editor-ref");
    await sleep(300);
    const line = await page.$eval(".cm-activeLine", (e) => e.textContent.trim());
    console.log("  active line:", line);
    if (!/plus/.test(line)) throw new Error("cursor not on a reference");
  });
  await step("select `a + b`, Ctrl+. offers assists; 'Extract into variable' applies as an unsaved edit", async () => {
    await clickAt("a + b", 0);
    await page.keyboard.press("Home");
    await page.keyboard.press("Control+ArrowRight");
    await page.keyboard.press("Shift+End");
    await page.keyboard.press("Control+.");
    await page.waitForSelector(".mk-editor-actions[data-count]", { timeout: 20000 });
    const titles = await page.$$eval(".mk-editor-actions button", (b) => b.map((x) => x.textContent).filter((t) => t !== "✕"));
    console.log("\n  actions:", titles.join(" | "));
    const extract = titles.find((t) => /Extract into variable/i.test(t));
    if (!extract) throw new Error("no extract action");
    await page.click(`.mk-editor-actions button:text-is("${extract}")`);
    await page.waitForFunction(() => [...document.querySelectorAll(".cm-content")].some((e) => /let \w+ = a \+ b;/.test(e.textContent)), null, { timeout: 20000 });
    console.log("  line:", await page.$$eval(".cm-line", (l) => l.map((x) => x.textContent).find((t) => /= a \+ b/.test(t))));
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
