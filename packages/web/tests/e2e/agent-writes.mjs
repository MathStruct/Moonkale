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
const ask = async (text) => { await page.locator('.wb-tab').filter({hasText:/^Agent$/}).click(); await page.fill(".mk-agent-input", text); await page.click(".mk-agent-compose button"); };
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
  await step("editor.replace queues a versioned review before writing", async () => {
    await ask(`/tool graph.fetch ${JSON.stringify({source:folder,node:readme})}`);await idle();
    const expected=(await lastAssistant()).match(/\[version (\d+)\]/)?.[1];if(!expected)throw Error('missing version');
    await ask(`/tool editor.replace ${JSON.stringify({source:folder,node:readme,expected,old:'# Sample',new:'# hello from the agent'})}`);await idle();
    await page.locator('.mk-proposals-open').first().click();
    await page.locator('.mk-proposal .mk-agent-diff-new').waitFor();
    if (fs.readFileSync(`${ROOT}/README.md`, "utf8").includes("hello from the agent")) throw new Error("file written before review");
    await page.screenshot({ path: `${S}/m5-agent-diff.png` });
    await page.locator('.mk-proposal-approve').click();
    await page.locator('.mk-proposal-report').waitFor();
    if (!fs.readFileSync(`${ROOT}/README.md`, "utf8").includes("hello from the agent")) throw new Error("reviewed file not written");
  });
  await step("file.create → Allow → new file exists and opens", async () => {
    await page.waitForTimeout(2100);
    await ask(`/tool file.create {"source":"${folder}","path":"notes/agent-made.md","text":"# Made by the agent\\n"}`);
    await page.locator('.mk-proposals-open').first().click();
    await page.waitForSelector(".mk-agent-approval", { timeout: 20000 });
    await page.click(".mk-agent-approval button:has-text('Allow')");
    await idle();
    if (!fs.existsSync(`${ROOT}/notes/agent-made.md`)) throw new Error("file not created");
    await page.locator(".mk-agent-display-show").last().click();
    await page.waitForFunction(() => [...document.querySelectorAll(".wb-tab")].some(el=>el.textContent.includes("agent-made.md") && el.querySelector(".mk-link-marker")), null, { timeout: 15000 });
  });
  await step("terminal.run → Allow → the command's output comes back", async () => {
    await ask(`/tool terminal.run {"command":"echo out_$((20+3))"}`);
    await page.locator('.mk-proposals-open').first().click();
    await page.waitForSelector(".mk-agent-approval .mk-agent-diff-cmd", { timeout: 20000 });
    await page.click(".mk-agent-approval button:has-text('Allow')");
    await page.waitForFunction(() => /out_23/.test([...document.querySelectorAll(".mk-agent-assistant")].map((e) => e.textContent).join(" ")), null, { timeout: 60000 });
  });
  await step("a destructive command asks even with allow_writes (Deny)", async () => {
    await ask(`/tool terminal.run {"command":"rm -rf target"}`);
    await page.locator('.mk-proposals-open').first().click();
    await page.waitForSelector(".mk-agent-approval", { timeout: 20000 });
    const badge = await page.$$eval(".mk-agent-tool .mk-agent-badge", (b) => b.map((x) => x.textContent).at(-2));
    console.log("\n  class:", badge);
    await page.click(".mk-agent-approval button:has-text('Deny')");
    await idle();
  });
  console.log("\nAGENT WRITES E2E: PASS");
} catch (e) { console.log("\nFAIL:", e.message); console.log(logs.slice(-10).join("\n")); await page.screenshot({ path: `${S}/m5-fail.png` }); process.exitCode = 1; } finally { await browser.close(); }
