import { chromium } from "playwright";
import assert from "node:assert/strict";
const browser = await chromium.launch();
const page = await browser.newPage();
const errors = [];
page.on("pageerror", error => errors.push(error.message));
try {
  await page.goto(`http://127.0.0.1:${process.env.PORT ?? "8099"}/`, { waitUntil: "networkidle" });
  await page.click("#show-proportional");
  const waitReady = () => page.waitForFunction(() => document.querySelector(".proportional-probe")?.dataset.ready === "true");
  await waitReady();
  const height = Number(await page.locator(".proportional-probe").getAttribute("data-height"));
  await page.click("#proportional-width");
  await page.waitForFunction(() => document.querySelector(".proportional-probe")?.dataset.ready === "true" && Number(document.querySelector(".proportional-probe").dataset.width) === 230);
  assert.ok(Number(await page.locator(".proportional-probe").getAttribute("data-height")) > height);
  await page.click("#proportional-reset");
  await waitReady();
  const source = "Wi 😀 e\u0301 中\tTabs";
  const rect = async (start, end) => page.locator(".proportional-source").evaluate((el, {start,end}) => {
    const node = [...el.childNodes].find(node => node.nodeType === Node.TEXT_NODE);
    const range = document.createRange(); range.setStart(node,start); range.setEnd(node,end);
    const rect = range.getBoundingClientRect(); return {x:rect.x,y:rect.y,width:rect.width,height:rect.height};
  }, {start,end});
  const w = await rect(0,1), i = await rect(1,2);
  assert.ok(w.width > i.width * 1.5, "fixture is not proportional");
  for (const [start,end,scalarStart,scalarEnd] of [[3,5,3,4],[6,8,5,7],[9,10,8,9],[10,11,9,10]]) {
    const box = await rect(start,end);
    for (const [fraction,expected] of [[.2,scalarStart],[.8,scalarEnd]]) {
      await page.mouse.click(box.x + box.width * fraction, box.y + box.height / 2);
      assert.equal(Number(await page.locator(".proportional-probe").getAttribute("data-cursor")), expected);
    }
  }
  const emoji = await rect(3,5);
  await page.mouse.click(emoji.x + emoji.width * .2, emoji.y + emoji.height / 2);
  await page.keyboard.insertText("Q");
  await page.waitForFunction(() => document.querySelector(".proportional-canonical")?.textContent === "Wi Q😀 é 中\tTabs");
  await page.locator(".proportional-input").focus();
  await page.keyboard.insertText("R");
  await page.waitForFunction(() => document.querySelector(".proportional-canonical")?.textContent === "Wi QR😀 é 中\tTabs");
  await page.keyboard.press("Control+z");
  await page.waitForTimeout(100);
  if (await page.locator(".proportional-canonical").textContent() !== source) await page.keyboard.press("Control+z");
  await page.waitForFunction(expected => document.querySelector(".proportional-canonical")?.textContent === expected, source);
  await waitReady();
  const begin = await rect(3,5), end = await rect(6,8);
  await page.mouse.move(begin.x + begin.width * .2, begin.y + begin.height / 2);
  await page.mouse.down();
  await page.mouse.move(end.x + end.width * .8, end.y + end.height / 2, {steps: 4});
  await page.mouse.up();
  assert.ok(await page.locator(".proportional-selection").count());
  await page.locator(".proportional-input").focus();
  await page.keyboard.insertText("X");
  await page.waitForFunction(() => document.querySelector(".proportional-canonical")?.textContent === "Wi X 中\tTabs");
  await page.locator(".proportional-input").focus();
  await page.keyboard.press("Control+z");
  await page.waitForFunction(expected => document.querySelector(".proportional-canonical")?.textContent === expected, source);
  await page.click("#proportional-delay");
  await page.click("#proportional-width");
  await page.click("#proportional-reset");
  await page.click("#proportional-delay");
  await page.waitForFunction(() => document.querySelector(".proportional-probe")?.dataset.ready === "true" && Number(document.querySelector(".proportional-probe").dataset.width) === 600);
  await page.waitForTimeout(400);
  assert.equal(await page.locator(".proportional-canonical").textContent(), source);
  assert.equal(Number(await page.locator(".proportional-probe").getAttribute("data-width")), 600);
  await page.click("#show-proportional");
  assert.deepEqual(errors, []);
  console.log("PASS: proportional widths, pixel wrapping, grapheme/tab/emoji hits, Rust editing/undo and stale geometry");
} finally { await browser.close(); }
