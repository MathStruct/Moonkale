// Milestone 16: Sources follow the disk. Files written, changed and deleted behind the app's
// back (by this script, straight into the fixture folder on the server) show up in the tree and
// the graph without any click; a database source is not watched and has a ↻ button that re-reads it.
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
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const rows = () => page.$$eval(".mk-tree-row", (r) => r.map((x) => x.title));
const row = (t) => `.mk-tree-row[title='${t}']`;
// The activity bar, not the tab: a narrow side tile squeezes the tab to "Sour…" beside its ×.
const showSources = async () => { if (!(await page.isVisible(".mk-explorer"))) await page.click("#mk-rail-explorer"); };
try {
  await step("open the folder: the side panel is called Sources and the folder has no ↻ (it is watched)", async () => {
    await page.goto(`http://127.0.0.1:${PORT}/`, { waitUntil: "networkidle" });
    await page.waitForSelector(".wb-workspace");
    await page.click(".mk-explorer-open button[type=submit]");
    await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
    await page.waitForSelector(".wb-tab:has-text('Sources')", { timeout: 5000 });
    await sleep(1500); // the first changes_since answer marks the folder watched
    const refresh = await page.$$(".mk-explorer-source .mk-source-refresh");
    if (refresh.length) throw new Error("a watched folder shows a refresh button");
  });
  await step("a file written on disk appears in the tree, unprompted", async () => {
    fs.writeFileSync(`${ROOT}/Outside.md`, "# Outside\nWritten behind the app's back. See [[Alpha]].\n");
    await page.waitForSelector(row("Outside.md"), { timeout: 15000 });
    console.log("\n  rows:", (await rows()).filter((r) => !r.includes("/")).join(" "));
  });
  await step("…and in the index: the search finds it", async () => {
    await page.click(".mk-explorer");
    await page.keyboard.press("Control+Shift+F");
    await page.waitForFunction(() => document.activeElement?.id === "mk-search-input", null, { timeout: 10000 });
    await page.keyboard.type("behind the app");
    await page.keyboard.press("Enter");
    await page.waitForSelector(".mk-search-hit[title^='Outside.md']", { timeout: 15000 });
    await showSources();
    await page.waitForSelector(".mk-explorer", { state: "visible", timeout: 5000 });
  });
  await step("a new directory with a file inside appears; expanding it shows the file", async () => {
    fs.mkdirSync(`${ROOT}/later`, { recursive: true });
    fs.writeFileSync(`${ROOT}/later/inside.md`, "inside\n");
    await page.waitForSelector(row("later"), { timeout: 15000 });
    await page.click(row("later"));
    await page.waitForSelector(row("later/inside.md"), { timeout: 10000 });
    // A file added to the now-expanded directory follows too.
    fs.writeFileSync(`${ROOT}/later/second.md`, "second\n");
    await page.waitForSelector(row("later/second.md"), { timeout: 15000 });
  });
  await step("deleting on disk removes the rows; ignored and hidden files never show", async () => {
    fs.writeFileSync(`${ROOT}/.secret`, "x");
    fs.mkdirSync(`${ROOT}/target`, { recursive: true });
    fs.writeFileSync(`${ROOT}/target/junk.md`, "x");
    fs.rmSync(`${ROOT}/Outside.md`);
    fs.rmSync(`${ROOT}/later`, { recursive: true });
    await page.waitForFunction(() => !document.querySelector(".mk-tree-row[title='Outside.md']") && !document.querySelector(".mk-tree-row[title='later']"), null, { timeout: 15000 });
    const r = await rows();
    if (r.some((x) => x.startsWith(".secret") || x.startsWith("target"))) throw new Error("hidden/ignored rows: " + r.join(" "));
    await page.screenshot({ path: `${S}/m16-watch.png` });
  });
  await step("a database source is not watched: it has ↻, which re-reads it", async () => {
    await page.click(".mk-tree-db >> text=data.sqlite");
    await page.waitForSelector(".mk-explorer-source-name:has-text('data.sqlite') .mk-source-refresh", { timeout: 15000 });
    await page.click(".mk-explorer-source-name:has-text('data.sqlite') .mk-source-refresh");
    await page.waitForFunction(() => /Refreshed data\.sqlite/.test(document.querySelector(".wb-status-bar").textContent), null, { timeout: 10000 });
    // The context menu offers Refresh for every source.
    await page.click(".mk-explorer-source-name:has-text('m2root')", { button: "right" });
    await page.waitForSelector(".mk-ctx-item:text-is('Refresh')", { timeout: 5000 });
    await page.keyboard.press("Escape");
    await page.mouse.click(5, 5);
  });
  console.log("\nWATCH E2E: PASS");
} catch (e) { console.log("\nFAIL:", e.message); console.log(logs.slice(-10).join("\n")); await page.screenshot({ path: `${S}/m16-watch-fail.png` }); process.exitCode = 1; } finally {
  for (const p of ["Outside.md", "later", ".secret", "target/junk.md"]) try { fs.rmSync(`${ROOT}/${p}`, { recursive: true, force: true }); } catch {}
  await browser.close();
}
