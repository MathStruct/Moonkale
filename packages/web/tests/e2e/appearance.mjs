// Spec 030: languages and themes. The UI follows the browser's language until Settings → Appearance
// picks one; the theme switches live between Dark, Light, Follow system and a theme file from the
// server's config directory; the colours come from the --mk-* tokens on <html>.
import { firefox } from "playwright";
import fs from "node:fs";
const S = process.env.M1_SHOTS ?? ".";
const PORT = process.env.PORT ?? 8080;
const CFG = `${process.env.MOONKALE_CONFIG_DIR ?? process.env.M1_ROOT + "/../cfg"}`;
const browser = await firefox.launch();
const step = async (n, f) => { process.stdout.write(`- ${n} … `); await f(); console.log("ok"); };
const token = (page, name) => page.evaluate((n) => getComputedStyle(document.documentElement).getPropertyValue(n).trim(), name);
const menus = (page) => page.$$eval(".mk-menu-button", (b) => b.map((x) => x.textContent.trim()));
const open = async (ctx) => {
  const page = await ctx.newPage();
  await page.goto(`http://127.0.0.1:${PORT}/`, { waitUntil: "networkidle" });
  await page.waitForSelector(".wb-workspace");
  return page;
};
const setting = async (page, cls, value) => {
  // Panels that mount late at start-up (the graph) can come to the front
  // over Settings: show it again until the choice is made.
  for (let i = 0; i < 10; i++) {
    await page.click(".wb-status-bar");
    await page.keyboard.press("Control+,");
    try {
      await page.waitForSelector(`.mk-settings-form select.${cls}`, { state: "visible", timeout: 1500 });
      await page.selectOption(`.mk-settings-form select.${cls}`, value, { timeout: 1500 });
      return;
    } catch {}
  }
  throw new Error(`could not set ${cls} to ${value}`);
};
// A theme file on the server (the web client's themes are the server's).
fs.mkdirSync(`${CFG}/themes`, { recursive: true });
fs.writeFileSync(`${CFG}/themes/paper.json`, JSON.stringify({ name: "Paper", base: "light", tokens: { accent: "#b0306a", canvas: "#faf6ee" } }));
let page;
try {
  await step("a German browser gets German menus without any setting (the first render stays English for hydration)", async () => {
    const ctx = await browser.newContext({ locale: "de-DE", viewport: { width: 1300, height: 800 } });
    const p = await open(ctx);
    await p.waitForFunction(() => [...document.querySelectorAll(".mk-menu-button")].some((b) => b.textContent.trim() === "Datei"), null, { timeout: 15000 });
    console.log("\n  menus:", (await menus(p)).join(" | "));
    await ctx.close();
  });
  await step("Settings → Appearance → Language: Deutsch, then 中文, switch live", async () => {
    const ctx = await browser.newContext({ locale: "en-US", viewport: { width: 1300, height: 800 } });
    page = await open(ctx);
    if (!(await menus(page)).includes("File")) throw new Error("not English first: " + (await menus(page)));
    await setting(page, "mk-settings-language", "de");
    await page.waitForFunction(() => [...document.querySelectorAll(".mk-menu-button")].some((b) => b.textContent.trim() === "Datei"), null, { timeout: 10000 });
    await page.waitForSelector(".mk-settings-form h3:text-is('Darstellung')", { timeout: 10000 });
    await setting(page, "mk-settings-language", "zh-CN");
    await page.waitForFunction(() => [...document.querySelectorAll(".mk-menu-button")].some((b) => b.textContent.trim() === "文件"), null, { timeout: 10000 });
    console.log("\n  menus:", (await menus(page)).join(" | "));
    await page.screenshot({ path: `${S}/spec030-zh.png` });
    await setting(page, "mk-settings-language", "en");
    await page.waitForFunction(() => [...document.querySelectorAll(".mk-menu-button")].some((b) => b.textContent.trim() === "File"), null, { timeout: 10000 });
  });
  await step("Theme: Dark tokens by default; Light switches <html> and the workbench", async () => {
    if ((await page.getAttribute("html", "data-theme")) !== "dark") throw new Error("data-theme " + (await page.getAttribute("html", "data-theme")));
    if ((await token(page, "--mk-canvas")) !== "#0f1116") throw new Error("dark canvas " + (await token(page, "--mk-canvas")));
    await setting(page, "mk-settings-theme", "light");
    await page.waitForFunction(() => document.documentElement.dataset.theme === "light", null, { timeout: 10000 });
    const canvas = await token(page, "--mk-canvas");
    const surface = await page.$eval(".mk-shell .wb-workspace", (e) => getComputedStyle(e).getPropertyValue("--wb-surface").trim());
    const bodyBg = await page.evaluate(() => getComputedStyle(document.body).backgroundColor);
    console.log("\n  light: canvas", canvas, "· workbench surface", surface, "· body", bodyBg);
    if (canvas !== "#f3f4f6" || surface !== "#ffffff" || bodyBg !== "rgb(243, 244, 246)") throw new Error("light tokens");
    await page.waitForTimeout(400);
    await page.screenshot({ path: `${S}/spec030-light.png` });
  });
  await step("Follow system: the OS preference decides, live", async () => {
    await setting(page, "mk-settings-theme", "system");
    await page.emulateMedia({ colorScheme: "light" });
    await page.waitForFunction(() => getComputedStyle(document.documentElement).getPropertyValue("--mk-canvas").trim() === "#f3f4f6", null, { timeout: 10000 });
    await page.emulateMedia({ colorScheme: "dark" });
    await page.waitForFunction(() => getComputedStyle(document.documentElement).getPropertyValue("--mk-canvas").trim() === "#0f1116", null, { timeout: 10000 });
  });
  await step("a theme file from the server's config directory: listed, selected, applied over its base", async () => {
    await page.reload({ waitUntil: "networkidle" });
    await page.waitForSelector(".wb-workspace");
    await page.waitForFunction(() => [...document.querySelectorAll(".mk-settings-theme option")].some((o) => o.value === "Paper"), null, { timeout: 15000 });
    await setting(page, "mk-settings-theme", "Paper");
    await page.waitForFunction(() => document.documentElement.dataset.theme === "Paper", null, { timeout: 10000 });
    const [accent, canvas, surface] = [await token(page, "--mk-accent"), await token(page, "--mk-canvas"), await token(page, "--mk-surface")];
    console.log("\n  Paper: accent", accent, "· canvas", canvas, "· surface (from the light base)", surface);
    if (accent !== "#b0306a" || canvas !== "#faf6ee" || surface !== "#ffffff") throw new Error("theme file tokens");
    await setting(page, "mk-settings-theme", "dark");
    await page.waitForFunction(() => document.documentElement.dataset.theme === "dark", null, { timeout: 10000 });
  });
  console.log("\nAPPEARANCE E2E: PASS");
} catch (e) {
  console.log("\nFAIL:", e.message);
  if (page) await page.screenshot({ path: `${S}/spec030-fail.png` }).catch(() => {});
  process.exitCode = 1;
} finally {
  fs.rmSync(`${CFG}/themes/paper.json`, { force: true });
  await browser.close();
}
