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

## Whole-document implementation (2026-10-13)

The native preview no longer has the 4096-scalar prefix or 63-construct reveal limit. Reveal identity is the contiguous source interval containing the caret/selection's active constructs, plus provider identity; moving outside constructs keeps geometry identity stable.

A CommonMark block pass determines complete top-level ranges and preserves container, fence and Setext context. Unchanged blocks reuse their presentation decorations and fenced-code highlights; only changed blocks enter the semantic presentation parser. Reference definitions currently use a conservative document-wide parse because their meaning crosses blocks. The block scan and canonical snapshot comparison remain linear; this is block reuse, not a fully incremental syntax engine. Source offsets rebase across inserted blocks, and front matter stays source.

A cached index owns all replacement ranges and collapsed continuation rows. Rendering materializes source cells only for visible rows plus the existing three-row margin, with grapheme-valid replacements from that window. Synthetic continuation indentation never becomes source geometry. The sparse `RowHeights` prefix index uses a 32-pixel proportional baseline plus measured exceptions; font epochs, viewport width, revision and revealed intervals reject stale measurements. A row-only engine lookup avoids scanning long offscreen lines to compute unused horizontal coordinates.

Unicode fallback glyph ascents share visual navigation rows when their vertical extents overlap; exact glyph boxes still draw the caret. Production Markdown uses measured pixel-column navigation. CRLF syntax edits now normalize both coordinates and text before updating the retained syntax tree. These changes affect the view and derived syntax, while Workspace keeps canonical bytes, selection and undo ownership.

The full-app long-note suite records constrained typing samples against a 3 MB Source reference and checks deep preview, complete copy and byte-exact CRLF save/undo. Large single atomic widgets still require their full content and geometry when visible; lists, links/wiki links, images, tables, mathematics and callouts remain the next provider work. Milkdown stays available as Rich mode.
