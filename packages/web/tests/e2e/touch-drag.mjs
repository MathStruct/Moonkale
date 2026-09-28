// Milestone 16 (Prompt26, "Android: one can not drag and drop to rearrange windows"): a long
// press on a tab starts a drag by touch, the finger carries it to a dock zone, lifting it docks
// the panel there. A tap still activates a tab and a quick swipe does not drag. Chromium with
// touch emulation at phone size; touches go through CDP Input.dispatchTouchEvent (real touch
// events, as on the phone).
import { chromium } from "playwright";
const S = process.env.M1_SHOTS ?? ".";
const PORT = process.env.PORT ?? 8080;
const browser = await chromium.launch();
const ctx = await browser.newContext({ viewport: { width: 420, height: 860 }, hasTouch: true, isMobile: true, deviceScaleFactor: 2 });
const page = await ctx.newPage();
const cdp = await ctx.newCDPSession(page);
const logs = [];
page.on("console", (m) => { const t = m.text(); if (!t.includes("session[")) logs.push(`[${m.type()}] ${t.slice(0, 300)}`); });
const step = async (n, f) => { process.stdout.write(`- ${n} … `); await f(); console.log("ok"); };
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const touch = (type, x, y) => cdp.send("Input.dispatchTouchEvent", { type, touchPoints: type === "touchEnd" ? [] : [{ x, y }] });
const tiles = () => page.$$eval(".wb-tile", (t) => t.length);
const center = (sel) => page.$eval(sel, (e) => { const r = e.getBoundingClientRect(); return { x: r.x + r.width / 2, y: r.y + r.height / 2 }; });
/** Press at `from`, hold `holdMs`, move to `to` in steps, release. */
const gesture = async (from, to, holdMs) => {
  await touch("touchStart", from.x, from.y);
  await sleep(holdMs);
  for (let i = 1; i <= 12; i++) {
    await touch("touchMove", from.x + ((to.x - from.x) * i) / 12, from.y + ((to.y - from.y) * i) / 12);
    await sleep(30);
  }
  await sleep(150);
  await touch("touchEnd");
  await sleep(600);
};
try {
  await step("phone shell: open the folder and a file; one tile with several tabs", async () => {
    await page.goto(`http://127.0.0.1:${PORT}/`, { waitUntil: "networkidle" });
    await page.waitForSelector(".mk-shell.mk-narrow .mk-phone-bar", { timeout: 15000 });
    await page.click(".mk-phone-btn[data-panel=explorer]");
    await page.click(".mk-explorer-open button[type=submit]");
    await page.waitForFunction(() => document.querySelector(".wb-status-bar")?.textContent.includes("index:"), null, { timeout: 30000 });
    await page.click(".mk-tree-file >> text=README.md");
    await page.waitForFunction(() => [...document.querySelectorAll(".wb-tab")].some((t) => t.textContent.includes("README.md")), null, { timeout: 15000 });
    console.log("\n  tiles:", await tiles(), "tabs:", (await page.$$eval(".wb-tab", (t) => t.map((x) => x.textContent.trim()))).join(" | "));
    if ((await tiles()) !== 1) throw new Error("expected one tile on the phone");
  });
  await step("a quick swipe on a tab does not start a drag", async () => {
    const from = await center(".wb-tab:has-text('Sources')");
    await gesture(from, { x: from.x, y: from.y + 400 }, 40);
    if ((await tiles()) !== 1) throw new Error("a swipe docked something");
    if (await page.$(".mk-touch-dragging")) throw new Error("drag state left behind");
  });
  await step("long press on the Sources tab, drag to the bottom edge, lift: the tile splits", async () => {
    // The swipe above scrolled the tab strip (as a swipe should): bring the tab back first.
    await page.$eval(".wb-tab:has-text('Sources')", (e) => e.scrollIntoView({ inline: "nearest", block: "nearest" }));
    await sleep(200);
    const from = await center(".wb-tab:has-text('Sources')");
    const body = await page.$eval(".wb-tile", (e) => { const r = e.getBoundingClientRect(); return { x: r.x + r.width / 2, y: r.y + r.height - 30 }; });
    await gesture(from, body, 500);
    await page.waitForFunction(() => document.querySelectorAll(".wb-tile").length === 2, null, { timeout: 5000 });
    const where = await page.$$eval(".wb-tile", (ts) => ts.map((t) => [...t.querySelectorAll(".wb-tab")].map((b) => b.textContent.trim()).join(",")));
    console.log("\n  tiles now:", where.join("  //  "));
    if (!where.some((w) => /^Sources/.test(w))) throw new Error("Sources is not in a tile of its own");
    if (await page.$(".mk-touch-dragging")) throw new Error("drag state left behind");
    await page.screenshot({ path: `${S}/m16-touch-drag.png` });
  });
  await step("a tap still activates a tab", async () => {
    await page.$eval(".wb-tab:has-text('Graph')", (e) => e.scrollIntoView({ inline: "nearest", block: "nearest" }));
    await sleep(200);
    const t = await center(".wb-tab:has-text('Graph')");
    await touch("touchStart", t.x, t.y); await sleep(60); await touch("touchEnd"); await sleep(600);
    const sel = await page.$eval(".wb-tab:has-text('Graph')", (e) => e.getAttribute("aria-selected"));
    if (sel !== "true") throw new Error("tap did not activate Graph");
  });
  console.log("\nTOUCH DRAG E2E: PASS");
} catch (e) { console.log("\nFAIL:", e.message); console.log(logs.slice(-10).join("\n")); await page.screenshot({ path: `${S}/m16-touch-fail.png` }); process.exitCode = 1; } finally { await browser.close(); }
