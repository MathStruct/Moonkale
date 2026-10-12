// Run against the standalone Dioxus fixture with Chromium and clipboard permissions.
import { chromium } from "playwright";

const port = process.env.PORT ?? "8091";
const browser = await chromium.launch();
const context = await browser.newContext();
const page = await context.newPage();

try {
  await page.goto(`http://127.0.0.1:${port}/`, { waitUntil: "networkidle" });
  const root = page.locator('[data-editor="editor-core-spike"]');
  await root.waitFor();
  await context.grantPermissions(["clipboard-read", "clipboard-write"]);

  const first = await root.locator('[data-line="0"] .mk-editor-core-cell').nth(0).boundingBox();
  const second = await root.locator('[data-line="0"] .mk-editor-core-cell').nth(1).boundingBox();
  await page.mouse.move(second.x + second.width / 2, second.y + second.height / 2);
  await page.mouse.down();
  await page.mouse.move(first.x + first.width * 0.2, first.y + first.height / 2, { steps: 3 });
  await page.mouse.up();
  await page.waitForFunction(() => document.querySelector('[data-editor="editor-core-spike"]')?.dataset.selection === "fn");
  await root.locator(".mk-editor-core-input-sink").focus();
  await page.keyboard.press("Control+c");
  const copied = await page.evaluate(() => navigator.clipboard.readText());
  if (copied !== "fn") throw new Error(`expected selected Rust text 'fn' in clipboard, got '${copied}'`);

  await page.keyboard.press("Control+x");
  await page.waitForFunction(() => {
    const text = document.querySelector('[data-editor="editor-core-spike"]')?.textContent ?? "";
    return text.replaceAll("│", "").includes(" main() {");
  });
  const cut = await page.evaluate(() => navigator.clipboard.readText());
  if (cut !== "fn") throw new Error(`expected cut text 'fn' in clipboard, got '${cut}'`);

  await page.evaluate(() => navigator.clipboard.writeText("clipboard paste works"));
  await root.locator(".mk-editor-core-input-sink").focus();
  await page.keyboard.press("Control+v");
  await page.waitForFunction(() => {
    const content = document.querySelector('[data-editor="editor-core-spike"]')?.textContent ?? "";
    return content.includes("clipboard paste works");
  });
  console.log("PASS: browser clipboard copy/cut use Rust selection; paste enters the Rust model through the input sink");
} finally {
  await browser.close();
}
