// Production RustCodeEditorPanel with Workspace: tests/fixtures/native-editor.
import { chromium } from "playwright";
const browser = await chromium.launch();
const context = await browser.newContext({ viewport: { width: 1200, height: 800 }, permissions: ["clipboard-read", "clipboard-write"] });
const page = await context.newPage();
const errors = [];
page.on("pageerror", e => errors.push(e.message));
const root = page.locator(".primary [data-editor=rust]");
const input = () => root.locator(".mk-editor-core-input-sink");
const text = () => page.locator(".canonical").textContent();
const wait = value => page.waitForFunction(value => document.querySelector(".canonical")?.textContent === value, value);
const focus = async () => { await input().focus(); };
const key = async value => page.keyboard.press(value);
const step = async (label, action) => { await action(); console.log(`ok: ${label}`); };
try {
  await page.goto(`http://127.0.0.1:${process.env.PORT ?? "8097"}/`, { waitUntil: "networkidle" });
  await root.waitFor();
  const original = await text();
  await step("highlighting, localized edits and Workspace save", async () => {
    if (!await root.locator(".mk-editor-core-cell[class*=a-]").count()) throw Error("no syntax tokens");
    if (await root.locator(".dxc-editor").count()) throw Error("legacy textarea mounted");
    await focus(); await key("Control+Home"); await page.keyboard.type("//!");
    await wait(`//!${original}`); await key("Control+s");
    await page.waitForFunction(() => document.querySelector(".saved")?.textContent === document.querySelector(".canonical")?.textContent);
  });
  await step("Unicode selection, replacement and undo/redo", async () => {
    await page.click("#replace"); await wait("external 😀\n中 update\n");
    await focus(); await key("Control+Home"); await key("End"); await key("Shift+ArrowLeft");
    await page.waitForFunction(() => document.querySelector(".primary .mk-crust")?.dataset.selectionAnchor === "11" && document.querySelector(".primary .mk-crust")?.dataset.selectionHead === "9");
    await page.keyboard.insertText("λ"); await wait("external λ\n中 update\n");
    await focus(); await key("Control+z"); await wait("external 😀\n中 update\n");
    await key("Control+y"); await wait("external λ\n中 update\n");
  });
  await step("local find/replace navigates Unicode and groups replace-all history", async () => {
    await page.click("#replace"); await wait("external 😀\n中 update\n");
    await focus(); await key("Control+h");
    await root.locator(".mk-search-query").fill("😀");
    await root.getByRole("button", { name: "Next", exact: true }).click();
    await page.waitForFunction(() => document.querySelector(".primary .mk-crust")?.dataset.selectionAnchor === "9" && document.querySelector(".primary .mk-crust")?.dataset.selectionHead === "11");
    await root.locator(".mk-search-replacement").fill("猫");
    await root.getByRole("button", { name: "Replace all", exact: true }).click();
    await wait("external 猫\n中 update\n");
    await page.click("#undo"); await wait("external 😀\n中 update\n");
    await page.click("#redo"); await wait("external 猫\n中 update\n");
    await root.locator(".mk-search-query").fill("external");
    await root.locator(".mk-search-replacement").fill("external");
    await root.getByRole("button", { name: "Cancel", exact: true }).click();
    await page.click("#undo"); await wait("external 😀\n中 update\n");
    await focus(); await key("Control+Home"); await key("End"); await key("Shift+ArrowLeft");
    await page.keyboard.insertText("λ"); await wait("external λ\n中 update\n");
  });
  await step("remount retains selection/history without command replay", async () => {
    await page.click("#undo"); await wait("external 😀\n中 update\n");
    const selection = [await root.getAttribute("data-selection-anchor"), await root.getAttribute("data-selection-head")];
    await page.click("#mount"); await root.waitFor({ state: "detached" });
    await page.click("#mount"); await root.waitFor(); await wait("external 😀\n中 update\n");
    if (await root.getAttribute("data-selection-anchor") !== selection[0] || await root.getAttribute("data-selection-head") !== selection[1]) throw Error("remount lost selection");
    await page.click("#redo"); await wait("external λ\n中 update\n");
  });
  await step("duplicate panels handle each shell undo once", async () => {
    await page.click("#duplicate"); await page.locator(".duplicate .mk-native-surface").waitFor();
    await focus(); await key("Control+Home"); await page.keyboard.type("ab"); await wait("abexternal λ\n中 update\n");
    await page.click("#undo"); await wait("aexternal λ\n中 update\n");
    await page.waitForFunction(() => [...document.querySelectorAll('.mk-native-surface [data-line="0"]')].every(row => row.textContent.includes("aexternal λ")));
    await page.click("#duplicate");
  });
  await step("reload clears obsolete history", async () => {
    await root.getByRole("button", { name: "Reload", exact: true }).click(); await wait(`//!${original}`);
    await page.click("#undo"); await wait(`//!${original}`);
  });
  await step("composition and repeated input commit exactly once", async () => {
    await focus(); await key("Control+Home");
    await input().evaluate(el => {
      el.dispatchEvent(new CompositionEvent("compositionstart", { bubbles: true, data: "" }));
      el.dispatchEvent(new CompositionEvent("compositionupdate", { bubbles: true, data: "你好" }));
      el.dispatchEvent(new CompositionEvent("compositionend", { bubbles: true, data: "你好" }));
    });
    await wait(`你好//!${original}`);
    await focus(); await page.keyboard.insertText("😀"); await wait(`你好😀//!${original}`);
    await focus(); await page.keyboard.insertText("中"); await wait(`你好😀中//!${original}`);
  });
  await step("pointer hit testing uses Rust Unicode cell positions", async () => {
    const cell = root.locator('[data-line="0"] .mk-editor-core-cell').nth(2);
    const box = await cell.boundingBox();
    await page.mouse.click(box.x + box.width * 0.2, box.y + box.height / 2);
    await page.waitForFunction(() => document.querySelector(".primary .mk-crust")?.dataset.selectionHead === "2");
    await page.mouse.click(box.x + box.width * 0.8, box.y + box.height / 2);
    await page.waitForFunction(() => document.querySelector(".primary .mk-crust")?.dataset.selectionHead === "4");
  });
  await step("clipboard uses the Rust selection", async () => {
    await focus(); await key("Control+Home"); await key("Shift+ArrowRight"); await key("Control+c");
    if (await page.evaluate(() => navigator.clipboard.readText()) !== "你") throw Error("wrong clipboard selection");
    await key("Control+x"); await wait(`好😀中//!${original}`);
    await focus(); await key("Control+v"); await wait(`你好😀中//!${original}`);
  });
  await step("CRLF edits and save remain exact", async () => {
    await page.click("#crlf"); await wait("a\r\nb\r\n");
    await focus(); await key("Control+End"); await page.keyboard.insertText("中"); await wait("a\r\nb\r\n中");
    await focus(); await key("Control+h");
    await root.locator(".mk-search-query").fill("b");
    await root.locator(".mk-search-replacement").fill("β");
    await root.getByRole("button", {name: "Replace all", exact: true}).click();
    await wait("a\r\nβ\r\n中");
    await root.locator(".mk-search-query").focus(); await key("Escape");
    await page.waitForFunction(() => document.activeElement?.classList.contains("mk-editor-core-input-sink"));
    await key("Control+s"); await page.waitForFunction(() => document.querySelector(".saved")?.textContent === "a\r\nβ\r\n中");
  });
  await step("brackets, indentation and shell comments share Workspace history", async () => {
    await focus(); await key("Control+a"); await page.keyboard.insertText("");
    // Backspace clears the selection; paste/input fragments remain literal.
    await key("Backspace"); await wait("");
    await page.keyboard.type("{"); await wait("{}");
    await key("Enter"); await wait("{\n    \n}");
    await page.keyboard.type("😀"); await wait("{\n    😀\n}");
    await key("Control+a"); await key("Tab"); await wait("    {\n        😀\n    }");
    await key("Shift+Tab"); await wait("{\n    😀\n}");
    await page.click("#comment"); await wait("// {\n    // 😀\n// }");
    await page.click("#undo"); await wait("{\n    😀\n}");
    await page.click("#redo"); await wait("// {\n    // 😀\n// }");
    await focus(); await key("Control+/"); await wait("{\n    😀\n}");
    // External replacement rebuilds the model without losing its editing policy.
    await page.click("#crlf"); await wait("a\r\nb\r\n");
    await focus(); await key("Control+Home"); await page.keyboard.type("("); await wait("()a\r\nb\r\n");
    await key("Backspace"); await wait("a\r\nb\r\n");
    await key("Control+a"); await page.click("#comment"); await wait("// a\r\n// b\r\n");
    await page.click("#undo"); await wait("a\r\nb\r\n");
  });
  await step("reveal uses UTF-16 coordinates", async () => {
    await page.click("#replace"); await page.click("#reveal");
    await page.waitForFunction(() => document.querySelector(".primary .mk-crust")?.dataset.selectionHead === "13");
  });
  await step("fold gutter, shell commands, bracket matches and hidden-target navigation", async () => {
    await page.click("#folds");
    const source = 'fn main() {\n    if true {\n        let text = "} 😀"; // {\n    }\n}\n';
    await wait(source);
    const toggle = line => root.locator(`[data-fold-line="${line}"]`);
    await toggle(0).waitFor();
    await focus(); await key("Control+Home"); await key("End");
    await page.waitForFunction(() => document.querySelectorAll(".primary .mk-native-bracket-match").length === 2);
    await toggle(1).focus(); await key("Enter");
    await page.waitForFunction(() => !document.querySelector('.primary .mk-editor-core-row[data-line="2"]'));
    await wait(source);
    await toggle(0).click();
    await page.waitForFunction(() => !document.querySelector('.primary .mk-editor-core-row[data-line="1"]'));
    await page.click("#mount"); await page.click("#mount"); await root.waitFor();
    if (await toggle(0).getAttribute("aria-expanded") !== "false") throw Error("fold lost on remount");
    await root.locator(".mk-native-fold-placeholder").first().click();
    await page.waitForFunction(() => !!document.querySelector('.primary .mk-editor-core-row[data-line="1"]'));
    if (await toggle(1).getAttribute("aria-expanded") !== "false") throw Error("inner fold lost");
    // Vertical movement follows visible rows and skips the inner body.
    await focus(); await key("Control+Home"); await key("ArrowDown"); await key("ArrowDown");
    await page.waitForFunction(() => document.querySelector(".primary .mk-native-surface")?.dataset.cursorOffset === "63");
    await page.click("#unfold-all"); await toggle(1).waitFor();
    await page.click("#fold-all");
    await page.click("#reveal");
    await page.waitForFunction(() => document.querySelector('.primary [data-fold-line="0"]')?.getAttribute("aria-expanded") === "true");
    await page.click("#fold-all");
    await focus(); await key("Control+f");
    await root.locator(".mk-search-query").fill("let text");
    await root.getByRole("button", {name: "Next", exact: true}).click();
    await page.waitForFunction(() => !!document.querySelector('.primary .mk-editor-core-row[data-line="2"]'));
    await root.locator(".mk-search-query").focus(); await key("Escape");
    // Braces inside strings/comments are excluded from match/fold metadata.
    await focus(); await key("Control+Home"); await key("ArrowDown"); await key("ArrowDown"); await key("End");
    if (await root.locator(".mk-native-bracket-match").count()) throw Error("comment brace highlighted");
    await page.click("#fold-all");
    await focus(); await key("Control+Home"); await page.keyboard.type(" ");
    await wait(` ${source}`);
    if (await toggle(0).getAttribute("aria-expanded") !== "false") throw Error("fold lost after header edit");
    await key("Control+z"); await wait(source);
    if (await toggle(0).getAttribute("aria-expanded") !== "false") throw Error("fold lost after undo");
    await page.click("#unfold-all");
    await focus(); await key("Control+Home"); await key("End"); await key("Backspace");
    await page.waitForFunction(() => !document.querySelector('.primary [data-fold-line="0"]'));
    await key("Control+z"); await wait(source); await toggle(0).waitFor();
  });
  await step("indentation settings persist and leave canonical text/history untouched", async () => {
    await page.click("#prefs"); const source = "fn main() {}\r\n"; await wait(source);
    await focus(); await key("Control+s");
    await page.waitForFunction(() => document.querySelector(".saved")?.textContent === "fn main() {}\r\n");
    await root.locator(".mk-native-indent-style").selectOption("spaces");
    await root.locator(".mk-native-indent-width").selectOption("2");
    await wait(source);
    if (await root.locator(".mk-editor-dirty").count()) throw Error("settings dirtied source");
    await focus(); await key("Control+Home"); await key("End"); await key("ArrowLeft"); await key("Enter");
    await wait("fn main() {\r\n  \r\n}\r\n");
    await key("Control+z"); await wait(source);
    await page.click("#mount"); await page.click("#mount"); await root.waitFor();
    if (await root.locator(".mk-native-indent-width").inputValue() !== "2") throw Error("width lost on remount");
    await root.locator(".mk-native-indent-style").selectOption("tabs");
    await focus(); await key("Control+Home"); await key("End"); await key("ArrowLeft"); await key("Tab");
    await wait("fn main() {\t}\r\n");
    await key("Control+z"); await wait(source);
    await root.locator(".mk-native-indent-style").selectOption("default");
    await root.locator(".mk-native-indent-width").selectOption("");
    await focus(); await key("Control+Home"); await key("End"); await key("ArrowLeft"); await key("Enter");
    await wait("fn main() {\r\n    \r\n}\r\n");
    await key("Control+z"); await wait(source);
  });
  await step("regex highlights, validation, captures and shared CRLF history", async () => {
    const source = "😀 猫=12\r\n中=34\r\n猫=56\r\n";
    await page.click("#regex"); await wait(source);
    await focus(); await key("Control+h");
    const query = root.locator(".mk-search-query");
    const count = root.locator(".mk-search-count");
    await query.fill("(\\p{L}+)=(\\d+)");
    await root.locator(".mk-search-regex").check();
    await page.waitForFunction(() => document.querySelector(".primary .mk-search-count")?.dataset.total === "3");
    if (await root.locator(".mk-native-search-match").count() !== 12) throw Error("missing all-match highlights");
    await query.press("Enter");
    await page.waitForFunction(() => document.querySelector(".primary .mk-search-count")?.dataset.current === "1");
    if (await root.locator(".mk-native-search-current").count() !== 4) throw Error("current match not decorated");
    await query.press("Shift+Enter");
    if (await count.getAttribute("data-current") !== "3") throw Error("previous did not wrap");
    await query.fill("(");
    await root.locator(".mk-search-error").waitFor();
    if (await query.getAttribute("aria-invalid") !== "true") throw Error("invalid regex not reported");
    if (await root.locator(".mk-native-search-match").count()) throw Error("invalid query retained highlights");
    await wait(source);
    await query.fill("(\\p{L}+)=(\\d+)");
    await root.locator(".mk-search-replacement").fill("$2-${1}-$$");
    await root.getByRole("button", { name: "Replace all", exact: true }).click();
    const replaced = "😀 12-猫-$\r\n34-中-$\r\n56-猫-$\r\n";
    await wait(replaced);
    await page.click("#undo"); await wait(source);
    await page.click("#redo"); await wait(replaced);
    await query.fill("^");
    await page.waitForFunction(() => document.querySelector(".primary .mk-search-count")?.dataset.total === "4");
    if (await root.locator(".mk-native-search-zero").count() !== 4) throw Error("zero-width matches not visible");
    await query.press("Enter"); await query.press("Enter");
    await root.locator(".mk-search-replacement").fill(">");
    await root.getByRole("button", { name: "Replace all", exact: true }).click();
    await wait(">😀 12-猫-$\r\n>34-中-$\r\n>56-猫-$\r\n>");
    await page.click("#undo"); await wait(replaced);
    await query.press("Escape");
    if (await root.locator(".mk-native-search-zero").count()) throw Error("close retained highlights");
  });
  await step("Enter uses syntax before comments and ignores string punctuation", async () => {
    const source = "fn f() { // 😀 comment\r\n}\r\n";
    await page.click("#syntax"); await wait(source);
    await focus(); await key("Control+Home"); await key("End"); await key("Enter");
    const expected = "fn f() { // 😀 comment\r\n    \r\n}\r\n";
    await wait(expected);
    await key("Control+z"); await wait(source);
    await key("Control+Shift+z"); await wait(expected);
    await page.click("#syntax-string"); await wait('let text = "{";\r\n');
    await focus(); await key("Control+Home"); await key("End"); await key("Enter");
    await wait('let text = "{";\r\n\r\n');
    await key("Control+z"); await wait('let text = "{";\r\n');
  });
  await step("closing bracket alignment is one edit and skips existing closers", async () => {
    const source = "fn f() { // 😀\r\n    ";
    await page.click("#closing"); await wait(source);
    await focus(); await key("Control+End"); await page.keyboard.type("}");
    const expected = "fn f() { // 😀\r\n}";
    await wait(expected);
    await page.keyboard.type("x"); await wait(`${expected}x`);
    await key("Control+z"); await wait(expected);
    await key("Control+z"); await wait(source);
    await key("Control+Shift+z"); await wait(expected);
    const existing = "fn f() { // 😀\r\n    }\r\n";
    await page.click("#closing-skip"); await wait(existing);
    await focus(); await key("Control+Home"); await key("ArrowDown"); await key("End"); await key("ArrowLeft");
    await page.keyboard.type("}"); await wait(`${expected}\r\n`);
    await key("Control+z"); await wait(existing);
    await page.click("#closing"); await wait(source);
    await focus(); await key("Control+End"); await page.keyboard.insertText("}");
    await wait(`${source}}`); // Plain input/paste fragments preserve supplied indentation.
  });
  await step("3 MB long line renders bounded rows and accepts an edit", async () => {
    await page.click("#large");
    await page.waitForFunction(() => document.querySelector(".canonical")?.textContent.length === 3_000_000, null, { timeout: 30000 });
    await page.waitForFunction(() => document.querySelector(".primary .mk-native-surface")?.dataset.cursorOffset === "0");
    await root.locator(".mk-editor-core-viewport").evaluate(el => { el.scrollTop = 2200; });
    await page.waitForFunction(() => Number(document.querySelector(".primary .mk-native-surface")?.dataset.firstRow) >= 100);
    const firstRow = await root.locator(".mk-native-surface").getAttribute("data-first-row");
    await page.click("#mount"); await root.waitFor({ state: "detached" });
    await page.click("#mount"); await root.waitFor();
    await page.waitForFunction(expected => document.querySelector(".primary .mk-native-surface")?.dataset.firstRow === expected, firstRow);
    const rows = await root.locator(".mk-editor-core-row").count();
    const cells = await root.locator(".mk-editor-core-cell").count();
    if (rows > 30 || cells > 3000) throw Error(`unbounded render: ${rows} rows, ${cells} cells`);
    await focus(); await key("Control+Home"); const start = Date.now(); await page.keyboard.type("x");
    await page.waitForFunction(() => document.querySelector(".canonical")?.textContent.length === 3_000_001, null, { timeout: 15000 });
    console.log(`3 MB input-to-Workspace: ${Date.now() - start} ms; ${rows} rows, ${cells} cells`);
  });
  await step("unwrapped 3 MB line stays bounded and scrolls to an editable tail", async () => {
    const before = await text();
    await root.locator(".mk-native-wrap").click();
    await page.waitForFunction(() => document.querySelector(".primary .mk-native-surface")?.dataset.wrap === "false");
    if (await root.locator(".mk-editor-core-cell").count() > 200) throw Error("unwrapped line rendered in full");
    await focus(); await key("Control+End");
    await page.waitForFunction(() => Number(document.querySelector(".primary .mk-native-surface")?.dataset.firstColumn) > 2_999_000);
    if (await root.locator(".mk-editor-core-cell").count() > 200) throw Error("unbounded tail render");
    await page.keyboard.type("z"); await wait(`${before}z`);
    await key("Control+z"); await wait(before);
    const column = await root.locator(".mk-native-surface").getAttribute("data-first-column");
    await page.click("#mount"); await page.click("#mount"); await root.waitFor();
    await page.waitForFunction(column => document.querySelector(".primary .mk-native-surface")?.dataset.firstColumn === column, column);
    if (await root.locator(".mk-native-wrap").getAttribute("aria-pressed") !== "false") throw Error("wrap lost on remount");
    await root.locator(".mk-native-wrap").click();
    await page.waitForFunction(() => document.querySelector(".primary .mk-native-surface")?.dataset.wrap === "true");
    await wait(before);
  });
  await step("outer Python suites with trailing comments fold every nesting level", async () => {
    await page.click("#python");
    const python = page.locator(".python [data-editor=rust]");
    const toggle = line => python.locator(`[data-fold-line="${line}"]`);
    await toggle(0).waitFor();
    await toggle(2).click();
    await page.waitForFunction(() => !document.querySelector('.python .mk-editor-core-row[data-line="3"]'));
    await toggle(1).click();
    await page.waitForFunction(() => !document.querySelector('.python .mk-editor-core-row[data-line="2"]'));
    await toggle(0).click();
    await page.waitForFunction(() => !document.querySelector('.python .mk-editor-core-row[data-line="1"]'));
    await toggle(0).click();
    await toggle(1).waitFor();
    if (await toggle(1).getAttribute("aria-expanded") !== "false") throw Error("nested Python collapse not retained");
    await page.click("#python");
  });
  await step("HTML, Markdown and Julia grammar folds survive remount and reveal", async () => {
    await page.click("#prefs"); await wait("fn main() {}\r\n");
    for (const [name, nested, hidden] of [["nested.html", 1, 2], ["nested.md", 2, 4], ["nested.jl", 1, 3]]) {
      const button = page.locator(`[data-language-fixture="${name}"]`);
      await button.click();
      const panel = page.locator(".language [data-editor=rust]");
      const toggle = line => panel.locator(`[data-fold-line="${line}"]`);
      const source = await page.locator(".language-canonical").textContent();
      await toggle(nested).waitFor();
      await toggle(nested).click();
      await page.waitForFunction(line => !document.querySelector(`.language .mk-editor-core-row[data-line="${line}"]`), hidden);
      await toggle(0).click();
      await page.waitForFunction(line => !document.querySelector(`.language [data-fold-line="${line}"]`), nested);
      await button.click(); await panel.waitFor({ state: "detached" });
      await button.click(); await panel.waitFor();
      if (await toggle(0).getAttribute("aria-expanded") !== "false") throw Error(`${name}: fold lost on remount`);
      await toggle(0).click();
      if (await toggle(nested).getAttribute("aria-expanded") !== "false") throw Error(`${name}: nested fold lost`);
      const input = panel.locator(".mk-editor-core-input-sink");
      await input.focus(); await key("Control+f");
      const query = panel.locator(".mk-search-query");
      await query.fill("needle"); await query.press("Enter");
      await panel.locator(`.mk-editor-core-row[data-line="${hidden}"]`).waitFor();
      if (await toggle(nested).getAttribute("aria-expanded") !== "true") throw Error(`${name}: search did not unfold`);
      await query.press("Escape");
      await input.focus(); await key("Control+End"); await page.keyboard.type("x");
      await page.waitForFunction(source => document.querySelector(".language-canonical")?.textContent === `${source}x`, source);
      await key("Control+z");
      await page.waitForFunction(source => document.querySelector(".language-canonical")?.textContent === source, source);
      await button.click();
    }
  });
  await step("syntax-aware Python, HTML and Julia Enter preserves CRLF history", async () => {
    for (const [name, expected] of [
      ["nested.py", "if ready: # 😀\r\n    \r\n    run()\r\n"],
      ["nested.html", "<main>\r\n  \r\n</main>\r\n"],
      ["nested.jl", "function outer(x) # 😀\r\n    \r\nend\r\n"],
    ]) {
      await page.locator(`[data-indent-fixture="${name}"]`).click();
      const panel = page.locator(".language [data-editor=rust]");
      await panel.waitFor();
      const source = await page.locator(".language-canonical").textContent();
      const input = panel.locator(".mk-editor-core-input-sink");
      await input.focus(); await key("Control+Home");
      if (name === "nested.html") { for (let i = 0; i < 6; i++) await key("ArrowRight"); }
      else await key("End");
      await key("Enter");
      await page.waitForFunction(expected => document.querySelector(".language-canonical")?.textContent === expected, expected);
      // Typing must land inside the new indented line, including split HTML pairs.
      await page.keyboard.type("x");
      const typed = expected.replace(name === "nested.html" ? "\r\n  \r\n" : "\r\n    \r\n", name === "nested.html" ? "\r\n  x\r\n" : "\r\n    x\r\n");
      await page.waitForFunction(expected => document.querySelector(".language-canonical")?.textContent === expected, typed);
      await key("Control+z");
      await page.waitForFunction(expected => document.querySelector(".language-canonical")?.textContent === expected, expected);
      await key("Control+z");
      await page.waitForFunction(source => document.querySelector(".language-canonical")?.textContent === source, source);
      await key("Control+Shift+z");
      await page.waitForFunction(expected => document.querySelector(".language-canonical")?.textContent === expected, expected);
    }
  });
  await step("Julia end and HTML end tags align nested parents with CRLF undo", async () => {
    for (const [name, ch, expected] of [
      ["nested.html", ">", "<main>\r\n  <section>\r\n  </section>"],
      ["nested.jl", "d", "module Demo\r\n    function outer(x)\r\n    end"],
    ]) {
      await page.locator(`[data-closing-fixture="${name}"]`).click();
      const panel = page.locator(".language [data-editor=rust]");
      await panel.waitFor();
      const source = await page.locator(".language-canonical").textContent();
      await panel.locator(".mk-editor-core-input-sink").focus(); await key("Control+End");
      await page.keyboard.type(ch);
      await page.waitForFunction(expected => document.querySelector(".language-canonical")?.textContent === expected, expected);
      await page.keyboard.type("x");
      await page.waitForFunction(expected => document.querySelector(".language-canonical")?.textContent === `${expected}x`, expected);
      await key("Control+z");
      await page.waitForFunction(expected => document.querySelector(".language-canonical")?.textContent === expected, expected);
      await key("Control+z");
      await page.waitForFunction(source => document.querySelector(".language-canonical")?.textContent === source, source);
      await key("Control+Shift+z");
      await page.waitForFunction(expected => document.querySelector(".language-canonical")?.textContent === expected, expected);
    }
  });
  await step("Lean nested scopes and proof folds preserve siblings, remount and history", async () => {
    // Release the large primary fixture before exercising additional panels.
    await page.click("#prefs"); await wait("fn main() {}\r\n");
    const button = page.locator('[data-language-fixture="nested.lean"]');
    await button.click();
    const panel = page.locator(".language [data-editor=rust]");
    await panel.waitFor();
    const source = await page.locator(".language-canonical").textContent();
    const toggle = line => panel.locator(`[data-fold-line="${line}"]`);
    for (const line of [0,1,2,3,8]) await toggle(line).waitFor();
    await toggle(3).click();
    await page.waitForFunction(() => !document.querySelector('.language .mk-editor-core-row[data-line="4"]'));
    await panel.locator('.mk-editor-core-row[data-line="5"]').waitFor();
    await toggle(2).click();
    await panel.locator('.mk-editor-core-row[data-line="6"]').waitFor();
    await toggle(1).click(); await toggle(0).click();
    await panel.locator('.mk-editor-core-row[data-line="8"]').waitFor();
    await button.click(); await panel.waitFor({ state:"detached" });
    await button.click(); await panel.waitFor();
    if (await toggle(0).getAttribute("aria-expanded") !== "false") throw Error("Lean namespace fold lost on remount");
    await toggle(0).click();
    if (await toggle(1).getAttribute("aria-expanded") !== "false") throw Error("Lean section fold lost");
    await panel.locator(".mk-editor-core-input-sink").focus(); await key("Control+f");
    const query = panel.locator(".mk-search-query");
    await query.fill("needle"); await query.press("Enter");
    await panel.locator('.mk-editor-core-row[data-line="4"]').waitFor();
    await panel.locator(".mk-editor-core-viewport").evaluate(el => { el.scrollTop = 0; });
    await toggle(0).waitFor();
    for (const line of [0,1,2,3]) {
      if (await toggle(line).getAttribute("aria-expanded") !== "true") throw Error(`Lean search did not reveal fold ${line}`);
    }
    await query.press("Escape");
    await panel.locator(".mk-editor-core-input-sink").focus(); await key("Control+End"); await page.keyboard.type("x");
    await page.waitForFunction(source => document.querySelector(".language-canonical")?.textContent === `${source}x`,source);
    await key("Control+z");
    await page.waitForFunction(source => document.querySelector(".language-canonical")?.textContent === source,source);
    await key("Control+Shift+z");
    await page.waitForFunction(source => document.querySelector(".language-canonical")?.textContent === `${source}x`,source);
    await key("Control+z");
    await page.waitForFunction(source => document.querySelector(".language-canonical")?.textContent === source,source);
    await button.click();
  });
  if (errors.length) throw Error(errors.join("\n"));
} catch (error) {
  console.error("canonical:", (await text())?.slice(0, 200), "browser errors:", errors);
  throw error;
} finally { await browser.close(); }
