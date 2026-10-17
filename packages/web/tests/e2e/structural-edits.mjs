// Experimental graph operations through the real renderer, remote planner and editor.
import { chromium } from "playwright";
import fs from "node:fs";
const root = process.env.M1_ROOT;
if (!root) throw Error("M1_ROOT must name a disposable fixture");
const file = `${root}/graph-edit.rs`;
const original = '// 😀\r\nfn 𐐀() { let _ = "<script>"; }\r\n\r\n/// docs\r\nfn second() {}\r\n';
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
async function definitions() {
  const result = await api('sources/query', { source: index.id, q: { All: { limit: 3000, kinds: null } } });
  return result.nodes.filter(node => node.native_key.startsWith('graph-edit.rs#fn:'));
}
async function selectNode(name) {
  await page.locator(".wb-tab[id^='wb-tab-graph']").click();
  // Saves publish index/graph refreshes asynchronously. A refresh can replace
  // our pinned fixture between set_graph and the click (CI run 39050073252,
  // attempt 1, CodeMirror). Retry the real pointer selection with fresh index
  // IDs/coordinates; retain the exact selected-node assertion and total bound.
  const deadline = Date.now() + 30_000;
  while (true) {
    const nodes = await definitions();
    const node = nodes.find(node => node.native_key === `graph-edit.rs#fn:${name}`);
    if (!node) throw Error(`missing definition ${name}`);
    await page.evaluate(async nodes => {
      Object.values(window.moonkale.graphViews)[0].set_graph(JSON.stringify({
        nodes: nodes.map((node, i) => ({ id: node.id, label: node.label, kind: 'symbol', pos: [i * 100, 0], pinned: true })), edges: [],
      }));
      await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    }, nodes);
    const point = await page.evaluate(id => Array.from(Object.values(window.moonkale.graphViews)[0].node_screen_position(id) ?? []), node.id);
    const box = await page.locator('.mk-graph-overlay').boundingBox();
    if (box && point.length === 2) {
      await page.mouse.click(box.x + point[0], box.y + point[1]);
      try {
        await page.waitForFunction(id => document.querySelector('[data-testid="structural-controls"]')?.dataset.selectedNode === id, node.id,
          { timeout: Math.max(1, Math.min(1000, deadline - Date.now())) });
        return node;
      } catch (error) {
        if (error.name !== 'TimeoutError' || Date.now() >= deadline) throw error;
      }
    } else if (Date.now() >= deadline) {
      throw Error(`graph node ${name} has no screen position`);
    }
  }
}
async function begin(name) {
  await selectNode(name);
  await page.getByRole('button', { name: 'Edit Rust function (experimental)', exact: true }).click();
  await page.waitForFunction(() => document.querySelector('[data-testid="structural-function"]')?.options.length > 1);
  await test('preview').waitFor({ state: 'visible' });
}
async function previewName(name) {
  await test('name').fill(name);
  await test('preview').click();
  await test('proposed').waitFor();
}
async function openSource() {
  await page.locator('.wb-tab').filter({ hasText: 'graph-edit.rs' }).click();
  await input().waitFor();
}
async function save(expected) {
  await page.locator('.mk-editor-dirty').filter({ visible: true }).waitFor();
  await input().focus(); await page.keyboard.press('Control+s');
  await page.waitForFunction(() => ![...document.querySelectorAll('.mk-editor-dirty')].some(e => e.offsetParent !== null));
  for (let attempt = 0; attempt < 100 && fs.readFileSync(file, 'utf8') !== expected; attempt++) await page.waitForTimeout(50);
  if (fs.readFileSync(file, 'utf8') !== expected) throw Error(`saved bytes differ: ${JSON.stringify({expected, actual: fs.readFileSync(file, 'utf8')})}`);
}
try {
  await step('external replacements never split UTF-16 pairs', async () => {
    const probe = await context.newPage();
    try {
      await probe.setContent('<div id="probe"></div>');
      await probe.addScriptTag({ path: `${process.env.MOONKALE_REPO}/packages/code-view/assets/codemirror.js` });
      const results = await probe.evaluate(() => {
        const el = document.querySelector('#probe'), cm = window.moonkale.codemirror;
        return ['𐐁', '𠐀'].map(name => {
          const batches = [];
          cm.mount(el, 'fn 𐐀() {}\r\n', changes => batches.push(changes));
          cm.setText(el, `fn ${name}() {}\r\n`);
          const text = cm.getText(el); cm.destroy(el);
          return { batches, text, name };
        });
      });
      for (const {batches, text, name} of results) {
        if (JSON.stringify(batches) !== JSON.stringify([[{from: 3, to: 5, insert: name}]])) throw Error(`split surrogate pair: ${JSON.stringify(batches)}`);
        if (text !== `fn ${name}() {}\r\n`) throw Error('replacement text changed');
      }
    } finally { await probe.close(); }
  });
  await step('experimental build, real source and graph ready', async () => {
    await page.goto(base, { waitUntil: 'networkidle' });
    await page.waitForSelector('.wb-workspace');
    await page.click('.mk-explorer-open button[type=submit]');
    await page.waitForFunction(() => document.querySelector('.wb-status-bar').textContent.includes('index:'));
    index = (await api('sources/list', {})).find(source => source.family === 'Index');
    if (!index) throw Error('missing index');
    await page.locator(".wb-tab[id^='wb-tab-graph']").click();
    await page.waitForFunction(() => /gl|gpu/i.test(document.querySelector('.mk-graph-info')?.dataset.backend || ''));
  });
  await step('preview and discard do not mutate or execute source', async () => {
    await begin('𐐀');
    await previewName('新名');
    if (!(await test('proposed').textContent()).includes('fn 新名()')) throw Error('missing preview');
    if (await test('proposed').locator('script').count()) throw Error('source interpreted as HTML');
    if (fs.readFileSync(file, 'utf8') !== original) throw Error('preview saved');
    await test('discard').click();
    if (fs.readFileSync(file, 'utf8') !== original) throw Error('discard changed source');
  });
  await step('dismissal ignores a late planner response', async () => {
    await begin('𐐀');
    let release, seen;
    const gate = new Promise(resolve => { release = resolve; });
    const captured = new Promise(resolve => { seen = resolve; });
    await page.route('**/api/structural/plan', async route => {
      const response = await route.fetch(); seen(); await gate; await route.fulfill({ response });
    });
    await test('name').fill('late'); await test('preview').click(); await captured;
    await test('discard').click(); release();
    await page.waitForTimeout(300); await page.unroute('**/api/structural/plan');
    if (await test('proposed').count()) throw Error('late response restored discarded preview');
    if (fs.readFileSync(file, 'utf8') !== original) throw Error('late response changed source');
  });
  const renamed = original.replace('fn 𐐀()', 'fn 𐐁()');
  await step('apply is unsaved and shares editor undo/redo and save', async () => {
    await begin('𐐀'); await previewName('𐐁'); await test('apply').click();
    if (fs.readFileSync(file, 'utf8') !== original) throw Error('apply autosaved');
    await openSource(); await save(renamed);
    await page.keyboard.press('Control+z'); await save(original);
    await page.keyboard.press('Control+y'); await save(renamed);
  });
  await step('move preview remains recoverable after concurrent unsaved editing', async () => {
    await begin('second');
    await test('operation').selectOption('move');
    await test('before').selectOption('0');
    await test('preview').click(); await test('proposed').waitFor();
    const proposed = await test('proposed').textContent();
    if (proposed.indexOf('fn second()') > proposed.indexOf('fn 𐐁()')) throw Error('move preview order wrong');
    await openSource(); await input().focus(); await page.keyboard.press('Control+Home'); await page.keyboard.press('Enter'); await page.keyboard.press('ArrowUp'); await page.keyboard.type('// unsaved');
    await page.locator(".wb-tab[id^='wb-tab-graph']").click();
    if (!await test('apply').isDisabled()) throw Error('stale preview can apply');
    await test('current').waitFor();
    if (!(await test('current').textContent()).startsWith('// unsaved')) throw Error('current text missing');
    if ((await test('proposed').textContent()) !== proposed) throw Error('conflict discarded preview');
    await test('recompute').click();
    await page.waitForFunction(() => document.querySelector('[data-testid="structural-function"]')?.value === '');
    await test('function').selectOption('1');
    await test('before').selectOption('0');
    await test('preview').click(); await test('proposed').waitFor();
    const moved = await test('proposed').textContent();
    await test('apply').click();
    if (fs.readFileSync(file, 'utf8') !== renamed) throw Error('move autosaved');
    await openSource(); await save(moved);
    await page.keyboard.press('Control+z'); await save('// unsaved\r\n' + renamed);
  });
  await step('endpoint enforces raw and decoded bounds and stale handles', async () => {
    const forbidden = await page.request.post(`${base}/api/structural/plan`, { data: {}, headers: { Origin: 'https://untrusted.example' } });
    if (forbidden.status() !== 403) throw Error('planner bypassed origin gate');
    const large = await page.request.post(`${base}/api/structural/plan`, { data: ' '.repeat(4 * 1024 * 1024 + 1), headers: { 'Content-Type': 'application/json' } });
    if (large.status() !== 413) throw Error(`body cap returned ${large.status()}`);
    const listed = await api('structural/plan', { request: { text: original, operation: 'List' } });
    const stale = await page.request.post(`${base}/api/structural/plan`, { data: { request: { text: original + '// later', operation: { DeclarationName: { target: listed.functions[0], name: 'other' } } } } });
    if (!JSON.stringify(await stale.json()).includes('Stale')) throw Error('stale handle accepted');
    const oversized = await page.request.post(`${base}/api/structural/plan`, { data: { request: { text: ' '.repeat(512 * 1024 + 1), operation: 'List' } } });
    if (!JSON.stringify(await oversized.json()).includes('Limit')) throw Error('decoded bound not enforced');
  });
  console.log(`STRUCTURAL EDITS E2E: PASS (${engine})`);
} catch (error) {
  console.error('FAIL:', error.stack || error);
  await page.screenshot({ path: `${process.env.M1_SHOTS ?? '.'}/structural-fail.png` }).catch(() => {});
  process.exitCode = 1;
} finally {
  await browser.close();
}
