// Spec 012: [[wiki-links]] like in Obsidian — decorated in source and rich mode, `[[` completion
// in both, click/Ctrl+click follows, an unresolved link is created on click, the Links panel
// offers Create, and renaming a note rewrites the links pointing at it.
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
const ctxMenu = async (selector, item) => {
  await page.click(selector, { button: "right" });
  await page.waitForSelector(".mk-ctx", { timeout: 5000 });
  await page.click(`.mk-ctx-item:text-is('${item}')`);
};
const title = () => page.$eval(".mk-titlebar-title", (e) => e.textContent);
try {
  await step("open folder and Home.md (source): [[Alpha]] is decorated as resolved, [[Missing]] as unresolved", async () => {
    await page.goto(`http://127.0.0.1:${PORT}/`, { waitUntil: "networkidle" });
    await page.waitForSelector(".wb-workspace");
    await page.click(".mk-explorer-open button[type=submit]");
    await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
    await page.click(".mk-tree-file >> text=Home.md");
    await page.waitForSelector(".mk-md .cm-content", { timeout: 15000 });
    await page.waitForSelector(".cm-wikilink", { timeout: 15000 });
    const marks = await page.$$eval(".cm-wikilink", (els) => els.map((e) => [e.textContent, e.classList.contains("cm-wikilink-unresolved")]));
    console.log("\n  marks:", JSON.stringify(marks));
    const alpha = marks.find((m) => m[0].includes("Alpha")), missing = marks.find((m) => m[0].includes("Missing"));
    if (!alpha || alpha[1]) throw new Error("[[Alpha]] should be resolved");
    if (!missing || !missing[1]) throw new Error("[[Missing]] should be unresolved");
  });
  await step("source: typing [[Be offers notes/Beta; Enter inserts [[Beta]]", async () => {
    await page.click(".mk-md .cm-content");
    await page.keyboard.press("Control+End");
    await page.keyboard.type("\nSee [[Be");
    await page.waitForSelector(".cm-tooltip-autocomplete .cm-completionLabel", { timeout: 10000 });
    const labels = await page.$$eval(".cm-tooltip-autocomplete .cm-completionLabel", (els) => els.map((e) => e.textContent));
    console.log("\n  completions:", JSON.stringify(labels));
    if (!labels.includes("Beta")) throw new Error("Beta not offered");
    await page.keyboard.press("Enter");
    await page.waitForFunction(() => /See \[\[Beta\]\]/.test(document.querySelector(".mk-md .cm-content")?.textContent || ""), null, { timeout: 5000 });
  });
  await step("source: Ctrl+click on [[Missing]] creates Missing.md and opens it", async () => {
    const box = await page.$eval(".cm-wikilink-unresolved", (e) => { const b = e.getBoundingClientRect(); return { x: b.x + b.width / 2, y: b.y + b.height / 2 }; });
    // Headless Firefox drops modifier flags on page.mouse (P-055): dispatch the event.
    await page.evaluate(({ x, y }) => { const t = document.elementFromPoint(x, y); t.dispatchEvent(new MouseEvent("mousedown", { bubbles: true, cancelable: true, ctrlKey: true, clientX: x, clientY: y, button: 0 })); }, box);
    await page.waitForFunction(() => document.querySelector(".mk-titlebar-title")?.textContent.startsWith("Missing.md"), null, { timeout: 15000 });
    if (!fs.existsSync(`${ROOT}/Missing.md`)) throw new Error("Missing.md not created");
    console.log("\n  created:", JSON.stringify(fs.readFileSync(`${ROOT}/Missing.md`, "utf8")));
  });
  await step("rich mode: links are decorated, brackets hidden; plain click on Alpha opens Alpha.md", async () => {
    await page.click(".wb-tab:has-text('Home.md')");
    await page.click(".mk-md-modes button:has-text('Rich'):visible");
    await page.waitForSelector(".mk-rich-host:visible .mk-wikilink:not(.mk-wiki-bracket)", { timeout: 30000 });
    await page.waitForFunction(() => [...document.querySelectorAll(".mk-rich-host .mk-wikilink")].some((e) => e.textContent === "Alpha"), null, { timeout: 15000 }).catch(() => {});
    const links = await page.$$eval(".mk-rich-host:visible .mk-wikilink", (els) => [...new Set(els.map((e) => e.getAttribute("data-target")))]);
    const hidden = await page.$$eval(".mk-rich-host:visible .mk-wiki-bracket", (els) => els.filter((e) => getComputedStyle(e).display === "none").length);
    console.log("\n  rich links:", JSON.stringify(links), "hidden bracket spans:", hidden);
    if (!links.includes("Alpha")) throw new Error("Alpha not decorated in rich mode");
    if (hidden < 2) throw new Error("brackets not hidden");
    await page.screenshot({ path: `${S}/m10-wiki-rich.png` });
    const el = await page.$(".mk-rich-host:visible .mk-wikilink[data-target='Alpha']:not(.mk-wiki-bracket)");
    await el.click();
    await page.waitForFunction(() => document.querySelector(".mk-titlebar-title")?.textContent.startsWith("Alpha.md"), null, { timeout: 15000 });
  });
  await step("rich mode: typing [[Ho opens the page popup; Enter inserts [[Home]]", async () => {
    await page.click(".mk-md-modes button:has-text('Rich'):visible");
    await page.waitForSelector(".mk-rich-host:visible .ProseMirror", { timeout: 30000 });
    await page.click(".mk-rich-host:visible .ProseMirror p");
    await page.keyboard.press("End");
    await page.keyboard.type(" [[Ho");
    await page.waitForSelector(".mk-rich-host:visible .mk-wiki-popup .mk-wiki-target", { timeout: 10000 });
    const items = await page.$$eval(".mk-rich-host:visible .mk-wiki-popup .mk-wiki-target", (els) => els.map((e) => e.textContent));
    const stray = await page.$$eval(".mk-wiki-popup", (els) => els.length);
    if (stray !== 1) throw new Error(`${stray} popups open (a hidden editor kept one)`);
    console.log("\n  popup:", JSON.stringify(items));
    if (!items.includes("Home")) throw new Error("Home not offered");
    await page.keyboard.press("Enter");
    await page.waitForFunction(() => [...document.querySelectorAll(".mk-rich-host .mk-wikilink")].some((e) => e.getAttribute("data-target") === "Home"), null, { timeout: 5000 });
    await page.click(".mk-rich button:has-text('Save'):visible");
    await page.waitForFunction(() => ![...document.querySelectorAll(".mk-rich .mk-editor-dirty")].some((e) => e.offsetParent !== null), null, { timeout: 15000 });
    const text = fs.readFileSync(`${ROOT}/Alpha.md`, "utf8");
    if (!text.includes("[[Home]]")) throw new Error("[[Home]] not saved: " + text);
  });
  await step("Links panel: an unresolved link has a Create button", async () => {
    await page.click(".mk-rich-host:visible .ProseMirror p");
    await page.keyboard.press("End");
    await page.keyboard.type(" Also [[Nowhere");
    await page.keyboard.press("Escape");
    await page.keyboard.type("]].");
    await page.click(".mk-rich button:has-text('Save'):visible");
    await page.waitForFunction(() => /\[\[Nowhere\]\]/.test(document.body.textContent) , null, { timeout: 5000 }).catch(() => {});
    await page.waitForFunction(() => ![...document.querySelectorAll(".mk-rich .mk-editor-dirty")].some((e) => e.offsetParent !== null), null, { timeout: 15000 });
    await page.click(".wb-tab:has-text('Links')");
    await page.waitForSelector(".mk-links-phantom .mk-links-create", { timeout: 30000 });
    await page.click(".mk-links-phantom .mk-links-create");
    await page.waitForFunction(() => document.querySelector(".mk-titlebar-title")?.textContent.startsWith("Nowhere.md"), null, { timeout: 15000 });
    if (!fs.existsSync(`${ROOT}/Nowhere.md`)) throw new Error("Nowhere.md not created");
  });
  await step("rename Alpha.md → Alpha2.md rewrites [[Alpha]] in Home.md and notes/Beta.md", async () => {
    await page.click(".wb-tab:has-text('Explorer')");
    await ctxMenu(".mk-tree-row[title='Alpha.md']", "Rename…");
    await page.waitForSelector("#mk-tree-edit");
    await page.fill("#mk-tree-edit", "Alpha2.md");
    await page.keyboard.press("Enter");
    await page.waitForSelector(".mk-tree-row[title='Alpha2.md']", { timeout: 10000 });
    await page.waitForFunction(() => /links updated/.test(document.querySelector(".wb-status-bar")?.textContent || ""), null, { timeout: 15000 });
    console.log("\n  status:", await page.$eval(".wb-status-bar", (e) => e.textContent.trim().slice(0, 120)));
    // Home.md is open (dirty from the typing above): the rewrite lands in the document, so save it.
    await page.click(".wb-tab:has-text('Home.md')");
    const homeDoc = await page.evaluate(() => [...document.querySelectorAll(".mk-rich-host .ProseMirror, .mk-md .cm-content")].filter((e) => e.offsetParent !== null).map((e) => e.textContent).join(" "));
    if (!homeDoc.includes("[[Alpha2]]") && !homeDoc.includes("Alpha2")) throw new Error("open Home.md not rewritten: " + homeDoc.slice(0, 120));
    const beta = fs.readFileSync(`${ROOT}/notes/Beta.md`, "utf8");
    console.log("\n  Beta.md:", JSON.stringify(beta));
    if (!beta.includes("[[Alpha2]]")) throw new Error("Beta.md not rewritten on disk");
  });
  console.log("\nWIKI E2E: PASS");
} catch (e) { console.log("\nFAIL:", e.message); console.log(logs.slice(-12).join("\n")); await page.screenshot({ path: `${S}/m10-wiki-fail.png` }); process.exitCode = 1; } finally { await browser.close(); }
