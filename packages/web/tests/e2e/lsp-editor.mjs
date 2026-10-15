import fs from "node:fs";
// Shared live-server scenarios select an engine without changing their assertions.
export function lspEditor(page) {
  const native = process.env.LSP_EDITOR === 'native';
  if (process.env.LSP_EDITOR && !['native', 'codemirror'].includes(process.env.LSP_EDITOR)) {
    throw Error('LSP_EDITOR must be native or codemirror');
  }
  const selectors = native ? {
    input: '.mk-editor-core-input-sink', row: '.mk-editor-core-row',
    error: '.mk-native-diagnostic-error', marker: '.mk-native-diagnostic-marker',
    completion: '.mk-native-completions', completionLabel: '.mk-native-completion-label',
    completionItem: '.mk-native-completion', references: '.mk-native-references',
    reference: '.mk-native-references .mk-native-tool-item', actions: '.mk-native-actions',
    action: '.mk-native-actions .mk-native-tool-item',
  } : {
    input: '.cm-content', row: '.cm-line', error: '.cm-lint-marker-error',
    marker: '.cm-lint-marker', completion: '.cm-tooltip-autocomplete',
    completionLabel: '.cm-completionLabel', completionItem: '.cm-tooltip-autocomplete li',
    references: '.mk-editor-refs', reference: '.mk-editor-ref',
    actions: '.mk-editor-actions', action: '.mk-editor-actions button',
  };
  const sent = [];
  page.on('websocket', socket => {
    if (!socket.url().includes('/api/lsp')) return;
    socket.on('framesent', ({ payload }) => {
      const frame = JSON.parse(String(payload));
      const message = typeof frame === 'string' ? JSON.parse(frame) : frame;
      if (message.method) sent.push(message);
    });
  });
  return {
    ...selectors, native,
    assertPosition(method, needle, off) {
      const text = fs.readFileSync(`${process.env.M1_ROOT}/src/main.rs`, 'utf8');
      const offset = text.indexOf(needle) + off;
      if (offset < off) throw Error(`missing position needle: ${needle}`);
      const prefix = text.slice(0, offset).split('\n');
      const expected = { line: prefix.length - 1, character: prefix.at(-1).length };
      const actual = sent.findLast(message => message.method === method)?.params.position;
      if (actual?.line !== expected.line || actual?.character !== expected.character) {
        throw Error(`${method}: expected UTF-16 ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`);
      }
    },
    async configure() {
      if (!process.env.M1_ROOT) throw Error('M1_ROOT must name a disposable fixture');
      const path = `${process.env.M1_ROOT}/src/main.rs`;
      // Both engines run the same wire check after a non-BMP character on CRLF.
      const text = fs.readFileSync(path, 'utf8').replace('= add(', '= /* 😀中 */ add(');
      fs.writeFileSync(path, text.replace(/\r?\n/g, '\r\n'));
      await page.addInitScript(native => localStorage.setItem('moonkale.settings', JSON.stringify({
        extensions: { enabled: ['dev.moonkale.editor-code-native'] },
        editor: { implementation: native ? 'native' : 'codemirror', markdown_rich: false },
      })), native);
    },
    async clickAt(needle, off = 0) {
      const point = await page.evaluate(({ row, native, needle, off }) => {
        const rows = [...document.querySelectorAll(row)].filter(el => el.getBoundingClientRect().height);
        const source = el => native ? [...el.querySelectorAll('.mk-editor-core-cell')].map(x => x.textContent).join('') : el.textContent;
        const line = rows.find(el => source(el).includes(needle));
        if (!line) throw Error(`source row not found: ${needle}`);
        const target = source(line).indexOf(needle) + off;
        const walker = document.createTreeWalker(line, NodeFilter.SHOW_TEXT, {
          acceptNode: node => !native || node.parentElement.closest('.mk-editor-core-cell') ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_REJECT,
        });
        let seen = 0, node;
        while ((node = walker.nextNode())) {
          if (target < seen + node.length) {
            const range = document.createRange();
            range.setStart(node, target - seen); range.setEnd(node, target - seen + 1);
            const box = range.getBoundingClientRect();
            return { x: box.x + box.width * 0.1, y: box.y + box.height / 2 };
          }
          seen += node.length;
        }
        throw Error('offset past source row');
      }, { row: selectors.row, native, needle, off });
      await page.mouse.click(point.x, point.y);
      await this.focus();
      return point;
    },
    async text() {
      return page.evaluate(({ row, native }) => [...document.querySelectorAll(row)]
        .filter(el => el.getBoundingClientRect().height)
        .map(el => native ? [...el.querySelectorAll('.mk-editor-core-cell')].map(x => x.textContent).join('') : el.textContent).join('\n'), { row: selectors.row, native });
    },
    async undoTo(expected) {
      for (let step = 0; step < 8; step++) {
        const before = await this.text();
        if (before === expected) return;
        await page.keyboard.press('Control+z');
        await page.waitForFunction(({ row, native, before }) => {
          const text = [...document.querySelectorAll(row)].filter(el => el.getBoundingClientRect().height)
            .map(el => native ? [...el.querySelectorAll('.mk-editor-core-cell')].map(x => x.textContent).join('') : el.textContent).join('\n');
          return text !== before;
        }, { row: selectors.row, native, before });
      }
      if (await this.text() !== expected) throw Error('undo did not restore source');
    },
    async waitText(predicate) {
      // A source snapshot from the rendered small fixture, never the hidden input sink.
      await page.waitForFunction(({ row, native, predicate }) => {
        const text = [...document.querySelectorAll(row)].filter(el => el.getBoundingClientRect().height)
          .map(el => native ? [...el.querySelectorAll('.mk-editor-core-cell')].map(x => x.textContent).join('') : el.textContent).join('\n');
        return new RegExp(predicate).test(text);
      }, { row: selectors.row, native, predicate });
    },
    async activeLine() {
      if (!native) return page.locator('.cm-activeLine').innerText();
      const head = Number(await page.locator('.mk-crust:visible').getAttribute('data-selection-head'));
      const text = await this.text();
      let offset = 0;
      for (const line of text.split('\n')) {
        if (head <= offset + line.length) return line;
        offset += line.length + 2; // These scenarios explicitly use CRLF.
      }
      throw Error('caret outside rendered fixture');
    },
    async focus() { await page.locator(selectors.input).filter({ visible: true }).focus(); },
  };
}
