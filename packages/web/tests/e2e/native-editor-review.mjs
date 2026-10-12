// Resize and geometry regressions from REVIEW-from-claude.md.
import { chromium } from "playwright";
const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1200, height: 1000 } });
const errors = [];
page.on("pageerror", error => errors.push(error.message));
try {
  await page.goto(`http://127.0.0.1:${process.env.PORT ?? "8099"}/`, { waitUntil: "networkidle" });
  await page.locator(".primary [data-editor=rust]").waitFor();
  const original = await page.locator(".canonical").textContent();
  const sink = page.locator(".primary .mk-editor-core-input-sink");
  const cells = page.locator('.primary [data-line="0"] .mk-editor-core-cell');
  for (const backward of [false, true]) {
    await sink.focus();
    await page.keyboard.press("Control+Home");
    await page.keyboard.press("End");
    await page.waitForFunction(() => document.querySelector(".primary .mk-editor-cursor-word")?.getAttribute("aria-hidden") === "true");
    const first = await cells.nth(0).boundingBox();
    const fourth = await cells.nth(3).boundingBox();
    const start = backward ? {x: fourth.x + fourth.width * .8, y: fourth.y + fourth.height / 2}
      : {x: first.x + first.width * .2, y: first.y + first.height / 2};
    const end = backward ? {x: first.x + first.width * .2, y: first.y + first.height / 2}
      : {x: fourth.x + fourth.width * .8, y: fourth.y + fourth.height / 2};
    await page.mouse.move(start.x, start.y);
    await page.mouse.down();
    await page.waitForFunction(() => document.querySelector(".primary .mk-editor-cursor-word")?.getAttribute("aria-hidden") === "false");
    const after = await cells.nth(0).boundingBox();
    if (Math.abs(after.y - first.y) > .5) throw Error("caret word moved source during mouse-down");
    await page.mouse.move(end.x, end.y, {steps: 4});
    await page.mouse.up();
    await page.keyboard.insertText("Q");
    await page.waitForFunction(expected => document.querySelector(".canonical")?.textContent === expected, "Q" + original.slice(4));
    await page.keyboard.press("Control+z");
    await page.waitForFunction(expected => document.querySelector(".canonical")?.textContent === expected, original);
  }
  console.log("ok: cursor word visibility preserves source geometry during forward/backward pointer selection");
  await page.click("#large");
  await page.waitForFunction(() => document.querySelector(".canonical")?.textContent.length === 3_000_000);
  const before = await page.locator(".primary").evaluate(el => ({
    rows: el.querySelectorAll(".mk-editor-core-row").length,
    cells: el.querySelector(".mk-editor-core-row").querySelectorAll(".mk-editor-core-cell").length,
  }));
  await page.locator(".primary").evaluate(el => { el.style.height = "700px"; el.style.width = "600px"; });
  await page.waitForFunction(before => {
    const el = document.querySelector(".primary");
    return el.querySelectorAll(".mk-editor-core-row").length > before.rows
      && el.querySelector(".mk-editor-core-row").querySelectorAll(".mk-editor-core-cell").length < before.cells;
  }, before);
  console.log("ok: resizing the dock tile updates visible rows and wrapping width");
  await page.locator(".primary .mk-native-surface").evaluate(el => { el.style.lineHeight = "30px"; });
  await page.waitForFunction(() => document.querySelector(".primary .mk-editor-core-row")?.getBoundingClientRect().height === 30);
  await page.locator(".primary .mk-editor-core-viewport").evaluate(el => { el.scrollTop = 3000; });
  await page.waitForFunction(() => document.querySelector(".primary .mk-native-surface")?.dataset.firstRow === "100");
  console.log("ok: measured line height controls rendered geometry and scroll hit testing");
  for (const [name, expected] of [
    ["nested.html", "<main>\r\n  \r\n</main>\r\n"],
    ["nested.jl", "function outer(x) # 😀\r\n    \r\nend\r\n"],
    ["nested.py", "if ready: # 😀\r\n    \r\n    run()\r\n"],
    ["nested.html", "<main>\r\n  \r\n</main>\r\n"],
    ["nested.jl", "function outer(x) # 😀\r\n    \r\nend\r\n"],
  ]) {
    await page.locator(`[data-indent-fixture="${name}"]`).click();
    await page.locator(".language .mk-editor-core-input-sink").focus();
    await page.keyboard.press("Control+Home");
    if (name === "nested.html") {
      for (let i = 0; i < 6; i++) await page.keyboard.press("ArrowRight");
    } else await page.keyboard.press("End");
    await page.keyboard.press("Enter");
    await page.waitForFunction(expected => document.querySelector(".language-canonical")?.textContent === expected, expected);
  }
  console.log("ok: switching document props and reopening externally reset views accepts the first key");
  if (errors.length) throw Error(errors.join("\n"));
} finally {
  await browser.close();
}
