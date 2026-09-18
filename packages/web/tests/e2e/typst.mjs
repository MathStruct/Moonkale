import { firefox } from "playwright";
const S = process.env.M1_SHOTS ?? ".";
const browser = await firefox.launch();
const page = await browser.newPage({ viewport: { width: 1400, height: 900 } });
const step = async (n, f) => { process.stdout.write(`- ${n} … `); await f(); console.log("ok"); };
try {
  await step("open folder and report.typ", async () => {
    await page.goto("http://127.0.0.1:8080/", { waitUntil: "networkidle" });
    await page.waitForSelector(".wb-workspace");
    await page.click(".mk-explorer-open button[type=submit]");
    await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
    await page.click(".mk-tree-file >> text=report.typ");
    await page.waitForSelector(".cm-content");
  });
  await step("a 'Typst preview' tab appears and shows an SVG page", async () => {
    await page.click(".wb-tab[id^='wb-tab-typst-preview']");
    await page.waitForSelector(".mk-typst-page svg", { timeout: 30000 });
    const pages = await page.$$eval(".mk-typst-page", (els) => els.length);
    if (pages !== 1) throw new Error(`pages ${pages}`);
    await page.screenshot({ path: `${S}/m3-typst.png` });
  });
  await step("editing the source recompiles: a pagebreak makes two pages", async () => {
    await page.click(".wb-tab[id^='wb-tab-editor-']");
    await page.click(".cm-content"); await page.keyboard.press("Control+End"); await page.keyboard.type("\n#pagebreak()\nSecond page");
    await page.click(".wb-tab[id^='wb-tab-typst-preview']");
    await page.waitForFunction(() => document.querySelectorAll(".mk-typst-page").length === 2, null, { timeout: 30000 });
  });
  await step("a syntax error shows diagnostics instead of pages", async () => {
    await page.click(".wb-tab[id^='wb-tab-editor-']");
    await page.click(".cm-content"); await page.keyboard.press("Control+End"); await page.keyboard.type("\n#let broken = (");
    await page.click(".wb-tab[id^='wb-tab-typst-preview']");
    await page.waitForSelector(".mk-typst-error", { timeout: 30000 });
    console.log("\n  diagnostic:", await page.$eval(".mk-typst-error", (e) => e.textContent.trim()));
  });
  console.log("\nTYPST E2E: PASS");
} catch (e) { console.log("\nFAIL:", e.message); await page.screenshot({ path: `${S}/m3-fail.png` }); process.exitCode = 1; } finally { await browser.close(); }
