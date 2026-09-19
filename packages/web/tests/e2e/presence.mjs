// Milestone 8: presence — two browsers (two users) open the same folder; each sees the other in the
// status bar, on the tab of the file the other has open, and in the Explorer; leaving removes them.
import { firefox } from "playwright";
const S = process.env.M1_SHOTS ?? ".";
const PORT = process.env.PORT ?? 8080;
const browser = await firefox.launch();
const step = async (n, f) => { process.stdout.write(`- ${n} … `); await f(); console.log("ok"); };
const openAs = async (name) => {
  const ctx = await browser.newContext({ viewport: { width: 1400, height: 800 } });
  const page = await ctx.newPage();
  await page.goto(`http://127.0.0.1:${PORT}/`, { waitUntil: "networkidle" });
  await page.waitForSelector(".wb-workspace");
  // Name before joining: Settings → You → Name (user scope, localStorage).
  await page.click(".wb-status-bar");
  await page.keyboard.press("Control+,");
  await page.waitForSelector(".mk-settings-form", { timeout: 10000 });
  await page.fill(".mk-settings-form label:text-is('Name') input", name);
  await page.press(".mk-settings-form label:text-is('Name') input", "Tab");
  await page.waitForFunction((n) => (localStorage.getItem("moonkale.settings") || "").includes(n), name, { timeout: 10000 });
  await page.click(".mk-explorer-open button[type=submit]");
  await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
  return { ctx, page };
};
let a, b;
try {
  await step("Ada and Bob open the folder; each status bar shows the other", async () => {
    a = await openAs("Ada Lovelace");
    b = await openAs("Bob");
    await a.page.waitForSelector(".mk-presence[data-count='1']", { timeout: 15000 });
    await b.page.waitForSelector(".mk-presence[data-count='1']", { timeout: 15000 });
    const seenByAda = await a.page.$eval(".mk-presence-badge", (e) => e.getAttribute("title"));
    const seenByBob = await b.page.$eval(".mk-presence-badge", (e) => e.getAttribute("title"));
    console.log("\n  Ada sees:", seenByAda, "| Bob sees:", seenByBob);
    if (!seenByAda.startsWith("Bob") || !seenByBob.startsWith("Ada")) throw new Error("names");
  });
  await step("Bob opens README.md → Ada's tab and Explorer row show 'BO'; Ada opens it too → Bob sees 'AL'", async () => {
    await b.page.click(".mk-tree-file >> text=README.md");
    await b.page.waitForSelector(".cm-content", { timeout: 15000 });
    await a.page.waitForSelector(".mk-tree-row[title='README.md'] .mk-tree-presence", { timeout: 15000 });
    const badge = await a.page.$eval(".mk-tree-row[title='README.md'] .mk-tree-presence", (e) => e.textContent);
    console.log("\n  explorer badge on Ada's side:", badge);
    if (badge !== "BO") throw new Error("initials");
    await a.page.click(".mk-tree-file >> text=README.md");
    await a.page.waitForSelector(".wb-tab:has-text('README.md') .mk-tab-presence", { timeout: 15000 });
    await b.page.waitForSelector(".wb-tab:has-text('README.md') .mk-tab-presence:text-is('AL')", { timeout: 15000 });
    await a.page.screenshot({ path: `${S}/m8-presence.png` });
  });
  await step("Bob leaves → Ada's status bar and badges clear", async () => {
    await b.ctx.close();
    b = null;
    await a.page.waitForFunction(() => !document.querySelector(".mk-presence"), null, { timeout: 15000 });
    if (await a.page.$(".mk-tab-presence")) throw new Error("badge remained");
  });
  console.log("PRESENCE E2E: PASS");
} catch (e) {
  console.log("\nFAIL:", e.message);
  if (a) await a.page.screenshot({ path: `${S}/m8-presence-fail.png` }).catch(() => {});
  process.exitCode = 1;
} finally {
  await browser.close();
}
