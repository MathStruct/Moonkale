// WP-21: the feature-gated folder cache is an additional source in the real UI.
import { chromium } from "playwright";
import fs from "node:fs";
import { createHash } from "node:crypto";
// Core's public deterministic NodeId format (UUID v5, source + NUL + key).
function nameId(source,key) {
  const bytes = createHash("sha1").update(Buffer.from("moonkale-node-01")).update(source).update(Buffer.from([0])).update(key).digest().subarray(0,16);
  bytes[6]=(bytes[6]&15)|80; bytes[8]=(bytes[8]&63)|128;
  const h=bytes.toString("hex"); return `${h.slice(0,8)}-${h.slice(8,12)}-${h.slice(12,16)}-${h.slice(16,20)}-${h.slice(20)}`;
}
import assert from "node:assert/strict";
const root = process.env.M1_ROOT;
const browser = await chromium.launch({args:["--use-angle=swiftshader","--enable-unsafe-swiftshader","--ignore-gpu-blocklist"]});
const page = await browser.newPage({ viewport: {width:1500,height:900} });
const logs = [];
page.on("console",m => logs.push(m.text()));
try {
  fs.writeFileSync(`${root}/Cache.jl`, "module Cache\nconst initial = 1\nend\n");
  await page.goto(`http://127.0.0.1:${process.env.PORT}/`,{waitUntil:"networkidle"});
  await page.waitForSelector(".wb-workspace");
  await page.click(".mk-explorer-open button[type=submit]");
  await page.waitForFunction(() => document.querySelector(".wb-status-bar")?.textContent.includes("index:"));
  assert(fs.statSync(`${root}/.moonkale/code.sophia`).size > 0);
  await page.click(".wb-tab[id^='wb-tab-graph']");
  await page.waitForSelector(".mk-graph-source option[value^='sophia:']",{state:"attached"});
  const source = await page.$eval(".mk-graph-source option[value^='sophia:']",e => e.value);
  await page.selectOption(".mk-graph-source",source);
  await page.waitForFunction(() => Number(document.querySelector(".mk-graph-info")?.dataset.nodes) > 0);
  await page.waitForFunction(() => /gl|gpu/i.test(document.querySelector(".mk-graph-info")?.dataset.backend || ""),null,{timeout:30000});
  // The names/definitions must survive the Graph panel's data-kind filter
  // even though a folder store declares Files truth.
  const initialId = nameId(source,"name:jl.Cache.initial");
  await page.waitForFunction(id => !!JSON.parse(window.moonkale.graphViews["mk-graph-panel"].node_positions())[id],initialId);
  const count = await page.$eval(".mk-graph-info",e => Number(e.dataset.nodes));
  assert(count >= 5, `folder graph has only ${count} nodes`);
  // A new definition must reach the additional Source without Reload.
  await new Promise(r => setTimeout(r,1500));
  fs.writeFileSync(`${root}/Cache.jl`, "module Cache\nconst initial = 1\nconst added = 2\nend\n");
  const addedId = nameId(source,"name:jl.Cache.added");
  await page.waitForFunction(id => !!JSON.parse(window.moonkale.graphViews["mk-graph-panel"].node_positions())[id],addedId,{timeout:20000});
  await page.screenshot({path:`${process.env.M1_SHOTS}/sophia-folder.png`});
  console.log("SOPHIA FOLDER E2E: PASS");
} catch(error) {
  console.error(error,logs.slice(-10));
  await page.screenshot({path:`${process.env.M1_SHOTS}/sophia-folder-fail.png`});
  process.exitCode=1;
} finally {
  fs.rmSync(`${root}/Cache.jl`,{force:true});
  await browser.close();
}
