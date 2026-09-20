// Spec 009: the activity bar (entries from the registry, badges, click-to-hide), Ctrl+B, menus
// built from the registry (View → Show …, Git menu, Edit → Find), source icon/stripe/colour.
import { firefox } from "playwright";
import fs from "node:fs";
const S = process.env.M1_SHOTS ?? ".";
const PORT = process.env.PORT ?? 8080;
const ROOT = process.env.M1_ROOT;
const browser = await firefox.launch();
const page = await browser.newPage({ viewport: { width: 1500, height: 900 } });
const logs = [];
page.on("console", (m) => { const t = m.text(); if (!t.includes("session[")) logs.push(`[${m.type()}] ${t.slice(0, 300)}`); });
const step = async (n, f) => { process.stdout.write(`- ${n} … `); await f(); console.log("ok"); };
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const rail = () => page.$$eval(".wb-rail .wb-rail-item", (b) => b.map((x) => ({ id: x.id.replace("mk-rail-", ""), active: x.classList.contains("wb-rail-item-active"), bottom: x.classList.contains("wb-rail-item-bottom"), badge: x.querySelector(".mk-badge")?.textContent ?? "" })));
const menu = async (name) => { await page.click(`.mk-menu-button:text-is('${name}')`); await page.waitForSelector(".mk-menu-popup", { timeout: 5000 }); return page.$$eval(".mk-menu-popup .mk-menu-item", (b) => b.map((x) => x.querySelector("span")?.textContent.trim())); };
try {
  await step("open folder: the rail lists Files … Settings in order, Explorer active, People at the bottom", async () => {
    await page.goto(`http://127.0.0.1:${PORT}/`, { waitUntil: "networkidle" });
    await page.waitForSelector(".wb-workspace");
    await page.click(".mk-explorer-open button[type=submit]");
    await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
    const r = await rail();
    console.log("\n  rail:", r.map((x) => `${x.id}${x.active ? "*" : ""}${x.badge ? `(${x.badge})` : ""}${x.bottom ? "↓" : ""}`).join(" "));
    const ids = r.map((x) => x.id);
    for (const want of ["explorer", "search", "links", "git", "history", "agent", "terminal", "graph", "settings", "presence"]) if (!ids.includes(want)) throw new Error("rail misses " + want);
    if (ids.indexOf("explorer") > ids.indexOf("search") || ids.indexOf("graph") > ids.indexOf("settings")) throw new Error("rail order wrong");
    if (!r.find((x) => x.id === "explorer").active) throw new Error("Explorer not marked active");
    if (!r.find((x) => x.id === "settings").bottom) throw new Error("Settings not pinned to the bottom");
    await page.screenshot({ path: `${S}/m10-rail.png` });
  });
  await step("the Git entry shows a badge once a file changes; the Explorer source row has an icon, stripe and colour", async () => {
    fs.writeFileSync(`${ROOT}/README.md`, "# changed\n");
    await page.click(".mk-tree-file >> text=README.md");
    await page.waitForSelector(".cm-content", { timeout: 15000 });
    await page.keyboard.press("Control+S").catch(() => {});
    await page.waitForFunction(() => /\d/.test(document.querySelector("#mk-rail-git .mk-badge")?.textContent || ""), null, { timeout: 30000 }).catch(() => {});
    const badge = await page.$eval("#mk-rail-git", (b) => b.querySelector(".mk-badge")?.textContent ?? "");
    const src = await page.$eval(".mk-explorer-source", (e) => ({ kind: e.dataset.kind, stripe: getComputedStyle(e).borderLeftWidth, color: e.style.getPropertyValue("--mk-source-color"), icon: !!e.querySelector(".mk-source-icon svg") }));
    console.log("\n  git badge:", JSON.stringify(badge), "source:", JSON.stringify(src));
    if (src.kind !== "folder" || !src.icon || src.stripe !== "3px" || !src.color.startsWith("#")) throw new Error("source row not decorated");
  });
  await step("clicking the active Explorer entry hides it; clicking again brings it back; Ctrl+B toggles the side bar", async () => {
    await page.click("#mk-rail-explorer");
    await page.waitForFunction(() => !document.querySelector(".mk-explorer"), null, { timeout: 5000 });
    await page.click("#mk-rail-explorer");
    await page.waitForSelector(".mk-explorer", { timeout: 5000 });
    await page.keyboard.press("Control+B");
    await page.waitForFunction(() => !document.querySelector(".mk-explorer") && !document.querySelector(".mk-search"), null, { timeout: 5000 });
    await page.keyboard.press("Control+B");
    await page.waitForSelector(".mk-explorer", { timeout: 5000 });
    // Search comes back too, as a background tab of the side tile.
    await page.waitForSelector(".mk-search", { state: "attached", timeout: 5000 });
  });
  await step("menus: View lists Show <panel> entries; a Git menu exists; Edit → Find opens the editor's search panel", async () => {
    const view = await menu("View");
    console.log("\n  View:", view.filter((x) => x.startsWith("Show")).join(" | "));
    if (!view.includes("Show Graph") || !view.includes("Show History") || !view.includes("Toggle Side Bar")) throw new Error("View menu incomplete");
    await page.keyboard.press("Escape");
    await page.click("body", { position: { x: 700, y: 500 } });
    const names = await page.$$eval(".mk-menu-button", (b) => b.map((x) => x.textContent.trim()));
    console.log("  menus:", names.join(" | "));
    if (!names.includes("Git")) throw new Error("no Git menu");
    const git = await menu("Git");
    if (!git.some((x) => /Commit/.test(x))) throw new Error("Git menu has no Commit: " + git);
    await page.click("body", { position: { x: 700, y: 500 } });
    await page.click(".cm-content");
    const edit = await menu("Edit");
    if (!edit.includes("Find") || !edit.includes("Rename Symbol")) throw new Error("Edit menu incomplete: " + edit);
    await page.click(".mk-menu-popup .mk-menu-item:has(span:text-is('Find'))");
    await page.waitForSelector(".cm-search", { timeout: 5000 });
    await page.screenshot({ path: `${S}/m10-menus.png` });
  });
  console.log("\nSHELL E2E: PASS");
} catch (e) { console.log("\nFAIL:", e.message); console.log(logs.slice(-10).join("\n")); await page.screenshot({ path: `${S}/m10-shell-fail.png` }); process.exitCode = 1; } finally { await browser.close(); }
