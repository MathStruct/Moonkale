# @moonkale/codemirror protocol

The bundle sets `window.moonkale.codemirror` with the functions below. Rust drives
them through one `document::eval` per mounted editor, using Dioxus's
`dioxus.send()` / `dioxus.recv()` channel. A Rust-native backend must satisfy
the same messages.

## JS API

| function | effect |
|---|---|
| `mount(el, text, onChange, features?)` | create an editor inside `el` showing `text`; `onChange(changes, length, stamp)` after every document change. `features.onHover(id, line, col)` and `features.onDefinition(line, col)` are optional LSP hooks (0-based line, UTF-16 column) |
| `setText(el, text, epoch?)` | adopt a Workspace replacement (reload / revert / structural proposal) as one isolated undo transaction, preserving CRLF and UTF-16 scalar boundaries; fires `onChange` |
| `setIndentation(el, spaces, width)` | apply optional indentation overrides; both null restore backend defaults; no document edit |
| `getText(el)` | current document text |
| `focus(el)` | focus the editor |
| `undo(el)` / `redo(el)` | step the editor's history (menu Edit → Undo/Redo; Ctrl+Z/Y work inside the view already) |
| `setLspDiagnostics(el, items)` | replace all diagnostics; `items: {line, col, endLine, endCol, severity, message}[]` shown by `@codemirror/lint`'s gutter |
| `hoverResult(el, id, text \| null)` | answer a pending `onHover(id, …)`; the tooltip shows `text` as plain text (`.mk-hover`), `null` shows nothing. Unanswered hovers time out after 3 s |
| `setCursor(el, line, col)` | move the cursor, scroll into view, focus (go-to-definition inside the same file) |
| `destroy(el)` | tear down; safe to call twice |

## Messages Rust → JS (`eval.send`)

```json
{ "kind": "setText", "text": "…", "epoch": 1 }
{ "kind": "setIndentation", "insert_spaces": true, "width": 2 }
{ "kind": "focus" }
{ "kind": "undo" }
{ "kind": "redo" }
{ "kind": "destroy" }
{ "kind": "diagnostics", "items": [{ "line": 6, "col": 24, "endLine": 6, "endCol": 29, "severity": "error", "message": "…" }] }
{ "kind": "hoverResult", "id": 3, "text": "fn add(a: u32, b: u32) -> u32" }
{ "kind": "setCursor", "line": 0, "col": 3 }
```

The very first message is `{ "kind": "init", "text": "…", "key": "cm-session-1", "epoch": 0, "sequence": 0, "language": "rust", "wrap": false }`; the script waits
for it before mounting (see Problem Log P-047 for why the handshake exists).

## Messages JS → Rust (`dioxus.send`)

```json
{ "kind": "ready" }
{ "kind": "splice", "changes": [{ "from": 0, "to": 0, "insert": "…" }], "length": 1, "stamp": { "epoch": 0, "sequence": 1, "workspace": false } }
{ "kind": "hover", "id": 3, "line": 5, "col": 21 }
{ "kind": "definition", "line": 5, "col": 21 }
```

`hover` is sent when the mouse rests on the text for 250 ms; Rust asks the
language server and replies with `hoverResult` carrying the same `id`.
`definition` is sent on F12 at the cursor; Rust either sends `setCursor`
(same file) or opens the target document.

`onCursor(line, col, anchor, head)` is throttled to 4 reports per second and
fires when the main selection changes. `line`/`col` are the caret's zero-based
LSP position; `anchor`/`head` are canonical UTF-16 text offsets, including both
units of CRLF. Rust stores the ordered endpoints as the Workspace selection for
the active document, without normalizing or expanding line breaks again.

`splice` carries UTF-16 ranges in the pre-change canonical text and the new
UTF-16 document length. The complete text is not sent on ordinary input.

## Lifecycle

1. Rust renders a `div` with a stable `id` and starts the eval.
2. The eval polls until `window.moonkale.codemirror` exists (the bundle is a
   deferred `<script>`), mounts, sends `ready`.
3. Edits stream as `splice`; Rust validates stamps and updates its `Document`.
4. On panel unmount Rust sends `destroy`, releasing the view and retaining
   document-session history. Document close sends `dropSession` to release it.

## Spec 010 / 012 (2026-09-20)
- `mount(el, text, onChange, features)`: `features.language` (Rust's id) picks the grammar in `src/languages.ts` — the one place with language knowledge (P-093); `features.onWikiQuery(id, query)` and `features.onWikiLink(target)` for markdown sources.
- `setWikiLinks(el, [{from, to, resolved}])` — UTF-16 offsets; marks are mapped through later edits until the next call.
- `[[` completion answers arrive through the existing `completionResult(el, id, items)`; the labels are page targets and are inserted as `[[target]]`.
- `features.wrap` at mount and `setWrap(el, bool)` later: soft wrap through a compartment (spec 014).
- `run(el, action)` (spec 009): `find | replace | rename | codeActions | definition | references | toggleComment | foldAll | unfoldAll`.
- `onChange(changes, length)` (spec 018): `changes` are `{from, to, insert}` splices in UTF-16 offsets of the text before the transaction, in document order; `length` is the new document length. The whole text is never sent on a change.
- `setIndentation(el, spaces, width)` uses a compartment for `indentUnit` and `EditorState.tabSize`. Rust resolves language defaults for partial user overrides; null/null removes the override. `editor.insert_spaces` and `editor.indent_width` are optional settings, with width clamped to 1–8.
- Line breaks (P-164, 2026-10-11): the view keeps the text's own separator (`EditorState.lineSeparator`: `"\r\n"` when the initial text has one, else `"\n"`), so `getText` and every `insert` are byte-exact. All offsets crossing this boundary (`onChange` splices and `length`, `setWikiLinks`) are UTF-16 units **of the text**, where `"\r\n"` counts two; CodeMirror's own positions count one, and `index.ts` converts (`toText` / `fromText`). Line/column positions are unaffected.

## Coordinated document history (WP-67)

`mount` accepts `sessionKey`, `epoch`, `sequence`, and `onGroupHistory(group,
redo)`. A document key lives until close; `destroy` retains opaque history and
selection for remount, and `dropSession(key)` releases it on close. Restoration
uses the public history field initializer, not history JSON (which omits custom
effects). Legacy callers without a session key retain their prior lifetime.

`onChange(changes, length, {epoch, sequence, workspace})` stamps every splice.
User changes increment sequence; Workspace changes echo the current epoch with
sequence zero. Rust rejects stale epochs and nonconsecutive user sequences.
`setText(el,text,epoch)` advances the epoch and isolates ordinary external edits.

`groupSession(key, {phase, direction, group, token, text, epoch, sequence})`
returns a boolean acknowledgement. IDs are decimal strings to avoid JS number
precision loss. `prepare` checks exact text/stamps and the next opaque history
marker, then holds the view read-only. `commit` installs an isolated marked
replacement or performs its prepared undo/redo. `cancel` releases only the same
token and resynchronizes deferred canonical changes. The host bounds preparation
to five seconds, revalidates all buffers and commits them synchronously before
sending view commits. Views remain held until cancellation releases the barrier.

Keyboard and exported menu undo/redo inspect command dispatches: CodeMirror
history transactions set `filter:false`, so a transaction filter cannot enforce
this boundary. A group marker emits `onGroupHistory` without dispatching a local
undo. Preparation probes also inspect without dispatching. Unknown/retired groups
must block at the host; they never fall back to one-file undo.

P-219: `setPhrases(el, phrases)` / `setPhrases { phrases }` reconfigures
`EditorState.phrases` with the shell locale's search strings. This is view
configuration and does not edit text or history. Search controls use shell
colour tokens in `theme.ts`.
