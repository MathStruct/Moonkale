import { firefox } from "playwright";
const S = process.env.M1_SHOTS ?? ".";
const browser = await firefox.launch();
const page = await browser.newPage({ viewport: { width: 1400, height: 900 } });
const logs = [];
page.on("console", (m) => { const t = m.text(); if (!t.includes("session[")) logs.push(`[${m.type()}] ${t.slice(0, 200)}`); });
const step = async (n, f) => { process.stdout.write(`- ${n} … `); await f(); console.log("ok"); };
const text = () => page.$$eval(".xterm-rows > div", (rows) => rows.map((r) => r.textContent).join("\n"));
try {
  await step("open folder, Terminal panel present (bottom tile)", async () => {
    await page.goto("http://127.0.0.1:8080/", { waitUntil: "networkidle" });
    await page.waitForSelector(".wb-workspace");
    await page.click(".mk-explorer-open button[type=submit]");
    await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
    await page.waitForSelector(".mk-term");
  });
  await step("View → New Terminal starts a remote shell (server PTY over websocket)", async () => {
    await page.click(".mk-menu-button:has-text('View')");
    await page.click(".mk-menu-item:has-text('New Terminal')");
    await page.waitForSelector(".mk-term-tab-active", { timeout: 15000 });
    await page.waitForSelector(".xterm-rows", { timeout: 15000 });
    await page.waitForFunction(() => document.querySelector(".xterm-rows")?.textContent.trim().length > 0, null, { timeout: 15000 }); // prompt
  });
  await step("typing a command produces output", async () => {
    await page.click(".mk-term-session");
    await page.keyboard.type("echo moon_$((20+3)) && pwd\n");
    await page.waitForFunction(() => /moon_23/.test(document.querySelector(".xterm-rows")?.textContent || ""), null, { timeout: 15000 });
    const t = await text();
    console.log("\n  cwd line:", (t.split("\n").find((l) => l.includes("m2root")) || "?").trim());
    await page.screenshot({ path: `${S}/m3-terminal.png` });
  });
  await step("second terminal via +, tabs switch, close works", async () => {
    await page.click(".mk-term-new");
    await page.waitForFunction(() => document.querySelectorAll(".mk-term-tab:not(.mk-term-new)").length === 2, null, { timeout: 15000 });
    await page.click(".mk-term-tab-active .mk-term-close");
    await page.waitForFunction(() => document.querySelectorAll(".mk-term-tab:not(.mk-term-new)").length === 1, null, { timeout: 5000 });
  });
  await step("Ctrl+click on a path:line in the output opens the file", async () => {
    await page.click(".mk-term-session");
    await page.keyboard.type("printf 'error at src/main.rs:1:4\\n'\n");
    await page.waitForFunction(() => /src\/main\.rs:1:4/.test(document.querySelector(".xterm-rows")?.textContent || ""), null, { timeout: 15000 });
    const row = await page.evaluateHandle(() => [...document.querySelectorAll(".xterm-rows > div")].find((r) => r.textContent.includes("src/main.rs:1:4")));
    // Dispatch the click (headless Firefox + xterm swallow a real modified mouse click).
    await row.asElement().evaluate((r) => {
      const rect = r.getBoundingClientRect();
      r.dispatchEvent(new MouseEvent("click", { bubbles: true, ctrlKey: true, clientX: rect.left + 20, clientY: rect.top + rect.height / 2 }));
    });
    try {
      await page.waitForFunction(() => document.querySelector(".mk-titlebar-title").textContent.startsWith("src/main.rs"), null, { timeout: 15000 });
    } catch (e) {
      console.log("\n  status:", await page.$eval(".wb-status-bar", (el) => el.textContent.trim()), "| row text:", await row.evaluate((r) => r.textContent.trim()));
      throw e;
    }
  });
  console.log("\nTERMINAL E2E: PASS");
} catch (e) { console.log("\nFAIL:", e.message); console.log(logs.slice(-8).join("\n")); await page.screenshot({ path: `${S}/m3-fail.png` }); process.exitCode = 1; } finally { await browser.close(); }
