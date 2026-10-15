// ADR-0027 (2026-10-14): toolbars show symbols, with the description (and
// shortcut) on hover and the label as hidden text; each extension's
// settings open next to its panel from a gear, named after the
// extension, take the focus, and close on Escape.
import { chromium } from "playwright";
const PORT = process.env.PORT ?? 8080, S = process.env.M1_SHOTS ?? ".";
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1400, height: 900 } });
const step = async (n, f) => { process.stdout.write(`- ${n} … `); await f(); console.log("ok"); };
try {
  await step("open a file in the code editor", async () => {
    await page.goto(`http://127.0.0.1:${PORT}/`, { waitUntil: "networkidle" });
    await page.waitForSelector(".wb-workspace");
    await page.click(".mk-explorer-open button[type=submit]");
    await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
    await page.click(".mk-tree-file >> text=Cargo.toml");
    await page.waitForSelector(".mk-editor-toolbar .mk-icon-btn:visible", { timeout: 15000 });
  });
  await step("the toolbar is symbols; hover says what each does, with its shortcut", async () => {
    const btns = await page.$$eval(".mk-editor-toolbar .mk-icon-btn", (b) => b.filter((x) => x.offsetParent).map((x) => ({ title: x.title, text: x.textContent.trim(), svg: !!x.querySelector("svg") })));
    console.log(`\n  ${JSON.stringify(btns.map((b) => b.title))}`);
    if (!btns.every((b) => b.svg)) throw new Error("a toolbar button has no symbol");
    const save = btns.find((b) => b.text === "Save");
    if (!save || !/^Save \((Ctrl|⌘)\+S\)$/.test(save.title)) throw new Error(`Save's tooltip: ${save?.title}`);
    if (!btns.some((b) => /^Wrap long lines/.test(b.title))) throw new Error("Wrap has no description");
    await page.locator(".mk-editor:visible .mk-editor-toolbar").screenshot({ path: `${S}/toolbar-icons.png` });
  });
  await step("the gear opens the extension's settings beside the panel; Escape closes them", async () => {
    const gear = page.locator(".mk-editor:visible .mk-panel-settings-btn");
    const tip = await gear.getAttribute("title");
    if (!/Code editor/i.test(tip)) throw new Error(`the gear names ${tip}, not the extension`);
    await gear.click();
    await page.waitForSelector(".mk-panel-settings", { timeout: 5000 });
    const head = await page.$eval(".mk-panel-settings-head", (h) => h.textContent);
    if (!/Code editor/i.test(head)) throw new Error(`the popover is titled ${head}`);
    if (!(await page.$(".mk-panel-settings input[type=checkbox]"))) throw new Error("the editor's wrap setting is missing");
    await page.waitForFunction(() => document.activeElement?.closest(".mk-panel-settings"), null, { timeout: 3000 });
    await page.screenshot({ path: `${S}/panel-settings.png` });
    await page.keyboard.press("Escape");
    await page.waitForFunction(() => !document.querySelector(".mk-panel-settings"), null, { timeout: 3000 });
  });
  console.log("\nTOOLBAR ICONS E2E: PASS");
} catch (e) {
  console.log("\nFAIL:", e.message);
  await page.screenshot({ path: `${S}/toolbar-icons-fail.png` }).catch(() => {});
  process.exitCode = 1;
} finally {
  await browser.close();
}
