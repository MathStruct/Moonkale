// Opt-in Rust engine through the real shell, folder source and editor switch.
import { firefox } from "playwright";
import fs from "fs";
const rootPath = process.env.M1_ROOT;
if (!rootPath) throw Error("M1_ROOT must name a disposable fixture folder");
const original = fs.readFileSync(`${rootPath}/src/main.rs`, "utf8");
fs.writeFileSync(`${rootPath}/perf.rs`, `//${"x".repeat(2_999_998)}`);
const browser = await firefox.launch();
const context = await browser.newContext({ viewport: { width: 1400, height: 900 } });
await context.addInitScript(() => localStorage.setItem("moonkale.settings", JSON.stringify({ extensions: { enabled: ["dev.moonkale.editor-code-native"] }, editor: { implementation: "native", markdown_rich: false } })));
const page = await context.newPage();
const panel = page.locator('.mk-crust[data-editor="rust"]').filter({ visible: true });
const input = () => panel.locator(".mk-editor-core-input-sink");
const save = async expected => {
  await input().focus(); await page.keyboard.press("Control+s");
  await page.waitForFunction(() => ![...document.querySelectorAll(".mk-crust")].filter(x => x.offsetParent !== null).some(x => x.querySelector(".mk-editor-dirty")));
  if (fs.readFileSync(`${rootPath}/src/main.rs`, "utf8") !== expected) throw Error("saved text differs from the Rust edit");
};
const step = async (label, action) => { await action(); console.log(`ok: ${label}`); };
try {
  const hydrated = page.waitForResponse(response => response.url().endsWith("/api/llm/info") && response.status() === 200);
  await page.goto(`http://127.0.0.1:${process.env.PORT ?? 8080}/`, { waitUntil: "domcontentloaded" });
  await hydrated;
  await page.waitForSelector(".wb-workspace");
  await page.click(".mk-explorer-open button[type=submit]");
  await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
  await page.click(".mk-tree-dir >> text=src"); await page.click(".mk-tree-file >> text=main.rs");
  await panel.locator(".mk-native-surface").waitFor();
  await step("Rust syntax, localized input and exact folder save", async () => {
    if (!await panel.locator(".mk-editor-core-cell[class*=a-]").count()) throw Error("missing syntax highlighting");
    await input().focus(); await page.keyboard.press("Control+Home"); await page.keyboard.type("// native\n");
    await panel.locator(".mk-editor-dirty").waitFor(); await save(`// native\n${original}`);
  });
  await step("Unicode selected-range replacement and undo/redo", async () => {
    await input().focus(); await page.keyboard.press("Control+Home"); await page.keyboard.insertText("λ😀");
    await input().focus(); await page.keyboard.press("Control+Home"); await page.keyboard.press("ArrowRight"); await page.keyboard.press("Shift+ArrowRight");
    await page.waitForFunction(() => [...document.querySelectorAll(".mk-crust")].some(x => x.offsetParent !== null && x.dataset.selectionAnchor === "1" && x.dataset.selectionHead === "3"));
    await page.keyboard.insertText("中"); await save(`λ中// native\n${original}`);
    await page.keyboard.press("Control+z"); await save(`λ😀// native\n${original}`);
    await page.keyboard.press("Control+y"); await save(`λ中// native\n${original}`);
  });
  await step("phone layout remount retains the model and history", async () => {
    await page.setViewportSize({ width: 420, height: 900 }); await page.waitForSelector(".mk-phone-bar");
    await page.setViewportSize({ width: 1400, height: 900 }); await input().waitFor();
    await input().focus(); await page.keyboard.press("Control+z"); await save(`λ😀// native\n${original}`);
    await page.keyboard.press("Control+y"); await save(`λ中// native\n${original}`);
  });
  await step("reload restores canonical source text", async () => {
    await input().focus(); await page.keyboard.type("discard"); await panel.locator(".mk-editor-dirty").waitFor();
    await panel.getByRole("button", { name: "Reload", exact: true }).click();
    await page.waitForFunction(() => ![...document.querySelectorAll(".mk-crust")].filter(x => x.offsetParent !== null).some(x => x.querySelector(".mk-editor-dirty")));
    await save(`λ中// native\n${original}`);
  });
  await step("editor switch preserves canonical text", async () => {
    await panel.locator(".mk-editor-switch").click(); await page.locator(".cm-content").filter({ visible: true }).waitFor();
    await page.locator(".mk-editor-switch").filter({ visible: true }).click(); await input().waitFor();
    await save(`λ中// native\n${original}`);
  });
  await step("3 MB typing dirties Workspace within 1.5 seconds", async () => {
    await page.click(".mk-tree-file >> text=perf.rs");
    await page.waitForFunction(() => [...document.querySelectorAll(".mk-crust")].some(x => x.offsetParent !== null && x.querySelector(".mk-editor-path")?.textContent.endsWith("perf.rs") && !x.querySelector(".mk-editor-dirty")));
    await input().waitFor();
    await input().focus(); await page.keyboard.press("Control+Home");
    const start = Date.now(); await page.keyboard.type("x"); await panel.locator(".mk-editor-dirty").waitFor({ timeout: 1500 });
    const latency = Date.now() - start;
    if (latency > 1500) throw Error(`3 MB input-to-dirty took ${latency} ms`);
    console.log(`3 MB input-to-dirty: ${latency} ms`);
    await page.keyboard.press("Control+s");
    await page.waitForFunction(() => ![...document.querySelectorAll(".mk-crust")].filter(x => x.offsetParent !== null).some(x => x.querySelector(".mk-editor-dirty")));
    const saved = fs.readFileSync(`${rootPath}/perf.rs`, "utf8");
    if (saved.length !== 3_000_001 || !saved.startsWith("x//")) throw Error("large edit did not reach the intended source document");
    if (await panel.locator(".mk-editor-core-row").count() > 100) throw Error("unbounded viewport rendering");
  });
  if (process.env.M1_SHOTS) await page.screenshot({ path: `${process.env.M1_SHOTS}/m1-rust-code-view.png` });
} finally { await browser.close(); }
