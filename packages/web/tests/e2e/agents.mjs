// Milestone 15 (Prompt24): saved agents in Settings, one chosen per session; several sessions
// with turns in flight at once; the folder's saved sessions listed in the Agent panel and
// restored after a reload. Mock provider throughout.
import { firefox } from "playwright";
import fs from "node:fs";
import { rows } from "./state.mjs";
import { control, revealControl } from "./chrome-controls.mjs";
const S = process.env.M1_SHOTS ?? ".";
const PORT = process.env.PORT ?? 8080;
const ROOT = process.env.M1_ROOT;
const browser = await firefox.launch();
const ctx = await browser.newContext({ viewport: { width: 1500, height: 900 } });
const page = await ctx.newPage();
const logs = [];
const errors = [];
let currentStep = "startup";
page.on("pageerror", error => errors.push((error.stack || String(error)).slice(0, 4096)));
page.on("console", (m) => { const t = m.text(); if (!t.includes("session[")) logs.push(`[${m.type()}] ${t.slice(0, 300)}`); });
const step = async (n, f) => { currentStep=n; process.stdout.write(`- ${n} … `); await f(); console.log("ok"); };
const ask = async (text) => { await page.fill(".mk-agent-input", text); await page.click(".mk-agent-compose button"); };
const idle = () => page.waitForFunction(() => !document.querySelector(".mk-agent-compose button[disabled]"), null, { timeout: 60000 });
const sessionId = () => page.$eval(".mk-agent", (e) => e.getAttribute("data-session"));
const userSettings = () => JSON.parse(localStorage.getItem("moonkale.settings") || "{}");
const openFolder = async () => {
  await page.goto(`http://127.0.0.1:${PORT}/`, { waitUntil: "networkidle" });
  await page.waitForSelector(".wb-workspace");
  await page.click(".mk-explorer-open button[type=submit]");
  await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
  await page.waitForFunction(() => /mock/.test(document.querySelector(".mk-agent-provider")?.textContent || ""), null, { timeout: 20000 });
};
try {
  await step("open folder; the Agent panel has a session select, a profile select and New", async () => {
    await openFolder();
    await revealControl(page.locator(".mk-agent-sessions"));
    await page.waitForSelector(".mk-agent-sessions");
    await page.waitForSelector(".mk-agent-profile");
    const profiles = await page.$$eval(".mk-agent-profile option", (o) => o.map((x) => x.value));
    console.log("\n  profiles:", profiles.join(", "));
    if (profiles.join(",") !== "Default") throw new Error("only Default expected at first");
  });
  await step("Settings → Agents: Add agent, rename it, set a model; it lands in the user file under agents[]", async () => {
    await page.click(".wb-status-bar");
    await page.keyboard.press("Control+,");
    await page.waitForSelector(".mk-settings-add-agent", { timeout: 10000 });
    if (await page.$(".mk-settings:not(.mk-extensions) .mk-settings-ext")) throw new Error("Settings must not list extensions (Milestone 15)");
    await page.click(".mk-settings-add-agent");
    await page.waitForSelector(".mk-settings-agent[data-agent='Agent 1']", { timeout: 5000 });
    const card = page.locator(".mk-settings-agent[data-agent='Agent 1']");
    await card.locator("input.mk-settings-agent-name").fill("Second");
    await card.locator("input.mk-settings-agent-name").press("Tab");
    await page.waitForSelector(".mk-settings-agent[data-agent='Second']", { timeout: 5000 });
    const second = page.locator(".mk-settings-agent[data-agent='Second']");
    await second.locator(".mk-settings-agent-model").fill("second-model");
    await second.locator(".mk-settings-agent-model").press("Tab");
    await page.waitForFunction(() => { try { const a = JSON.parse(localStorage.getItem("moonkale.settings")).agents; return a?.length === 1 && a[0].name === "Second" && a[0].model === "second-model"; } catch { return false; } }, null, { timeout: 5000 });
    console.log("\n  user file agents:", JSON.stringify(await page.evaluate(userSettings).then((s) => s.agents)));
    await page.screenshot({ path: `${S}/m15-settings-agents.png` });
  });
  await step("'runs by default' on Second → agent.default in the user file; the Default card keeps its radio", async () => {
    await page.click(".mk-settings-agent[data-agent='Second'] .mk-settings-agent-runs input");
    await page.waitForFunction(() => { try { return JSON.parse(localStorage.getItem("moonkale.settings")).agent?.default === "Second"; } catch { return false; } }, null, { timeout: 5000 });
    await page.click(".mk-settings-agent[data-agent='Default'] .mk-settings-agent-runs input");
    await page.waitForFunction(() => { try { return JSON.parse(localStorage.getItem("moonkale.settings")).agent?.default === undefined; } catch { return false; } }, null, { timeout: 5000 });
  });
  let first;
  await step("the session's profile select now offers Second; picking it reconnects the session", async () => {
    await page.waitForFunction(() => document.querySelectorAll(".mk-agent-profile option").length === 2, null, { timeout: 5000 });
    first = await sessionId();
    await control(page.locator(".mk-agent-profile"), "selectOption", "Second");
    await page.keyboard.press("Escape");
    await page.waitForFunction(() => document.querySelector(".mk-agent-profile").value === "Second", null, { timeout: 5000 });
    await page.waitForFunction(() => /mock/.test(document.querySelector(".mk-agent-provider")?.textContent || ""), null, { timeout: 10000 });
  });
  let second;
  await step("two sessions at once: A waits for an approval while B answers", async () => {
    await ask(`/tool terminal.run {"command":"echo from_A"}`);
    await page.waitForSelector(".mk-agent-approval", { timeout: 20000 });
    if (!(await page.$(".mk-agent-compose button[disabled]"))) throw new Error("A should be busy");
    await control(page.locator(".mk-agent-new"), "click");
    await page.keyboard.press("Escape");
    await page.waitForFunction((a) => document.querySelector(".mk-agent")?.getAttribute("data-session") !== a, first, { timeout: 5000 });
    second = await sessionId();
    await page.waitForFunction(() => /mock/.test(document.querySelector(".mk-agent-provider")?.textContent || ""), null, { timeout: 10000 });
    if (await page.$(".mk-agent-approval")) throw new Error("B shows A's approval");
    await ask("hello from B");
    await page.waitForFunction(() => /mock: hello from B/.test(document.querySelector(".mk-agent-assistant")?.textContent || ""), null, { timeout: 20000 });
    const options = await page.$$eval(".mk-agent-sessions option", (o) => o.map((x) => x.textContent.trim()));
    console.log("\n  sessions:", options.join(" | "));
    if (!options.some((t) => t.startsWith("●"))) throw new Error("A should be marked running");
    await page.screenshot({ path: `${S}/m15-two-sessions.png` });
  });
  await step("back to A: its approval is still pending; Allow → the command ran; both sessions are stored in the folder host's store (no files)", async () => {
    await control(page.locator(".mk-agent-sessions"), "selectOption", first);
    await page.keyboard.press("Escape");
    await page.waitForSelector(".mk-agent-approval", { timeout: 5000 });
    await page.click(".mk-agent-approval button:has-text('Allow')");
    await idle();
    const text = await page.$$eval(".mk-agent-assistant", (e) => e.map((x) => x.textContent).join("\n"));
    if (!/from_A/.test(text)) throw new Error("no output from A: " + text.slice(0, 200));
    const local = () => rows("agent_sessions").filter((r) => r.parts[1] === "local" && r.parts[2] === "$session" && r.parts[3] === "default");
    let ids = [];
    for (let i = 0; i < 30; i++) { ids = local().map((r) => r.parts[4]); if (ids.length >= 2) break; await new Promise((r) => setTimeout(r, 300)); }
    console.log("\n  stored:", ids.join(", "));
    if (!ids.includes(first) || !ids.includes(second)) throw new Error("sessions not stored");
    if (fs.existsSync(`${ROOT}/.moonkale/agent-sessions/local`)) throw new Error("session files still written");
    const saved = local().find((r) => r.parts[4] === first).data;
    if (saved.profile !== "Second" || !saved.messages?.length) throw new Error("bad snapshot " + JSON.stringify(saved).slice(0, 200));
  });
  await step("reload: the saved sessions are listed under 'saved in this folder'; picking one restores its transcript and profile", async () => {
    await openFolder();
    await page.waitForFunction(() => document.querySelectorAll(".mk-agent-sessions optgroup option").length === 2, null, { timeout: 15000 });
    await control(page.locator(".mk-agent-sessions"), "selectOption", `saved:${first}`);
    await page.keyboard.press("Escape");
    await page.waitForFunction((a) => document.querySelector(".mk-agent")?.getAttribute("data-session") === a, first, { timeout: 10000 });
    await page.waitForFunction(() => /from_A/.test(document.querySelector(".mk-agent-log")?.textContent || ""), null, { timeout: 5000 });
    await page.waitForFunction(() => document.querySelector(".mk-agent-profile").value === "Second", null, { timeout: 5000 });
    await page.waitForFunction(() => /mock/.test(document.querySelector(".mk-agent-provider")?.textContent || ""), null, { timeout: 10000 });
    await ask("still here?");
    await page.waitForFunction(() => /mock: still here\?/.test([...document.querySelectorAll(".mk-agent-assistant")].at(-1)?.textContent || ""), null, { timeout: 20000 });
    await page.screenshot({ path: `${S}/m15-restored.png` });
  });
  console.log("\nAGENTS E2E: PASS");
} catch (e) { console.log("\nFAIL:", currentStep, e.stack || e.message); console.log(errors.join("\n")); fs.writeFileSync(`${S}/m15-errors.txt`, errors.join("\n")); console.log(logs.slice(-12).join("\n")); await page.screenshot({ path: `${S}/m15-fail.png` }); process.exitCode = 1; } finally { await browser.close(); }
