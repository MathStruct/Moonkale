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

The full-app long-note suite records constrained typing samples against a 3 MB Source reference and checks deep preview, complete copy and byte-exact CRLF save/undo. Large single atomic widgets still require their full content and geometry when visible; links/wiki links, images, tables, mathematics and callouts remain further provider work. Milkdown stays available as Rich mode.

## Lists and tasks (2026-10-13)

CommonMark list items now produce independent source replacements: bullet and ordered markers, nested indentation, checked/unchecked task status, decoded text, inline emphasis/code, and wrapped single-paragraph bodies. A parent’s preview stops before its nested children so each item reveals locally. The canonical Markdown remains editable, copyable and undoable; task state changes by editing its source marker. Items containing links/wiki links, images, quotes, code blocks or multiple paragraphs retain source until those providers support the complete construct.

Unchanged block keys/decorations and list fragment payloads are shared across revisions to avoid repeated allocation. Replacement offsets use one byte-to-scalar index per changed block, and overlap checks use sorted item ranges. Presentation filters collapsed folds once rather than scanning every expanded heading/list fold for every replacement. The long-note gate includes hundreds of list/task items and checks deep list previews while retaining its existing latency limits. Rich mode stays available; links/wiki links, images, tables, math and callouts remain next.


## Link preview and navigation (2026-10-13)

CommonMark inline/reference/autolinks and wiki-links now share structured inline fragments in paragraphs, headings and supported list/task items. Wiki aliases are literal text; their temporary CommonMark projection retains a byte-boundary map to canonical source. Escaped wiki syntax, embeds, code, images, HTML and metadata remain source. Reference definitions still select the conservative document-wide semantic parse.

Ordinary clicks reveal the enclosing source replacement. Ctrl/Cmd-click follows only a measured link glyph rectangle, including wrapped fragments inside an atomic heading/list widget. The existing narrow geometry bridge reports identifiers and rectangles; Rust validates identifiers and retains destinations. Revision, font epoch, viewport and presentation identity reject stale measurements.

File links resolve relative to the origin within its source, with normalized paths and one percent-decoding pass; missing regular files are not created. Wiki links retain guarded resolve/create behavior and explicit `.md` suffixes. Same-note and cross-note fragments use canonical unsaved text, rendered heading slugs with duplicate suffixes, or case-insensitive heading text. A missing heading opens the target at the top and reports a localized status. Background loading does not activate a document; origin revision/selection and target text guards protect the eventual reveal.

`ExternalLinkOpener` is a workspace service. Web calls `Window.open` synchronously during the gesture with `_blank` and `noopener,noreferrer`; desktop/mobile use the platform browser opener. Only HTTP, HTTPS and mailto destinations are accepted. Hover previews, link completion, images/embeds and Rich-mode replacement remain later work.

Wiki resolution invalidates on the origin node, unique page targets and index epoch, rather than offsets or unrelated edits. Same-note headings remain resolved immediately. Visible row lookups reuse fold visibility from the engine row index. The long-note fixture turns its existing bold fragments into external links and adds wiki aliases inside existing list items, preserving comparable widget counts while testing 480 links.

Replacement validation converts requested scalar endpoints to bytes in one pass and asks `GraphemeCursor` about those boundaries only. It avoids materializing every plain-source grapheme boundary; comparison tests retain the full-segmentation oracle for CRLF, ZWJ emoji, regional indicators, combining marks and Indic clusters.

The derived-fold refresh keeps the row prefix index while collapsed ranges are unchanged. Wiki span cache identity includes normalized scalar positions as well as raw byte ranges, so equal-byte Unicode edits still move source marks correctly. CRLF counting uses substring matches rather than an allocated byte-window walk.

The deferred structural pair pass classifies parser spans only at bracket characters, while retaining Unicode scalar offsets and line counts across prose. Performance measurements wait for pending network/font/save work before timing either mode; the fixture keeps the preceding byte budget by replacing plain filler with link syntax, while preserving every section and list/task item.

Measured proportional runs may reuse geometry across revisions only when their source, marks, tags, replacement payloads, model/row/source start, viewport width, font epoch and presentation identity match exactly. The result is rebound to the current revision and republished to the height index. Request sequence numbers still cancel pending old responses; resize clears the cache, and font/viewport/presentation changes force measurement.
