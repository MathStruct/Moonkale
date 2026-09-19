// Milestone 3 step 4: rust-analyzer over the server relay → diagnostics gutter, hover, F12.
import { firefox } from "playwright";
const S = process.env.M1_SHOTS ?? ".";
const browser = await firefox.launch();
const page = await browser.newPage({ viewport: { width: 1400, height: 900 } });
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
    await page.waitForSelector(".cm-content", { timeout: 15000 });
  });
  await step("language server starts (status bar shows it)", async () => {
    await page.waitForFunction(() => /rust-analyzer|initializ|indexing|ready/i.test(document.querySelector(".wb-status-bar").textContent), null, { timeout: 60000 });
    console.log("\n  status:", await status());
  });
  await step("deliberate type error shows up in the lint gutter", async () => {
    await page.waitForSelector(".cm-lint-marker-error", { timeout: 120000 });
    const n = await page.$$eval(".cm-lint-marker", (m) => m.length);
    console.log("\n  markers:", n);
    await page.screenshot({ path: `${S}/m3-lsp-diag.png` });
  });
  await step("hover over `add` shows a tooltip from the server", async () => {
    const h = await page.evaluateHandle(() => {
      const w = document.evaluate("//div[contains(@class,'cm-line')]//text()[contains(., 'add(1, 2)')]", document, null, XPathResult.FIRST_ORDERED_NODE_TYPE, null).singleNodeValue;
      const r = document.createRange(); const i = w.textContent.indexOf("add("); r.setStart(w, i); r.setEnd(w, i + 3); return r.getBoundingClientRect();
    });
    const rect = await h.jsonValue();
    await page.mouse.move(rect.x + 4, rect.y + rect.height / 2);
    await page.mouse.move(rect.x + 5, rect.y + rect.height / 2);
    await page.waitForSelector(".mk-hover", { timeout: 15000 });
    console.log("\n  hover:", (await page.$eval(".mk-hover", (e) => e.textContent)).replace(/\s+/g, " ").slice(0, 120));
    await page.screenshot({ path: `${S}/m3-lsp-hover.png` });
  });
  await step("F12 on `add(` jumps to the definition (line 1)", async () => {
    const rect = await page.evaluate(() => {
      const w = document.evaluate("//div[contains(@class,'cm-line')]//text()[contains(., 'add(1, 2)')]", document, null, XPathResult.FIRST_ORDERED_NODE_TYPE, null).singleNodeValue;
      const r = document.createRange(); const i = w.textContent.indexOf("add("); r.setStart(w, i + 1); r.setEnd(w, i + 2); const b = r.getBoundingClientRect(); return { x: b.x, y: b.y + b.height / 2 };
    });
    await page.mouse.click(rect.x, rect.y);
    await page.keyboard.press("F12");
    await page.waitForFunction(() => {
      const active = document.querySelector(".cm-activeLine"); return active && /fn add\(/.test(active.textContent);
    }, null, { timeout: 15000 });
    console.log("\n  active line:", await page.$eval(".cm-activeLine", (e) => e.textContent.trim()));
  });
  await step("fixing the error and saving clears the gutter (didChange + didSave → cargo check)", async () => {
    await page.click(".cm-content");
    await page.keyboard.press("Control+End");
    // replace `let wrong: String = total;` by selecting its line via keyboard is fiddly; edit through CodeMirror directly:
    await page.evaluate(() => { const el = document.querySelector(".mk-editor-host") ?? document.querySelector(".cm-editor").parentElement; const t = window.moonkale.codemirror.getText(el).replace("let wrong: String = total;", "let wrong: String = total.to_string();"); window.moonkale.codemirror.setText(el, t); });
    // The type error is a cargo-check diagnostic (rust-analyzer 1.98 has no native one for it), so it
    // clears on save, when the check re-runs (P-080).
    await page.waitForSelector(".mk-tab-dirty", { timeout: 5000 });
    await page.keyboard.press("Control+S");
    await page.waitForFunction(() => !document.querySelector(".mk-tab-dirty"), null, { timeout: 10000 });
    await page.waitForFunction(() => !document.querySelector(".cm-lint-marker-error"), null, { timeout: 120000 });
  });
  console.log("\nLSP E2E: PASS");
} catch (e) { console.log("\nFAIL:", e.message); console.log(logs.slice(-10).join("\n")); await page.screenshot({ path: `${S}/m3-fail.png` }); process.exitCode = 1; } finally { await browser.close(); }
