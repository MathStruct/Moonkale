// Milestone 6 measurement: layout step time of the wasm renderer in Firefox (no GPU needed).
// Prints ms/step for 1k, 10k, 50k and 100k nodes. Not part of the pass/fail suite.
import { firefox } from "playwright";
const PORT = process.env.PORT ?? 8080;
const browser = await firefox.launch();
const page = await browser.newPage();
await page.goto(`http://127.0.0.1:${PORT}/`, { waitUntil: "networkidle" });
await page.waitForSelector(".wb-workspace");
// The renderer module is loaded by the Graph panel; grab its URL from the panel's script once mounted.
await page.click(".mk-explorer-open button[type=submit]");
await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
await page.waitForFunction(() => document.querySelector(".mk-graph-info")?.dataset.module === "loaded", null, { timeout: 30000 });
const url = await page.evaluate(() => [...document.querySelectorAll("script, link")].map((e) => e.src || e.href).find((u) => /graph_render.*\.js/.test(u || "")) || null);
const results = await page.evaluate(async () => {
  // The module was imported dynamically by the panel; import it again from the same URL the panel used.
  const src = performance.getEntriesByType("resource").map((r) => r.name).find((n) => /graph_render.*\.js$/.test(n));
  const mod = await import(src);
  const out = {};
  for (const n of [1000, 10000, 50000, 100000]) {
    const steps = n >= 50000 ? 3 : 10;
    out[n] = +mod.bench_layout(n, steps).toFixed(1);
  }
  return out;
});
console.log("wasm layout ms/step:", JSON.stringify(results));
await browser.close();
