// Milestone 6: phone-sized shell — at 420 px the workbench collapses to one tile with a bottom bar;
// the bar switches Files / Editor / Graph / Terminal / Agent / Settings; the layout is not persisted;
// widening back restores the desktop layout; the wide layout in .moonkale/settings.json is untouched.
import { firefox } from "playwright";
import fs from "node:fs";
const S = process.env.M1_SHOTS ?? ".";
const PORT = process.env.PORT ?? 8080;
const ROOT = process.env.M1_ROOT;
const browser = await firefox.launch();
const ctx = await browser.newContext({ viewport: { width: 420, height: 820 }, hasTouch: true, isMobile: true });
const page = await ctx.newPage();
const logs = [];
page.on("console", (m) => { const t = m.text(); if (!t.includes("session[")) logs.push(`[${m.type()}] ${t.slice(0, 300)}`); });
const step = async (n, f) => { process.stdout.write(`- ${n} … `); await f(); console.log("ok"); };
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const front = () => page.$eval(".mk-phone-bar", (b) => [...b.querySelectorAll(".mk-phone-btn.mk-active")].map((x) => x.dataset.panel).join(","));
const tiles = () => page.$$eval(".wb-tile", (t) => t.length);
try {
  await step("420 px: one tile, a bottom bar, no rail", async () => {
    await page.goto(`http://127.0.0.1:${PORT}/`, { waitUntil: "networkidle" });
    await page.waitForSelector(".mk-shell.mk-narrow .mk-phone-bar", { timeout: 15000 });
    const rail = await page.$eval(".wb-rail", (r) => getComputedStyle(r).display).catch(() => "none");
    const n = await tiles();
    const buttons = await page.$$eval(".mk-phone-btn", (b) => b.map((x) => x.textContent.trim()));
    console.log("\n  tiles:", n, "rail:", rail, "bar:", buttons.join(" | "));
    if (n !== 1 || rail !== "none") throw new Error("not collapsed");
    if (!buttons.includes("Files") || !buttons.includes("Agent")) throw new Error("bar incomplete");
    const h = await page.$eval(".mk-phone-btn", (b) => b.getBoundingClientRect().height);
    if (h < 44) throw new Error(`touch target ${h}px`);
    await page.screenshot({ path: `${S}/m6-phone-start.png` });
  });
  await step("open folder from the Explorer; open README.md → Editor comes to the front", async () => {
    await page.click(".mk-phone-btn[data-panel=explorer]");
    await page.click(".mk-explorer-open button[type=submit]");
    await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
    await page.click(".mk-tree-file >> text=README.md");
    await page.waitForSelector(".cm-content", { timeout: 15000 });
    await sleep(300);
    const f = await front();
    console.log("\n  front:", f);
    if (!f.startsWith("editor:")) throw new Error("editor not in front");
    if ((await tiles()) !== 1) throw new Error("split appeared");
    await page.screenshot({ path: `${S}/m6-phone-editor.png` });
  });
  await step("bar switches Graph → Terminal → Agent → Settings → Editor", async () => {
    for (const id of ["graph", "terminal", "agent", "settings"]) {
      await page.click(`.mk-phone-btn[data-panel=${id}]`);
      await sleep(250);
      if ((await front()) !== id) throw new Error(`${id} not in front (${await front()})`);
    }
    await page.waitForSelector(".mk-settings", { timeout: 5000 });
    await page.click(".mk-phone-btn:has-text('Editor')");
    await sleep(250);
    if (!(await front()).startsWith("editor:")) throw new Error("editor not back");
    await page.waitForSelector(".cm-content", { timeout: 5000 });
    await page.screenshot({ path: `${S}/m6-phone-agent.png` });
  });
  await step("the phone layout is not persisted (settings.json has no narrow layout)", async () => {
    await sleep(800);
    let f = {};
    try { f = JSON.parse(fs.readFileSync(`${ROOT}/.moonkale/settings.json`, "utf8")); } catch {}
    console.log("\n  layout saved:", !!f.layout, "open:", JSON.stringify(f.open_documents));
    if (f.layout && /"main"/.test(f.layout) && !/"side"/.test(f.layout)) throw new Error("phone layout persisted");
  });
  await step("widen to 1200 px: rail and the multi-tile layout return; narrow again: one tile", async () => {
    await page.setViewportSize({ width: 1200, height: 820 });
    await page.waitForSelector(".mk-shell:not(.mk-narrow) .wb-rail", { timeout: 10000 });
    await sleep(400);
    const n = await tiles();
    console.log("\n  tiles wide:", n);
    if (n < 3) throw new Error("wide layout not restored");
    await page.setViewportSize({ width: 420, height: 820 });
    await page.waitForSelector(".mk-shell.mk-narrow .mk-phone-bar", { timeout: 10000 });
    await sleep(300);
    if ((await tiles()) !== 1) throw new Error("not collapsed again");
  });
  console.log("PHONE SUITE PASS");
} catch (e) {
  console.log("\nFAIL:", e.message);
  await page.screenshot({ path: `${S}/m6-phone-fail.png` }).catch(() => {});
  console.log(logs.slice(-25).join("\n"));
  process.exitCode = 1;
} finally {
  await browser.close();
}
