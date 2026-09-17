// Two tabs of one session: sources propagate, "New Window" opens a peer,
// and a document dragged from tab A drops into tab B (drag simulated by
// dispatching the HTML5 events — the OS-level drag itself can't be scripted).
import { firefox } from "playwright";
const S = process.env.M1_SHOTS ?? ".";
const browser = await firefox.launch();
const ctx = await browser.newContext({ viewport: { width: 1200, height: 800 } });
const a = await ctx.newPage();
const logs = [];
a.on("console", (m) => logs.push(`[A ${m.type()}] ${m.text()}`));
const step = async (n, f) => { process.stdout.write(`- ${n} … `); await f(); console.log("ok"); };
try {
  await step("tab A opens folder + README.md", async () => {
    await a.goto("http://127.0.0.1:8080/", { waitUntil: "networkidle" });
    await a.waitForSelector(".wb-workspace");
    await a.click(".mk-explorer-open button[type=submit]");
    await a.click(".mk-tree-file >> text=README.md");
    await a.waitForSelector(".cm-content");
  });
  let b;
  await step("View → New Window opens tab B, which learns the source", async () => {
    const [page] = await Promise.all([
      ctx.waitForEvent("page"),
      (async () => { await a.click(".mk-menu-button:has-text('View')"); await a.click(".mk-menu-item:has-text('New Window')"); })(),
    ]);
    b = page;
    b.on("console", (m) => logs.push(`[B ${m.type()}] ${m.text()}`));
    await b.waitForSelector(".wb-workspace", { timeout: 30000 });
    await b.waitForSelector(".mk-explorer-source-name", { timeout: 15000 });   // arrived via SourceOpened
    const editorsB = await b.$$eval(".cm-content", (e) => e.length);
    if (editorsB !== 0) throw new Error("B should have no editors yet");
  });
  await step("dragstart in A → B shows a drop target", async () => {
    await a.dispatchEvent(".mk-editor-path", "dragstart");
    await b.waitForSelector(".mk-drop-target", { timeout: 10000 });
    const label = await b.$eval(".mk-drop-target-label", (e) => e.textContent);
    if (!label.includes("README.md")) throw new Error(`label: ${label}`);
    await b.screenshot({ path: `${S}/session-droptarget.png` });
  });
  await step("drop in B → B opens README.md, A closes it", async () => {
    await b.dispatchEvent(".mk-drop-target", "drop");
    await b.waitForFunction(() => document.querySelectorAll(".cm-content").length === 1, null, { timeout: 15000 });
    await a.waitForFunction(() => document.querySelectorAll(".cm-content").length === 0, null, { timeout: 15000 });
    const txt = await b.$eval(".cm-content", (e) => e.textContent);
    if (!txt.includes("Edit me")) throw new Error(`B shows ${JSON.stringify(txt)}`);
    const status = await a.$eval(".wb-status-bar", (e) => e.textContent);
    if (!status.includes("Moved to window")) throw new Error(`A status: ${status}`);
    await b.screenshot({ path: `${S}/session-moved-b.png` });
    await a.screenshot({ path: `${S}/session-moved-a.png` });
  });
  await step("real mouse drag in B that ends without a drop → A shows a 'Move it here' banner", async () => {
    // Uses the browser's own DnD: dragstart only fires in Firefox if dataTransfer data was set.
    const h = await b.locator(".mk-editor-path").boundingBox();
    await b.mouse.move(h.x + 10, h.y + h.height / 2);
    await b.mouse.down();
    await b.mouse.move(h.x + 60, h.y + 40, { steps: 8 });
    await b.mouse.move(h.x + 200, h.y + 200, { steps: 8 });
    await a.waitForSelector(".mk-drop-target", { timeout: 10000 });          // live drag
    await b.mouse.up();                                                        // dragend in B
    await a.waitForSelector(".mk-drop-banner", { timeout: 10000 });           // pending offer
    await a.screenshot({ path: `${S}/session-banner.png` });
  });
  await step("'Move it here' in A moves the document back", async () => {
    await a.click(".mk-drop-banner button:has-text('Move it here')");
    await a.waitForFunction(() => document.querySelectorAll(".cm-content").length === 1, null, { timeout: 15000 });
    await b.waitForFunction(() => document.querySelectorAll(".cm-content").length === 0, null, { timeout: 15000 });
    await a.waitForSelector(".mk-drop-banner", { state: "detached", timeout: 5000 });
  });
  await step("dismiss works", async () => {
    await a.dispatchEvent(".mk-editor-path", "dragstart");
    await a.dispatchEvent(".mk-editor-path", "dragend");
    await b.waitForSelector(".mk-drop-banner", { timeout: 10000 });
    await b.click(".mk-drop-banner button[title=Dismiss]");
    await b.waitForSelector(".mk-drop-banner", { state: "detached", timeout: 5000 });
  });
  console.log("\nSESSION E2E: PASS");
} catch (e) {
  console.log("\nFAIL:", e.message); console.log(logs.slice(-12).join("\n")); process.exitCode = 1;
} finally { await browser.close(); }
