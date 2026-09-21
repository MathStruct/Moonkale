// Spec 010: syntax highlighting per language (Lezer/legacy grammars picked by Rust's language id),
// with folding, bracket matching and Mod-/ comment toggling.
import { firefox } from "playwright";
import fs from "node:fs";
const S = process.env.M1_SHOTS ?? ".";
const PORT = process.env.PORT ?? 8080;
const browser = await firefox.launch();
const page = await browser.newPage({ viewport: { width: 1500, height: 900 } });
const logs = [];
page.on("console", (m) => { const t = m.text(); if (!t.includes("session[")) logs.push(`[${m.type()}] ${t.slice(0, 300)}`); });
const step = async (n, f) => { process.stdout.write(`- ${n} … `); await f(); console.log("ok"); };
const tokens = () => page.$$eval(".cm-content:not([style*='display: none']) span[class*='ͼ']", (els) => els.length);
try {
  await step("open folder and src/main.rs: Rust tokens are highlighted, fold gutter present", async () => {
    await page.goto(`http://127.0.0.1:${PORT}/`, { waitUntil: "networkidle" });
    await page.waitForSelector(".wb-workspace");
    await page.click(".mk-explorer-open button[type=submit]");
    await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
    await page.click(".mk-tree-row[title='src']");
    await page.click(".mk-tree-file >> text=main.rs");
    await page.waitForSelector(".cm-content", { timeout: 15000 });
    await page.waitForFunction(() => document.querySelectorAll(".cm-content span[class*='ͼ']").length > 5, null, { timeout: 15000 });
    const n = await tokens();
    const fold = await page.$$eval(".cm-foldGutter", (els) => els.length);
    console.log(`\n  rust tokens: ${n}, fold gutters: ${fold}`);
    if (fold < 1) throw new Error("no fold gutter");
    await page.screenshot({ path: `${S}/m10-highlight-rust.png` });
  });
  await step("Mod-/ toggles a Rust line comment", async () => {
    await page.click(".cm-content");
    await page.keyboard.press("Control+Home");
    await page.keyboard.press("Control+/");
    await page.waitForFunction(() => /^\/\/ ?fn add/.test(document.querySelector(".cm-content .cm-line")?.textContent || ""), null, { timeout: 5000 });
    await page.keyboard.press("Control+/");
    await page.waitForFunction(() => /^fn add/.test(document.querySelector(".cm-content .cm-line")?.textContent || ""), null, { timeout: 5000 });
  });
  await step("Home.md (source) highlights markdown; Cargo.toml highlights TOML", async () => {
    await page.click(".mk-tree-file >> text=Home.md");
    await page.waitForFunction(() => [...document.querySelectorAll(".cm-content")].some((c) => c.offsetParent && c.querySelectorAll("span[class*='ͼ']").length > 0), null, { timeout: 15000 });
    await page.click(".mk-tree-file >> text=Cargo.toml");
    await page.waitForFunction(() => [...document.querySelectorAll(".cm-content")].some((c) => c.offsetParent && c.textContent.includes("[package]") && c.querySelectorAll("span[class*='ͼ']").length > 0), null, { timeout: 15000 });
  });
  await step("Wrap toggles soft wrap (spec 014): toolbar button, then Alt+Z; the setting persists", async () => {
    await page.click(".mk-tree-file >> text=main.rs");
    await page.waitForFunction(() => [...document.querySelectorAll(".cm-content")].some((c) => c.offsetParent), null, { timeout: 15000 });
    const wrapped = () => page.evaluate(() => [...document.querySelectorAll(".cm-content")].filter((c) => c.offsetParent).some((c) => c.classList.contains("cm-lineWrapping")));
    if (await wrapped()) throw new Error("wrap on by default");
    await page.click(".mk-editor-toolbar button:has-text('Wrap'):visible");
    await page.waitForFunction(() => [...document.querySelectorAll(".cm-content")].filter((c) => c.offsetParent).some((c) => c.classList.contains("cm-lineWrapping")), null, { timeout: 5000 });
    await page.waitForSelector(".mk-editor-toolbar button.mk-btn-on:has-text('Wrap'):visible", { timeout: 5000 });
    await page.click(".cm-content:visible");
    await page.keyboard.press("Alt+z");
    await page.waitForFunction(() => ![...document.querySelectorAll(".cm-content")].filter((c) => c.offsetParent).some((c) => c.classList.contains("cm-lineWrapping")), null, { timeout: 5000 });
    await page.keyboard.press("Alt+z");
    await page.waitForFunction(() => [...document.querySelectorAll(".cm-content")].filter((c) => c.offsetParent).some((c) => c.classList.contains("cm-lineWrapping")), null, { timeout: 5000 });
    // The web client keeps user settings in localStorage (desktop: settings.json).
    const saved = await page.evaluate(() => Object.keys(localStorage).map((k) => localStorage.getItem(k) || "").join("\n"));
    console.log("\n  wrap persisted in the settings store:", /"wrap"\s*:\s*true/.test(saved) ? "yes" : "no");
    if (!/"wrap"\s*:\s*true/.test(saved)) throw new Error("editor.wrap not saved to localStorage");
  });
  await step("a 3 MB file: a keystroke reaches Rust as a splice, not the whole text (spec 018) — dirty within a second", async () => {
    const line = "let value = compute(alpha, beta, gamma) + 42; // filler text to make the line long enough\n";
    fs.writeFileSync(`${process.env.M1_ROOT}/big.txt`, line.repeat(Math.ceil(3_000_000 / line.length)));
    await page.click(".mk-explorer-open button[type=submit]");
    await page.waitForSelector(".mk-tree-file >> text=big.txt", { timeout: 15000 });
    await page.click(".mk-tree-file >> text=big.txt");
    await page.waitForFunction(() => [...document.querySelectorAll(".cm-content")].some((c) => c.offsetParent && c.textContent.includes("filler")), null, { timeout: 60000 });
    await page.click(".cm-content:visible");
    await page.keyboard.press("Control+Home");
    const t0 = Date.now();
    await page.keyboard.type("x");
    await page.waitForSelector(".mk-tab-dirty", { timeout: 5000 });
    const dt = Date.now() - t0;
    console.log(`\n  3 MB file: dirty after ${dt} ms`);
    if (dt > 1500) throw new Error("keystroke took " + dt + " ms to reach Rust");
    await page.click(".mk-editor-toolbar button:has-text('Reload'):visible");
  });
  console.log("\nHIGHLIGHT E2E: PASS");
} catch (e) { console.log("\nFAIL:", e.message); console.log(logs.slice(-10).join("\n")); await page.screenshot({ path: `${S}/m10-highlight-fail.png` }); process.exitCode = 1; } finally { await browser.close(); }
