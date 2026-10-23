// Production TablePanel over a hermetic source; run the typed-cells fixture on 8112.
import { chromium } from "playwright";
const browser = await chromium.launch({headless:true});
const page = await browser.newPage({viewport:{width:1200,height:800}});
try {
  await page.goto(`http://127.0.0.1:${process.env.PORT ?? 8112}/`);
  await page.waitForSelector(".mk-table-grid");
  const date = () => page.locator(".mk-table-cell").first().innerText();
  if (!(await date()).includes("2026-10-23 10:30:00 +02:00")) throw new Error("English date");
  await page.click("#lang-de");
  await page.waitForFunction(() => document.querySelector(".mk-table-cell").textContent.includes("23.10.2026"));
  await page.click("#lang-zh-CN");
  await page.waitForFunction(() => document.querySelector(".mk-table-cell").textContent.includes("2026年10月23日"));
  if (await page.locator(".mk-cell-vector polyline").count() !== 1) throw new Error("sparkline missing");
  await page.click(".mk-cell-ref");
  await page.waitForFunction(() => document.querySelector("#active").dataset.node.length > 0);
  console.log("TABLE TYPED E2E: PASS");
} finally { await browser.close(); }
