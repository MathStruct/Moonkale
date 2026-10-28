import { control } from './chrome-controls.mjs';
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
// Retained tabs keep multiple Markdown panels mounted. Resolve the requested
// document before taking its stable identity across Source/Rich transitions.
const documentPanel = async (path, host = page) => {
  const pathLabel = host.locator('.mk-editor-path').and(host.getByText(path, { exact: true }));
  const candidate = host.locator('.mk-md:visible').filter({ has: pathLabel });
  await candidate.waitFor({ timeout: 15000 });
  const id = await candidate.getAttribute('data-document-id');
  if (!id) throw Error(`Markdown document identity missing: ${path}`);
  return host.locator(`.mk-md[data-document-id="${id}"]:visible`);
};
const sourcePanel = async (path, text) => {
  const panel = await documentPanel(path);
  const content = panel.locator('.cm-content');
  await content.waitFor({ timeout: 15000 });
  await page.waitForFunction(([id, text]) => {
    const panel = document.querySelector(`.mk-md[data-document-id="${id}"]`);
    return panel?.querySelector('.cm-content')?.textContent.includes(text);
  }, [await panel.getAttribute('data-document-id'), text], { timeout: 15000 });
  return panel;
};
// Optional diagnostics observe the normal click without delaying or forcing it.
const traceTransition = async (path) => {
  if (!process.env.RICH_TRANSITION_TRACE) return async () => {};
  await page.evaluate(() => {
    const identities = new WeakMap(); let next = 1;
    const id = el => el ? (identities.has(el) ? identities.get(el) : (identities.set(el, next), next++)) : null;
    const samples = []; const start = performance.now();
    const sample = () => {
      if (samples.length >= 160) return;
      samples.push({ ms: Math.round(performance.now() - start), active: document.querySelector('.mk-titlebar-title')?.textContent,
        panels: [...document.querySelectorAll('.mk-md')].map(panel => {
          const toolbar = panel.querySelector('.mk-md-modes');
          const button = [...(toolbar?.querySelectorAll('button') ?? [])].find(b => b.textContent === 'Rich');
          const rect = button?.getBoundingClientRect();
          const covering = rect && document.elementFromPoint(rect.x + rect.width / 2, rect.y + rect.height / 2);
          const style = button && getComputedStyle(button);
          return { document: panel.dataset.documentId, path: panel.querySelector('.mk-editor-path')?.textContent,
            panel: id(panel), toolbar: id(toolbar), button: id(button), connected: button?.isConnected,
            rect: rect && { x: rect.x, y: rect.y, width: rect.width, height: rect.height },
            display: style?.display, visibility: style?.visibility, pointerEvents: style?.pointerEvents,
            hit: !!button && button.contains(covering), covering: covering && { tag: covering.tagName, class: covering.className } };
        }) });
    };
    sample(); window.__richTransition = { samples, sample, timer: setInterval(sample, 250) };
  });
  return async () => {
    const samples = await page.evaluate(() => {
      const trace = window.__richTransition; clearInterval(trace.timer); trace.sample(); return trace.samples;
    });
    fs.writeFileSync(`${S}/rich-${path}-transition.json`, JSON.stringify(samples, null, 2));
  };
};
try {
  await step("open folder and Home.md in Source mode", async () => {
    await page.goto(`http://127.0.0.1:${PORT}/`, { waitUntil: "networkidle" });
    await page.waitForSelector(".wb-workspace");
    await control(page, 'click', ".mk-explorer-open button[type=submit]");
    await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
    await control(page, 'click', ".mk-tree-file >> text=Home.md");
    await sourcePanel("Home.md", "# Home");
  });
  await step("Rich mode renders the heading as an <h1>", async () => {
    const home = await documentPanel("Home.md");
    await control(home.locator(".mk-md-modes button:has-text('Rich')"), 'click');
    await page.waitForSelector(".mk-rich-host .ProseMirror h1", { timeout: 30000 });
    console.log("\n  h1:", await page.$eval(".mk-rich-host .ProseMirror h1", (e) => e.textContent));
    await page.screenshot({ path: `${S}/m5-rich.png` });
  });
  await step("typing in rich mode dirties the document; Save writes markdown", async () => {
    await control(page, 'click', ".mk-rich-host .ProseMirror h1");
    await page.keyboard.press("End");
    await page.keyboard.type(" Rich");
    await page.waitForSelector(".mk-rich .mk-editor-dirty", { timeout: 10000 });
    await control(page, 'click', ".mk-rich button:has-text('Save')");
    await page.waitForFunction(() => !document.querySelector(".mk-rich .mk-editor-dirty"), null, { timeout: 15000 });
    const text = fs.readFileSync(`${ROOT}/Home.md`, "utf8");
    console.log("\n  file:", JSON.stringify(text.split("\n")[0]));
    if (!text.startsWith("# Home Rich")) throw new Error("markdown not saved: " + text.slice(0, 60));
    if (!text.includes("[[Alpha]]")) throw new Error("wiki-link lost in round trip: " + text);
  });
  await step("click on the [[Alpha]] link opens Alpha.md (spec 012: decorated, plain click follows)", async () => {
    await page.waitForSelector(".mk-rich-host .mk-wikilink[data-target='Alpha']:not(.mk-wiki-bracket)", { timeout: 15000 });
    await control(page, 'click', ".mk-rich-host .mk-wikilink[data-target='Alpha']:not(.mk-wiki-bracket)");
    await page.waitForFunction(() => document.querySelector(".mk-titlebar-title")?.textContent.startsWith("Alpha.md"), null, { timeout: 15000 });
  });
  await step("back to Source: CodeMirror shows the saved markdown", async () => {
    await control(page, 'click', ".wb-tab:has-text('Home.md')");
    const home = await documentPanel("Home.md");
    await control(home.locator(".mk-md-modes button:has-text('Source')"), 'click');
    await sourcePanel("Home.md", "# Home Rich");
  });
  await step("formulas: $\\alpha$ and $$…$$ render with KaTeX, fonts load, markdown keeps the source (spec 013)", async () => {
    fs.writeFileSync(`${ROOT}/Math.md`, "# Math\n\nInline $\\alpha + \\beta^2$ here, and a macro $\\R$.\n\n$$\n\\int_0^1 x^2 \\, dx = \\frac{1}{3}\n$$\n");
    fs.mkdirSync(`${ROOT}/.moonkale`, { recursive: true });
    fs.writeFileSync(`${ROOT}/.moonkale/katex.json`, JSON.stringify({ macros: { "\\R": "\\mathbb{R}" } }));
    await control(page, 'click', ".mk-explorer-open button[type=submit]");
    await page.waitForSelector(".mk-tree-file >> text=Math.md", { timeout: 15000 });
    await control(page, 'click', ".mk-tree-file >> text=Math.md");
    const math = await sourcePanel("Math.md", "# Math");
    await control(math.locator(".mk-md-modes button:has-text('Rich')"), 'click');
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
    await control(page, 'click', ".mk-explorer-open button[type=submit]");
    await page.waitForSelector(".mk-tree-file >> text=Front.md", { timeout: 15000 });
    const stopTrace = await traceTransition("Front.md");
    let front;
    const started = Date.now();
    try {
      await control(page, 'click', ".mk-tree-file >> text=Front.md");
      front = await sourcePanel("Front.md", 'title: "Front"');
      await control(front.locator(".mk-md-modes button:has-text('Rich')"), 'click');
      await front.locator(".mk-rich-host .ProseMirror h1").filter({ hasText: /^Front$/ }).waitFor({ timeout: 30000 });
    } finally { await stopTrace(); }
    console.log(`\n  Front.md open → Rich: ${Date.now() - started} ms`);
    const pm = await front.locator(".mk-rich-host .ProseMirror").evaluate((e) => e.textContent);
    if (/title:|---/.test(pm)) throw new Error("front matter leaked into the rich view: " + pm.slice(0, 60));
    const summary = await front.locator(".mk-props-summary").evaluate((e) => e.textContent);
    console.log("\n  properties:", summary);
    if (!/title: Front/.test(summary)) throw new Error("summary wrong");
    await control(front.locator(".mk-props-head"), 'click');
    await front.locator(".mk-props-yaml").waitFor({ timeout: 5000 });
    await control(front.locator(".mk-props-yaml"), 'fill', 'title: "Front"\ntags: [demo, edited]');
    await control(front.locator(".mk-rich-host .ProseMirror p"), 'click');
    await page.keyboard.press("End");
    await page.keyboard.type(" More.");
    await front.locator(".ProseMirror p").filter({ hasText: "More." }).waitFor({ timeout: 5000 });
    await page.waitForTimeout(400);
    await control(front.locator("button:has-text('Save')"), 'click');
    await front.locator(".mk-editor-dirty").waitFor({ state: "hidden", timeout: 15000 });
    const saved = fs.readFileSync(`${ROOT}/Front.md`, "utf8");
    console.log("  saved:", JSON.stringify(saved));
    if (!saved.startsWith('---\ntitle: "Front"\ntags: [demo, edited]\n---\n')) throw new Error("front matter not saved intact");
    if (!/Body text\. More\./.test(saved)) throw new Error("body edit lost");
  });
  await step("retained Markdown panels keep identity, mode and unsaved edits during rapid switching", async () => {
    const front = await documentPanel("Front.md");
    const id = await front.getAttribute("data-document-id");
    const saved = fs.readFileSync(`${ROOT}/Front.md`, "utf8");
    await control(front.locator(".ProseMirror p"), 'click');
    await page.keyboard.press("End");
    await page.keyboard.type(" Pending.");
    await front.locator(".mk-editor-dirty").waitFor({ timeout: 10000 });
    for (let i = 0; i < 3; i++) {
      await control(front.locator(".mk-md-modes button:has-text('Source')"), 'click');
      await sourcePanel("Front.md", "Body text. More. Pending.");
      await control(page, 'click', ".wb-tab:has-text('Math.md')");
      const math = await documentPanel("Math.md");
      if (!(await math.locator(".mk-md-modes button:has-text('Rich')").getAttribute("class")).includes("mk-btn-on")) throw Error("Math mode was changed by Front");
      await control(math.locator(".mk-md-modes button:has-text('Source')"), 'click');
      await sourcePanel("Math.md", "# Math");
      await control(math.locator(".mk-md-modes button:has-text('Rich')"), 'click');
      await math.locator(".ProseMirror .katex").first().waitFor({ timeout: 30000 });
      await control(page, 'click', ".wb-tab:has-text('Front.md')");
      const active = await sourcePanel("Front.md", "Body text. More. Pending.");
      if (await active.getAttribute("data-document-id") !== id) throw Error("Front identity changed");
      await control(active.locator(".mk-md-modes button:has-text('Rich')"), 'click');
      await active.locator(".ProseMirror p").filter({ hasText: "Body text. More. Pending." }).waitFor({ timeout: 30000 });
    }
    if (fs.readFileSync(`${ROOT}/Front.md`, "utf8") !== saved) throw Error("unsaved edit reached disk");
    if (/Pending|Body text/.test(fs.readFileSync(`${ROOT}/Math.md`, "utf8"))) throw Error("Front edit reached Math");
    // Restore the saved text so the following reload does not discard an edit.
    await control(front.locator(".mk-md-modes button:has-text('Source')"), 'click');
    const source = await sourcePanel("Front.md", "Pending.");
    await control(source.locator(".cm-content"), 'click');
    await page.keyboard.press("Control+a");
    await page.keyboard.insertText(saved);
    await source.locator(".mk-editor-dirty").waitFor({ state: "hidden", timeout: 15000 });
  });
  await step("spec 021: without the workspace override markdown opens in Rich mode, and loading does not dirty the file", async () => {
    // run-all.sh writes .moonkale/settings.json with markdown_rich=false for the other suites.
    fs.rmSync(`${ROOT}/.moonkale/settings.json`, { force: true });
    fs.writeFileSync(`${ROOT}/Norm.md`, "Title\n=====\n\n* one\n* two\n\n\nDone.\n");
    await page.goto(`http://127.0.0.1:${PORT}/`, { waitUntil: "networkidle" });
    await page.waitForSelector(".wb-workspace");
    await control(page, 'click', ".mk-explorer-open button[type=submit]");
    await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
    await control(page, 'click', ".mk-tree-file >> text=Norm.md");
    const norm = await documentPanel("Norm.md");
    await norm.locator(".ProseMirror h1").filter({ hasText: /^Title$/ }).waitFor({ timeout: 30000 });
    const on = await norm.locator(".mk-md-modes button:has-text('Rich')").evaluate((b) => b.classList.contains("mk-btn-on"));
    if (!on) throw new Error("Rich is not the default mode");
    await page.waitForTimeout(800);
    if (await page.$(".mk-rich:visible .mk-editor-dirty")) throw new Error("opening in Rich mode marked the file dirty");
    if (fs.readFileSync(`${ROOT}/Norm.md`, "utf8") !== "Title\n=====\n\n* one\n* two\n\n\nDone.\n") throw new Error("file changed on open");
    await control(page, 'click', ".mk-rich-host:visible .ProseMirror h1");
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
    await control(p2, 'click', ".mk-explorer-open button[type=submit]");
    await p2.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
    await control(p2, 'click', ".mk-tree-file >> text=Norm.md");
    const norm = await documentPanel("Norm.md", p2);
    await norm.locator(".ProseMirror").waitFor({ timeout: 30000 });
    const style = await norm.locator(".ProseMirror").evaluate((e) => { const c = getComputedStyle(e); return { size: c.fontSize, family: c.fontFamily }; });
    await ctx2.close();
    console.log("\n  ProseMirror:", JSON.stringify(style));
    if (style.size !== "22px" || !/Georgia/.test(style.family)) throw new Error("typography settings not applied");
  });
  await step("Prompt26: no <br /> — old ones are dropped on the next save, blank lines typed become one", async () => {
    // Written behind the app's back: the tree picks it up by itself (Milestone 16).
    fs.writeFileSync(`${ROOT}/Gaps.md`, "# Gaps\n\nfirst\n\n<br />\n\n<br />\n\nsecond\n");
    if (!(await page.isVisible(".mk-explorer"))) await control(page, 'click', "#mk-rail-explorer"); // the rail entry toggles
    await page.waitForSelector(".mk-tree-file >> text=Gaps.md", { timeout: 15000 });
    await control(page, 'click', ".mk-tree-file >> text=Gaps.md");
    const gaps = await documentPanel("Gaps.md");
    await control(gaps.locator(".mk-md-modes button:has-text('Rich')"), 'click');
    await gaps.locator(".ProseMirror h1").filter({ hasText: /^Gaps$/ }).waitFor({ timeout: 30000 });
    await new Promise((r) => setTimeout(r, 500));
    if (await page.$(".mk-rich:visible .mk-editor-dirty")) throw new Error("opening a file with <br /> lines marked it dirty");
    await control(page, 'click', ".mk-rich-host:visible .ProseMirror p:has-text('second')");
    await page.keyboard.press("End");
    await page.keyboard.press("Enter");
    await page.keyboard.press("Enter");
    await page.keyboard.press("Enter");
    await page.keyboard.type("third");
    await page.waitForSelector(".mk-rich:visible .mk-editor-dirty", { timeout: 10000 });
    await control(page, 'click', ".mk-rich:visible button:has-text('Save')");
    await page.waitForSelector(".mk-rich:visible .mk-editor-dirty", { state: "hidden", timeout: 15000 });
    const text = fs.readFileSync(`${ROOT}/Gaps.md`, "utf8");
    console.log("\n  file:", JSON.stringify(text));
    if (/<br/i.test(text)) throw new Error("<br /> written: " + JSON.stringify(text));
    if (!/first\n\n+second/.test(text) || !/third/.test(text)) throw new Error("text lost: " + JSON.stringify(text));
    if (/\n\n\n/.test(text)) throw new Error("blank lines not collapsed: " + JSON.stringify(text));
  });
  await step("P-165: a file Milkdown would rewrite opens clean, and Reload after an outside change leaves it clean", async () => {
    // `-` bullets serialise as `*`: before the fix the view took the raw text as its baseline,
    // so the normalised form counted as an edit — on open and after Reload.
    fs.writeFileSync(`${ROOT}/Reload.md`, "# Reload\n\n- one\n- two\n");
    if (!(await page.isVisible(".mk-explorer"))) await control(page, 'click', "#mk-rail-explorer");
    await page.waitForSelector(".mk-tree-file >> text=Reload.md", { timeout: 15000 });
    await control(page, 'click', ".mk-tree-file >> text=Reload.md");
    const reload = await documentPanel("Reload.md");
    await control(reload.locator(".mk-md-modes button:has-text('Rich')"), 'click');
    await reload.locator(".ProseMirror li").first().waitFor({ timeout: 30000 });
    await page.waitForTimeout(1000);
    if (await page.$(".mk-rich:visible .mk-editor-dirty")) throw new Error("opening Reload.md marked it dirty");
    fs.writeFileSync(`${ROOT}/Reload.md`, "# Reload\n\n- one\n- two\n- three\n");
    await page.waitForTimeout(500);
    await control(page, 'click', ".mk-rich:visible button:has-text('Reload')");
    await page.waitForFunction(() => [...document.querySelectorAll(".mk-rich-host .ProseMirror")].some((e) => e.offsetParent && /three/.test(e.textContent)), null, { timeout: 15000 });
    await page.waitForTimeout(1000);
    if (await page.$(".mk-rich:visible .mk-editor-dirty")) throw new Error("Reload left the document dirty");
    if (fs.readFileSync(`${ROOT}/Reload.md`, "utf8") !== "# Reload\n\n- one\n- two\n- three\n") throw new Error("file changed");
  });
  console.log("\nRICH E2E: PASS");
} catch (e) {
  console.log("\nFAIL:", e.message);
  console.log(logs.slice(-10).join("\n"));
  console.log("FAILED AT:", e.message.split("\n").slice(0, 3).join(" | ").slice(0, 300));
  // P-154: the Rich click sometimes finds no visible target on CI — sample what the page shows.
  for (let i = 0; i < 6; i++) {
    const d = await page.evaluate(() => ({
      active: document.querySelector(".mk-titlebar-title")?.textContent,
      status: document.querySelector(".wb-status-bar")?.textContent,
      documents: [...document.querySelectorAll('.mk-editor-path')].map(el => el.textContent),
      modes: [...document.querySelectorAll(".mk-md-modes button")].filter((b) => /Rich/.test(b.textContent)).map((b) => { const r = b.getBoundingClientRect(); const cs = getComputedStyle(b); return `${Math.round(r.x)},${Math.round(r.y)} ${Math.round(r.width)}x${Math.round(r.height)} vis=${cs.visibility} op=${cs.opacity} disp=${cs.display} parent=${!!b.offsetParent}`; }),
    })).catch((x) => String(x));
    console.log("DIAG", JSON.stringify(d));
    await page.waitForTimeout(150);
  }
  await page.screenshot({ path: `${S}/m5-fail.png` }); process.exitCode = 1; } finally { await browser.close(); }
