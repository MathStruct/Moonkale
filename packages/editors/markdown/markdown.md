---
title: "editor-markdown — implementation notes"
tags: [crate-notes, milestone-2]
---
Notes for `moonkale-editor-markdown` (Milestone 2). Design: [[Markdown and Typst Editor]].

Only the index-backed part exists so far: `LinksExtension` contributes a **Links** panel (home `side`, listed after the Explorer in the default layout so it doesn't steal the active tab — P-052). `LinksPanel` re-queries on active document / `graph_epoch` / source changes: `Neighbours{depth 1, Both}` on the index, then splits `Links` edges into *Backlinks* (edges into the document) and *Outgoing* (edges out of it). Unresolved targets are phantom `Page` nodes shown in amber, not clickable; files open on click. WYSIWYG (Milkdown), the block model and link completion are still design notes.

**Milestone 3 — Typst preview** (`typst_preview.rs`): while any `.typ` document is open and the platform provides `WorkspaceConfig::compile_typst`, the extension also contributes a **typst-preview** panel (home `main`). It watches the `.typ` document's text, recompiles with coalescing (one compile in flight; the latest text wins — note the `busy.peek()` in the effect, P-056) through `compile_typst(root, rel, text)` — in-process `moonkale-typst` on desktop, the `api::compile_typst` server function on web — and renders the returned SVG pages (`dangerous_inner_html`) or the diagnostics. E2E: `packages/web/tests/e2e/typst.mjs`.

## Milestone 5 — Rich mode
`rich.rs`: `RichTextBackend` trait, `MilkdownBackend` (eval over `assets/milkdown.js`, the Crepe bundle from `packages/js/milkdown`; messages `init` / `setText` / `focus` / `destroy` and `ready` / `change` / `wikiLink`), and `RichPanel` (toolbar with Save/Reload, Ctrl+S, pushes external text changes into the view). The extension now **owns `.md` documents** (`is_markdown`; the code editor is built with `.skipping(is_markdown)`): each gets an `editor:<uuid>` tab with a **Source | Rich** toggle (`MarkdownPanel`; Source hosts `CodeEditorPanel`). Ctrl+click on `[[Target]]` in rich mode opens `Target.md` (by path, then by index label). The bundle un-escapes `\[\[` so wiki-links survive the round trip; other remark reformatting is accepted. E2E: `packages/web/tests/e2e/rich.mjs`.

## Milestone 10 (specs 012, 013)
- **Formulas**: Crepe's Latex feature is on (`throwOnError: false`); `assets/katex/` (stylesheet + 20 woff2 fonts, copied by `packages/js/milkdown/katex-assets.mjs` at bundle build) is a dx **folder asset** (`asset!("/assets/katex", AssetOptions::folder())`) linked with `StylesheetUrl`, so KaTeX's relative `fonts/…` urls resolve. Macros per folder: `.moonkale/katex.json` → `{"macros": {...}}` → `init.katexMacros`.
- **Wiki-links**: `packages/js/milkdown/src/wiki.ts` (ProseMirror plugin: decorations over the literal text, hidden brackets, click-to-follow, `[[` popup) ↔ `rich.rs` (`wikiStatus` / `wikiCandidates` down, `wikiLink` / `wikiQuery` up). Status is recomputed 300 ms after the last change (`futures-timer`) and on `graph_epoch`; follow uses `Workspace::follow_wiki(.., create = true)`. The Links panel's phantom rows have a **Create** button.
- The rich toolbar hint reads "click a [[link]] to follow it · type [[ to link a page".
