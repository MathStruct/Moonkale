// Milestone 1 end-to-end: open folder → tree → open file → edit → save → verify on disk → reload.
import { firefox } from "playwright";
import fs from "node:fs";

const ROOT = process.env.M1_ROOT;
const URL = process.env.M1_URL ?? "http://127.0.0.1:8080/";
const shots = process.env.M1_SHOTS ?? ".";
const browser = await firefox.launch();
const page = await browser.newPage({ viewport: { width: 1400, height: 900 } });
const logs = [];
page.on("console", (m) => logs.push(`[${m.type()}] ${m.text()}`));
page.on("pageerror", (e) => logs.push(`[pageerror] ${e.message}`));

const step = async (name, fn) => { process.stdout.write(`- ${name} … `); await fn(); console.log("ok"); };
try {
  await step("load app", async () => {
    await page.goto(URL, { waitUntil: "networkidle" });
    await page.waitForSelector(".wb-workspace", { timeout: 30000 });
    await page.screenshot({ path: `${shots}/m1-1-empty.png` });
  });
  await step("open folder (blank = MOONKALE_ROOT)", async () => {
    await page.click(".mk-explorer-open button");
    await page.waitForSelector(".mk-tree-row", { timeout: 15000 });
    const rows = await page.$$eval(".mk-tree-label", (els) => els.map((e) => e.textContent));
    if (JSON.stringify(rows) !== JSON.stringify(["src", "README.md"])) throw new Error(`unexpected tree ${JSON.stringify(rows)}`);
  });
  await step("expand src/ lazily", async () => {
    await page.click(".mk-tree-dir");
    await page.waitForSelector("text=main.rs", { timeout: 10000 });
  });
  await step("open README.md in an editor tab", async () => {
    await page.click(".mk-tree-file >> text=README.md");
    await page.waitForSelector(".cm-content", { timeout: 20000 });
    const text = await page.$eval(".cm-content", (e) => e.textContent);
    if (!text.includes("Edit me.")) throw new Error(`editor shows ${JSON.stringify(text)}`);
    const tabs = await page.$$eval(".wb-tab", (els) => els.map((e) => e.textContent.trim()));
    if (!tabs.some((t) => t.includes("README.md"))) throw new Error(`tabs ${JSON.stringify(tabs)}`);
    await page.screenshot({ path: `${shots}/m1-2-opened.png` });
  });
  await step("edit → dirty marker appears", async () => {
    await page.click(".cm-content");
    await page.keyboard.press("End");
    await page.keyboard.type(" Edited by e2e.");
    await page.waitForSelector(".mk-editor-dirty", { timeout: 5000 });
    await page.waitForSelector(".mk-tab-dirty", { timeout: 5000 });
  });
  await step("Ctrl+S saves to disk and clears dirty", async () => {
    await page.keyboard.press("Control+s");
    await page.waitForSelector(".mk-editor-dirty", { state: "detached", timeout: 10000 });
    const onDisk = fs.readFileSync(`${ROOT}/README.md`, "utf8");
    if (!onDisk.includes("Edited by e2e.")) throw new Error(`disk has ${JSON.stringify(onDisk)}`);
    const status = await page.$eval(".wb-status-bar", (e) => e.textContent);
    if (!status.includes("Saved README.md")) throw new Error(`status: ${status}`);
    await page.screenshot({ path: `${shots}/m1-3-saved.png` });
  });
  await step("external change → save conflicts → Reload picks it up", async () => {
    fs.writeFileSync(`${ROOT}/README.md`, "# Changed outside the editor\n");
    await page.click(".cm-content");
    await page.keyboard.type("x");
    await page.click("button:has-text('Save')");
    await page.waitForSelector(".mk-editor-error", { timeout: 10000 });
    await page.click("button:has-text('Reload')");
    await page.waitForFunction(() => document.querySelector(".cm-content")?.textContent.includes("Changed outside"), null, { timeout: 10000 });
    await page.waitForSelector(".mk-editor-error", { state: "detached", timeout: 5000 });
  });
  await step("open a second file, drag its tab to split right, editor still works", async () => {
    await page.click(".mk-tree-file >> text=main.rs");
    await page.waitForFunction(() => document.querySelectorAll(".cm-content").length === 2, null, { timeout: 15000 });
    // Drag the main.rs tab to the right edge of the main tile → split.
    const tab = page.locator(".wb-tab", { hasText: "main.rs" }).first();
    const tile = page.locator(".wb-tile").last();
    const tb = await tab.boundingBox(); const tl = await tile.boundingBox();
    await page.mouse.move(tb.x + tb.width / 2, tb.y + tb.height / 2);
    await page.mouse.down();
    await page.mouse.move(tb.x + 40, tb.y + 40, { steps: 5 });
    await page.mouse.move(tl.x + tl.width - 20, tl.y + tl.height / 2, { steps: 15 });
    await page.mouse.up();
    await page.waitForTimeout(500);
    const tiles = await page.$$eval(".wb-tile", (els) => els.length);
    if (tiles < 3) throw new Error(`expected a split (>=3 tiles), got ${tiles}`);
    // The remounted editor still shows main.rs and is editable.
    await page.waitForFunction(() => [...document.querySelectorAll(".cm-content")].some((e) => e.textContent.includes("println")), null, { timeout: 10000 });
    await page.screenshot({ path: `${shots}/m1-4-split.png` });
  });
  console.log("\nMILESTONE 1 E2E: PASS");
} catch (e) {
  console.log("\nFAIL:", e.message);
  await page.screenshot({ path: `${shots}/m1-fail.png` });
  console.log("console:", logs.slice(-15).join("\n"));
  process.exitCode = 1;
} finally {
  await browser.close();
}
