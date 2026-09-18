// Milestone 4 with a real provider (whatever the server has configured): the model must use a
// tool to answer a question about the folder. Not part of the regular suite (needs a key).
import { firefox } from "playwright";
const S = process.env.M1_SHOTS ?? ".";
const PORT = process.env.PORT ?? 8080;
const browser = await firefox.launch();
const page = await browser.newPage({ viewport: { width: 1500, height: 900 } });
const logs = [];
page.on("console", (m) => { const t = m.text(); if (!t.includes("session[")) logs.push(`[${m.type()}] ${t.slice(0, 300)}`); });
const step = async (n, f) => { process.stdout.write(`- ${n} … `); await f(); console.log("ok"); };
try {
  await step("open folder; provider is not the mock", async () => {
    await page.goto(`http://127.0.0.1:${PORT}/`, { waitUntil: "networkidle" });
    await page.waitForSelector(".wb-workspace");
    await page.click(".mk-explorer-open button[type=submit]");
    await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
    await page.waitForFunction(() => /·/.test(document.querySelector(".mk-agent-provider")?.textContent || ""), null, { timeout: 20000 });
    const p = await page.$eval(".mk-agent-provider", (e) => e.textContent);
    console.log("\n  provider:", p);
    if (/mock/.test(p)) throw new Error("server runs the mock");
  });
  await step("the model answers a folder question through tools", async () => {
    await page.fill(".mk-agent-input", "Which file in the open folder defines the Rust function `add`, and on which line? Use your tools (index.search) and answer with the relative path and line.");
    await page.click(".mk-agent-compose button");
    await page.waitForFunction(() => document.querySelectorAll(".mk-agent-tool").length >= 1, null, { timeout: 90000 });
    await page.waitForFunction(() => !document.querySelector(".mk-agent-compose button[disabled]") && document.querySelectorAll(".mk-agent-assistant").length >= 1, null, { timeout: 180000 });
    const tools = await page.$$eval(".mk-agent-tool .mk-agent-tool-name", (t) => t.map((x) => x.textContent));
    const answer = await page.$$eval(".mk-agent-assistant", (e) => e.map((x) => x.textContent).join(" | "));
    const errors = await page.$$eval(".mk-agent-error", (e) => e.map((x) => x.textContent));
    console.log("\n  tools:", tools.join(", "), "\n  answer:", answer.slice(0, 400), errors.length ? "\n  errors: " + errors.join(" | ") : "");
    await page.screenshot({ path: `${S}/m4-agent-live.png` });
    if (errors.length) throw new Error(errors[0]);
    if (!/main\.rs/.test(answer)) throw new Error("answer does not name src/main.rs");
  });
  console.log("\nAGENT LIVE E2E: PASS");
} catch (e) { console.log("\nFAIL:", e.message); console.log(logs.slice(-10).join("\n")); await page.screenshot({ path: `${S}/m4-fail.png` }); process.exitCode = 1; } finally { await browser.close(); }
