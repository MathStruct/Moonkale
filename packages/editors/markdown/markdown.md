---
title: "editor-markdown — implementation notes"
tags: [crate-notes, milestone-2]
---
Notes for `moonkale-editor-markdown` (Milestone 2). Design: [[Markdown and Typst Editor]].

Only the index-backed part exists so far: `LinksExtension` contributes a **Links** panel (home `side`, listed after the Explorer in the default layout so it doesn't steal the active tab — P-052). `LinksPanel` re-queries on active document / `graph_epoch` / source changes: `Neighbours{depth 1, Both}` on the index, then splits `Links` edges into *Backlinks* (edges into the document) and *Outgoing* (edges out of it). Unresolved targets are phantom `Page` nodes shown in amber, not clickable; files open on click. WYSIWYG (Milkdown), the block model and link completion are still design notes.

**Milestone 3 — Typst preview** (`typst_preview.rs`): while any `.typ` document is open and the platform provides `WorkspaceConfig::compile_typst`, the extension also contributes a **typst-preview** panel (home `main`). It watches the `.typ` document's text, recompiles with coalescing (one compile in flight; the latest text wins — note the `busy.peek()` in the effect, P-056) through `compile_typst(root, rel, text)` — in-process `moonkale-typst` on desktop, the `api::compile_typst` server function on web — and renders the returned SVG pages (`dangerous_inner_html`) or the diagnostics. E2E: `packages/web/tests/e2e/typst.mjs`.
