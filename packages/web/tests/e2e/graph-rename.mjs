import { control } from './chrome-controls.mjs';
// Experimental graph operations through the real renderer, remote planner and editor.
import { chromium } from "playwright";
import fs from "node:fs";
import { pathToFileURL } from "node:url";
const root = process.env.M1_ROOT;
if (!root) throw Error("M1_ROOT must name a disposable fixture");
const file = `${root}/graph-edit.rs`;
const original = '// 😀 𐐀() stays in comments\r\n/* 😀 */ pub fn 𐐀() { if false { 𐐀(); } }\r\npub fn second() { 𐐀(); { let 𐐀 = 1; let _ = 𐐀; } let _: u32 = ""; }\r\n#[path = "graph-consumer.rs"] mod consumer;\r\n';
fs.appendFileSync(`${root}/Cargo.toml`, '\n[lib]\npath = "graph-edit.rs"\n');
fs.writeFileSync(`${root}/graph-consumer.rs`, 'pub fn caller() { crate::second(); }\n');
fs.writeFileSync(file, original);
const browser = await chromium.launch({ args: ["--use-angle=swiftshader", "--enable-unsafe-swiftshader", "--ignore-gpu-blocklist"] });
const context = await browser.newContext({ viewport: { width: 1500, height: 1100 } });
const engine = process.env.STRUCTURAL_EDITOR ?? 'native';
await context.addInitScript(engine => localStorage.setItem('moonkale.settings', JSON.stringify({ editor: { implementation: engine, markdown_rich: false } })), engine);
const page = await context.newPage();
const base = `http://127.0.0.1:${process.env.PORT ?? 8080}`;
const test = id => page.getByTestId(`structural-${id}`);
const input = () => engine === 'native' ? page.locator('.mk-crust[data-editor="rust"]').filter({ visible: true }).locator('.mk-editor-core-input-sink') : page.locator('.cm-content').filter({ visible: true });
const step = async (name, run) => { await run(); console.log(`ok: ${name}`); };
const api = async (path, data) => {
  const response = await page.request.post(`${base}/api/${path}`, { data });
  if (!response.ok()) throw Error(`${path}: ${response.status()} ${await response.text()}`);
  const value = await response.json();
  if (value.Err) throw Error(JSON.stringify(value.Err));
  return value.Ok ?? value;
};
let index;
async function selectNode(name) {
  await control(page.locator(".wb-tab[id^='wb-tab-graph']"), 'click');
  if (await test('discard').isVisible()) await control(test('discard'), 'click');
  const deadline = Date.now() + 30_000;
  while (Date.now() < deadline) {
    const result = await api('sources/query', { source: index.id, q: { Text: {
      dialect: 'code-graph', text: JSON.stringify([false, { All: { limit: 3000, kinds: null } }]),
    } } });
    const nodes = result.nodes.filter(n => n.kind?.Custom === 'Definition' && n.source.startsWith('sophia:'));
    // Resolve through the same host protocol as the controls; click the cache id.
    let node, mapped;
    for (const candidate of nodes) {
      const match = await api('sources/query', { source: index.id, q: { Text: {
        dialect: 'index-node', text: JSON.stringify(candidate),
      } } });
      if (match.nodes[0]?.native_key === `graph-edit.rs#fn:${name}`) {
        node = candidate; mapped = match.nodes[0]; break;
      }
    }
    if (!node) { await page.waitForTimeout(100); continue; }
    if (node.id === mapped.id) throw Error('expected distinct cache and index ids');
    await page.evaluate(async nodes => {
      Object.values(window.moonkale.graphViews)[0].set_graph(JSON.stringify({
        nodes: nodes.map((node, i) => ({ id: node.id, label: node.label, kind: 'symbol', pos: [i * 100, 0], pinned: true })), edges: [],
      }));
      await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    }, nodes);
    const point = await page.evaluate(id => Array.from(Object.values(window.moonkale.graphViews)[0].node_screen_position(id) ?? []), node.id);
    const box = await page.locator('.mk-graph-overlay').boundingBox();
    if (box && point.length === 2) {
      // Separate successive selections from the renderer’s 350 ms double-click gesture.
      await page.waitForTimeout(400);
      await page.mouse.click(box.x + point[0], box.y + point[1]);
      try {
        await page.waitForFunction(id => document.querySelector('[data-testid="structural-controls"]')?.dataset.selectedNode === id, mapped.id,
          { timeout: Math.max(1, Math.min(1000, deadline - Date.now())) });
        return mapped;
      } catch (error) { if (error.name !== 'TimeoutError') throw error; }
    }
  }
  throw Error(`cache definition ${name} did not map to the selected index node`);
}
async function begin(name) {
  await selectNode(name);
  await control(page.getByRole('button', { name: 'Edit Rust function (experimental)', exact: true }), 'click');
  await page.waitForFunction(() => document.querySelector('[data-testid="structural-function"]')?.options.length > 1);
  await test('preview').waitFor({ state: 'visible' });
}
async function previewName(name) {
  await control(test('name'), 'fill', name);
  await control(test('preview'), 'click');
  await test('proposed').waitFor();
}
async function openSource() {
  await control(page.locator('.wb-tab').filter({ hasText: 'graph-edit.rs' }), 'click');
  await input().waitFor();
}
async function save(expected) {
  await page.locator('.mk-editor-dirty').filter({ visible: true }).waitFor();
  await input().focus(); await page.keyboard.press('Control+s');
  await page.waitForFunction(() => ![...document.querySelectorAll('.mk-editor-dirty')].some(e => e.offsetParent !== null));
  for (let attempt = 0; attempt < 100 && fs.readFileSync(file, 'utf8') !== expected; attempt++) await page.waitForTimeout(50);
  if (fs.readFileSync(file, 'utf8') !== expected) throw Error(`saved bytes differ: ${JSON.stringify({expected, actual: fs.readFileSync(file, 'utf8')})}`);
}
const sent = [];
let holdRename = false, renameId, heldReply, onHeld;
let injectAt = 0, injectId, injectTarget;
const decode = payload => { const frame = JSON.parse(String(payload)); return typeof frame === 'string' ? JSON.parse(frame) : frame; };
await page.routeWebSocket(/\/api\/lsp/, socket => {
  const server = socket.connectToServer();
  socket.onMessage(message => {
    const frame = decode(message);
    if (frame.method === 'textDocument/rename') {
      sent.push(frame); if (holdRename) renameId = frame.id;
      if (sent.length === injectAt) injectId = frame.id;
    }
    server.send(message);
  });
  server.onMessage(message => {
    const frame = decode(message);
    if (injectId !== undefined && frame.id === injectId) {
      injectId = undefined;
      frame.result.documentChanges ??= [];
      frame.result.documentChanges.push({textDocument:{uri:injectTarget,version:null},edits:[{range:{start:{line:0,character:0},end:{line:0,character:0}},newText:'// unexpected\n'}]});
      const encoded = JSON.stringify(frame);
      socket.send(typeof JSON.parse(String(message)) === 'string' ? JSON.stringify(encoded) : encoded);
    } else if (holdRename && renameId !== undefined && frame.id === renameId) {
      heldReply = () => socket.send(message); onHeld();
    } else socket.send(message);
  });
});
async function semanticPreview(name, index = '0') {
  await control(test('function'), 'selectOption', index);
  await control(test('operation'), 'selectOption', 'rename');
  await previewName(name);
}
try {
  await step('real Rust crate, graph and rust-analyzer ready', async () => {
    await page.goto(base, { waitUntil: 'networkidle' });
    await page.waitForSelector('.wb-workspace');
    await control(page, 'click', '.mk-explorer-open button[type=submit]');
    await page.waitForFunction(() => document.querySelector('.wb-status-bar').textContent.includes('index:'));
    index = (await api('sources/list', {})).find(source => source.family === 'Index');
    await begin('𐐀'); await openSource();
    await page.locator(engine === 'native' ? '.mk-native-diagnostic-error' : '.cm-lint-marker-error').first().waitFor({ timeout: 120000 });
  });
  await step('dismissal ignores a delayed semantic rename response', async () => {
    await begin('𐐀'); await control(test('function'), 'selectOption', '0');
    await control(test('operation'), 'selectOption', 'rename'); await control(test('name'), 'fill', 'discarded');
    const captured = new Promise(resolve => { onHeld = resolve; });
    holdRename = true; await control(test('preview'), 'click');
    await Promise.race([captured, new Promise((_, reject) => setTimeout(() => reject(Error('no held rename reply')), 15000))]);
    await control(test('discard'), 'click'); holdRename = false; heldReply();
    await page.waitForTimeout(250);
    if (await test('proposed').count() || fs.readFileSync(file, 'utf8') !== original) throw Error('delayed rename restored dismissed state');
  });
  const extra = '\r\nfn unsaved_user() { 𐐀(); }';
  const changed = original.replace('pub fn 𐐀()', 'pub fn 新名()').replace('if false { 𐐀();', 'if false { 新名();').replace('second() { 𐐀();', 'second() { 新名();') + extra.replace('𐐀();', '新名();');
  await step('rename includes unsaved references and respects shadowing/comments', async () => {
    await openSource(); await input().focus(); await page.keyboard.press('Control+End');
    await page.keyboard.press('Enter'); await page.keyboard.type('fn unsaved_user() { 𐐀(); }');
    await begin('𐐀'); await semanticPreview('新名');
    if (await test('proposed').textContent() !== changed) throw Error('semantic preview differs from exact expected bytes');
    if (fs.readFileSync(file, 'utf8') !== original) throw Error('preview wrote source');
    const request = sent.at(-1)?.params;
    if (request?.position.line !== 1 || request.position.character !== 16) throw Error(`wrong declaration position: ${JSON.stringify(request)}`);
    // A different open buffer participates in resolution. Even editing and
    // restoring equal text must invalidate the retained resolver guard.
    await control(page.locator('.mk-tree-file').filter({ hasText: 'graph-consumer.rs' }), 'click');
    await input().waitFor(); await input().focus(); await page.keyboard.press('Control+Home'); await page.keyboard.type('// changed ');
    await control(page.locator(".wb-tab[id^='wb-tab-graph']"), 'click');
    if (!await test('apply').isDisabled()) throw Error('other-buffer change left rename applicable');
    if (await test('proposed').textContent() !== changed) throw Error('other-buffer conflict lost preview');
    await control(page.locator('.wb-tab').filter({ hasText: 'graph-consumer.rs' }), 'click');
    await input().focus(); await page.keyboard.press('Control+a'); await page.keyboard.type('pub fn caller() { crate::second(); }'); await page.keyboard.press('Enter');
    await control(page.locator(".wb-tab[id^='wb-tab-graph']"), 'click');
    if (!await test('apply').isDisabled()) throw Error('other-buffer equal-text ABA passed the guard');
    await control(test('recompute'), 'click');
    await page.waitForFunction(() => document.querySelector('[data-testid="structural-function"]')?.value === '');
    await semanticPreview('新名');
    if (await test('proposed').textContent() !== changed) throw Error('fresh plan differs after resolver conflict');
    await control(test('apply'), 'click'); await openSource(); await save(changed);
    await page.keyboard.press('Control+z'); await save(original + extra);
    await page.keyboard.press('Control+y'); await save(changed);
  });
  await step('new authoritative targets and outside-folder plans are refused in full', async () => {
    for (const [offset, target, expected] of [
      [2, pathToFileURL(`${root}/graph-consumer.rs`).href, 'set of affected files changed'],
      [1, 'file:///outside-moonkale/consumer.rs', 'inside the current folder'],
    ]) {
      await begin('新名'); await control(test('function'), 'selectOption', '0'); await control(test('operation'), 'selectOption', 'rename');
      injectAt = sent.length + offset; injectTarget = target;
      await control(test('name'), 'fill', 'not_applied'); await control(test('preview'), 'click');
      await test('rename-error').waitFor();
      if (!(await test('rename-error').textContent()).includes(expected)) throw Error('wrong target preflight refusal');
      if (await test('proposed').count() || !await test('apply').isDisabled()) throw Error('incomplete plan retained an applicable subset');
      if (fs.readFileSync(file,'utf8') !== changed || fs.readFileSync(`${root}/graph-consumer.rs`,'utf8') !== 'pub fn caller() { crate::second(); }\n') throw Error('rejected target plan wrote source');
    }
  });
  await step('multi-file preview, grouped keyboard/button history and separate saving', async () => {
    await begin('second');
    await control(test('function'), 'selectOption', '1'); await control(test('operation'), 'selectOption', 'rename');
    await control(test('name'), 'fill', 'other'); await control(test('preview'), 'click');
    await page.waitForFunction(() => document.querySelectorAll('[data-testid="structural-file"]').length === 2);
    const previews = await test('proposed').allTextContents();
    if (!previews.some(text => text.includes('pub fn other()')) || !previews.some(text => text.includes('crate::other()'))) throw Error('missing complete multi-file preview');
    const consumer = `${root}/graph-consumer.rs`;
    const caller = 'pub fn caller() { crate::second(); }\n';
    if (fs.readFileSync(file,'utf8') !== changed || fs.readFileSync(consumer,'utf8') !== caller) throw Error('preview saved source');
    await control(test('apply'), 'click');
    await test('save-all').waitFor(); await control(test('save-all'), 'click');
    await page.waitForFunction(() => document.querySelectorAll('[data-testid="structural-save-result"]').length === 2);
    const both = changed.replace('pub fn second()', 'pub fn other()');
    if (fs.readFileSync(file,'utf8') !== both || fs.readFileSync(consumer,'utf8') !== caller.replace('second','other')) throw Error('Save affected files did not save both');
    // A newer edit in either file blocks the entire operation.
    await openSource(); await input().focus(); await page.keyboard.press('Control+End'); await page.keyboard.type(' ');
    await control(page.locator(".wb-tab[id^='wb-tab-graph']"), 'click'); await control(test('undo'), 'click'); await test('group-error').waitFor();
    if (fs.readFileSync(consumer,'utf8') !== caller.replace('second','other')) throw Error('blocked undo wrote another file');
    await openSource(); await input().focus(); await page.keyboard.press('Control+z'); // only newer space
    await page.keyboard.press('Control+z'); // whole rename
    await control(page.locator(".wb-tab[id^='wb-tab-graph']"), 'click');
    await page.waitForFunction(() => !document.querySelector('[data-testid="structural-redo"]')?.disabled);
    if (fs.readFileSync(file,'utf8') !== both) throw Error('Undo rename wrote disk');
    await control(test('redo'), 'click');
    await page.waitForFunction(() => !document.querySelector('[data-testid="structural-undo"]')?.disabled);
    // Undo from the other document must also restore both files.
    await control(page.locator('.wb-tab').filter({hasText:'graph-consumer.rs'}), 'click');
    await input().focus(); await page.keyboard.press('Control+z');
    await control(page.locator(".wb-tab[id^='wb-tab-graph']"), 'click');
    await page.waitForFunction(() => !document.querySelector('[data-testid="structural-redo"]')?.disabled);
    await openSource(); await save(changed);
    await control(page.locator('.wb-tab').filter({hasText:'graph-consumer.rs'}), 'click');
    await input().focus(); await page.keyboard.press('Control+s');
    await page.waitForFunction(() => ![...document.querySelectorAll('.mk-editor-dirty')].some(e => e.offsetParent !== null));
    if (fs.readFileSync(consumer,'utf8') !== caller) throw Error('group undo did not restore caller');
  });
  await step('semantic preview survives conflict and requires recomputation', async () => {
    await begin('新名'); await control(test('function'), 'selectOption', '0'); await semanticPreview('final_name');
    const preview = await test('proposed').textContent();
    await openSource(); await input().focus(); await page.keyboard.press('Control+Home'); await page.keyboard.type('// newer ');
    await control(page.locator(".wb-tab[id^='wb-tab-graph']"), 'click');
    if (!await test('apply').isDisabled()) throw Error('stale rename can apply');
    await test('current').waitFor();
    if (await test('proposed').textContent() !== preview) throw Error('conflict lost preview');
    await control(test('recompute'), 'click');
    await page.waitForFunction(() => document.querySelector('[data-testid="structural-function"]')?.value === '');
    await semanticPreview('final_name');
    const recomputed = await test('proposed').textContent();
    if (!recomputed.startsWith('// newer ')) throw Error('recompute dropped newer text');
    await control(test('discard'), 'click');
    if (fs.readFileSync(file, 'utf8') !== changed) throw Error('discard wrote source');
  });
  console.log(`GRAPH RENAME E2E: PASS (${engine})`);
} catch (error) {
  console.error('FAIL:', error.stack || error);
  await page.screenshot({ path: `${process.env.M1_SHOTS ?? '.'}/graph-rename-fail.png` }).catch(() => {});
  process.exitCode = 1;
} finally { await browser.close(); }
