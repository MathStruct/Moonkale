---
title: "ADR-0017 — Markdown live preview on the code editor"
tags: [adr, editors, markdown, code]
status: accepted
date: 2026-10-08
---
**Status:** accepted 2026-10-08 (Daniel asked for the note after asking whether the Rust code editor could also become the markdown editor, and render doc comments as markdown). Related: [[ADR-0008 Rust owns the document, JS is a view]], [[ADR-0016 Freya is a reference, not a target]], [[Markdown and Typst Editor]], [[Rust-native Editor Candidates]], [[Code Editor]].

## Context
There are two kinds of markdown editor:
- **Live preview** (Obsidian's Live Preview, Typora): the markdown **text is the document**. The editor decorates it: headings set larger, `**bold**` shown bold with its markers hidden until the cursor is inside, images and formulas drawn in place, a table as a table. Obsidian's Live Preview is CodeMirror 6, a code editor with a presentation layer.
- **Block WYSIWYG** (Milkdown/ProseMirror, Notion): the document is a tree of blocks, and markdown is an import/export format. Moonkale's Rich mode is this today, and the round trip is where it loses things: front matter needs special handling, a leading thematic break hid content until #19, and formatting is normalised on save.

The Rust code editor on the `codex` branch is a headless text model (`editor-core`: buffer, cursor, selection, undo, viewport) with a virtualized Dioxus view. It already has *decorations* that never own document state: search highlights and diagnostics. But its view assumes **monospace text on a grid of equal rows**. There is one measured row height (`row_pixels`) and one measured cell width (`cell_pixels`); virtualization is `first_row × row_height`, and a click becomes an offset by grid arithmetic.

Daniel's further idea, rendering doc comments (`///`, docstrings) as markdown inside the code editor, is the same mechanism as live preview: a range of the text shown rendered until the cursor enters it.

## Decision
1. **Markdown editing moves toward live preview on the code editor**, not toward a second engine. The markdown text stays the document (ADR-0008); nothing has to round-trip. Milkdown stays as Rich mode until live preview can replace it.
2. **The code editor's view is designed for this now**, while it is still young. Monospace on an equal-row grid becomes the code editor's special case, not the assumption:
   - **Rows of different heights.** Each visual row has a measured height; virtualization uses prefix sums of heights (a Fenwick tree or a chunked index) instead of `first_row × row_height`. Headings, images, formula and table widgets, and rendered doc comments are rows of their own height.
   - **Proportional text.** Layout and wrapping by pixel width, not by cell count; the monospace grid stays as a fast path for code.
   - **Hit testing** (a click to a text offset) without grid arithmetic. For proportional text that needs the browser's layout. Either measure the rendered spans of a row on demand, or allow one narrow, documented JS call (`caretPositionFromPoint` / `Range.getClientRects`) in a single module. The view has no JS bridge so far by design; this is the one place where one may be justified, and the choice should be written down when it is made.
   - **A decoration interface** with four kinds: mark (style a range), replace (hide a range or show a widget instead), line (style a whole row) and block widget (a row of its own between text rows). Decorations come from providers and never own the document: search, diagnostics, wiki-links, markdown live preview, doc comments.
3. **Live preview and doc comments are providers.** A markdown provider parses with tree-sitter markdown (already a dependency through arborium) and emits decorations, showing the source of the construct under the cursor. A doc-comment provider takes comment nodes from the language's tree-sitter tree and renders their markdown as a block widget, which turns back into source when the cursor enters it.
4. **Unchanged:** the text model, undo, the revisioned workspace contract (`ext-api::editor`), UTF-16 positions and the LSP client. Everything in this decision is in the view layer, as ADR-0016 asks.

## Consequences
- The variable-height virtualization is the expensive part. It is cheap to design in now and costly to retrofit after the monospace assumption has spread into scrolling, reveal and hit testing.
- Selection and IME across replaced ranges (a hidden `**`, a widget) need defined behaviour: the cursor skips or enters them. CodeMirror 6's "atomic ranges" are the reference.
- Milkdown stays until live preview covers what Rich mode does today: tables, KaTeX, images, wiki-link popups, front-matter properties.
- Not decided here: whether live preview becomes Rich mode's replacement, or a third mode next to Source and Rich. That is for when it exists.
