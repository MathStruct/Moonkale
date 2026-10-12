// Run against the standalone Dioxus fixture in tests/fixtures/editor-core-spike.
import { chromium } from "playwright";

const port = process.env.PORT ?? "8091";
const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1200, height: 800 } });

try {
  await page.goto(`http://127.0.0.1:${port}/`, { waitUntil: "networkidle" });
  const root = page.locator('[data-editor="editor-core-spike"]');
  await root.waitFor();

  const caretCount = await root.locator(".mk-editor-core-caret").count();
  if (caretCount !== 1) throw new Error(`expected one rendered caret, got ${caretCount}`);
  const initialCaretBar = await root.locator(".mk-editor-core-caret-bar").boundingBox();
  const caretCellBox = await root.locator('[data-line="0"] .mk-editor-core-cell').first().boundingBox();
  if (Math.abs(initialCaretBar.x - caretCellBox.x) > 0.5 || initialCaretBar.width > 1.5) {
    throw new Error("caret should be a thin bar on the left edge of the character cell");
  }

  const firstLineCells = root.locator('[data-line="0"] .mk-editor-core-cell');
  const beforeCaretMove = await Promise.all([
    firstLineCells.nth(0).boundingBox(),
    firstLineCells.nth(1).boundingBox(),
  ]);
  await firstLineCells.first().click();
  await page.keyboard.press("ArrowRight");
  const afterCaretMove = await Promise.all([
    firstLineCells.nth(0).boundingBox(),
    firstLineCells.nth(1).boundingBox(),
  ]);
  for (let i = 0; i < beforeCaretMove.length; i += 1) {
    if (Math.abs(beforeCaretMove[i].x - afterCaretMove[i].x) > 0.5) {
      throw new Error("moving the caret changed the document text layout");
    }
  }

  const visibleRows = await root.locator(".mk-editor-core-row").count();
  if (visibleRows !== 40) throw new Error(`expected 40 bounded viewport rows, got ${visibleRows}`);

  const viewport = root.locator(".mk-editor-core-viewport");
  await viewport.evaluate((element) => { element.scrollTop = 2200; });
  await page.waitForFunction(() => Number(document.querySelector('[data-editor="editor-core-spike"]')?.dataset.firstRow) >= 100);
  const scrolledStartLine = Number(await root.locator(".mk-editor-core-row").first().getAttribute("data-line"));
  if (scrolledStartLine < 100) throw new Error(`expected virtual window past row 100, got ${scrolledStartLine}`);
  await viewport.evaluate((element) => { element.scrollTop = 0; });
  await page.waitForFunction(() => document.querySelector('[data-editor="editor-core-spike"]')?.dataset.firstRow === "0");

  const beforeEmoji = Array.from('fn main() {\n    println!("hello Moonkale");\n}\n\nSelection and edits live in Rust.\n').length;
  const emojiCell = root.locator('[data-line="5"] .mk-editor-core-cell').first();
  const emojiBox = await emojiCell.boundingBox();
  await page.mouse.move(emojiBox.x + emojiBox.width * 0.2, emojiBox.y + emojiBox.height / 2);
  await page.waitForFunction(() => document.querySelector('[data-editor="editor-core-spike"]')?.dataset.hoverPosition === "5:0");
  await page.mouse.move(emojiBox.x + emojiBox.width * 0.8, emojiBox.y + emojiBox.height / 2);
  await page.waitForFunction(() => document.querySelector('[data-editor="editor-core-spike"]')?.dataset.hoverPosition === "5:1");
  const viewportBox = await viewport.boundingBox();
  await page.mouse.move(viewportBox.x - 2, viewportBox.y + 10);
  await page.waitForFunction(() => document.querySelector('[data-editor="editor-core-spike"]')?.dataset.hoverPosition === "");
  await page.mouse.click(emojiBox.x + emojiBox.width * 0.2, emojiBox.y + emojiBox.height / 2);
  await page.waitForFunction((offset) => Number(document.querySelector('[data-editor="editor-core-spike"]')?.dataset.cursorOffset) === offset, beforeEmoji);
  await page.mouse.click(emojiBox.x + emojiBox.width * 0.8, emojiBox.y + emojiBox.height / 2);
  await page.waitForFunction((offset) => Number(document.querySelector('[data-editor="editor-core-spike"]')?.dataset.cursorOffset) === offset + 1, beforeEmoji);
  await page.mouse.move(emojiBox.x + emojiBox.width * 0.2, emojiBox.y + emojiBox.height / 2);
  await page.mouse.down();
  await page.mouse.move(emojiBox.x + emojiBox.width * 0.8, emojiBox.y + emojiBox.height / 2, { steps: 3 });
  await page.mouse.up();
  await page.waitForFunction(() => document.querySelector('[data-editor="editor-core-spike"]')?.dataset.selection === "😀");
  const selectedEmojiColor = await emojiCell.evaluate((element) => getComputedStyle(element).backgroundColor);
  if (selectedEmojiColor === "rgba(0, 0, 0, 0)") throw new Error("selected text should have a visible highlight");
  await root.locator('[data-line="5"] .mk-editor-core-cell').nth(1).click();
  await page.waitForFunction((offset) => Number(document.querySelector('[data-editor="editor-core-spike"]')?.dataset.cursorOffset) === offset + 2, beforeEmoji);
  await page.keyboard.press("ArrowRight");
  await page.waitForFunction((offset) => Number(document.querySelector('[data-editor="editor-core-spike"]')?.dataset.cursorOffset) === offset + 3, beforeEmoji);

  const firstCell = root.locator(".mk-editor-core-cell").first();
  const firstCellBox = await firstCell.boundingBox();
  await page.mouse.click(firstCellBox.x + firstCellBox.width * 0.2, firstCellBox.y + firstCellBox.height / 2);
  await root.locator(".mk-editor-core-input-sink").focus();
  for (let line = 0; line < 35; line += 1) await page.keyboard.press("ArrowDown");
  await page.waitForFunction(() => Number(document.querySelector('[data-editor="editor-core-spike"]')?.dataset.firstRow) > 0);
  await viewport.evaluate((element) => { element.scrollTop = 0; });
  await page.waitForFunction(() => document.querySelector('[data-editor="editor-core-spike"]')?.dataset.firstRow === "0");

  const initialCell = root.locator(".mk-editor-core-cell").first();
  const initialCellBox = await initialCell.boundingBox();
  await page.mouse.click(initialCellBox.x + initialCellBox.width * 0.2, initialCellBox.y + initialCellBox.height / 2);
  await root.locator(".mk-editor-core-input-sink").focus();
  await page.keyboard.type("Q");
  await page.waitForFunction(() => {
    const content = document.querySelector('[data-editor="editor-core-spike"]')?.textContent ?? "";
    return content.replaceAll("│", "").includes("Qfn main");
  });

  await page.keyboard.press("Shift+ArrowRight");
  await page.waitForFunction(() => document.querySelector('[data-editor="editor-core-spike"]')?.dataset.selection === "f");
  await page.keyboard.press("Control+z");
  await page.waitForFunction(() => {
    const content = document.querySelector('[data-editor="editor-core-spike"]')?.textContent ?? "";
    return content.replaceAll("│", "").includes("fn main") && !content.replaceAll("│", "").includes("Qfn main");
  });

  const cells = root.locator(".mk-editor-core-cell");
  const first = await cells.nth(0).boundingBox();
  const fifth = await cells.nth(4).boundingBox();
  await page.mouse.move(first.x + first.width * 0.2, first.y + first.height / 2);
  await page.mouse.down();
  await page.mouse.move(fifth.x + fifth.width * 0.2, fifth.y + fifth.height / 2, { steps: 4 });
  await page.mouse.up();
  await page.waitForFunction(() => document.querySelector('[data-editor="editor-core-spike"]')?.dataset.selection === "fn m");

  const inputSink = root.locator(".mk-editor-core-input-sink");
  await inputSink.evaluate((element) => {
    element.dispatchEvent(new CompositionEvent("compositionstart", { data: "", bubbles: true }));
    element.dispatchEvent(new CompositionEvent("compositionupdate", { data: "に", bubbles: true }));
  });
  await page.waitForFunction(() => document.querySelector('[data-editor="editor-core-spike"]')?.dataset.preedit === "に");
  await inputSink.evaluate((element) => {
    element.dispatchEvent(new CompositionEvent("compositionend", { data: "に", bubbles: true }));
  });
  await page.waitForFunction(() => {
    const content = document.querySelector('[data-editor="editor-core-spike"]')?.textContent ?? "";
    return content.includes("に") && document.querySelector('[data-editor="editor-core-spike"]')?.dataset.composing === "false";
  });

  await inputSink.fill("PASTE");
  await page.waitForFunction(() => {
    const content = document.querySelector('[data-editor="editor-core-spike"]')?.textContent ?? "";
    return content.includes("PASTE");
  });

  const keyboardPage = await browser.newPage({ viewport: { width: 1200, height: 800 } });
  await keyboardPage.goto(`http://127.0.0.1:${port}/`, { waitUntil: "networkidle" });
  const keyboardRoot = keyboardPage.locator('[data-editor="editor-core-spike"]');
  const keyboardCells = keyboardRoot.locator('[data-line="0"] .mk-editor-core-cell');
  const keyboardFirstCell = await keyboardCells.first().boundingBox();
  await keyboardPage.mouse.click(keyboardFirstCell.x + keyboardFirstCell.width * 0.2, keyboardFirstCell.y + keyboardFirstCell.height / 2);
  await keyboardRoot.locator(".mk-editor-core-input-sink").focus();
  await keyboardPage.keyboard.press("Shift+ArrowRight");
  await keyboardPage.waitForFunction(() => document.querySelector('[data-editor="editor-core-spike"]')?.dataset.selection === "f");
  await keyboardPage.keyboard.press("Shift+ArrowRight");
  await keyboardPage.waitForFunction(() => document.querySelector('[data-editor="editor-core-spike"]')?.dataset.selection === "fn");
  await keyboardPage.keyboard.press("Shift+ArrowLeft");
  await keyboardPage.waitForFunction(() => document.querySelector('[data-editor="editor-core-spike"]')?.dataset.selection === "f");
  await keyboardPage.keyboard.press("Shift+ArrowLeft");
  await keyboardPage.waitForFunction(() => document.querySelector('[data-editor="editor-core-spike"]')?.dataset.selection === "");
  await keyboardPage.keyboard.press("ArrowRight");
  await keyboardPage.waitForFunction(() => Number(document.querySelector('[data-editor="editor-core-spike"]')?.dataset.cursorOffset) === 1);
  await keyboardPage.keyboard.press("Shift+ArrowLeft");
  await keyboardPage.waitForFunction(() => document.querySelector('[data-editor="editor-core-spike"]')?.dataset.selection === "f");
  if (await keyboardRoot.locator(".mk-editor-core-selected").count() !== 1) {
    throw new Error("backward keyboard selection should highlight its character");
  }
  await keyboardPage.keyboard.press("ArrowRight");
  await keyboardPage.waitForFunction(() => {
    const root = document.querySelector('[data-editor="editor-core-spike"]');
    return root?.dataset.selection === "" && Number(root.dataset.cursorOffset) === 1;
  });
  await keyboardPage.keyboard.press("ArrowLeft");
  await keyboardPage.waitForFunction(() => Number(document.querySelector('[data-editor="editor-core-spike"]')?.dataset.cursorOffset) === 0);

  console.log(`PASS: ${visibleRows} bounded rows; scrolling, repeated Shift+Arrow selection, Rust-owned typing/selection/undo, pointer drag, composition commit and input-sink text`);
} finally {
  await browser.close();
}
