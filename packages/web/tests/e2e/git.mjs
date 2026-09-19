// Milestone 7: git — Changes panel (status, diff, stage, commit, log), explorer/tab decorations,
// discard of an untracked file, history drawn as a graph.
import { firefox } from "playwright";
import fs from "node:fs";
import { execSync } from "node:child_process";
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
const git = (args) => execSync(`git -C ${ROOT} ${args}`, { encoding: "utf8" }).trim();
const entries = () => page.$$eval(".mk-git-entry", (l) => l.map((x) => `${x.dataset.status} ${x.querySelector(".mk-git-path").textContent}`));
try {
  await step("open folder; Changes tab shows branch main, a clean tree and the fixture commit", async () => {
    await page.goto(`http://127.0.0.1:${PORT}/`, { waitUntil: "networkidle" });
    await page.waitForSelector(".wb-workspace");
    await page.click(".mk-explorer-open button[type=submit]");
    await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
    await page.click(".wb-tab:has-text('Changes')");
    await page.waitForSelector(".mk-git-branch", { timeout: 15000 });
    const branch = await page.$eval(".mk-git-branch", (e) => e.textContent);
    await page.waitForSelector(".mk-git-commit-row", { timeout: 10000 });
    const log = await page.$$eval(".mk-git-subject", (l) => l.map((x) => x.textContent));
    console.log("\n  branch:", branch, "| log:", log.join(", "));
    if (!branch.includes("main") || !log.includes("fixture")) throw new Error("status/log wrong");
    if (!(await page.$("text=Working tree clean"))) throw new Error("expected clean tree");
  });
  await step("edit README.md and save → 'M README.md' under Changes; explorer and tab show the status", async () => {
    await page.click(".wb-tab:has-text('Explorer')");
    await page.click(".mk-tree-file >> text=README.md");
    await page.waitForSelector(".cm-content", { timeout: 15000 });
    await page.click(".cm-content");
    await page.keyboard.press("Control+End");
    await page.keyboard.type("\nA new line.\n");
    await page.keyboard.press("Control+S");
    await page.waitForFunction(() => !document.querySelector(".mk-tab-dirty"), null, { timeout: 5000 });
    await page.waitForSelector(".mk-tree-row[title='README.md'].mk-vcs-modified", { timeout: 15000 });
    await page.waitForSelector(".wb-tab:has-text('README.md') .mk-tab-vcs[data-status='M']", { timeout: 5000 });
    await page.click(".wb-tab:has-text('Changes')");
    await page.waitForSelector(".mk-git-entry[data-status='M']", { timeout: 10000 });
    console.log("\n  entries:", (await entries()).join(" | "));
    await page.screenshot({ path: `${S}/m7-git-changes.png` });
  });
  await step("clicking the entry opens a diff with the added line", async () => {
    await page.click(".mk-git-entry[data-status='M']");
    await page.waitForSelector(".mk-git-diff-body", { timeout: 10000 });
    await page.waitForSelector(".mk-diff-add:has-text('A new line.')", { timeout: 10000 });
    await page.screenshot({ path: `${S}/m7-git-diff.png` });
  });
  await step("stage (+) → Staged; commit with a message → log grows, tree clean, git agrees", async () => {
    await page.hover(".mk-git-entry[data-status='M']");
    await page.click(".mk-git-entry[data-status='M'] .mk-git-actions button[title='Stage']");
    await page.waitForFunction(() => /Staged \(1\)/.test(document.querySelector(".mk-git").textContent), null, { timeout: 10000 });
    await page.fill("#mk-git-commit-message", "Add a line from the E2E");
    await page.click(".mk-git-commit button");
    await page.waitForFunction(() => [...document.querySelectorAll(".mk-git-subject")].some((s) => s.textContent === "Add a line from the E2E"), null, { timeout: 15000 });
    await page.waitForSelector("text=Working tree clean", { timeout: 10000 });
    const head = git("log -1 --pretty=%s");
    console.log("\n  HEAD:", head);
    if (head !== "Add a line from the E2E") throw new Error("commit missing");
    await page.waitForFunction(() => !document.querySelector(".mk-tree-row[title='README.md'].mk-vcs-modified"), null, { timeout: 10000 });
  });
  await step("a new file is untracked (?) — discard removes it", async () => {
    fs.writeFileSync(`${ROOT}/scratch.txt`, "temporary\n");
    await page.click(".mk-git-head button[title='Refresh']");
    await page.waitForSelector(".mk-git-entry[data-status='?']", { timeout: 10000 });
    await page.hover(".mk-git-entry[data-status='?']");
    await page.click(".mk-git-entry[data-status='?'] .mk-git-actions button[title^='Discard']");
    await page.waitForSelector(".mk-git-entry[data-status='?']", { state: "detached", timeout: 10000 });
    if (fs.existsSync(`${ROOT}/scratch.txt`)) throw new Error("still on disk");
  });
  await step("Graph button draws the history: 2 commits + touched files in the Graph panel", async () => {
    await page.click(".mk-git-head button[title^='Draw']");
    await page.waitForFunction(() => [...document.querySelectorAll(".wb-tab[aria-selected=true]")].some((t) => t.textContent.includes("Graph")), null, { timeout: 10000 });
    await page.waitForFunction(() => /git history/.test(document.querySelector(".mk-graph-source")?.selectedOptions[0]?.textContent || ""), null, { timeout: 10000 });
    await page.waitForFunction(() => +(document.querySelector(".mk-graph-info")?.getAttribute("data-nodes") || 0) >= 3, null, { timeout: 15000 });
    const info = await page.$eval(".mk-graph-info", (e) => e.textContent.trim());
    console.log("\n  graph:", info);
    await page.screenshot({ path: `${S}/m7-git-graph.png` });
  });
  console.log("GIT E2E: PASS");
} catch (e) {
  console.log("\nFAIL:", e.message);
  await page.screenshot({ path: `${S}/m7-git-fail.png` }).catch(() => {});
  console.log(logs.filter((l) => !/WARN/.test(l)).slice(-20).join("\n"));
  process.exitCode = 1;
} finally {
  await browser.close();
}
