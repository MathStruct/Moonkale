# @moonkale/milkdown protocol

The bundle (Milkdown **Crepe**) sets `window.moonkale.milkdown`. Rust drives it
through one `document::eval` per rich view (see `editors/markdown/src/rich.rs`).
Whole-document events, like CodeMirror's first milestone.

## JS API

| function | effect |
|---|---|
| `mount(el, markdown, onChange, onWikiLink?)` | create the editor in `el`; `onChange(markdown)` after every change (wiki-link brackets un-escaped); `onWikiLink(target)` on Ctrl/Cmd+click inside `[[target]]` |
| `setText(el, markdown)` | replace the document without firing `onChange` |
| `getText(el)` | current markdown |
| `focus(el)` / `destroy(el)` | |

## Messages Rust → JS
`{ "kind": "init", "text" }` (first, awaited before mounting), `setText`, `focus`, `destroy`.

## Messages JS → Rust
`{ "kind": "ready" }`, `{ "kind": "change", "text" }`, `{ "kind": "wikiLink", "target" }`, `{ "kind": "error", "message" }`.

Build: `npm run build` → `packages/editors/markdown/assets/milkdown.{js,css}` (2.6 MB / 75 kB; fonts not bundled). Committed.
