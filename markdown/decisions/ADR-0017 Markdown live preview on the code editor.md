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

### Source-backed image blocks

Standalone top-level Markdown images (including references) and wiki image embeds render through the originating Source. Relative paths decode once and stay within that source; wiki resolution prefers the same directory, then root, then complete filename matches with shortest-path/lexical ordering. Mixed paragraphs, containers, sizing modifiers, note embeds and remote/data input URLs retain source. Alt text and titles remain literal. Plain click reveals the full canonical paragraph at its start; Ctrl/Cmd-click opens the asset through guarded navigation.

The shared workspace cache coalesces fetches, allows two active byte reads and retains at most 64 MiB of encoded payload. Versioned keys refresh when the asset version changes; unversioned keys follow the graph epoch. Metadata and actual bytes enforce 24 MiB, and known raster headers enforce 100 megapixels. SVG is loaded as an image data URL, never injected as markup. Visible blocks load lazily; loading/error placeholders are localized. Images preserve aspect ratio without upscaling, with 640 px width and 320 px height caps.

Opaque image identities retain exact canonical source ownership in the geometry bridge. Loading, decode, error and version transitions invalidate measured geometry and pending requests, including when the dimensions happen to match. Shared format helpers live in `moonkale_ext_api::media::image`; the image editor keeps its existing public reexports.

### Source-owned tables

Top-level pipe tables render as a single replacement owning the complete original table, including its delimiter row. CommonMark's table extension supplies cell boundaries, escaped pipes, entities and column alignment. Headers and cells use semantic table/row/columnheader/cell roles with an equal-column grid, wrapped text and theme borders. Inline emphasis, strong text, code, ordinary links and wiki aliases use the existing literal fragment renderer and guarded navigation. HTML and Markdown images inside cells, and tables inside lists or quotes, retain source.

The caret or an intersecting selection reveals the whole table locally. Plain pointer hits use the existing atomic source boundaries; modified link clicks use measured fragment identities. Continuation source rows collapse into the table's measured height, while adjacent blank rows remain source. The geometry bridge still validates exact concatenated cell text and canonical ownership; no new JS parsing or model/history contract is introduced. Shared table payloads participate in block reuse and revision/font/viewport/presentation guards.

### Source-owned math

Rust enables the CommonMark math extension and retains exact scalar ownership for inline dollar expressions and standalone display paragraphs. Code, metadata, escaped/unmatched delimiters and mixed display text retain source. Closing inline delimiters followed by digits remain literal currency. Inline fragments carry typed math payloads through headings, lists, table cells and link labels; wiki projection never enters TeX. Paragraph formulas reveal independently; existing atomic headings/list items/tables reveal their enclosing source, and display paragraphs reveal their complete delimiters/body.

An isolated, locally bundled KaTeX 0.18.7 renderer accepts TeX/display/macros and returns HTML/MathML or failure. Markdown parsing, macro lookup, cache ownership and navigation remain Rust. CSS/fonts are shared through `ext-api::media::math` with Rich mode. Visible widgets read saved `.moonkale/katex.json` through their originating Source; configuration is coalesced per source/graph epoch, and source refresh re-reads it. Missing config uses defaults; invalid/oversized config reports localized status. Macros are cloned per render, so definitions do not leak.

The renderer disables trusted commands, caps expansions at 1000 and requested sizes at 20 em, bounds TeX at 16 KiB and returned markup at 1 MiB. Rust bounds config at 64 KiB and retained markup at 8 MiB/256 entries. Keys include source, TeX, mode and macro fingerprint. Errors remain literal canonical text with a localized indication. Stale responses cannot paint changed/disposed widgets; unchanged prose does not restart rendering.

Geometry validates math owner identities and projects their canonical labels while checking surrounding text exactly. Render/config/error/font transitions invalidate pending measurements even when dimensions match. Display equations scroll independently: scrolling does not reach the editor viewport, and measured scrollbar areas retain native interaction. Display content uses a guarded owner-relative pointer path to canonical source; inline/link hits retain existing navigation. No document/history/LSP contract changes.


### Source-owned callouts

Root blockquotes beginning with `[!type]` render as a whole source-owned callout. Types are case-insensitive ASCII identifiers; custom types use the same layout. A custom title supports inline emphasis, code, links/wiki links and math. An omitted title uses the type. Bodies support paragraphs (including lazy continuations), headings, lists/task lists, nested quotes, and the same inline fragments. HTML, images, tables, fenced code and display math inside a callout keep the whole quote as source; callouts inside lists/other quotes and inline title spans crossing into the body also retain source.

A trailing `-` initially folds the body; `+` initially opens it. A separate native button toggles presentation with pointer, Enter or Space without moving the editor caret or changing source/history. Its expanded state is local to the mounted source range; it is not saved to the document. Title/body clicks continue to reveal the enclosing source range, while modified link clicks follow the existing revision-checked routing. Selections and composition use canonical text.

Geometry validates every literal fragment and math owner even while folded, measures the enclosing block's current bounds, and ignores hidden math hit rectangles. Folding invalidates geometry and the row-height index; asynchronous math and font changes use the existing guarded invalidation. The long-note fixture includes callouts beyond the old 4096-character prefix and retains its existing typing-latency bounds.

Callout acceptance (2026-10-14): the 247,882-byte CRLF note retains 240 sections, 720 list items, 480 links, three tables, four images and three display equations, and adds three source-owned callouts at sections 60, 180 and 239. With both dev server and Firefox restricted to two cores, software rendering, three warmups and seven samples, preview typing measured 249 ms median / 279 ms p95 against the 3 MB Source baseline's 539 / 544 ms. These are dev-build harness measurements for the existing relative acceptance bound, not release-build latency estimates; the 25% median / 50% p95 limits are unchanged.


### Wiki-link hover excerpts

The native editor previews wiki targets through a view-local controller, without loading the target into Workspace or changing activation, selection, history or canonical text. Rendered links supply the semantic target from revision-checked geometry, including aliases inside headings, lists, tables and callouts. Source links use the actual character under the pointer, independently of the LSP caret-boundary hover position. Code, embeds, escapes, math, HTML containers and leading front matter are excluded. Source tokens are cached by canonical revision and parsed lazily while source text is hovered; the disabled branch reads no document signals during typing.

After a 250 ms dwell, the controller uses Workspace's existing wiki resolver, then reads the open target's live text or fetches text directly from its Source. Same-note and heading targets share the navigation heading resolver and duplicate-slug rules. Heading excerpts stop at the next peer/higher heading. Excerpts omit front matter and HTML, project wiki aliases to literal text, and render plain CommonMark text without media or nested popups. Returned text is limited to 1200 scalars and 14 lines, scanned from at most 4096 source scalars. Advertised sizes above 256 KiB are rejected before fetching; unknown-size Sources still use their whole-text API and are checked after fetching. Resolution/fetching has a five-second timeout, with localized loading, missing, unavailable, oversized and missing-heading states.

Requests/results retain origin node, canonical revision, target identity and source epoch. Origin edits, navigation, scrolling, composition and leaving the link dismiss the popup; a 200 ms close grace lets the pointer enter it for reading. Open target edits update the excerpt, and discarding/closing an unsaved target invalidates its cached excerpt and re-fetches saved text. Popups have independent view state and clamp to the browser viewport. Pointer, clipboard and keyboard events stay inside the read-only popup; Escape restores the editor input focus. Existing LSP hover remains separate, with the wiki popup taking precedence while displayed.

Wiki-hover acceptance (2026-10-14): with dev server and Firefox on two cores, the unchanged 247,882-byte fixture measured 253 ms median / 282 ms p95 against the 3 MB Source baseline's 539 / 549 ms. The existing 25% median / 50% p95 bounds remain unchanged. Native checks also copy the popup's DOM-selected excerpt while the editor has its full source selected, proving clipboard ownership stays separate.
