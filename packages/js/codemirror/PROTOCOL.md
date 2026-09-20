# @moonkale/codemirror protocol

The bundle sets `window.moonkale.codemirror` with the functions below. Rust drives
them through one `document::eval` per mounted editor, using Dioxus's
`dioxus.send()` / `dioxus.recv()` channel. A Rust-native backend must satisfy
the same messages.

## JS API

| function | effect |
|---|---|
| `mount(el, text, onChange, features?)` | create an editor inside `el` showing `text`; `onChange(fullText)` after every document change. `features.onHover(id, line, col)` and `features.onDefinition(line, col)` are optional LSP hooks (0-based line, UTF-16 column) |
| `setText(el, text)` | replace the whole document (reload / revert); fires `onChange` |
| `getText(el)` | current document text |
| `focus(el)` | focus the editor |
| `undo(el)` / `redo(el)` | step the editor's history (menu Edit → Undo/Redo; Ctrl+Z/Y work inside the view already) |
| `setLspDiagnostics(el, items)` | replace all diagnostics; `items: {line, col, endLine, endCol, severity, message}[]` shown by `@codemirror/lint`'s gutter |
| `hoverResult(el, id, text \| null)` | answer a pending `onHover(id, …)`; the tooltip shows `text` as plain text (`.mk-hover`), `null` shows nothing. Unanswered hovers time out after 3 s |
| `setCursor(el, line, col)` | move the cursor, scroll into view, focus (go-to-definition inside the same file) |
| `destroy(el)` | tear down; safe to call twice |

## Messages Rust → JS (`eval.send`)

```json
{ "kind": "setText", "text": "…" }
{ "kind": "focus" }
{ "kind": "undo" }
{ "kind": "redo" }
{ "kind": "destroy" }
{ "kind": "diagnostics", "items": [{ "line": 6, "col": 24, "endLine": 6, "endCol": 29, "severity": "error", "message": "…" }] }
{ "kind": "hoverResult", "id": 3, "text": "fn add(a: u32, b: u32) -> u32" }
{ "kind": "setCursor", "line": 0, "col": 3 }
```

The very first message is `{ "kind": "init", "text": "…" }`; the script waits
for it before mounting (see Problem Log P-047 for why the handshake exists).

## Messages JS → Rust (`dioxus.send`)

```json
{ "kind": "ready" }
{ "kind": "change", "text": "…" }
{ "kind": "hover", "id": 3, "line": 5, "col": 21 }
{ "kind": "definition", "line": 5, "col": 21 }
```

`hover` is sent when the mouse rests on the text for 250 ms; Rust asks the
language server and replies with `hoverResult` carrying the same `id`.
`definition` is sent on F12 at the cursor; Rust either sends `setCursor`
(same file) or opens the target document.

`change` carries the **whole document** in Milestone 1. Splices
(`{start,end,text}` in char offsets) are the planned replacement; the Rust
`TextPatch` type already accepts them.

## Lifecycle

1. Rust renders a `div` with a stable `id` and starts the eval.
2. The eval polls until `window.moonkale.codemirror` exists (the bundle is a
   deferred `<script>`), mounts, sends `ready`.
3. Edits stream as `change`; Rust updates its `Document`.
4. On panel unmount Rust drops the eval; JS receives nothing more. The
   element is removed by Dioxus, which releases the view via the WeakMap.

## Spec 010 / 012 (2026-09-20)
- `mount(el, text, onChange, features)`: `features.language` (Rust's id) picks the grammar in `src/languages.ts` — the one place with language knowledge (P-093); `features.onWikiQuery(id, query)` and `features.onWikiLink(target)` for markdown sources.
- `setWikiLinks(el, [{from, to, resolved}])` — UTF-16 offsets; marks are mapped through later edits until the next call.
- `[[` completion answers arrive through the existing `completionResult(el, id, items)`; the labels are page targets and are inserted as `[[target]]`.
- `features.wrap` at mount and `setWrap(el, bool)` later: soft wrap through a compartment (spec 014).
- `run(el, action)` (spec 009): `find | replace | rename | codeActions | definition | references | toggleComment | foldAll | unfoldAll`.
