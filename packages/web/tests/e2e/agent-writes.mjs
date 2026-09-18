// Milestone 5: agent write tools through the gate — editor.replace (diff card → unsaved edit),
// file.create, terminal.run — with the mock provider.
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
const ask = async (text) => { await page.fill(".mk-agent-input", text); await page.click(".mk-agent-compose button"); };
const idle = () => page.waitForFunction(() => !document.querySelector(".mk-agent-compose button[disabled]"), null, { timeout: 60000 });
const lastAssistant = () => page.$$eval(".mk-agent-assistant", (e) => e.map((x) => x.textContent).at(-1) || "");
try {
  await step("open folder; agent connected", async () => {
    await page.goto(`http://127.0.0.1:${PORT}/`, { waitUntil: "networkidle" });
    await page.waitForSelector(".wb-workspace");
    await page.click(".mk-explorer-open button[type=submit]");
    await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
    await page.waitForFunction(() => /mock/.test(document.querySelector(".mk-agent-provider")?.textContent || ""), null, { timeout: 20000 });
  });
  const folder = "folder:" + ROOT;
  let readme;
  await step("graph.query lists README.md's node id", async () => {
    await ask(`/tool graph.query {"source":"${folder}","mode":"children"}`);
    await idle();
    const m = (await lastAssistant()).match(/([0-9a-f-]{36})\tFile\tREADME\.md/);
    if (!m) throw new Error("no README id in " + (await lastAssistant()).slice(0, 200));
    readme = m[1];
  });
  await step("editor.replace shows a diff card; Allow → unsaved edit in the editor", async () => {
    await ask(`/tool editor.replace {"source":"${folder}","node":"${readme}","old":"# Sample","new":"# hello from the agent"}`);
    await page.waitForSelector(".mk-agent-approval .mk-agent-diff-new", { timeout: 20000 });
    console.log("\n  diff:", await page.$eval(".mk-agent-diff-old", (e) => e.textContent), "→", await page.$eval(".mk-agent-diff-new", (e) => e.textContent));
    await page.screenshot({ path: `${S}/m5-agent-diff.png` });
    await page.click(".mk-agent-approval button:has-text('Allow')");
    await idle();
    await page.waitForFunction(() => /hello from the agent/.test(document.querySelector(".mk-md .cm-content")?.textContent || ""), null, { timeout: 15000 });
    await page.waitForSelector(".mk-editor-dirty", { timeout: 5000 });
    if (fs.readFileSync(`${ROOT}/README.md`, "utf8").includes("hello from the agent")) throw new Error("file written without the user saving");
  });
  await step("file.create → Allow → new file exists and opens", async () => {
    await ask(`/tool file.create {"source":"${folder}","path":"notes/agent-made.md","text":"# Made by the agent\\n"}`);
    await page.waitForSelector(".mk-agent-approval", { timeout: 20000 });
    await page.click(".mk-agent-approval button:has-text('Allow')");
    await idle();
    if (!fs.existsSync(`${ROOT}/notes/agent-made.md`)) throw new Error("file not created");
    await page.waitForFunction(() => /agent-made\.md/.test(document.querySelector(".mk-titlebar-title")?.textContent || ""), null, { timeout: 15000 });
  });
  await step("terminal.run → Allow → the command's output comes back", async () => {
    await ask(`/tool terminal.run {"command":"echo out_$((20+3))"}`);
    await page.waitForSelector(".mk-agent-approval .mk-agent-diff-cmd", { timeout: 20000 });
    await page.click(".mk-agent-approval button:has-text('Allow')");
    await page.waitForFunction(() => /out_23/.test([...document.querySelectorAll(".mk-agent-assistant")].map((e) => e.textContent).join(" ")), null, { timeout: 60000 });
  });
  await step("a destructive command asks even with allow_writes (Deny)", async () => {
    await ask(`/tool terminal.run {"command":"rm -rf target"}`);
    await page.waitForSelector(".mk-agent-approval", { timeout: 20000 });
    const badge = await page.$$eval(".mk-agent-tool .mk-agent-badge", (b) => b.map((x) => x.textContent).at(-2));
    console.log("\n  class:", badge);
    await page.click(".mk-agent-approval button:has-text('Deny')");
    await idle();
  });
  console.log("\nAGENT WRITES E2E: PASS");
} catch (e) { console.log("\nFAIL:", e.message); console.log(logs.slice(-10).join("\n")); await page.screenshot({ path: `${S}/m5-fail.png` }); process.exitCode = 1; } finally { await browser.close(); }
