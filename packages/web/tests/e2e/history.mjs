// Milestone 8: the entity log — edits, creates, renames, deletes and commits become events in
// .moonkale/history.jsonl; the History panel lists them, filters to the active file, and shows a
// file's text as it was after any event; the log survives a reload.
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
const kinds = () => page.$$eval(".mk-history-event", (l) => l.map((x) => x.dataset.kind));
const waitKinds = (pred, timeout = 15000) => page.waitForFunction((src) => { const k = [...document.querySelectorAll(".mk-history-event")].map((x) => x.dataset.kind); return new Function("k", "return " + src)(k); }, pred, { timeout });
const ctxMenu = async (selector, item) => { await page.click(selector, { button: "right" }); await page.waitForSelector(".mk-ctx"); await page.click(`.mk-ctx-item:text-is('${item}')`); };
try {
  await step("open folder; History tab is empty", async () => {
    await page.goto(`http://127.0.0.1:${PORT}/`, { waitUntil: "networkidle" });
    await page.waitForSelector(".wb-workspace");
    await page.click(".mk-explorer-open button[type=submit]");
    await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
    await page.click(".wb-tab:has-text('History')");
    await page.waitForSelector(".mk-history", { timeout: 10000 });
    if ((await kinds()).length !== 0) throw new Error("expected no events");
  });
  await step("edit README.md and save → a content event by the user; the file records it", async () => {
    await page.click(".wb-tab:has-text('Explorer')");
    await page.click(".mk-tree-file >> text=README.md");
    await page.waitForSelector(".cm-content", { timeout: 15000 });
    await page.click(".cm-content");
    await page.keyboard.press("Control+End");
    await page.keyboard.type("\nLogged line.\n");
    await page.keyboard.press("Control+S");
    await page.waitForFunction(() => !document.querySelector(".mk-tab-dirty"), null, { timeout: 5000 });
    await page.click(".wb-tab:has-text('History')");
    await waitKinds("k.includes('content')");
    const actor = await page.$eval(".mk-history-event[data-kind=content] .mk-history-actor", (e) => e.getAttribute("title"));
    console.log("\n  actor:", actor, "| summary:", await page.$eval(".mk-history-event[data-kind=content] .mk-history-summary", (e) => e.textContent));
    if (!actor.startsWith("user:")) throw new Error("actor");
    for (let i = 0; i < 20 && !fs.existsSync(`${ROOT}/.moonkale/history.jsonl`); i++) await sleep(250);
    const lines = fs.readFileSync(`${ROOT}/.moonkale/history.jsonl`, "utf8").trim().split("\n");
    if (lines.length !== 1 || !/"kind":"content"/.test(lines[0])) throw new Error("file: " + lines.join(" | "));
  });
  await step("new file, rename, delete → add / rename / remove events", async () => {
    await page.click(".wb-tab:has-text('Explorer')");
    await ctxMenu(".mk-explorer-source-name", "New File…");
    await page.waitForSelector("#mk-tree-edit");
    await page.keyboard.type("note.md");
    await page.keyboard.press("Enter");
    await page.waitForSelector(".mk-tree-row[title='note.md']", { timeout: 10000 });
    await ctxMenu(".mk-tree-row[title='note.md']", "Rename…");
    await page.fill("#mk-tree-edit", "notes.md");
    await page.keyboard.press("Enter");
    await page.waitForSelector(".mk-tree-row[title='notes.md']", { timeout: 10000 });
    await ctxMenu(".mk-tree-row[title='notes.md']", "Delete…");
    await page.click(".mk-explorer-confirm .mk-btn-danger");
    await page.waitForSelector(".mk-tree-row[title='notes.md']", { state: "detached", timeout: 10000 });
    await page.click(".wb-tab:has-text('History')");
    await waitKinds("k.includes('add') && k.includes('rename') && k.includes('remove')");
    console.log("\n  kinds (newest first):", (await kinds()).join(", "));
  });
  await step("commit from the Changes tab → a checkpoint event", async () => {
    await page.click(".wb-tab:has-text('Changes')");
    await page.waitForSelector(".mk-git-entry[data-status='M']", { timeout: 15000 });
    await page.click(".mk-git-group-head button:has-text('stage all')");
    await page.waitForFunction(() => /Staged \(1\)/.test(document.querySelector(".mk-git").textContent), null, { timeout: 10000 });
    await page.fill("#mk-git-commit-message", "history checkpoint");
    await page.click(".mk-git-commit button");
    await page.waitForSelector("text=Working tree clean", { timeout: 15000 });
    await page.click(".wb-tab:has-text('History')");
    await waitKinds("k[0] === 'checkpoint'");
    const s = await page.$eval(".mk-history-checkpoint .mk-history-summary", (e) => e.textContent);
    console.log("\n  checkpoint:", s);
    if (!/^commit [0-9a-f]{7} history checkpoint/.test(s)) throw new Error("checkpoint summary");
  });
  await step("'active file' filter and 'text' view: README.md as it was after the edit", async () => {
    await page.click(".wb-tab:has-text('README.md')");
    await page.click(".wb-tab:has-text('History')");
    await page.click(".mk-history-check input");
    await waitKinds("k.length === 1 && k[0] === 'content'");
    await page.click(".mk-history-event[data-kind=content] button:has-text('text')");
    await page.waitForSelector(".mk-history-text", { timeout: 10000 });
    const t = await page.$eval(".mk-history-text", (e) => e.textContent);
    if (!t.includes("Logged line.")) throw new Error("text: " + JSON.stringify(t));
    await page.screenshot({ path: `${S}/m8-history.png` });
  });
  await step("reload: the events come back from the file", async () => {
    await page.reload({ waitUntil: "networkidle" });
    await page.waitForSelector(".wb-workspace");
    await page.click(".mk-explorer-open button[type=submit]");
    await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
    await page.click(".wb-tab:has-text('History')");
    await waitKinds("k.length >= 5 && k.includes('checkpoint')");
    console.log("\n  after reload:", (await kinds()).length, "events");
  });
  await step("Milestone 9: edit again, then Restore the earlier text → unsaved edit; saving records cause", async () => {
    await page.click(".wb-tab:has-text('README.md')");
    await page.click(".cm-content");
    await page.keyboard.press("Control+End");
    await page.keyboard.type("Second edit.\n");
    await page.keyboard.press("Control+S");
    await page.waitForFunction(() => !document.querySelector(".mk-tab-dirty"), null, { timeout: 5000 });
    await page.click(".wb-tab:has-text('History')");
    await page.click(".mk-history-check input"); // off
    await page.click(".mk-history-check input"); // on again → active file
    await waitKinds("k.length === 2");
    // The older content event is the last in the list (newest first).
    await page.click(".mk-history-event[data-kind=content]:last-child button:has-text('text')");
    await page.waitForSelector(".mk-history-view button:has-text('Restore')", { timeout: 10000 });
    await page.click(".mk-history-view button:has-text('Restore')");
    await page.waitForSelector(".wb-tab:has-text('README.md') .mk-tab-dirty", { timeout: 10000 });
    await page.click(".wb-tab:has-text('README.md')");
    const t = await page.$$eval(".cm-content", (l) => l.map((e) => e.textContent).find((x) => x.includes("Logged line")));
    if (!t || t.includes("Second edit")) throw new Error("editor text: " + JSON.stringify(t));
    if (fs.readFileSync(`${ROOT}/README.md`, "utf8").includes("Second edit.") === false) throw new Error("disk changed before save");
    await page.keyboard.press("Control+S");
    await page.waitForFunction(() => !document.querySelector(".mk-tab-dirty"), null, { timeout: 5000 });
    for (let i = 0; i < 20; i++) { if (/"cause"/.test(fs.readFileSync(`${ROOT}/.moonkale/history.jsonl`, "utf8"))) break; await sleep(250); }
    const lines = fs.readFileSync(`${ROOT}/.moonkale/history.jsonl`, "utf8").trim().split("\n");
    const last = JSON.parse(lines[lines.length - 1]);
    console.log("\n  last event:", last.kind, "cause:", last.cause ? "set" : "none");
    if (last.kind !== "content" || !last.cause) throw new Error("no cause on the restore save");
  });
  console.log("HISTORY E2E: PASS");
} catch (e) {
  console.log("\nFAIL:", e.message);
  await page.screenshot({ path: `${S}/m8-history-fail.png` }).catch(() => {});
  console.log(logs.filter((l) => !/WARN/.test(l)).slice(-15).join("\n"));
  process.exitCode = 1;
} finally {
  await browser.close();
}
