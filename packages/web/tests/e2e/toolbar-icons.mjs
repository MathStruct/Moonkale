// ADR-0027 (2026-10-14): toolbars show symbols, with the description (and
// shortcut) on hover and the label as hidden text; each extension's
// settings open next to its panel from a gear, named after the
// extension, take the focus, and close on Escape.
import { chromium } from "playwright";
const PORT = process.env.PORT ?? 8080, S = process.env.M1_SHOTS ?? ".";
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1400, height: 900 } });
const step = async (n, f) => { process.stdout.write(`- ${n} … `); await f(); console.log("ok"); };
try {
  await step("open a file in the code editor", async () => {
    await page.goto(`http://127.0.0.1:${PORT}/`, { waitUntil: "networkidle" });
    await page.waitForSelector(".wb-workspace");
    await page.click(".mk-explorer-open button[type=submit]");
    await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
    await page.click(".mk-tree-file >> text=Cargo.toml");
    await page.waitForSelector(".mk-editor-toolbar .mk-icon-btn:visible", { timeout: 15000 });
  });
  await step("the toolbar is symbols; hover says what each does, with its shortcut", async () => {
    const btns = await page.$$eval(".mk-editor-toolbar .mk-icon-btn", (b) => b.filter((x) => x.offsetParent).map((x) => ({ title: x.title, text: x.textContent.trim(), svg: !!x.querySelector("svg") })));
    console.log(`\n  ${JSON.stringify(btns.map((b) => b.title))}`);
    if (!btns.every((b) => b.svg)) throw new Error("a toolbar button has no symbol");
    const save = btns.find((b) => b.text === "Save");
    if (!save || !/^Save \((Ctrl|⌘)\+S\)$/.test(save.title)) throw new Error(`Save's tooltip: ${save?.title}`);
    if (!btns.some((b) => /^Wrap long lines/.test(b.title))) throw new Error("Wrap has no description");
    await page.locator(".mk-editor:visible .mk-editor-toolbar").screenshot({ path: `${S}/toolbar-icons.png` });
  });
  await step("the gear opens the extension's settings beside the panel; Escape closes them", async () => {
    const gear = page.locator(".mk-editor:visible .mk-panel-settings-btn");
    const tip = await gear.getAttribute("title");
    if (!/Code editor/i.test(tip)) throw new Error(`the gear names ${tip}, not the extension`);
    await gear.click();
    await page.waitForSelector(".mk-panel-settings", { timeout: 5000 });
    const head = await page.$eval(".mk-panel-settings-head", (h) => h.textContent);
    if (!/Code editor/i.test(head)) throw new Error(`the popover is titled ${head}`);
    if (!(await page.$(".mk-panel-settings input[type=checkbox]"))) throw new Error("the editor's wrap setting is missing");
    await page.waitForFunction(() => document.activeElement?.closest(".mk-panel-settings"), null, { timeout: 3000 });
    await page.screenshot({ path: `${S}/panel-settings.png` });
    await page.keyboard.press("Escape");
    await page.waitForFunction(() => !document.querySelector(".mk-panel-settings"), null, { timeout: 3000 });
  });
  await step("Rust toolbar keeps four controls, accessible overflow and user settings", async () => {
    const context = await browser.newContext({ viewport: { width: 1400, height: 900 }, hasTouch: true });
    await context.addInitScript(() => localStorage.setItem("moonkale.settings", JSON.stringify({ extensions: { enabled: ["dev.moonkale.editor-code-native"] }, editor: { implementation: "native", markdown_rich: false } })));
    const native = await context.newPage();
    try {
      const hydrated = native.waitForResponse(r => r.url().endsWith("/api/llm/info") && r.status() === 200);
      await native.goto(`http://127.0.0.1:${PORT}/`, { waitUntil: "domcontentloaded" });
      await hydrated;
      await native.click(".mk-explorer-open button[type=submit]");
      await native.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"));
      await native.click(".mk-tree-dir >> text=src");
      await native.click(".mk-tree-file >> text=main.rs");
      const panel = native.locator('.mk-crust[data-editor="rust"]:visible');
      await panel.locator(".mk-native-surface").waitFor();
      const toolbar = panel.locator(".mk-editor-toolbar");
      const controls = await toolbar.locator("button:visible, summary:visible, select:visible").evaluateAll(items => items.map(x => ({ svg: !!x.querySelector("svg"), title: x.title, text: x.textContent.trim() })));
      if (controls.length !== 4 || controls.some(x => !x.svg || !x.title)) throw Error(`Rust controls: ${JSON.stringify(controls)}`);
      if (!controls.some(x => x.text === "Save" && /Save \((Ctrl|⌘)\+S\)/.test(x.title))) throw Error("Rust Save shortcut missing");
      const more = toolbar.locator("summary");
      await more.focus(); await native.keyboard.press("Enter");
      const menu = toolbar.locator(".mk-native-more-items"); await menu.waitFor();
      for (const selector of [".mk-native-doc-preview", ".mk-native-show-hover", ".mk-native-complete", ".mk-native-definition", ".mk-native-rename", ".mk-native-code-actions", ".mk-native-find-references", ".mk-native-comment", ".mk-native-fold-all", ".mk-native-unfold-all", ".mk-editor-switch", ".mk-panel-settings-btn"]) {
        if (!await menu.locator(selector).isVisible()) throw Error(`Rust overflow missing ${selector}`);
      }
      await native.keyboard.press("Tab");
      await native.keyboard.press("Escape");
      await native.waitForFunction(() => document.activeElement?.matches('.mk-crust summary'));
      if (await menu.isVisible()) throw Error("Escape did not close overflow");
      for (const command of [".mk-native-fold-all", ".mk-native-unfold-all"]) {
        await more.click(); await menu.locator(command).click();
        await native.waitForFunction(() => document.activeElement?.matches('.mk-crust .mk-editor-core-input-sink'));
        if (await menu.isVisible()) throw Error("choosing a command left overflow open");
      }
      await more.click(); await menu.locator(".mk-panel-settings-btn").click();
      await native.locator(".mk-panel-settings .mk-native-indent-style").waitFor();
      const width = native.locator(".mk-panel-settings .mk-native-indent-width");
      const originalWidth = await width.inputValue();
      await width.selectOption("2");
      await width.selectOption(originalWidth);
      await native.keyboard.press("Escape");
      await native.keyboard.press("Control+Shift+P");
      await native.locator(".mk-palette-input").waitFor();
      for (const id of ["file.save", "editor.toggleWrap", "editor.rename", "editor.codeActions", "editor.definition", "editor.references", "editor.toggleComment", "editor.foldAll", "editor.unfoldAll", "native.reload", "native.hover", "native.complete", "native.preview", "native.docPreview", "native.codeMirror"]) {
        // The palette caps unfiltered rows; search each command by its visible title.
        const query = id.startsWith("native.") ? "Rust editor:" : id === "file.save" ? "Save" : id === "editor.toggleWrap" ? "Wrap" : id.split(".").at(-1).replace(/([A-Z])/g, " $1");
        await native.locator(".mk-palette-input").fill(query);
        await native.locator(`.mk-palette-item[data-key="${id}"]`).waitFor();
      }
      for (const [id, query, key] of [["editor.rename", "Rename", "F2"], ["editor.definition", "definition", "F12"], ["editor.references", "references", "Shift+F12"], ["native.complete", "Rust editor:", "Ctrl+Space"]]) {
        await native.locator(".mk-palette-input").fill(query);
        if (!(await native.locator(`.mk-palette-item[data-key="${id}"]`).textContent()).includes(key)) throw Error(`palette shortcut missing: ${id}`);
      }
      await native.keyboard.press("Escape");
      await native.setViewportSize({ width: 420, height: 900 });
      await native.locator(".mk-phone-bar").waitFor();
      await native.locator('.mk-phone-bar .mk-phone-btn[data-panel^="editor"]').click();
      await panel.waitFor();
      if (await panel.locator(".mk-editor-cursor-word").isVisible()) throw Error("cursor word costs phone space");
      const bounds = await toolbar.boundingBox();
      if (bounds.height > 60) throw Error(`phone toolbar uses ${bounds.height}px`);
      await panel.locator("summary").tap();
      await panel.locator(".mk-native-more-items").waitFor();
      await native.keyboard.press("Escape");
      await toolbar.screenshot({ path: `${S}/native-toolbar-phone.png` });
    } finally { await context.close(); }
  });
  console.log("\nTOOLBAR ICONS E2E: PASS");
} catch (e) {
  console.log("\nFAIL:", e.message);
  await page.screenshot({ path: `${S}/toolbar-icons-fail.png` }).catch(() => {});
  process.exitCode = 1;
} finally {
  await browser.close();
}
