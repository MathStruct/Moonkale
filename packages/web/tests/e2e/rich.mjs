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
  await step("front matter (spec 019): hidden from the rich view, shown as Properties, editable, saved intact", async () => {
    fs.writeFileSync(`${ROOT}/Front.md`, "---\ntitle: \"Front\"\ntags: [demo]\n---\n# Front\n\nBody text.\n");
    await page.click(".mk-explorer-open button[type=submit]");
    await page.waitForSelector(".mk-tree-file >> text=Front.md", { timeout: 15000 });
    await page.click(".mk-tree-file >> text=Front.md");
    await page.waitForSelector(".mk-md .cm-content:visible", { timeout: 15000 });
    await page.click(".mk-md-modes button:has-text('Rich'):visible");
    await page.waitForSelector(".mk-rich-host:visible .ProseMirror h1", { timeout: 30000 });
    const pm = await page.$eval(".mk-rich-host:visible .ProseMirror", (e) => e.textContent);
    if (/title:|---/.test(pm)) throw new Error("front matter leaked into the rich view: " + pm.slice(0, 60));
    const summary = await page.$eval(".mk-rich:visible .mk-props-summary", (e) => e.textContent);
    console.log("\n  properties:", summary);
    if (!/title: Front/.test(summary)) throw new Error("summary wrong");
    await page.click(".mk-rich:visible .mk-props-head");
    await page.waitForSelector(".mk-rich:visible .mk-props-yaml", { timeout: 5000 });
    await page.fill(".mk-rich:visible .mk-props-yaml", 'title: "Front"\ntags: [demo, edited]');
    await page.click(".mk-rich-host:visible .ProseMirror p");
    await page.keyboard.press("End");
    await page.keyboard.type(" More.");
    await page.waitForFunction(() => [...document.querySelectorAll(".mk-rich-host .ProseMirror")].some((e) => e.offsetParent && /More\./.test(e.textContent)), null, { timeout: 5000 });
    await page.waitForTimeout(400);
    await page.click(".mk-rich button:has-text('Save'):visible");
    await page.waitForFunction(() => ![...document.querySelectorAll(".mk-rich .mk-editor-dirty")].some((e) => e.offsetParent !== null), null, { timeout: 15000 });
    const saved = fs.readFileSync(`${ROOT}/Front.md`, "utf8");
    console.log("  saved:", JSON.stringify(saved));
    if (!saved.startsWith('---\ntitle: "Front"\ntags: [demo, edited]\n---\n')) throw new Error("front matter not saved intact");
    if (!/Body text\. More\./.test(saved)) throw new Error("body edit lost");
  });
  await step("spec 021: without the workspace override markdown opens in Rich mode, and loading does not dirty the file", async () => {
    // run-all.sh writes .moonkale/settings.json with markdown_rich=false for the other suites.
    fs.rmSync(`${ROOT}/.moonkale/settings.json`, { force: true });
    fs.writeFileSync(`${ROOT}/Norm.md`, "Title\n=====\n\n* one\n* two\n\n\nDone.\n");
    await page.goto(`http://127.0.0.1:${PORT}/`, { waitUntil: "networkidle" });
    await page.waitForSelector(".wb-workspace");
    await page.click(".mk-explorer-open button[type=submit]");
    await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
    await page.click(".mk-tree-file >> text=Norm.md");
    await page.waitForSelector(".mk-rich-host:visible .ProseMirror h1", { timeout: 30000 });
    const on = await page.$eval(".mk-md-modes button:has-text('Rich'):visible", (b) => b.classList.contains("mk-btn-on"));
    if (!on) throw new Error("Rich is not the default mode");
    await page.waitForTimeout(800);
    if (await page.$(".mk-rich:visible .mk-editor-dirty")) throw new Error("opening in Rich mode marked the file dirty");
    if (fs.readFileSync(`${ROOT}/Norm.md`, "utf8") !== "Title\n=====\n\n* one\n* two\n\n\nDone.\n") throw new Error("file changed on open");
    await page.click(".mk-rich-host:visible .ProseMirror h1");
    await page.keyboard.press("End");
    await page.keyboard.type(" X");
    await page.waitForSelector(".mk-rich:visible .mk-editor-dirty", { timeout: 10000 });
    console.log("\n  clean on open, dirty after typing");
  });
  await step("Prompt23: the rich editor's font size and family follow the settings", async () => {
    // A fresh context: the app persists its own settings on changes, so editing localStorage
    // under a running page races with it.
    const ctx2 = await browser.newContext({ viewport: { width: 1500, height: 900 } });
    await ctx2.addInitScript(() => { try { localStorage.setItem("moonkale.settings", JSON.stringify({ editor: { rich_font_size: 22, rich_font: "Georgia, serif" } })); } catch {} });
    const p2 = await ctx2.newPage();
    await p2.goto(`http://127.0.0.1:${PORT}/`, { waitUntil: "networkidle" });
    await p2.waitForSelector(".wb-workspace");
    await p2.click(".mk-explorer-open button[type=submit]");
    await p2.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
    await p2.click(".mk-tree-file >> text=Norm.md");
    await p2.waitForSelector(".mk-rich-host:visible .ProseMirror", { timeout: 30000 });
    const style = await p2.$eval(".mk-rich-host:visible .ProseMirror", (e) => { const c = getComputedStyle(e); return { size: c.fontSize, family: c.fontFamily }; });
    await ctx2.close();
    console.log("\n  ProseMirror:", JSON.stringify(style));
    if (style.size !== "22px" || !/Georgia/.test(style.family)) throw new Error("typography settings not applied");
  });
  console.log("\nRICH E2E: PASS");
} catch (e) { console.log("\nFAIL:", e.message); console.log(logs.slice(-10).join("\n")); await page.screenshot({ path: `${S}/m5-fail.png` }); process.exitCode = 1; } finally { await browser.close(); }
