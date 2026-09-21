// Milestone 12: Claude Code as a provider. The server runs the mock CLI (MOONKALE_CLAUDE_BIN, set by
// serve.sh); the user picks "Claude Code" in the settings; a turn shows the CLI's tool activity as
// ▸ lines and its reply; a second turn resumes the session; the turn ran in the open folder.
import { firefox } from "playwright";
const S = process.env.M1_SHOTS ?? ".";
const PORT = process.env.PORT ?? 8080;
const browser = await firefox.launch();
const ctx = await browser.newContext({ viewport: { width: 1400, height: 900 } });
await ctx.addInitScript(() => { try { localStorage.setItem("moonkale.settings", JSON.stringify({ llm: { provider: "claude-code", options: { permission_mode: "acceptEdits" } }, editor: { markdown_rich: false } })); } catch {} });
const page = await ctx.newPage();
const logs = [];
page.on("console", (m) => { const t = m.text(); if (!t.includes("session[")) logs.push(`[${m.type()}] ${t.slice(0, 300)}`); });
const step = async (n, f) => { process.stdout.write(`- ${n} … `); await f(); console.log("ok"); };
const ask = async (text) => { await page.fill(".mk-agent-input", text); await page.click(".mk-agent-compose button"); };
try {
  await step("the provider is claude-code (subscription, no key)", async () => {
    await page.goto(`http://127.0.0.1:${PORT}/`, { waitUntil: "networkidle" });
    await page.waitForSelector(".wb-workspace");
    await page.click(".mk-explorer-open button[type=submit]");
    await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
    await page.waitForSelector(".mk-agent");
    await page.waitForFunction(() => /claude-code/.test(document.querySelector(".mk-agent-provider")?.textContent || ""), null, { timeout: 20000 });
    console.log("\n  provider:", await page.$eval(".mk-agent-provider", (e) => e.textContent));
  });
  await step("a turn: the CLI's Read shows as a ▸ line, the reply arrives, it ran in the folder with the chosen permission mode", async () => {
    await ask("hello claude");
    await page.waitForFunction(() => /mock claude: hello claude/.test(document.querySelector(".mk-agent-assistant")?.textContent || ""), null, { timeout: 30000 });
    const text = await page.$eval(".mk-agent-assistant", (e) => e.textContent);
    console.log("\n  reply:", JSON.stringify(text.slice(0, 160)));
    if (!/▸ Read README\.md/.test(text)) throw new Error("tool line missing");
    if (!/cwd=m2root/.test(text)) throw new Error("did not run in the open folder");
    if (!/mode=acceptEdits/.test(text)) throw new Error("permission mode not passed");
    if (!/resumed=no/.test(text)) throw new Error("first turn should start a session");
    await page.screenshot({ path: `${S}/m12-claude-code.png` });
  });
  await step("a second turn resumes the same session", async () => {
    await ask("and again");
    await page.waitForFunction(() => [...document.querySelectorAll(".mk-agent-assistant")].some((e) => /mock claude: and again/.test(e.textContent)), null, { timeout: 30000 });
    const all = await page.$$eval(".mk-agent-assistant", (l) => l.map((e) => e.textContent));
    if (!all.some((t) => /and again.*resumed=yes/.test(t))) throw new Error("second turn did not resume: " + all.join(" | "));
  });
  console.log("\nCLAUDE-CODE E2E: PASS");
} catch (e) { console.log("\nFAIL:", e.message); console.log(logs.slice(-8).join("\n")); await page.screenshot({ path: `${S}/m12-claude-code-fail.png` }); process.exitCode = 1; } finally { await browser.close(); }
