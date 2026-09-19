import { firefox } from "playwright";
const S = process.env.M1_SHOTS ?? ".";
const browser = await firefox.launch();
const page = await browser.newPage({ viewport: { width: 1400, height: 900 } });
const PORT = process.env.PORT ?? 8080;
await page.goto(`http://127.0.0.1:${PORT}/`, { waitUntil: "networkidle" });
await page.waitForSelector(".wb-workspace");
// open a folder + file so the title shows something
await page.click(".mk-explorer-open button[type=submit]");
await page.click(".mk-tree-file >> text=README.md");
await page.waitForSelector(".cm-content");
await page.click(".mk-menu-button:has-text('File')");
await page.waitForSelector(".mk-menu-popup");
await page.screenshot({ path: `${S}/menu-file.png` });
const items = await page.$$eval(".mk-menu-item", (els) => els.map((e) => e.textContent.trim()));
console.log("File menu:", items);
// Edit → Undo after typing should remove the typed text
await page.click(".mk-menu-backdrop");
await page.click(".cm-content"); await page.keyboard.press("End"); await page.keyboard.type(" ZZZ");
await page.click(".mk-menu-button:has-text('Edit')");
await page.click(".mk-menu-item:has-text('Undo')");
await page.waitForFunction(() => !document.querySelector(".cm-content").textContent.includes("ZZZ"), null, { timeout: 5000 });
console.log("Edit → Undo: ok");
// View → Reset Layout keeps working; Help → About sets status
await page.click(".mk-menu-button:has-text('Help')");
await page.click(".mk-menu-item:has-text('About')");
await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("Moonkale 0.1.0"), null, { timeout: 5000 });
console.log("Help → About: ok");
console.log("title:", await page.$eval(".mk-titlebar-title", (e) => e.textContent));
await browser.close();
