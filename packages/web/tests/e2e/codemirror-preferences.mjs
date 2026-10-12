// Standalone compatibility check for shared editor preference overrides.
import { chromium } from "playwright";
import { resolve } from "node:path";
const browser = await chromium.launch();
const page = await browser.newPage();
const bundle = process.env.CM_BUNDLE ?? resolve("packages/code-view/assets/codemirror.js");
const source = "fn main() {}";
try {
  await page.setContent('<div id="editor" style="width:300px;height:200px"></div>');
  await page.addScriptTag({ path: bundle });
  await page.evaluate(source => {
    window.changes = 0;
    window.moonkale.codemirror.mount(document.querySelector("#editor"), source, () => window.changes++, {language: "rust", wrap: false});
  }, source);
  const preferences = (spaces, width) => page.evaluate(([spaces, width]) => window.moonkale.codemirror.setIndentation(document.querySelector("#editor"), spaces, width), [spaces, width]);
  const text = () => page.evaluate(() => window.moonkale.codemirror.getText(document.querySelector("#editor")));
  const enter = async () => {
    await page.evaluate(() => { const cm = window.moonkale.codemirror; const el = document.querySelector("#editor"); cm.setText(el, "fn main() {}"); cm.setCursor(el, 0, 11); cm.focus(el); });
    await page.keyboard.press("Enter");
    return text();
  };
  await preferences(true, 2);
  if (await text() !== source || await page.evaluate(() => window.changes) !== 0) throw Error("preferences changed source");
  if (!(await enter()).includes("\n  ")) throw Error("space indentation not applied");
  await preferences(false, 4);
  if (!(await enter()).includes("\n\t")) throw Error("tab indentation not applied");
  await preferences(null, null);
  if (!(await enter()).includes("\n  ")) throw Error("backend default was not restored");
  const before = await text();
  await page.evaluate(() => window.moonkale.codemirror.setWrap(document.querySelector("#editor"), true));
  if (!await page.locator(".cm-lineWrapping").count() || await text() !== before) throw Error("wrap setting changed source or was not applied");
  console.log("ok: CodeMirror preference overrides and default restoration preserve source");
} finally { await browser.close(); }
