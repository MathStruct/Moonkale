// Milestone 12: the Rust/Dioxus terminal (no JavaScript of ours). Enabled as an opt-in extension and chosen
// as the implementation: Ctrl+` opens it, a shell prompt renders in the vt100 grid, typed commands run,
// ANSI colours become classes, Ctrl+click on a path opens the file. Then, with `ask`, the chooser appears.
import { firefox } from "playwright";
const S = process.env.M1_SHOTS ?? ".";
const PORT = process.env.PORT ?? 8080;
const browser = await firefox.launch();
const ctx = await browser.newContext({ viewport: { width: 1400, height: 900 } });
await ctx.addInitScript(() => { try { localStorage.setItem("moonkale.settings", JSON.stringify({ extensions: { enabled: ["dev.moonkale.editor-terminal-native"] }, terminal: { implementation: "native" }, editor: { markdown_rich: false } })); } catch {} });
const page = await ctx.newPage();
const logs = [];
page.on("console", (m) => { const t = m.text(); if (!t.includes("session[")) logs.push(`[${m.type()}] ${t.slice(0, 300)}`); });
const step = async (n, f) => { process.stdout.write(`- ${n} … `); await f(); console.log("ok"); };
const grid = () => page.$eval(".mk-tn-screen:not(.mk-tn-hidden) .mk-tn-grid", (g) => g.textContent);
try {
  await step("Ctrl+` opens the Rust terminal; a prompt renders in the grid", async () => {
    await page.goto(`http://127.0.0.1:${PORT}/`, { waitUntil: "networkidle" });
    await page.waitForSelector(".wb-workspace");
    await page.click(".mk-explorer-open button[type=submit]");
    await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
    await page.click(".wb-status-bar");
    await page.keyboard.press("Control+`");
    await page.waitForSelector(".mk-tn-screen:not(.mk-tn-hidden) .mk-tn-row", { timeout: 20000 });
    await page.waitForFunction(() => /[$#>%]/.test(document.querySelector(".mk-tn-screen:not(.mk-tn-hidden) .mk-tn-grid")?.textContent || ""), null, { timeout: 20000 });
    const size = await page.$$eval(".mk-tn-screen:not(.mk-tn-hidden) .mk-tn-row", (r) => r.length);
    console.log(`\n  rows: ${size}, first line: ${JSON.stringify((await grid()).split("\n")[0].slice(0, 60))}`);
    if (size < 3) throw new Error("grid too small");
  });
  await step("typing runs a command; its output appears", async () => {
    await page.click(".mk-tn-screen:not(.mk-tn-hidden)");
    await page.keyboard.type("echo native-o''k");
    await page.keyboard.press("Enter");
    await page.waitForFunction(() => /native-ok/.test(document.querySelector(".mk-tn-screen:not(.mk-tn-hidden) .mk-tn-grid")?.textContent || ""), null, { timeout: 15000 });
  });
  await step("ANSI colours become classes; bold too", async () => {
    await page.keyboard.type("printf '\\033[31mred\\033[0m \\033[1mbold\\033[0m\\n'");
    await page.keyboard.press("Enter");
    await page.waitForFunction(() => [...document.querySelectorAll(".mk-tn-screen:not(.mk-tn-hidden) .mk-tn-run.mk-tn-fg1")].some((s) => s.textContent === "red"), null, { timeout: 15000 });
    await page.waitForFunction(() => [...document.querySelectorAll(".mk-tn-screen:not(.mk-tn-hidden) .mk-tn-run.mk-tn-b")].some((s) => s.textContent === "bold"), null, { timeout: 15000 });
    await page.screenshot({ path: `${S}/m12-terminal-native.png` });
  });
  await step("Ctrl+click on src/main.rs:2 opens the file", async () => {
    await page.keyboard.type("echo src/main.rs:2:1");
    await page.keyboard.press("Enter");
    await page.waitForFunction(() => (document.querySelector(".mk-tn-screen:not(.mk-tn-hidden) .mk-tn-grid")?.textContent || "").split("src/main.rs:2:1").length > 2, null, { timeout: 15000 });
    await page.locator(".mk-tn-screen:not(.mk-tn-hidden) .mk-tn-row", { hasText: "src/main.rs:2:1" }).last().click({ modifiers: ["Control"] });
    await page.waitForSelector(".cm-content", { timeout: 15000 });
    await page.waitForFunction(() => /main\.rs/.test(document.querySelector(".mk-titlebar-title")?.textContent || ""), null, { timeout: 10000 });
  });
  await step("with `ask` and both terminals enabled, New Terminal shows the chooser", async () => {
    const ctx2 = await browser.newContext({ viewport: { width: 1400, height: 900 } });
    await ctx2.addInitScript(() => { try { localStorage.setItem("moonkale.settings", JSON.stringify({ extensions: { enabled: ["dev.moonkale.editor-terminal-native"] }, terminal: { implementation: "ask" }, editor: { markdown_rich: false } })); } catch {} });
    const p2 = await ctx2.newPage();
    await p2.goto(`http://127.0.0.1:${PORT}/`, { waitUntil: "networkidle" });
    await p2.waitForSelector(".wb-workspace");
    await p2.click(".mk-explorer-open button[type=submit]");
    await p2.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
    await p2.click(".wb-status-bar");
    await p2.keyboard.press("Control+`");
    await p2.waitForSelector(".mk-terminal-chooser", { timeout: 15000 });
    await p2.click(".mk-terminal-chooser button:has-text('Rust')");
    await p2.waitForSelector(".mk-tn-screen:not(.mk-tn-hidden) .mk-tn-row", { timeout: 20000 });
    await ctx2.close();
  });
  console.log("\nTERMINAL-NATIVE E2E: PASS");
} catch (e) { console.log("\nFAIL:", e.message); console.log(logs.slice(-8).join("\n")); await page.screenshot({ path: `${S}/m12-terminal-native-fail.png` }); process.exitCode = 1; } finally { await browser.close(); }
