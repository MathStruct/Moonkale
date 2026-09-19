// Milestone 7: command palette (Ctrl+Shift+P), quick open (Ctrl+P with :line), an extension-contributed
// command (agent.focus), menus showing effective keybindings, and rebinding through Settings.
import { firefox } from "playwright";
const S = process.env.M1_SHOTS ?? ".";
const PORT = process.env.PORT ?? 8080;
const browser = await firefox.launch();
const ctx = await browser.newContext({ viewport: { width: 1500, height: 900 } });
const page = await ctx.newPage();
const logs = [];
page.on("console", (m) => { const t = m.text(); if (!t.includes("session[")) logs.push(`[${m.type()}] ${t.slice(0, 300)}`); });
const step = async (n, f) => { process.stdout.write(`- ${n} … `); await f(); console.log("ok"); };
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const items = () => page.$$eval(".mk-palette-item", (l) => l.map((x) => x.dataset.key));
try {
  await step("open folder; Ctrl+Shift+P opens the palette with every command", async () => {
    await page.goto(`http://127.0.0.1:${PORT}/`, { waitUntil: "networkidle" });
    await page.waitForSelector(".wb-workspace");
    await page.click(".mk-explorer-open button[type=submit]");
    await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
    await page.click(".wb-status-bar");
    await page.keyboard.press("Control+Shift+P");
    await page.waitForSelector(".mk-palette-input", { timeout: 10000 });
    const all = await items();
    console.log("\n  commands:", all.length, all.slice(0, 5).join(", "), "…");
    if (!all.includes("file.save") || !all.includes("agent.focus")) throw new Error("missing commands");
    if (await page.evaluate(() => document.activeElement?.id) !== "mk-palette-input") throw new Error("input not focused");
    await page.screenshot({ path: `${S}/m7-palette.png` });
  });
  await step("fuzzy 'agask' ranks the agent command first; Enter runs it → agent input focused", async () => {
    await page.type(".mk-palette-input", "agask");
    await sleep(150);
    const first = (await items())[0];
    if (first !== "agent.focus") throw new Error(`first is ${first}`);
    await page.keyboard.press("Enter");
    await page.waitForFunction(() => document.activeElement?.id === "mk-agent-input", null, { timeout: 5000 });
    if (await page.$(".mk-palette")) throw new Error("palette still open");
  });
  await step("Ctrl+P quick open: 'beta:1' opens notes/Beta.md at line 1", async () => {
    await page.keyboard.press("Control+P");
    await page.waitForSelector(".mk-palette-input", { timeout: 10000 });
    await page.waitForFunction(() => document.querySelectorAll(".mk-palette-item").length > 3, null, { timeout: 10000 });
    await page.type(".mk-palette-input", "beta:1");
    await sleep(150);
    const first = (await items())[0];
    console.log("\n  first:", first);
    if (first !== "notes/Beta.md") throw new Error(`first is ${first}`);
    await page.keyboard.press("Enter");
    await page.waitForSelector(".wb-tab:has-text('Beta.md')", { timeout: 10000 });
    await page.waitForSelector(".cm-content", { timeout: 10000 });
  });
  await step("Escape and backdrop close it; menus show the effective shortcut", async () => {
    await page.keyboard.press("Control+Shift+P");
    await page.waitForSelector(".mk-palette-input");
    await page.keyboard.press("Escape");
    await page.waitForSelector(".mk-palette", { state: "detached", timeout: 3000 });
    await page.click(".mk-menu-button:has-text('View')");
    const entry = await page.$eval(".mk-menu-item:has-text('Command Palette')", (b) => b.textContent);
    console.log("\n  menu:", entry);
    if (!entry.includes("Ctrl+Shift+P")) throw new Error("shortcut missing in menu");
    await page.keyboard.press("Escape");
    await page.click(".mk-menu-backdrop").catch(() => {});
  });
  await step("Settings → Keybindings: rebind the palette to Ctrl+K; Ctrl+K opens it, Ctrl+Shift+P no longer does", async () => {
    await page.click(".wb-status-bar");
    await page.keyboard.press("Control+,");
    await page.waitForSelector(".mk-settings-form", { timeout: 10000 });
    const input = page.locator(".mk-settings-key input[data-command='view.palette']");
    await input.scrollIntoViewIfNeeded();
    await input.fill("Ctrl+K");
    await input.press("Tab");
    await page.waitForFunction(() => /view\.palette/.test(localStorage.getItem("moonkale.settings") || ""), null, { timeout: 10000 });
    await page.click(".wb-status-bar");
    await page.keyboard.press("Control+K");
    await page.waitForSelector(".mk-palette-input", { timeout: 5000 });
    await page.keyboard.press("Escape");
    await page.waitForSelector(".mk-palette", { state: "detached" });
    await page.keyboard.press("Control+Shift+P");
    await sleep(400);
    if (await page.$(".mk-palette")) throw new Error("old binding still active");
    console.log("\n  settings:", JSON.parse(await page.evaluate(() => localStorage.getItem("moonkale.settings"))).keybindings);
  });
  console.log("PALETTE E2E: PASS");
} catch (e) {
  console.log("\nFAIL:", e.message);
  await page.screenshot({ path: `${S}/m7-palette-fail.png` }).catch(() => {});
  console.log(logs.filter((l) => !/WARN/.test(l)).slice(-20).join("\n"));
  process.exitCode = 1;
} finally {
  await browser.close();
}
