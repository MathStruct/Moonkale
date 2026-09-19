// Milestone 7: find & replace — CodeMirror's search panel in a file (Ctrl+F / Ctrl+H, replace all
// flows back as an unsaved edit) and workspace replace from the Search panel (open documents stay
// unsaved, closed files are written through the source and re-indexed).
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
try {
  await step("open folder and README.md; Ctrl+H opens the editor's search panel; replace all 'Edit' → 'Change' → unsaved edit", async () => {
    await page.goto(`http://127.0.0.1:${PORT}/`, { waitUntil: "networkidle" });
    await page.waitForSelector(".wb-workspace");
    await page.click(".mk-explorer-open button[type=submit]");
    await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
    await page.click(".mk-tree-file >> text=README.md");
    await page.waitForSelector(".cm-content", { timeout: 15000 });
    await page.click(".cm-content");
    await page.keyboard.press("Control+H");
    await page.waitForSelector(".cm-search", { timeout: 5000 });
    await page.fill(".cm-search input[name=search]", "Edit");
    await page.fill(".cm-search input[name=replace]", "Change");
    await page.click(".cm-search button[name=replaceAll]");
    await page.waitForSelector(".mk-tab-dirty", { timeout: 5000 });
    const text = await page.$eval(".cm-content", (e) => e.textContent);
    if (!text.includes("Change me")) throw new Error("editor text: " + text);
    if (fs.readFileSync(`${ROOT}/README.md`, "utf8").includes("Change")) throw new Error("written to disk without save");
    await page.keyboard.press("Escape");
    await page.keyboard.press("Control+S");
    await page.waitForFunction(() => !document.querySelector(".mk-tab-dirty"), null, { timeout: 5000 });
    if (!fs.readFileSync(`${ROOT}/README.md`, "utf8").includes("Change me")) throw new Error("not saved");
    await page.screenshot({ path: `${S}/m7-replace-editor.png` });
  });
  await step("workspace search 'Alpha' → open Home.md → Replace with 'Omega' → Preview lists files with counts", async () => {
    await page.click(".mk-tree-file >> text=Home.md");
    await page.waitForSelector(".wb-tab:has-text('Home.md')", { timeout: 10000 });
    await page.click(".wb-status-bar");
    await page.keyboard.press("Control+Shift+F");
    await page.waitForFunction(() => document.activeElement?.id === "mk-search-input", null, { timeout: 5000 });
    await page.keyboard.type("Alpha");
    await page.keyboard.press("Enter");
    await page.waitForSelector(".mk-search-hit", { timeout: 15000 });
    await page.fill(".mk-search-replace input", "Omega");
    await page.click(".mk-search-replace button");
    await page.waitForSelector(".mk-search-preview[data-total]", { timeout: 10000 });
    const files = await page.$eval(".mk-search-preview", (e) => e.dataset.files);
    const total = await page.$eval(".mk-search-preview", (e) => e.dataset.total);
    const list = await page.$$eval(".mk-search-preview li", (l) => l.map((x) => x.textContent.trim()));
    console.log("\n  preview:", files, "files,", total, "occurrences:", list.join(" | "));
    if (+files < 2 || +total < 2) throw new Error("preview too small");
    if (!list.some((l) => l.startsWith("Home.md") && l.includes("open"))) throw new Error("Home.md not marked open");
    await page.screenshot({ path: `${S}/m7-replace-preview.png` });
  });
  await step("Replace all: Home.md editor is dirty with 'Omega' (disk untouched); notes/Beta.md changed on disk", async () => {
    const before = fs.readFileSync(`${ROOT}/Home.md`, "utf8");
    await page.click(".mk-search-preview button");
    try {
      await page.waitForFunction(() => /Replaced \d+ occurrence/.test(document.querySelector(".wb-status-bar").textContent), null, { timeout: 15000 });
    } catch (e) {
      console.log("\n  status:", await page.$eval(".wb-status-bar", (x) => x.textContent));
      throw e;
    }
    await page.click(".wb-tab:has-text('Home.md')");
    // Both editors are in the DOM (README.md's tab is inactive): look at all of them.
    await page.waitForFunction(() => [...document.querySelectorAll(".cm-content")].some((e) => e.textContent.includes("Omega")), null, { timeout: 5000 });
    if (!(await page.$(".wb-tab:has-text('Home.md') .mk-tab-dirty"))) throw new Error("Home.md should be dirty");
    if (fs.readFileSync(`${ROOT}/Home.md`, "utf8") !== before) throw new Error("Home.md written without save");
    const beta = fs.readFileSync(`${ROOT}/notes/Beta.md`, "utf8");
    console.log("\n  Beta.md:", JSON.stringify(beta));
    if (!beta.includes("Omega") || beta.includes("Alpha")) throw new Error("Beta.md not replaced on disk");
  });
  console.log("REPLACE E2E: PASS");
} catch (e) {
  console.log("\nFAIL:", e.message);
  await page.screenshot({ path: `${S}/m7-replace-fail.png` }).catch(() => {});
  console.log(logs.filter((l) => !/WARN/.test(l)).slice(-20).join("\n"));
  process.exitCode = 1;
} finally {
  await browser.close();
}
