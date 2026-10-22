# @moonkale/milkdown protocol

The bundle (Milkdown **Crepe**) sets `window.moonkale.milkdown`. Rust drives it
through one `document::eval` per rich view (see `editors/markdown/src/rich.rs`).
Whole-document events, like CodeMirror's first milestone.

## JS API

| function | effect |
|---|---|
| `mount(el, markdown, onChange, onWikiLink?, opts?)` | create the editor in `el`; `onChange(markdown)` after every *user* change (wiki-link brackets un-escaped) — the parse → serialize normalisation at load is the baseline and is not reported (spec 021); `onWikiLink(target)` on a click on a decorated `[[target]]`; `opts.katexMacros` (KaTeX macros), `opts.onWikiQuery(id, query)` (`[[` typed) |
| `setWikiStatus(el, [{target, resolved}])` | which targets resolve (decoration classes) |
| `wikiCandidates(el, id, [{target, key}])` | the answer to `onWikiQuery` |
| `setText(el, markdown)` | replace the document without firing `onChange` |
| `getText(el)` | current markdown |
| `focus(el)` / `destroy(el)` | |

## Messages Rust → JS
`{ "kind": "init", "text", "katexMacros" }` (first, awaited before mounting), `setText`, `wikiStatus { entries }`, `wikiCandidates { id, items }`, `focus`, `destroy`.

## Messages JS → Rust
`{ "kind": "ready" }`, `{ "kind": "change", "text" }`, `{ "kind": "wikiLink", "target" }`, `{ "kind": "wikiQuery", "id", "query" }`, `{ "kind": "error", "message" }`.

Build: `npm run build` → `packages/editors/markdown/assets/milkdown.{js,css}` (2.7 MB / 75 kB; fonts not bundled) and `assets/katex/` (KaTeX css + woff2 fonts via `katex-assets.mjs`). Committed.

P-218: `opts.onImage(id, path)` requests source-owned image display bytes;
`imageResult(el, id, url | null)` answers it (wire `image { id, path }` /
`image { id, url }`). Node views keep original Markdown attributes. The view
requests at most 128 images per mount and rejects paths over 4096 bytes. Rust
reuses the native-preview source jail, image type/byte/pixel limits, freshness
checks, two-reader queue and bounded workspace cache. External URLs are not
fetched. Responses from a replaced backend are discarded.
