// Milestone 12 step 1: the Extensions activity — a rail entry opens the Extensions panel, which lists the
// built-in extensions with their tier; toggling an opt-in one persists in the user scope.
import { firefox } from "playwright";
const S = process.env.M1_SHOTS ?? ".";
const PORT = process.env.PORT ?? 8080;
const browser = await firefox.launch();
const page = await browser.newPage({ viewport: { width: 1400, height: 900 } });
const step = async (n, f) => { process.stdout.write(`- ${n} … `); await f(); console.log("ok"); };
try {
  await step("the rail has an Extensions entry; clicking it opens the panel", async () => {
    await page.goto(`http://127.0.0.1:${PORT}/`, { waitUntil: "networkidle" });
    await page.waitForSelector(".wb-workspace");
    await page.waitForSelector("#mk-rail-extensions", { timeout: 15000 });
    await page.click("#mk-rail-extensions");
    await page.waitForSelector(".mk-extensions", { timeout: 10000 });
    const names = await page.$$eval(".mk-extensions .mk-settings-ext-name", (l) => l.map((x) => x.textContent.trim()));
    const tiers = await page.$$eval(".mk-extensions .mk-settings-scope", (l) => l.map((x) => x.textContent.trim()));
    console.log(`\n  ${names.length} extensions, tiers: core ${tiers.filter((t) => t === "core").length}, opt-in ${tiers.filter((t) => t === "opt-in").length}`);
    if (!names.includes("Flow editor") && !names.some((n) => /Flow/.test(n))) throw new Error("opt-in Flow editor missing: " + names.join(", "));
    await page.screenshot({ path: `${S}/m12-extensions.png` });
  });
  await step("toggling an opt-in extension persists in the user scope", async () => {
    const row = page.locator(".mk-extensions .mk-settings-ext", { hasText: /Flow/ }).first();
    const box = row.locator("input[type=checkbox]").first();
    const before = await box.isChecked();
    await box.click();
    await page.waitForFunction((b) => { try { return JSON.stringify(JSON.parse(localStorage.getItem("moonkale.settings")).extensions).includes("flow") === true; } catch { return false; } }, before, { timeout: 5000 });
    const after = await box.isChecked();
    if (after === before) throw new Error("checkbox did not toggle");
    await box.click();
  });
  await step("View → Show Extensions exists in the palette", async () => {
    await page.keyboard.press("Control+Shift+P");
    await page.waitForSelector(".mk-palette-input", { timeout: 10000 });
    await page.type(".mk-palette-input", "show extensions");
    await page.waitForFunction(() => [...document.querySelectorAll(".mk-palette-item")].some((i) => i.dataset.key === "view.panel.extensions"), null, { timeout: 5000 });
    await page.keyboard.press("Escape");
  });
  console.log("\nEXTENSIONS E2E: PASS");
} catch (e) { console.log("\nFAIL:", e.message); await page.screenshot({ path: `${S}/m12-extensions-fail.png` }); process.exitCode = 1; } finally { await browser.close(); }
