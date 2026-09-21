// Milestone 14: the Rust code editor (dioxus-code-editor). Enabled as opt-in and chosen as the
// implementation: a .rs file opens in it with tree-sitter tokens, typing dirties and Ctrl+S saves,
// the caret's word shows in the toolbar (Workspace::cursor_word), and the switches move the document
// between the two editors.
import { firefox } from "playwright";
import fs from "node:fs";
const S = process.env.M1_SHOTS ?? ".";
const PORT = process.env.PORT ?? 8080;
const ROOT = process.env.M1_ROOT;
const browser = await firefox.launch();
const ctx = await browser.newContext({ viewport: { width: 1400, height: 900 } });
await ctx.addInitScript(() => { try { localStorage.setItem("moonkale.settings", JSON.stringify({ extensions: { enabled: ["dev.moonkale.editor-code-native"] }, editor: { implementation: "native", markdown_rich: false } })); } catch {} });
const page = await ctx.newPage();
const logs = [];
page.on("console", (m) => { const t = m.text(); if (!t.includes("session[")) logs.push(`[${m.type()}] ${t.slice(0, 300)}`); });
const step = async (n, f) => { process.stdout.write(`- ${n} … `); await f(); console.log("ok"); };
try {
  await step("a .rs file opens in the Rust editor with tree-sitter tokens", async () => {
    await page.goto(`http://127.0.0.1:${PORT}/`, { waitUntil: "networkidle" });
    await page.waitForSelector(".wb-workspace");
    await page.click(".mk-explorer-open button[type=submit]");
    await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
    await page.click(".mk-tree-dir >> text=src");
    await page.click(".mk-tree-file >> text=main.rs");
    await page.waitForSelector(".mk-cn .dxc-editor", { timeout: 20000 });
    const tokens = await page.$$eval(".mk-cn .dxc-editor-highlight span[class]", (s) => s.length);
    const classes = await page.$$eval(".mk-cn .dxc-editor-highlight span[class]", (s) => [...new Set(s.map((x) => x.className))].slice(0, 6));
    console.log(`\n  ${tokens} tokens, e.g. ${classes.join(" ")}`);
    if (tokens < 5) throw new Error("no highlighting");
    if (await page.$(".cm-content")) throw new Error("CodeMirror also mounted");
  });
  await step("the caret's word shows in the toolbar; typing dirties; Ctrl+S saves", async () => {
    const ta = page.locator(".mk-cn textarea");
    await ta.click();
    await page.keyboard.press("Control+Home");
    await page.keyboard.press("End");
    await page.keyboard.press("ArrowLeft"); await page.keyboard.press("ArrowLeft");
    await page.waitForFunction(() => /‹\w+›/.test(document.querySelector(".mk-cn-word")?.textContent || ""), null, { timeout: 5000 });
    console.log("\n  word:", await page.$eval(".mk-cn-word", (e) => e.textContent));
    await page.keyboard.press("Control+End");
    await page.keyboard.type("\n// rust editor");
    await page.waitForSelector(".mk-cn .mk-editor-dirty", { timeout: 5000 });
    await page.keyboard.press("Control+s");
    await page.waitForFunction(() => !document.querySelector(".mk-cn .mk-editor-dirty"), null, { timeout: 10000 });
    const text = fs.readFileSync(`${ROOT}/src/main.rs`, "utf8");
    if (!text.includes("// rust editor")) throw new Error("not saved: " + text.slice(-60));
    await page.screenshot({ path: `${S}/m14-code-native.png` });
  });
  await step("the switch moves the document to CodeMirror and back", async () => {
    await page.click(".mk-cn .mk-editor-switch");
    await page.waitForSelector(".cm-content", { timeout: 20000 });
    await page.waitForFunction(() => !document.querySelector(".mk-cn"), null, { timeout: 5000 });
    await page.waitForFunction(() => /rust editor/.test(document.querySelector(".cm-content")?.textContent || ""), null, { timeout: 10000 });
    await page.click(".mk-editor-toolbar .mk-editor-switch");
    await page.waitForSelector(".mk-cn .dxc-editor", { timeout: 20000 });
    await page.waitForFunction(() => !document.querySelector(".cm-content"), null, { timeout: 5000 });
  });
  console.log("\nCODE-NATIVE E2E: PASS");
} catch (e) { console.log("\nFAIL:", e.message); console.log(logs.slice(-8).join("\n")); await page.screenshot({ path: `${S}/m14-code-native-fail.png` }); process.exitCode = 1; } finally { await browser.close(); }
