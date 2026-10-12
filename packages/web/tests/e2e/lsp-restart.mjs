// Shared restart works across editor surfaces and retains canonical unsaved text.
import { firefox } from 'playwright';
import fs from 'node:fs';
const port = process.env.PORT ?? 8090;
const root = process.env.M1_ROOT;
const shots = process.env.M1_SHOTS ?? '.';
const browser = await firefox.launch();
const page = await browser.newPage({ viewport: { width: 1500, height: 900 } });
await page.addInitScript(() => localStorage.setItem('moonkale.settings', JSON.stringify({ extensions: { enabled: ['dev.moonkale.editor-code-native'] }, editor: { markdown_rich: false } })));
const connections = [];
page.on('websocket', socket => {
  if (!socket.url().includes('/api/lsp')) return;
  const connection = { language: null, sent: [], closed: false };
  connections.push(connection);
  socket.on('framesent', ({ payload }) => {
    try {
      const frame = JSON.parse(String(payload));
      const msg = typeof frame === 'string' ? JSON.parse(frame) : frame;
      if (msg.language) connection.language = msg.language;
      connection.sent.push(msg);
    } catch {}
  });
  socket.on('close', () => connection.closed = true);
});
const step = async (name, action) => { process.stdout.write(`- ${name} … `); await action(); console.log('ok'); };
const wait = async predicate => {
  const deadline = Date.now() + 30000;
  while (!predicate()) {
    if (Date.now() > deadline) throw Error('LSP connection condition timed out');
    await new Promise(resolve => setTimeout(resolve, 25));
  }
};
const forLanguage = language => connections.filter(connection => connection.language === language);
const initialized = connection => connection?.sent.some(msg => msg.method === 'initialized');
const opened = (connection, text) => connection?.sent.some(msg => msg.method === 'textDocument/didOpen' && msg.params.textDocument.text.includes(text));
const restart = async language => {
  const before = forLanguage(language).length;
  const old = forLanguage(language).at(-1);
  await page.keyboard.press('Control+Shift+P');
  await page.waitForSelector('.mk-palette-input');
  await page.fill('.mk-palette-input', 'Restart Language Server');
  await page.locator('.mk-palette-item[data-key="editor.restartLanguageServer"]').click();
  await page.waitForSelector('.mk-palette', { state: 'detached' });
  await wait(() => forLanguage(language).length === before + 1);
  if (old) await wait(() => old.closed);
  return forLanguage(language).at(-1);
};
try {
  const original = fs.readFileSync(`${root}/src/main.rs`, 'utf8');
  await step('open Rust Source and wait for real rust-analyzer diagnostics', async () => {
    await page.goto(`http://127.0.0.1:${port}/`, { waitUntil: 'networkidle' });
    await page.waitForSelector('.wb-workspace');
    await page.click('.mk-explorer-open button[type=submit]');
    await page.waitForFunction(() => document.querySelector('.wb-status-bar')?.textContent.includes('index:'));
    await page.click('.mk-tree-dir >> text=src');
    await page.click('.mk-tree-file >> text=main.rs');
    await page.waitForSelector('.cm-content');
    await page.waitForSelector('.cm-lint-marker-error', { timeout: 120000 });
    await wait(() => initialized(forLanguage('rust').at(-1)));
  });
  await step('CodeMirror restart closes the old socket and reopens unsaved Unicode text', async () => {
    await page.click('.cm-content');
    await page.keyboard.press('Control+Home');
    await page.keyboard.insertText('// unsaved 猫\n');
    await page.waitForSelector('.mk-editor-dirty');
    const replacement = await restart('rust');
    await wait(() => initialized(replacement) && opened(replacement, '// unsaved 猫\n'));
    await page.waitForSelector('.cm-lint-marker-error', { timeout: 120000 });
    if (fs.readFileSync(`${root}/src/main.rs`, 'utf8') !== original) throw Error('restart saved an unsaved edit');
    if (!(await page.locator('.cm-content').textContent()).includes('unsaved 猫')) throw Error('CodeMirror text lost');
  });
  await step('native Source restart keeps the surface and current canonical text', async () => {
    await page.locator('.mk-editor-switch').filter({ visible: true }).click();
    const input = page.locator('.mk-editor-core-input-sink:visible');
    await input.waitFor();
    await input.focus();
    await page.keyboard.press('Control+End');
    await page.keyboard.insertText('// native restart\n');
    await wait(() => forLanguage('rust').at(-1).sent.some(msg => msg.method === 'textDocument/didChange' && msg.params.contentChanges.some(change => change.text?.includes('// native restart'))));
    const replacement = await restart('rust');
    await wait(() => initialized(replacement) && opened(replacement, '// native restart\n') && opened(replacement, '// unsaved 猫\n'));
    await input.waitFor();
    const lastChange = () => replacement.sent.filter(msg => msg.method === 'textDocument/didChange').at(-1)?.params.contentChanges.at(-1)?.text;
    await input.focus();
    await page.keyboard.press('Control+z');
    await wait(() => lastChange()?.includes('// unsaved 猫') && !lastChange()?.includes('// native restart'));
    await page.keyboard.press('Control+y');
    await wait(() => lastChange()?.includes('// native restart'));
    if (fs.readFileSync(`${root}/src/main.rs`, 'utf8') !== original) throw Error('native restart saved an edit');
  });
  await step('Markdown Source and Rich restart without losing edits, including unavailable servers', async () => {
    await page.click('.mk-tree-file >> text=Home.md');
    const markdown = page.locator('.mk-md:visible').filter({ has: page.locator('.mk-editor-path').and(page.getByText('Home.md', { exact: true })) });
    await markdown.locator('.cm-content').waitFor();
    await wait(() => forLanguage('markdown').at(-1)?.closed || initialized(forLanguage('markdown').at(-1)));
    const sourceConnection = await restart('markdown');
    await wait(() => sourceConnection.closed || initialized(sourceConnection));
    await markdown.locator('.mk-md-modes button:has-text("Rich")').click();
    await markdown.locator('.ProseMirror h1').waitFor({ timeout: 30000 });
    const saved = fs.readFileSync(`${root}/Home.md`, 'utf8');
    await markdown.locator('.ProseMirror h1').click();
    await page.keyboard.press('End');
    await page.keyboard.type(' Unsaved');
    await markdown.locator('.mk-editor-dirty').waitFor();
    const richConnection = await restart('markdown');
    await wait(() => richConnection.closed || (initialized(richConnection) && opened(richConnection, 'Unsaved')));
    if (!(await markdown.locator('.ProseMirror h1').textContent()).includes('Unsaved')) throw Error('Rich edit lost');
    if (fs.readFileSync(`${root}/Home.md`, 'utf8') !== saved) throw Error('Rich restart saved an edit');
  });
  console.log('LSP RESTART E2E: PASS');
} catch (error) {
  console.log('FAIL:', error.message);
  await page.screenshot({ path: `${shots}/lsp-restart-fail.png` }).catch(() => {});
  console.log(JSON.stringify(connections.map(({ language, closed, sent }) => ({ language, closed, methods: sent.map(msg => msg.method).filter(Boolean) })), null, 2));
  process.exitCode = 1;
} finally { await browser.close(); }
