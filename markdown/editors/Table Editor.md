---
title: "Table Editor"
tags: [editor, table, sql]
---
Crate: `editors/table`. Fully Rust/Dioxus — no JS dependency. Two panels:

**Query panel** — a [[Code Editor]] embedding with `lang: sql | cypher | typeql`, a source picker, Run/Cancel, history (stored as nodes → linkable from wiki pages), and a warning ribbon when the statement is classified as a write ([[LLM and RAG]] uses the same classifier).

**Grid** — virtualised rows *and* columns; only the visible window is in the DOM. Typed cells from `Value`: `Ref` cells are links into the graph, `Vector` cells show a sparkline, `DateTime` localised. Direct edits use typed row batches with expected versions; refused operations show per cell.

Because the grid renders any `QueryResult::Rows`, it is also "open as table" for a `GraphView`, a Redis hash, or a CSV in a folder (via DuckDB, [[Data Sources]]).

## As built (2026-10-30)
The grid is virtualised in rows and columns: only the cells under the viewport,
plus overscan, are in the DOM. SQL tables with usable row identities support
source-side multi-column sort, typed AND filters and keyset pages. Other results
sort/filter only loaded rows and show a visible partial indicator. Raw SQL
results remain capped at 50,000 rows and do not acquire edit authority.

Not yet: "open as table" for other sources.

Prior art for what comes next (the Supabase table editor) will be studied for requirements only; what Moonkale rejects goes to [[Scrutiny]].

## Pushdown
SQLite, DuckDB and Turso advertise `Capabilities::table_query` and accept
`Source::query_rows(table, TableQuery)`. Sort names/directions, typed column
predicates (equals, comparisons, contains, prefix and is-null), a limit and a
cursor are data. Filters combine with AND. The planner checks identifiers against
fresh schema metadata, quotes them, and binds every value. All orders use NULLs
last and end in the complete row key. Cursors retain native sort values and
that key; paging never uses OFFSET. Changing the query starts a fresh traversal.
Tables without usable identities and raw query results use the loaded-row path.

Click a header, or focus it and press Enter/Space, to cycle ascending,
descending and off. Shift-click/Shift-Enter adds another sort key (up to eight).
Each header's ⋯ menu contains operator/value and Apply/Clear filter controls;
Escape closes it and returns focus. There is no always-visible filter row.
Loaded-result global search is in its ⋯ disclosure. Next page is also available
as a button; scrolling near the end loads the same keyset page. Refresh reruns
the default table query and discards local browsing state.

Pages contain at most 1,000 rows; paging retains at most 4,000 rows and targets
16 MiB of serialized snapshots, evicting the
oldest unedited rows and adjusting scroll position. Edited rows remain pinned
until refresh, including rows now outside the filter, with a visible note.
Editing checks a 1,000-row/4-MiB pinned-snapshot budget before writing.
A source trigger may enlarge the returned row by one bounded page (at most
4 MiB), so pinned snapshots have an 8-MiB hard bound. Between pages, edits
can raise retained snapshot bytes to at most 24 MiB; the next page evicts
unedited snapshots back to 16 MiB. Refresh clears pins. Retained undo
entries track the surviving row identities when pages move. Exact-key conflict
rereads work beyond the first page. Source/session and generation checks discard
stale replies. Existing page/value byte limits and the 64-entry/8-MiB undo cap
still apply. Backward traversal of evicted pages requires refresh.

Validation complete (WP-13, 2026-10-30): per-engine brute-force reference tests
cover duplicate values, NULLs, mixed directions, composite/quoted keys, AND
predicates, injection and checked writes beyond the first page. Thirteen grid
and edit tests include partial sorting/filtering and byte-budget eviction/pins.
All AGENTS gates and the reconciled 61-case browser feature matrix pass.

The million-row DuckDB browser fixture completed all 1,000 pages: page-to-paint
p50 **198.95 ms**, p95 **215.68 ms**, max **247.14 ms**.
Request-only p50/p95 were 102.36/107.32 ms. It retained at most
4,000 rows and 41 DOM rows. Exported Wasm memory stayed at 70.125 MiB
at every sample from page 100 through 1,000; collected JavaScript heap was
8.50 MiB. These are development Chromium measurements, not release or
physical-desktop frame guarantees. The scratch report is `table-grid-million.json`.

A real SQLite schema-change regression adds a column between pages. The grid
refuses the incompatible merge with a localized Refresh notice; Refresh obtains
new columns/versions and the next page loads normally. The schema layout cannot
silently change underneath existing cell contexts.

## Why not AG Grid / a JS grid
The grid is where Rust-native pays off immediately: it is data-heavy, not text-input-heavy, and Dioxus signals + virtualisation are sufficient. Keeping it Rust also means it works identically in a wasm extension's `moonkale_shell::Tree` (the `Table` primitive is this component).

## Typed cells (WP-15, 2026-10-23)

`Value::DateTime` holds a UTC instant and optional original offset. Grid display
uses English, German or Chinese date ordering and retains the original wall
time/offset (UTC when none was supplied). Loaded-row sorting compares instants including
subseconds; pushed queries use the database's native ordering and collation
(including SQLite/Turso stored date text). SQLite and Turso use declared DATE/DATETIME/TIMESTAMP metadata;
invalid values and untyped strings remain text. Time-only values have no UTC
instant and remain text. DuckDB dates/timestamps map directly; values outside
the core date range retain a text representation rather than becoming NULL.

`Value::Ref` carries a NodeId and label; index search's path cells provide known
file identities. A grid link opens the corresponding result/open node through
the workspace editor. UUID-shaped arbitrary text is never inferred as a link.

`Value::Vector` holds up to 4096 finite f64 samples (f32 widens losslessly).
DuckDB numeric lists/fixed arrays map to vectors; oversized, nullable and
unsupported lists show a fixed marker. The grid draws a bounded SVG sparkline
in current theme text colour, with numeric samples as its accessible label.
Empty/constant/extreme vectors keep finite coordinates. Sorting vectors uses
length; filter uses their numeric text. Existing Value wire encodings remain
unchanged; the new variants have explicit serde names.


## Density and chrome (WP-63, 2026-10-28)

Query text remains visible in both densities. Run is primary when table chrome is accepted; dialect choices and Show in Graph are in ⋯ in Simple, inline in Detailed.


## Versioned cell edits (WP-14, 2026-10-30)

The unchanged default table query in SQLite, DuckDB and Turso loads an
identity-bearing page (up to 1000 rows). Double-click or Enter opens a cell;
Enter or Save commits, Escape or Cancel discards, and Tab/Shift+Tab save and move through the displayed order, scrolling the next cell into view.
The cell uses its existing type for validation. Undo is another write checked
against the version produced by the edit, so it cannot overwrite a later writer.
Mouse controls expose the same save/cancel/undo/conflict actions as the keyboard.
Loaded-result sort and filter use updated values. On pushed-down pages, an edited row stays visible until refresh; a note marks an edit outside the active filter.

Sources check the full current row inside the transaction. A stale edit writes
nothing and shows the read value, mine and current on the cell. Keep mine
prepares another checked save against the current row; keep current accepts it;
Reopen rereads it. Refused operations retain the draft and their reason. Tables
without usable identity, views, generated columns, read-only attachments and
lossy cell representations remain read-only with an explanation. Arbitrary query
results never inherit edit authority from column names or aliases.

Agents use `source.read_rows` and submit typed operations in `editor.propose`.
The session inbox reviews each table's old/new cells beside text-file diffs;
only review approval applies them. Each database's selected batch is atomic;
other sources persist in order with a report. No DDL or arbitrary SQL write path
is exposed. Graph/KV sources stay read-only.

WP-12's Supabase study can refine editor controls, nullable/type selection,
row creation/deletion affordances and pushdown requirements on this contract.
It is not a dependency of the checked write model. Insert/delete currently have
the typed proposal/source API; direct grid controls edit existing cells.


Validation: `table-edit.mjs` exercises SQLite and DuckDB save/reload, IME Enter,
keyboard Tab navigation, mouse controls, checked undo and an external writer's
cell conflict. DuckDB's separate fixture writer makes at most fifty attempts with 100 ms
read-lock backoffs, then fails; it cannot hide a permanently retained file connection.
The shared engine tests also cover Turso, typed dates/vectors, native date keys,
exact date spelling in versions/undo, rollback, refusals and quoted identifiers.
All gates and the complete 59-suite browser feature matrix pass.
