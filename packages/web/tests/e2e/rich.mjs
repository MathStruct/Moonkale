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
  await step("click on the [[Alpha]] link opens Alpha.md (spec 012: decorated, plain click follows)", async () => {
    await page.waitForSelector(".mk-rich-host .mk-wikilink[data-target='Alpha']:not(.mk-wiki-bracket)", { timeout: 15000 });
    await page.click(".mk-rich-host .mk-wikilink[data-target='Alpha']:not(.mk-wiki-bracket)");
    await page.waitForFunction(() => document.querySelector(".mk-titlebar-title")?.textContent.startsWith("Alpha.md"), null, { timeout: 15000 });
  });
  await step("back to Source: CodeMirror shows the saved markdown", async () => {
    await page.click(".wb-tab:has-text('Home.md')");
    await page.click(".mk-md-modes button:has-text('Source')");
    await page.waitForFunction(() => /# Home Rich/.test(document.querySelector(".mk-md .cm-content")?.textContent || ""), null, { timeout: 15000 });
  });
  await step("formulas: $\\alpha$ and $$…$$ render with KaTeX, fonts load, markdown keeps the source (spec 013)", async () => {
    fs.writeFileSync(`${ROOT}/Math.md`, "# Math\n\nInline $\\alpha + \\beta^2$ here, and a macro $\\R$.\n\n$$\n\\int_0^1 x^2 \\, dx = \\frac{1}{3}\n$$\n");
    fs.mkdirSync(`${ROOT}/.moonkale`, { recursive: true });
    fs.writeFileSync(`${ROOT}/.moonkale/katex.json`, JSON.stringify({ macros: { "\\R": "\\mathbb{R}" } }));
    await page.click(".mk-explorer-open button[type=submit]");
    await page.waitForSelector(".mk-tree-file >> text=Math.md", { timeout: 15000 });
    await page.click(".mk-tree-file >> text=Math.md");
    await page.waitForSelector(".mk-md .cm-content:visible", { timeout: 15000 });
    await page.click(".mk-md-modes button:has-text('Rich'):visible");
    await page.waitForSelector(".mk-rich-host:visible .ProseMirror .katex", { timeout: 30000 });
    const n = await page.$$eval(".mk-rich-host:visible .ProseMirror .katex", (els) => els.length);
    const css = await page.evaluate(() => [...document.querySelectorAll("link[rel=stylesheet]")].some((l) => /katex\.min\.css/.test(l.href)));
    const font = await page.evaluate(async () => { await document.fonts.ready; return [...document.fonts].filter((f) => f.family.startsWith("KaTeX") && f.status === "loaded").length; });
    const macroOk = await page.evaluate(() => [...document.querySelectorAll(".mk-rich-host .katex")].some((k) => k.textContent.includes("R") && !k.querySelector(".katex-error")));
    console.log(`\n  katex elements: ${n}, css linked: ${css}, KaTeX fonts loaded: ${font}, macro \\R rendered: ${macroOk}`);
    if (n < 3) throw new Error("formulas not rendered");
    if (!macroOk) throw new Error("\\R from .moonkale/katex.json not applied");
    if (!css) throw new Error("katex.min.css not linked");
    if (font < 1) throw new Error("no KaTeX font loaded (fonts folder not served?)");
    await page.screenshot({ path: `${S}/m10-katex.png` });
    const text = fs.readFileSync(`${ROOT}/Math.md`, "utf8");
    if (!text.includes("$\\alpha + \\beta^2$") || !text.includes("\\frac{1}{3}")) throw new Error("math source changed on disk: " + text);
  });
  console.log("\nRICH E2E: PASS");
} catch (e) { console.log("\nFAIL:", e.message); console.log(logs.slice(-10).join("\n")); await page.screenshot({ path: `${S}/m5-fail.png` }); process.exitCode = 1; } finally { await browser.close(); }
