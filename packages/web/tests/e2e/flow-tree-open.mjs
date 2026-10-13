// P-183: a .flow.json opened from the explorer tree keeps its tab in front
// through the first edit. The shell's command effect used to replay the
// last command whenever a signal it read changed: the document turning
// dirty re-ran the rail's ShowPanel(extensions) and sent the editor tab to
// the background (0×0, aria-selected=false) while its model took the edit.
import { chromium } from "playwright";
import fs from "node:fs";
const PORT = process.env.PORT, ROOT = process.env.M1_ROOT;
fs.writeFileSync(`${ROOT}/repro.flow.json`, JSON.stringify({ version: 1, blocks: [{ id: "in_1", kind: "lux/input", x: 60, y: 60, params: {} }], wires: [] }));
const browser = await chromium.launch({ args: ["--use-angle=swiftshader", "--enable-unsafe-swiftshader", "--ignore-gpu-blocklist"] });
const page = await browser.newPage({ viewport: { width: 1500, height: 900 } });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const state = () => page.evaluate(() => { const tabs=[...document.querySelectorAll("[role=tab]")].map(t=>`${t.textContent.trim().slice(0,24)}|${t.getAttribute("aria-selected")}|${t.getAttribute("aria-controls")}`); const c = [...document.querySelectorAll(".mk-flow-canvas")].map((e) => { const r = e.getBoundingClientRect(); return `${Math.round(r.width)}x${Math.round(r.height)}`; }); const s = document.querySelector(".mk-flow-status"); return { canvases: c, blocks: s?.dataset.blocks, tabs }; });
await page.goto(`http://127.0.0.1:${PORT}/`, { waitUntil: "networkidle" });
await page.waitForSelector(".wb-workspace");
await page.click(".mk-explorer-open button[type=submit]");
await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
await page.click("#mk-rail-extensions");
await page.waitForSelector(".mk-settings-ext");
await page.click(".mk-settings-ext:has(.mk-settings-ext-name:text-is('Flow editor')) > label input");
await page.click(".mk-settings-ext:has(.mk-settings-ext-name:text-is('Lux.jl library')) > label input");
await sleep(1000);
if (!(await page.isVisible(".mk-explorer"))) await page.click("#mk-rail-explorer");
await page.locator(".mk-tree-file").filter({ hasText: "repro.flow.json" }).click();
await page.waitForSelector(".mk-flow-palette-block:visible", { timeout: 20000 });
if ((await state()).blocks !== "1") throw new Error("the fixture did not load");
await page.click(".mk-flow-palette-block:visible >> nth=1");
await sleep(1500);
const after = await state();
const visible = after.canvases.some((c) => c !== "0x0");
if (visible && after.blocks === "2") console.log("FLOW TREE OPEN E2E: PASS"); else { console.log("FAIL: the tab went to the background " + JSON.stringify(after)); process.exitCode = 1; }
await browser.close();
