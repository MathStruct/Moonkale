// Milestone 12: agent sessions on the server. With agent.on_server the Agent panel sends the turn to the
// server; the transcript is polled; a tool call shows; closing the page does not end the session and a
// new page finds it in the session list with its transcript; a mutating call waits for an approval that
// any client can answer.
import { firefox } from "playwright";
import fs from "node:fs";
const S = process.env.M1_SHOTS ?? ".";
const PORT = process.env.PORT ?? 8080;
const ROOT = process.env.M1_ROOT;
const browser = await firefox.launch();
const init = (ctx) => ctx.addInitScript(() => { try { localStorage.setItem("moonkale.settings", JSON.stringify({ agent: { on_server: true }, editor: { markdown_rich: false } })); } catch {} });
const step = async (n, f) => { process.stdout.write(`- ${n} … `); await f(); console.log("ok"); };
const open = async (page) => {
  await page.goto(`http://127.0.0.1:${PORT}/`, { waitUntil: "networkidle" });
  await page.waitForSelector(".wb-workspace");
  await page.click(".mk-explorer-open button[type=submit]");
  await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
  await page.waitForSelector(".mk-agent-server", { timeout: 15000 });
};
const ask = async (page, text) => { await page.fill(".mk-agent-input", text); await page.click(".mk-agent-compose button"); };
let ctx = await browser.newContext({ viewport: { width: 1400, height: 900 } });
await init(ctx);
let page = await ctx.newPage();
try {
  await step("the Agent panel is in server mode; a turn runs on the server and its reply is polled in", async () => {
    await open(page);
    await ask(page, "hello there");
    await page.waitForFunction(() => [...document.querySelectorAll(".mk-agent-assistant")].some((e) => /mock: hello there/.test(e.textContent)), null, { timeout: 30000 });
    await page.waitForFunction(() => !document.querySelector(".mk-agent-running"), null, { timeout: 15000 });
    console.log("\n  provider:", await page.$eval(".mk-agent-provider", (e) => e.textContent));
  });
  await step("a read-only tool call runs on the server (workspace.list_sources)", async () => {
    await ask(page, "/tool workspace.list_sources {}");
    await page.waitForFunction(() => [...document.querySelectorAll(".mk-agent-tool")].some((t) => /workspace.list_sources/.test(t.textContent) && /ok/.test(t.textContent)), null, { timeout: 30000 });
    await page.waitForFunction(() => [...document.querySelectorAll(".mk-agent-assistant")].some((e) => /returned: id\tname/.test(e.textContent)), null, { timeout: 30000 });
    const dir = fs.readdirSync(`${ROOT}/.moonkale/agent-sessions`);
    console.log("\n  on disk:", dir.join(", "));
    if (dir.length !== 1) throw new Error("expected one session log");
  });
  await step("the page closes; a new one lists the session and shows its transcript", async () => {
    await ctx.close();
    ctx = await browser.newContext({ viewport: { width: 1400, height: 900 } });
    await init(ctx);
    page = await ctx.newPage();
    await open(page);
    await page.waitForFunction(() => document.querySelectorAll(".mk-agent-sessions option").length === 2, null, { timeout: 15000 });
    const label = await page.$$eval(".mk-agent-sessions option", (o) => o.map((x) => x.textContent.trim()).join(" | "));
    console.log("\n  sessions:", label);
    await page.selectOption(".mk-agent-sessions", { index: 1 });
    await page.waitForFunction(() => [...document.querySelectorAll(".mk-agent-assistant")].some((e) => /mock: hello there/.test(e.textContent)), null, { timeout: 15000 });
    await page.screenshot({ path: `${S}/m12-agent-server.png` });
  });
  await step("a mutating call waits for an approval; Deny is reported to the model", async () => {
    const sources = await page.$$eval(".mk-agent-assistant", (e) => e.map((x) => x.textContent).join("\n"));
    const m = sources.match(/(folder:\S+)\tm2root/);
    if (!m) throw new Error("no folder source id in " + sources.slice(0, 300));
    await ask(page, `/tool source.text_query {"source":"${m[1]}","dialect":"sql","text":"DELETE FROM t"}`);
    await page.waitForSelector(".mk-agent-approval", { timeout: 30000 });
    console.log("\n  approval:", (await page.$eval(".mk-agent-approval-title", (e) => e.textContent)).trim().slice(0, 80));
    await page.click(".mk-agent-approval button:has-text('Deny')");
    await page.waitForFunction(() => [...document.querySelectorAll(".mk-agent-tool")].some((t) => /declined/.test(t.textContent)), null, { timeout: 30000 });
    await page.waitForFunction(() => [...document.querySelectorAll(".mk-agent-assistant")].some((e) => /declined this action/.test(e.textContent)), null, { timeout: 30000 });
  });
  console.log("\nAGENT-SERVER E2E: PASS");
} catch (e) { console.log("\nFAIL:", e.message); await page.screenshot({ path: `${S}/m12-agent-server-fail.png` }).catch(() => {}); process.exitCode = 1; } finally { await browser.close(); }
