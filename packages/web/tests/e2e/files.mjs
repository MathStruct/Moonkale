// Milestone 7: file operations in the Explorer — context menu, new file/folder, inline rename,
// drag a file onto a folder, delete to .moonkale/trash; open documents follow renames.
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
const labels = () => page.$$eval(".mk-tree-row", (r) => r.map((x) => x.title));
const ctxMenu = async (selector, item) => {
  await page.click(selector, { button: "right" });
  await page.waitForSelector(".mk-ctx", { timeout: 5000 });
  await page.click(`.mk-ctx-item:text-is('${item}')`);
};
try {
  await step("open folder; right-click the root → New File… 'todo.md' → created, listed and opened", async () => {
    await page.goto(`http://127.0.0.1:${PORT}/`, { waitUntil: "networkidle" });
    await page.waitForSelector(".wb-workspace");
    await page.click(".mk-explorer-open button[type=submit]");
    await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
    await ctxMenu(".mk-explorer-source-name", "New File…");
    await page.waitForSelector("#mk-tree-edit", { timeout: 5000 });
    await sleep(100);
    const focused = await page.evaluate(() => document.activeElement?.id);
    console.log("\n  focused:", focused);
    if (focused !== "mk-tree-edit") throw new Error("edit field not focused");
    await page.keyboard.type("todo.md");
    await page.keyboard.press("Enter");
    await page.waitForSelector(".mk-tree-row[title='todo.md']", { timeout: 10000 });
    await page.waitForSelector(".wb-tab:has-text('todo.md')", { timeout: 10000 });
    if (!fs.existsSync(`${ROOT}/todo.md`)) throw new Error("not on disk");
    await page.screenshot({ path: `${S}/m7-files-new.png` });
  });
  await step("right-click 'notes' → New Folder… 'drafts' → listed; on disk", async () => {
    await ctxMenu(".mk-tree-row[title='notes']", "New Folder…");
    await page.waitForSelector("#mk-tree-edit");
    await page.keyboard.type("drafts");
    await page.keyboard.press("Enter");
    await page.waitForSelector(".mk-tree-row[title='notes/drafts']", { timeout: 10000 });
    if (!fs.statSync(`${ROOT}/notes/drafts`).isDirectory()) throw new Error("not a directory");
  });
  await step("type into todo.md, then Rename… → 'TODO.md': tree, tab and disk follow; the unsaved text survives", async () => {
    await page.click(".cm-content");
    await page.keyboard.type("hello");
    await page.waitForSelector(".mk-tab-dirty", { timeout: 5000 });
    await ctxMenu(".mk-tree-row[title='todo.md']", "Rename…");
    await page.waitForSelector("#mk-tree-edit");
    await page.fill("#mk-tree-edit", "TODO.md");
    await page.keyboard.press("Enter");
    await page.waitForSelector(".mk-tree-row[title='TODO.md']", { timeout: 10000 });
    await page.waitForSelector(".wb-tab:has-text('TODO.md')", { timeout: 10000 });
    if (fs.existsSync(`${ROOT}/todo.md`) || !fs.existsSync(`${ROOT}/TODO.md`)) throw new Error("disk not renamed");
    await page.waitForSelector(".cm-content:has-text('hello')", { timeout: 5000 });
    if (!(await page.$(".mk-tab-dirty"))) throw new Error("dirty state lost");
    await page.keyboard.press("Control+S");
    await page.waitForFunction(() => !document.querySelector(".mk-tab-dirty"), null, { timeout: 5000 });
    if (fs.readFileSync(`${ROOT}/TODO.md`, "utf8") !== "hello") throw new Error("saved text: " + JSON.stringify(fs.readFileSync(`${ROOT}/TODO.md`, "utf8")));
  });
  await step("drag TODO.md onto notes → notes/TODO.md", async () => {
    await page.dragAndDrop(".mk-tree-row[title='TODO.md']", ".mk-tree-row[title='notes']");
    await page.waitForSelector(".mk-tree-row[title='notes/TODO.md']", { timeout: 10000 });
    if (!fs.existsSync(`${ROOT}/notes/TODO.md`)) throw new Error("not moved on disk");
    await page.waitForSelector(".wb-tab:has-text('TODO.md')", { timeout: 5000 });
  });
  await step("Delete… notes/TODO.md → confirm → gone from tree and editor, kept in .moonkale/trash", async () => {
    await ctxMenu(".mk-tree-row[title='notes/TODO.md']", "Delete…");
    await page.waitForSelector(".mk-explorer-confirm", { timeout: 5000 });
    await page.click(".mk-explorer-confirm .mk-btn-danger");
    await page.waitForSelector(".mk-tree-row[title='notes/TODO.md']", { state: "detached", timeout: 10000 });
    await page.waitForSelector(".wb-tab:has-text('TODO.md')", { state: "detached", timeout: 5000 });
    if (fs.existsSync(`${ROOT}/notes/TODO.md`)) throw new Error("still on disk");
    const trash = fs.readdirSync(`${ROOT}/.moonkale/trash`);
    const kept = fs.readFileSync(`${ROOT}/.moonkale/trash/${trash[0]}/notes/TODO.md`, "utf8");
    console.log("\n  trash:", trash, JSON.stringify(kept));
    if (kept !== "hello") throw new Error("trash copy wrong");
    console.log("  tree:", (await labels()).join(" "));
  });
  await step("Close Folder (spec 015): refused while a document is unsaved; then closes everything", async () => {
    await page.click(".mk-tree-file >> text=README.md");
    await page.waitForSelector(".cm-content", { timeout: 15000 });
    await page.click(".cm-content");
    await page.keyboard.type("x");
    await page.waitForSelector(".mk-tab-dirty", { timeout: 5000 });
    await ctxMenu(".mk-explorer-source-name", "Close Folder");
    await page.waitForFunction(() => /unsaved document/.test(document.querySelector(".wb-status-bar")?.textContent || ""), null, { timeout: 5000 });
    if (!(await page.$(".mk-explorer-source-name"))) throw new Error("folder closed despite an unsaved document");
    await page.click(".mk-editor-toolbar button:has-text('Reload'):visible");
    await page.waitForFunction(() => !document.querySelector(".mk-tab-dirty"), null, { timeout: 5000 });
    await ctxMenu(".mk-explorer-source-name", "Close Folder");
    await page.waitForFunction(() => !document.querySelector(".mk-explorer-source-name"), null, { timeout: 10000 });
    const tabs = await page.$$eval(".wb-tab", (t) => t.map((x) => x.textContent.trim()).filter((x) => /\.md|\.rs|\.toml/.test(x)));
    console.log("\n  document tabs after close:", JSON.stringify(tabs));
    if (tabs.length) throw new Error("documents still open: " + tabs);
    await page.waitForFunction(() => /No folder open/.test(document.querySelector(".wb-status-bar")?.textContent || ""), null, { timeout: 5000 });
  });
  console.log("FILES E2E: PASS");
} catch (e) {
  console.log("\nFAIL:", e.message);
  await page.screenshot({ path: `${S}/m7-files-fail.png` }).catch(() => {});
  console.log(logs.filter((l) => !/WARN/.test(l)).slice(-20).join("\n"));
  process.exitCode = 1;
} finally {
  await browser.close();
}
