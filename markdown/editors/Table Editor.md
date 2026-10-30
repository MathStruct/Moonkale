---
title: "Table Editor"
tags: [editor, table, sql]
---
Crate: `editors/table`. Fully Rust/Dioxus — no JS dependency. Two panels:

**Query panel** — a [[Code Editor]] embedding with `lang: sql | cypher | typeql`, a source picker, Run/Cancel, history (stored as nodes → linkable from wiki pages), and a warning ribbon when the statement is classified as a write ([[LLM and RAG]] uses the same classifier).

**Grid** — virtualised rows *and* columns; only the visible window is in the DOM. Typed cells from `Value`: `Ref` cells are links into the graph, `Vector` cells show a sparkline, `DateTime` localised. Direct edits use typed row batches with expected versions; refused operations show per cell.

Because the grid renders any `QueryResult::Rows`, it is also "open as table" for a `GraphView`, a Redis hash, or a CSV in a folder (via DuckDB, [[Data Sources]]).

## As built (2026-10-30)
The grid is virtualised in rows and columns: only the cells under the viewport, plus a little overscan, are in the DOM. It has a sticky header and row-number column, sort by header click (ascending, descending, off; NULLs last), resizable columns, a row filter, and a keyboard cell cursor that scrolls into view. Sort and filter apply to the loaded rows. The SQL sources now return up to 50 000 rows (was 500).

Not yet:
- pushdown;
- "open as table" for other sources.

Prior art for what comes next (the Supabase table editor) will be studied for requirements only; what Moonkale rejects goes to [[Scrutiny]].

## Pushdown
Sort/filter/paginate are sent to the source when `Capabilities` allow; otherwise applied client-side with a "partial" indicator. Keyset pagination, never OFFSET on large tables.

## Why not AG Grid / a JS grid
The grid is where Rust-native pays off immediately: it is data-heavy, not text-input-heavy, and Dioxus signals + virtualisation are sufficient. Keeping it Rust also means it works identically in a wasm extension's `moonkale_shell::Tree` (the `Table` primitive is this component).

## Typed cells (WP-15, 2026-10-23)

`Value::DateTime` holds a UTC instant and optional original offset. Grid display
uses English, German or Chinese date ordering and retains the original wall
time/offset (UTC when none was supplied). Sorting compares instants including
subseconds. SQLite and Turso use declared DATE/DATETIME/TIMESTAMP metadata;
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
Sort and filter use the updated values.

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
