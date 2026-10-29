import { control } from './chrome-controls.mjs';
// ADR-0030: folder code graph defaults to Sophia, with a visible backend.
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
const storeRoot = `${root}/store-truth-fixture`;
const browser = await chromium.launch({args:["--use-angle=swiftshader","--enable-unsafe-swiftshader","--ignore-gpu-blocklist"]});
const page = await browser.newPage({ viewport: {width:1500,height:900} });
const logs = [];
const warningSaves = [];
page.on("request",request=>{if(request.url().includes("/api/sources/apply") && request.postData()?.includes("warnings.sophia")) warningSaves.push(request.postData());});
page.on("console",m => logs.push(m.text()));
try {
  fs.writeFileSync(`${root}/cross_helper.py`, "def cross_plus(x):\n    return x + 1\n");
  fs.writeFileSync(`${root}/cross_caller.py`, "from cross_helper import cross_plus\ndef cross_caller(x):\n    return cross_plus(x)\n");
  fs.writeFileSync(`${root}/Cache.jl`, "module Cache\nconst initial = 1\nend\n");
  await page.goto(`http://127.0.0.1:${process.env.PORT}/`,{waitUntil:"networkidle"});
  await page.waitForSelector(".wb-workspace");
  await control(page, 'click', ".mk-explorer-open button[type=submit]");
  await page.waitForFunction(() => document.querySelector(".wb-status-bar")?.textContent.includes("index:"));
  await page.waitForFunction(() => document.querySelector(".wb-status-bar")?.textContent.includes("Code graph: Sophia cache"));
  await page.waitForFunction(() => document.querySelector(".wb-status-bar")?.textContent.includes("Truth: files"));
  assert.equal(await page.locator(".mk-source-truth").first().textContent(), "Truth: files");
  assert(fs.statSync(`${root}/.moonkale/code.sophia`).size > 0);
  await control(page, 'click', ".wb-tab[id^='wb-tab-graph']");
  await page.waitForSelector(".mk-graph-source option[value^='sophia:']",{state:"attached"});
  const source = await page.$eval(".mk-graph-source option[value^='sophia:']",e => e.value);
  // Default folder view must read the cache without manually picking it.
  assert.equal(await page.$eval(".mk-graph-source", e => e.value), "");
  await page.waitForFunction(() => Number(document.querySelector(".mk-graph-info")?.dataset.nodes) > 0);
  await page.waitForFunction(() => /gl|gpu/i.test(document.querySelector(".mk-graph-info")?.dataset.backend || ""),null,{timeout:30000});
  // WP-22: inspect real cache graph edges through the server boundary.
  const cacheCalls = async () => {
    const response = await page.request.post(`http://127.0.0.1:${process.env.PORT}/api/sources/query`, {
      data: {source, q: {All: {limit:10000,kinds:null}}},
    });
    assert(response.ok());
    const result = await response.json();
    assert(!result.Err, JSON.stringify(result));
    return (result.Ok ?? result).edges.filter(e => e.kind === "Calls" && e.props?.resolved_via?.Text === "import" && e.props?.import_target?.Text === "cross_helper.cross_plus");
  };
  const initialCalls = await cacheCalls();
  assert.equal(initialCalls.length, 1, "cross-file call reaches cache graph");
  assert.equal(initialCalls[0].props.import.Text, "from cross_helper import cross_plus");
  fs.writeFileSync(`${root}/cross_helper.py`, "def cross_plus(x):\n    return x + 77\n");
  await assert.doesNotReject(async () => {
    const deadline = Date.now()+20000;
    while (Date.now()<deadline) {
      const calls=await cacheCalls();
      if (calls.length===1 && calls[0].to!==initialCalls[0].to) return;
      await page.waitForTimeout(200);
    }
    throw Error("cache kept old imported target");
  });
  fs.rmSync(`${root}/cross_helper.py`);
  {
    const deadline = Date.now()+20000;
    while ((await cacheCalls()).length && Date.now()<deadline) await page.waitForTimeout(200);
    assert.equal((await cacheCalls()).length,0,"deleting target retracts cache edge");
  }
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
  // Settings are persisted through the regular settings store and affect
  // graph reads immediately, including the visible status item.
  await page.keyboard.press("Control+,");
  await page.waitForSelector(".mk-settings-code-graph");
  await control(page, 'selectOption', ".mk-settings-code-graph", "in-memory");
  await page.waitForFunction(() => {
    const text = document.querySelector(".wb-status-bar")?.textContent || "";
    return text.includes("Code graph: in-memory index") && !text.includes("Code graph: Sophia cache");
  });
  await control(page, 'click', ".wb-tab[id^='wb-tab-graph']");
  await page.waitForFunction(id => {
    const positions = JSON.parse(window.moonkale.graphViews["mk-graph-panel"].node_positions());
    return Object.keys(positions).length > 0 && !positions[id];
  }, initialId);
  await page.keyboard.press("Control+,");
  await page.waitForSelector(".mk-settings-code-graph");
  await control(page, 'selectOption', ".mk-settings-code-graph", "auto");
  await page.waitForFunction(() => document.querySelector(".wb-status-bar")?.textContent.includes("Code graph: Sophia cache"));
  await control(page, 'click', ".wb-tab[id^='wb-tab-graph']");
  fs.rmSync(`${root}/.moonkale/code.sophia`);
  await control(page, 'click', ".mk-graph-reload");
  await page.waitForFunction(() => document.querySelector(".wb-status-bar")?.textContent.includes("Code graph: in-memory index (fallback:"));
  await page.keyboard.press("Control+Shift+P");
  await control(page, 'fill', ".mk-palette-input", "Check the cache");
  await control(page, 'click', ".mk-palette-item[data-key='codeGraph.checkCache']");
  await page.waitForFunction(() => document.querySelector(".wb-status-bar")?.textContent.includes("Cache problem; rebuilt:"));
  await page.waitForFunction(() => document.querySelector(".wb-status-bar")?.textContent.includes("Code graph: Sophia cache"));
  await page.waitForFunction(id => !!JSON.parse(window.moonkale.graphViews["mk-graph-panel"].node_positions())[id], initialId);
  await page.keyboard.press("Control+Shift+P");
  await control(page, 'fill', ".mk-palette-input", "Check the cache");
  await control(page, 'click', ".mk-palette-item[data-key='codeGraph.checkCache']");
  await page.waitForFunction(() => document.querySelector(".wb-status-bar")?.textContent.includes("Cache check: same"));
  await page.screenshot({path:`${process.env.M1_SHOTS}/sophia-folder.png`});
  // A separate marked folder opens without touching its unreadable store.
  fs.mkdirSync(`${storeRoot}/.moonkale`, {recursive:true});
  fs.writeFileSync(`${storeRoot}/.moonkale/truth.json`, JSON.stringify({truth:"store",since:"2026-10-27"}));
  const authored = "unreadable authored store: preserve these bytes";
  fs.writeFileSync(`${storeRoot}/.moonkale/code.sophia`, authored);
  fs.writeFileSync(`${storeRoot}/owned.py`, "def owned():\n    return 42\n");
  await control(page, 'fill', ".mk-explorer-open input", storeRoot);
  await control(page, 'click', ".mk-explorer-open button[type=submit]");
  await control(page.locator(".mk-explorer-source-name").filter({hasText:"store-truth-fixture"}), 'click');
  await page.waitForFunction(() => {
    const text = document.querySelector(".wb-status-bar")?.textContent || "";
    return text.includes("Truth: Sophia store") && text.includes("Code graph read-only: store editing is not available yet");
  });
  const marked = page.locator(".mk-explorer-source").filter({has: page.locator(".mk-source-label",{hasText:"store-truth-fixture"})});
  assert.equal(await marked.locator(".mk-source-truth").textContent(), "Truth: Sophia store");
  assert.equal(fs.readFileSync(`${storeRoot}/.moonkale/code.sophia`, "utf8"), authored);
  assert(!fs.existsSync(`${storeRoot}/.moonkale/.gitignore`));
  const folderStatus = page.locator(".mk-status-folder");
  assert(!(await folderStatus.textContent()).includes("Truth: files"));
  const tooltip = await folderStatus.locator("..").getAttribute("title");
  assert(tooltip.includes("store-truth-fixture: Truth: Sophia store"));
  assert(tooltip.includes("Truth: files · Code graph: Sophia cache"));
  assert(tooltip.includes("Code graph read-only: store editing is not available yet"));
  await control(page.locator(".mk-explorer-source-name").first(), 'click');
  await page.waitForFunction(() => document.querySelector(".mk-status-folder")?.textContent === "Truth: files · Code graph: Sophia cache");
  await control(page.locator(".mk-explorer-source").first().locator(".mk-tree-file").filter({hasText:"Cache.jl"}), 'click');
  await page.locator(".wb-tab").filter({hasText:"Cache.jl"}).waitFor();
  await control(marked.locator(".mk-tree-file").filter({hasText:"owned.py"}), 'click');
  await page.waitForFunction(() => document.querySelector(".mk-status-folder")?.textContent.includes("Truth: Sophia store"));
  await control(page.locator(".wb-tab").filter({hasText:"Cache.jl"}), 'click');
  await page.waitForFunction(() => document.querySelector(".mk-status-folder")?.textContent === "Truth: files · Code graph: Sophia cache");
  await control(page.locator(".wb-tab").filter({hasText:"owned.py"}), 'click');
  await page.waitForFunction(() => document.querySelector(".mk-status-folder")?.textContent.includes("Truth: Sophia store"));
  await page.screenshot({path:`${process.env.M1_SHOTS}/sophia-folder-truth.png`});
  // SR-150: a real authored store, another writer changes a referenced name,
  // then the user saves the referrer through the editor's regular save path.
  await control(page, 'fill', ".mk-explorer-open input", `${root}/warnings.sophia`);
  await control(page, 'click', ".mk-explorer-open button[type=submit]");
  const authoredSource = page.locator(".mk-explorer-source").filter({has:page.locator(".mk-source-label",{hasText:/^warnings$/})});
  await authoredSource.waitFor();
  await authoredSource.locator('.mk-tree-row[title="ns:sample"]').click();
  await authoredSource.locator('.mk-tree-row[title="name:sample.user"]').click();
  await page.locator('.cm-content:visible, .mk-native-surface:visible').first().waitFor();
  const warningSource = `sophia:${root}/warnings.sophia`;
  const query = await page.request.post(`http://127.0.0.1:${process.env.PORT}/api/sources/query`, {data:{source:warningSource,q:{Text:{dialect:"path",text:"name:sample.helper"}}}});
  const helper = (await query.json()).Ok.nodes[0];
  const fetched = await page.request.post(`http://127.0.0.1:${process.env.PORT}/api/sources/fetch_text`,{data:{source:warningSource,node:helper.id}});
  const [helperText, helperVersion] = (await fetched.json()).Ok;
  const changed = await page.request.post(`http://127.0.0.1:${process.env.PORT}/api/sources/apply`,{data:{source:warningSource,tx:{ops:[{WriteText:{node:helper.id,expected:helperVersion,patch:{Replace:[{start:0,end:[...helperText].length,text:helperText.replace("    x\n","    x + x\n")}]}}}]}}});
  assert((await changed.json()).Ok.results[0].Ok);
  if (await page.locator('.mk-native-surface:visible').count()) await page.locator('.mk-editor-core-input-sink:visible').focus();
  else await page.locator('.cm-content:visible').click();
  await page.keyboard.press('Control+Home');
  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('End');
  await page.keyboard.type(' + x');
  await page.keyboard.press('Control+s');
  await page.waitForFunction(()=>document.querySelector('.wb-status-bar')?.textContent.includes('`sample.helper` now refers to other content'));
  assert(!(await page.locator('.cm-content:visible, .mk-native-surface:visible').first().innerText()).includes('now refers'));
  assert((await page.locator('.wb-status-message').getAttribute('title')).includes('the save uses it'), 'full warning remains readable on hover');
  await page.screenshot({path:`${process.env.M1_SHOTS}/sophia-save-warning.png`});
  console.log("SOPHIA FOLDER E2E: PASS");
} catch(error) {
  console.error(error,logs.slice(-10),warningSaves);
  await page.screenshot({path:`${process.env.M1_SHOTS}/sophia-folder-fail.png`});
  process.exitCode=1;
} finally {
  fs.rmSync(`${root}/Cache.jl`,{force:true});
  fs.rmSync(`${root}/cross_helper.py`,{force:true});
  fs.rmSync(`${root}/cross_caller.py`,{force:true});
  fs.rmSync(storeRoot,{recursive:true,force:true});
  await browser.close();
}
