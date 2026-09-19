// Milestone 7: access token — server started with MOONKALE_TOKEN: the app redirects to /login,
// server functions answer 401 without the cookie, a wrong token is refused, the right one sets an
// HttpOnly cookie and everything works (folder, index, terminal relay); a bearer works for scripts.
import { firefox } from "playwright";
const S = process.env.M1_SHOTS ?? ".";
const PORT = process.env.PORT ?? 8091;
const TOKEN = process.env.MOONKALE_TOKEN ?? "e2e-secret-token";
const base = `http://127.0.0.1:${PORT}`;
const browser = await firefox.launch();
const ctx = await browser.newContext({ viewport: { width: 1300, height: 800 } });
const page = await ctx.newPage();
const step = async (n, f) => { process.stdout.write(`- ${n} … `); await f(); console.log("ok"); };
try {
  await step("GET / redirects to /login; a server function without the cookie is 401", async () => {
    const r = await page.goto(`${base}/`, { waitUntil: "networkidle" });
    if (!page.url().endsWith("/login")) throw new Error("not redirected: " + page.url());
    const api = await page.request.post(`${base}/api/sources/open_folder`, { data: { path: "", embed: null }, headers: { "content-type": "application/json" } });
    console.log("\n  /api without cookie:", api.status());
    if (api.status() !== 401) throw new Error("expected 401");
    await page.screenshot({ path: `${S}/m7-auth-login.png` });
  });
  await step("wrong token → 401 and stays on the form", async () => {
    await page.fill("input[name=token]", "nope");
    await page.click("button[type=submit]");
    await page.waitForSelector("p.err", { timeout: 5000 });
  });
  await step("right token → cookie (HttpOnly) → app loads, folder opens, terminal relay works", async () => {
    await page.fill("input[name=token]", TOKEN);
    await page.click("button[type=submit]");
    await page.waitForSelector(".wb-workspace", { timeout: 20000 });
    await page.waitForLoadState("networkidle"); // hydration before clicking
    const cookies = await ctx.cookies();
    const c = cookies.find((x) => x.name === "moonkale_token");
    console.log("\n  cookie:", c && { httpOnly: c.httpOnly, sameSite: c.sameSite, path: c.path });
    if (!c || !c.httpOnly) throw new Error("cookie not HttpOnly");
    if (await page.evaluate(() => document.cookie.includes("moonkale_token"))) throw new Error("cookie readable from JS");
    await page.click(".mk-explorer-open button[type=submit]");
    await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
    await page.click(".wb-status-bar");
    await page.keyboard.press("Control+`");
    await page.waitForSelector(".xterm", { timeout: 20000 });
    await page.waitForFunction(() => document.querySelector(".xterm-rows")?.textContent.trim().length > 0, null, { timeout: 20000 });
  });
  await step("bearer token works for scripts; a bad bearer does not", async () => {
    const fresh = await browser.newContext();
    const ok = await fresh.request.post(`${base}/api/sources/open_folder`, { data: { path: "", embed: null }, headers: { "content-type": "application/json", authorization: `Bearer ${TOKEN}` } });
    const bad = await fresh.request.post(`${base}/api/sources/open_folder`, { data: { path: "", embed: null }, headers: { "content-type": "application/json", authorization: "Bearer wrong" } });
    console.log("\n  bearer ok:", ok.status(), "| bad:", bad.status());
    if (ok.status() !== 200 || bad.status() !== 401) throw new Error("bearer handling");
    await fresh.close();
  });
  console.log("AUTH E2E: PASS");
} catch (e) {
  console.log("\nFAIL:", e.message);
  await page.screenshot({ path: `${S}/m7-auth-fail.png` }).catch(() => {});
  process.exitCode = 1;
} finally {
  await browser.close();
}
