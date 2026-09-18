// Milestone 5: settings persist — user scope (localStorage), workspace scope (.moonkale/settings.json),
// open documents + layout restored on reopen, recent folders, policy override reaches the agent.
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
const openFolder = async () => {
  await page.click(".mk-explorer-open button[type=submit]");
  await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
};
const wsFile = () => JSON.parse(fs.readFileSync(`${ROOT}/.moonkale/settings.json`, "utf8"));
try {
  await step("open folder, open README.md, activate the Links tab", async () => {
    await page.goto(`http://127.0.0.1:${PORT}/`, { waitUntil: "networkidle" });
    await page.waitForSelector(".wb-workspace");
    await openFolder();
    await page.click(".mk-tree-file >> text=README.md");
    await page.waitForSelector(".cm-content", { timeout: 15000 });
    await page.click(".wb-tab:has-text('Links')");
    await page.waitForFunction(() => fetch("/").then(() => true), null, { timeout: 5000 });
  });
  await step("workspace file records the open document and the layout", async () => {
    await page.waitForFunction(() => true, null, { timeout: 1000 });
    let f; for (let i = 0; i < 30; i++) { try { f = wsFile(); if (f.open_documents?.includes("README.md") && f.layout) break; } catch {} await new Promise((r) => setTimeout(r, 300)); }
    console.log("\n  workspace:", JSON.stringify({ open: f.open_documents, active: f.active_document, layout: !!f.layout }));
    if (!f.open_documents?.includes("README.md") || !f.layout) throw new Error("not recorded");
  });
  await step("Ctrl+, opens Settings; a user-scope change lands in localStorage", async () => {
    await page.click(".wb-status-bar");
    await page.keyboard.press("Control+,");
    await page.waitForSelector(".mk-settings", { timeout: 10000 });
    await page.fill(".mk-settings-form label:has-text('Model') input", "test-model");
    await page.press(".mk-settings-form label:has-text('Model') input", "Tab");
    await page.waitForFunction(() => /test-model/.test(localStorage.getItem("moonkale.settings") || ""), null, { timeout: 10000 });
    await page.screenshot({ path: `${S}/m5-settings.png` });
  });
  await step("workspace-scope 'allow writes' → the agent runs a write without asking", async () => {
    await page.click(".mk-settings-toolbar button:has-text('workspace')");
    await page.click(".mk-settings-form label:has-text('Allow mutating') input");
    let f; for (let i = 0; i < 30; i++) { try { f = wsFile(); if (f.policy?.allow_writes) break; } catch {} await new Promise((r) => setTimeout(r, 300)); }
    if (!f.policy?.allow_writes) throw new Error("allow_writes not saved");
    const src = await page.$eval(".mk-agent", () => "");
    void src;
    const id = "folder:" + ROOT;
    await page.fill(".mk-agent-input", `/tool source.text_query {"source":"${id}","dialect":"sql","text":"DELETE FROM t"}`);
    await page.click(".mk-agent-compose button");
    await page.waitForFunction(() => [...document.querySelectorAll(".mk-agent-tool .mk-agent-badge")].some((b) => b.textContent === "Allow") && [...document.querySelectorAll(".mk-agent-tool")].some((t) => /failed|ok/.test(t.textContent)), null, { timeout: 20000 });
    const approval = await page.$(".mk-agent-approval");
    if (approval) throw new Error("approval box shown despite allow_writes");
  });
  await step("reload + reopen: README.md is open again, Links tab active, recent folder listed", async () => {
    await page.reload({ waitUntil: "networkidle" });
    await page.waitForSelector(".wb-workspace");
    await openFolder();
    await page.waitForSelector(".cm-content", { timeout: 20000 });
    await page.waitForFunction(() => /README\.md/.test(document.querySelector(".mk-titlebar-title")?.textContent || ""), null, { timeout: 15000 });
    await page.waitForFunction(() => [...document.querySelectorAll(".wb-tab[aria-selected=true]")].some((t) => t.textContent.includes("Links")), null, { timeout: 15000 });
    await page.click(".mk-menu-button:has-text('File')");
    await page.waitForSelector(".mk-menu-recent", { timeout: 5000 });
    console.log("\n  recent:", await page.$eval(".mk-menu-recent", (e) => e.getAttribute("title")));
    await page.keyboard.press("Escape");
  });
  console.log("\nSETTINGS E2E: PASS");
} catch (e) { console.log("\nFAIL:", e.message); console.log(logs.slice(-10).join("\n")); await page.screenshot({ path: `${S}/m5-fail.png` }); process.exitCode = 1; } finally { await browser.close(); }
