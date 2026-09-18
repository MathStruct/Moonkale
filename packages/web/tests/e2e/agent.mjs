// Milestone 4: agent panel with the mock provider → relay, tool loop, policy, transcript.
import { firefox } from "playwright";
const S = process.env.M1_SHOTS ?? ".";
const browser = await firefox.launch();
const page = await browser.newPage({ viewport: { width: 1500, height: 900 } });
const logs = [];
page.on("console", (m) => { const t = m.text(); if (!t.includes("session[")) logs.push(`[${m.type()}] ${t.slice(0, 300)}`); });
const step = async (n, f) => { process.stdout.write(`- ${n} … `); await f(); console.log("ok"); };
const ask = async (text) => { await page.fill(".mk-agent-input", text); await page.click(".mk-agent-compose button"); };
try {
  await step("open folder; Agent panel connects to the server's provider (mock)", async () => {
    await page.goto(`http://127.0.0.1:${process.env.PORT ?? 8080}/`, { waitUntil: "networkidle" });
    await page.waitForSelector(".wb-workspace");
    await page.click(".mk-explorer-open button[type=submit]");
    await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
    await page.waitForSelector(".mk-agent");
    await page.waitForFunction(() => /mock/.test(document.querySelector(".mk-agent-provider")?.textContent || ""), null, { timeout: 20000 });
    console.log("\n  provider:", await page.$eval(".mk-agent-provider", (e) => e.textContent));
  });
  await step("a plain message streams back an assistant reply", async () => {
    await ask("hello there");
    await page.waitForSelector(".mk-agent-assistant", { timeout: 15000 });
    await page.waitForFunction(() => /mock: hello there/.test(document.querySelector(".mk-agent-assistant")?.textContent || ""), null, { timeout: 15000 });
  });
  await step("a read-only tool call runs (workspace.list_sources) and the model sees the result", async () => {
    await ask("/tool workspace.list_sources {}");
    await page.waitForFunction(() => document.querySelectorAll(".mk-agent-tool").length === 1 && /ok/.test(document.querySelector(".mk-agent-tool").textContent), null, { timeout: 15000 });
    await page.waitForFunction(() => [...document.querySelectorAll(".mk-agent-assistant")].some((e) => /returned: id\tname/.test(e.textContent)), null, { timeout: 15000 });
    const badges = await page.$$eval(".mk-agent-tool .mk-agent-badge", (b) => b.map((x) => x.textContent));
    console.log("\n  badges:", badges.join(", "));
    if (badges.join() !== "ReadOnly,Allow") throw new Error("policy badges");
  });
  await step("a write query asks for approval; Deny is reported to the model", async () => {
    const sid = await page.$eval(".mk-agent-assistant:last-of-type", () => "");
    const sources = await page.$$eval(".mk-agent-assistant", (e) => e.map((x) => x.textContent).join("\n"));
    const m = sources.match(/(folder:\S+)\tm2root/);
    if (!m) throw new Error("no folder source id in " + sources.slice(0, 300));
    await ask(`/tool source.text_query {"source":"${m[1]}","dialect":"sql","text":"DELETE FROM t"}`);
    await page.waitForSelector(".mk-agent-approval", { timeout: 15000 });
    console.log("\n  approval:", (await page.$eval(".mk-agent-approval-title", (e) => e.textContent)).trim());
    await page.click(".mk-agent-approval button:has-text('Deny')");
    await page.waitForFunction(() => [...document.querySelectorAll(".mk-agent-tool")].some((t) => /declined/.test(t.textContent)), null, { timeout: 15000 });
    await page.waitForFunction(() => [...document.querySelectorAll(".mk-agent-assistant")].some((e) => /declined this action/.test(e.textContent)), null, { timeout: 15000 });
    void sid;
  });
  await step("Activity shows the audit log", async () => {
    await page.click(".mk-agent-toolbar button:has-text('Activity')");
    await page.waitForFunction(() => document.querySelectorAll(".mk-agent-audit").length === 2, null, { timeout: 5000 });
    await page.screenshot({ path: `${S}/m4-agent.png` });
  });
  await step("Save writes .moonkale/chats/*.md into the folder and opens it", async () => {
    await page.click(".mk-agent-toolbar button:has-text('Save')");
    await page.waitForFunction(() => /\.moonkale\/chats\/.*\.md/.test(document.querySelector(".mk-titlebar-title")?.textContent || ""), null, { timeout: 15000 });
    console.log("\n  opened:", await page.$eval(".mk-titlebar-title", (e) => e.textContent));
    await page.waitForFunction(() => /# hello there/.test(document.querySelector(".cm-content")?.textContent || ""), null, { timeout: 15000 });
  });
  console.log("\nAGENT E2E: PASS");
} catch (e) { console.log("\nFAIL:", e.message); console.log(logs.slice(-10).join("\n")); await page.screenshot({ path: `${S}/m4-fail.png` }); process.exitCode = 1; } finally { await browser.close(); }
