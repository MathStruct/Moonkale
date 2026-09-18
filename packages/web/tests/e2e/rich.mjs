// Milestone 5: Rich (Milkdown) markdown mode — toggle, edit, markdown round trip on save, wiki-link click.
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
try {
  await step("open folder and Home.md in Source mode", async () => {
    await page.goto(`http://127.0.0.1:${PORT}/`, { waitUntil: "networkidle" });
    await page.waitForSelector(".wb-workspace");
    await page.click(".mk-explorer-open button[type=submit]");
    await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
    await page.click(".mk-tree-file >> text=Home.md");
    await page.waitForSelector(".mk-md .cm-content", { timeout: 15000 });
  });
  await step("Rich mode renders the heading as an <h1>", async () => {
    await page.click(".mk-md-modes button:has-text('Rich')");
    await page.waitForSelector(".mk-rich-host .ProseMirror h1", { timeout: 30000 });
    console.log("\n  h1:", await page.$eval(".mk-rich-host .ProseMirror h1", (e) => e.textContent));
    await page.screenshot({ path: `${S}/m5-rich.png` });
  });
  await step("typing in rich mode dirties the document; Save writes markdown", async () => {
    await page.click(".mk-rich-host .ProseMirror h1");
    await page.keyboard.press("End");
    await page.keyboard.type(" Rich");
    await page.waitForSelector(".mk-rich .mk-editor-dirty", { timeout: 10000 });
    await page.click(".mk-rich button:has-text('Save')");
    await page.waitForFunction(() => !document.querySelector(".mk-rich .mk-editor-dirty"), null, { timeout: 15000 });
    const text = fs.readFileSync(`${ROOT}/Home.md`, "utf8");
    console.log("\n  file:", JSON.stringify(text.split("\n")[0]));
    if (!text.startsWith("# Home Rich")) throw new Error("markdown not saved: " + text.slice(0, 60));
    if (!text.includes("[[Alpha]]")) throw new Error("wiki-link lost in round trip: " + text);
  });
  await step("Ctrl+click on [[Alpha]] opens Alpha.md", async () => {
    const box = await page.evaluate(() => {
      const walker = document.createTreeWalker(document.querySelector(".mk-rich-host .ProseMirror"), NodeFilter.SHOW_TEXT);
      let n; while ((n = walker.nextNode())) { const i = n.textContent.indexOf("[[Alpha]]"); if (i >= 0) { const r = document.createRange(); r.setStart(n, i + 3); r.setEnd(n, i + 6); const b = r.getBoundingClientRect(); return { x: b.x + b.width / 2, y: b.y + b.height / 2 }; } }
      return null;
    });
    if (!box) throw new Error("[[Alpha]] not rendered as text");
    // Headless Firefox drops modifier flags on page.mouse.click (P-055): dispatch the click.
    await page.evaluate(({ x, y }) => { const t = document.elementFromPoint(x, y); t.dispatchEvent(new MouseEvent("click", { bubbles: true, ctrlKey: true, clientX: x, clientY: y })); }, box);
    await page.waitForFunction(() => document.querySelector(".mk-titlebar-title")?.textContent.startsWith("Alpha.md"), null, { timeout: 15000 });
  });
  await step("back to Source: CodeMirror shows the saved markdown", async () => {
    await page.click(".wb-tab:has-text('Home.md')");
    await page.click(".mk-md-modes button:has-text('Source')");
    await page.waitForFunction(() => /# Home Rich/.test(document.querySelector(".mk-md .cm-content")?.textContent || ""), null, { timeout: 15000 });
  });
  console.log("\nRICH E2E: PASS");
} catch (e) { console.log("\nFAIL:", e.message); console.log(logs.slice(-10).join("\n")); await page.screenshot({ path: `${S}/m5-fail.png` }); process.exitCode = 1; } finally { await browser.close(); }
