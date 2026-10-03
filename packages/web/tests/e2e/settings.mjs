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
// ADR-0014: the layout lives in the client's state store — in the browser,
// localStorage items `moonkale.state/layout/<hex key>` holding a hex-encoded
// `{"v":1,"data":{layout, open_documents, active_document}}`.
const layoutRecord = (page) => page.evaluate(() => {
  for (let i = 0; i < localStorage.length; i++) {
    const k = localStorage.key(i);
    if (!k.startsWith("moonkale.state/layout/")) continue;
    const bytes = new Uint8Array((localStorage.getItem(k).match(/../g) || []).map((x) => parseInt(x, 16)));
    return JSON.parse(new TextDecoder().decode(bytes)).data;
  }
  return {};
});

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
  await step("the state store records the open document and the layout; the folder file does not", async () => {
    let f = {}; for (let i = 0; i < 30; i++) { f = await layoutRecord(page); if (f.open_documents?.includes("README.md") && f.layout) break; await new Promise((r) => setTimeout(r, 300)); }
    console.log("\n  layout record:", JSON.stringify({ open: f.open_documents, active: f.active_document, layout: !!f.layout }));
    if (!f.open_documents?.includes("README.md") || !f.layout) throw new Error("not recorded");
    let disk = {}; try { disk = wsFile(); } catch {}
    if (disk.layout || disk.open_documents) throw new Error("layout written into the folder: " + JSON.stringify(disk));
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
  await step("Milestone 18 phase 4.5: a folder's 'allow writes' is saved but ignored — the agent still asks, and Settings says so", async () => {
    // The switch is not offered for the folder scope …
    await page.click("#mk-rail-extensions");
    await page.waitForSelector(".mk-extensions", { timeout: 10000 });
    await page.selectOption(".mk-extensions .mk-settings-target select", "workspace");
    const row = page.locator(".mk-extensions .mk-settings-ext:has(.mk-settings-ext-name:text-is('Agent'))").first();
    if (!(await row.locator(".mk-settings-ext-settings label:has-text('Allow mutating') input").isDisabled())) throw new Error("the folder scope offers allow_writes");
    // … and a folder file that sets it anyway (a cloned repository) is ignored.
    const f = wsFile(); f.policy = { ...(f.policy || {}), allow_writes: true };
    fs.writeFileSync(`${ROOT}/.moonkale/settings.json`, JSON.stringify(f));
    await page.click("#mk-rail-explorer");
    await openFolder();
    const id = "folder:" + ROOT;
    await page.fill(".mk-agent-input", `/tool source.text_query {"source":"${id}","dialect":"sql","text":"DELETE FROM t"}`);
    await page.click(".mk-agent-compose button");
    await page.waitForSelector(".mk-agent-approval", { timeout: 20000 });
    await page.click(".mk-agent-approval button:has-text('Deny')");
    await page.waitForFunction(() => !document.querySelector(".mk-agent-approval"), null, { timeout: 10000 });
    await page.keyboard.press("Control+,");
    await page.waitForFunction(() => /policy\.allow_writes/.test(document.querySelector(".mk-settings-ignored")?.textContent || ""), null, { timeout: 10000 });
  });
  await step("the same switch in the user scope → the agent runs a write without asking", async () => {
    await page.click("#mk-rail-extensions");
    await page.waitForSelector(".mk-extensions", { timeout: 10000 });
    await page.selectOption(".mk-extensions .mk-settings-target select", "user");
    const row = page.locator(".mk-extensions .mk-settings-ext:has(.mk-settings-ext-name:text-is('Agent'))").first();
    await row.locator(".mk-settings-ext-settings label:has-text('Allow mutating') input").click();
    await page.waitForFunction(() => { try { return JSON.parse(localStorage.getItem("moonkale.settings")).policy?.allow_writes === true; } catch { return false; } }, null, { timeout: 10000 });
    const before = await page.$$eval(".mk-agent-tool", (t) => t.length);
    const id = "folder:" + ROOT;
    await page.fill(".mk-agent-input", `/tool source.text_query {"source":"${id}","dialect":"sql","text":"DELETE FROM t"}`);
    await page.click(".mk-agent-compose button");
    await page.waitForFunction((n) => { const t = [...document.querySelectorAll(".mk-agent-tool")]; return t.length > n && /failed|ok/.test(t[t.length - 1].textContent) && [...t[t.length - 1].querySelectorAll(".mk-agent-badge")].some((b) => b.textContent === "Allow"); }, before, { timeout: 20000 });
    if (await page.$(".mk-agent-approval")) throw new Error("approval box shown despite the user's allow_writes");
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
