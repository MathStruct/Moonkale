// Milestone 6: a third-party wasm extension — listed from the server's extension dir, off by default,
// permissions granted in Settings, its command runs as an agent tool through the mock provider.
// Needs the example module installed in the server's config dir (packages/extensions/wordcount/build.sh,
// or MOONKALE_CONFIG_DIR pointing at a dir with extensions/wordcount.wasm).
import { firefox } from "playwright";
const S = process.env.M1_SHOTS ?? ".";
const PORT = process.env.PORT ?? 8080;
const ROOT = process.env.M1_ROOT;
const browser = await firefox.launch();
const page = await browser.newPage({ viewport: { width: 1500, height: 900 } });
const logs = [];
page.on("console", (m) => { const t = m.text(); if (!t.includes("session[")) logs.push(`[${m.type()}] ${t.slice(0, 300)}`); });
const step = async (n, f) => { process.stdout.write(`- ${n} … `); await f(); console.log("ok"); };
let serverRuns = 0;
page.on("request", (r) => { if (/\/api\/ext\/run/.test(r.url())) serverRuns++; });
const ask = async (text) => { await page.fill(".mk-agent-input", text); await page.click(".mk-agent-compose button"); };
const idle = () => page.waitForFunction(() => !document.querySelector(".mk-agent-compose button[disabled]"), null, { timeout: 60000 });
const lastAssistant = () => page.$$eval(".mk-agent-assistant", (e) => e.map((x) => x.textContent).at(-1) || "");
try {
  await step("open folder; the wasm extension is listed in Settings, off", async () => {
    await page.goto(`http://127.0.0.1:${PORT}/`, { waitUntil: "networkidle" });
    await page.waitForSelector(".wb-workspace");
    await page.click(".mk-explorer-open button[type=submit]");
    await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
    await page.click(".wb-status-bar");
    await page.keyboard.press("Control+,");
    await page.waitForSelector(".mk-settings-ext:has(.mk-settings-ext-name:text-is('Word count (wasm example)'))", { timeout: 15000 });
    const on = await page.$eval(".mk-settings-ext:has(.mk-settings-ext-name:text-is('Word count (wasm example)')) > label input", (e) => e.checked);
    if (on) throw new Error("wasm extension enabled by default");
  });
  let readme;
  await step("while disabled, the agent has no wordcount tool", async () => {
    await ask(`/tool graph.query {"source":"folder:${ROOT}","mode":"children"}`);
    await idle();
    readme = (await lastAssistant()).match(/([0-9a-f-]{36})\tFile\tREADME\.md/)[1];
    await ask(`/tool wordcount.count {"source":"folder:${ROOT}","node":"${readme}"}`);
    // Third-party tools are Mutating by policy: the gate asks even for unknown ones.
    await page.waitForSelector(".mk-agent-approval", { timeout: 20000 });
    await page.click(".mk-agent-approval button:has-text('Allow')");
    await idle();
    if (!/unknown tool wordcount\.count/.test(await lastAssistant())) throw new Error("tool ran while disabled: " + (await lastAssistant()).slice(0, 120));
  });
  await step("enable it without permissions: the host refuses the read", async () => {
    await page.click(".mk-settings-ext:has(.mk-settings-ext-name:text-is('Word count (wasm example)')) > label input");
    await page.waitForFunction(() => /example-wordcount/.test(localStorage.getItem("moonkale.settings") || ""), null, { timeout: 10000 });
    await ask(`/tool wordcount.count {"source":"folder:${ROOT}","node":"${readme}"}`);
    await page.waitForSelector(".mk-agent-approval", { timeout: 20000 });
    await page.click(".mk-agent-approval button:has-text('Allow')");
    await idle();
    if (!/permission read-sources not granted/.test(await lastAssistant())) throw new Error("expected permission error: " + (await lastAssistant()).slice(0, 160));
  });
  await step("grant read-sources: the command counts the file", async () => {
    await page.click(".mk-settings-ext:has(.mk-settings-ext-name:text-is('Word count (wasm example)')) .mk-settings-perm input");
    await page.waitForFunction(() => /read-sources/.test(localStorage.getItem("moonkale.settings") || ""), null, { timeout: 10000 });
    await ask(`/tool wordcount.count {"source":"folder:${ROOT}","node":"${readme}"}`);
    await page.waitForSelector(".mk-agent-approval", { timeout: 20000 });
    await page.click(".mk-agent-approval button:has-text('Allow')");
    await idle();
    const a = await lastAssistant();
    console.log("\n  result:", a.slice(0, 120));
    if (!/3 lines, 4 words/.test(a)) throw new Error("unexpected result");
    await page.screenshot({ path: `${S}/m6-wasm.png` });
  });
  await step("Milestone 8: the page is cross-origin isolated and the module ran in the browser (server run endpoint never called)", async () => {
    const isolated = await page.evaluate(() => globalThis.crossOriginIsolated === true && !!window.moonkale?.wasmHost?.available());
    console.log("\n  crossOriginIsolated + runtime:", isolated, "| server /api/ext/run calls:", serverRuns);
    if (!isolated) throw new Error("not isolated");
    if (serverRuns !== 0) throw new Error("the server ran the module");
    // And with the server endpoint blocked outright, it still works.
    await page.route("**/api/ext/run", (r) => r.abort());
    await ask(`/tool wordcount.top {"source":"folder:${ROOT}","node":"${readme}","n":2}`);
    await page.waitForSelector(".mk-agent-approval", { timeout: 20000 });
    await page.click(".mk-agent-approval button:has-text('Allow')");
    await idle();
    const b = await lastAssistant();
    console.log("  top words:", b.slice(0, 100));
    if (!/sample|edit|me/i.test(b)) throw new Error("browser run failed: " + b.slice(0, 200));
  });
  console.log("\nWASM EXT E2E: PASS");
} catch (e) { console.log("\nFAIL:", e.message); console.log(logs.slice(-10).join("\n")); await page.screenshot({ path: `${S}/m6-fail.png` }); process.exitCode = 1; } finally { await browser.close(); }
