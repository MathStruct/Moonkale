---
title: "Table Editor"
tags: [editor, table, sql]
---
Crate: `editors/table`. Fully Rust/Dioxus — no JS dependency. Two panels:

**Query panel** — a [[Code Editor]] embedding with `lang: sql | cypher | typeql`, a source picker, Run/Cancel, history (stored as nodes → linkable from wiki pages), and a warning ribbon when the statement is classified as a write ([[LLM and RAG]] uses the same classifier).

**Grid** — virtualised rows *and* columns; only the visible window is in the DOM. Typed cells from `Value`: `Ref` cells are links into the graph, `Vector` cells show a sparkline, `DateTime` localised. Inline edits accumulate into one `Transaction` with expected versions; refused ops show per-cell.

Because the grid renders any `QueryResult::Rows`, it is also "open as table" for a `GraphView`, a Redis hash, or a CSV in a folder (via DuckDB, [[Data Sources]]).

## As built (2026-10-14)
The grid is virtualised in rows and columns: only the cells under the viewport, plus a little overscan, are in the DOM. It has a sticky header and row-number column, sort by header click (ascending, descending, off; NULLs last), resizable columns, a row filter, and a keyboard cell cursor that scrolls into view. Sort and filter apply to the loaded rows. The SQL sources now return up to 50 000 rows (was 500).

Not yet:
- typed cells beyond NULL, numbers and bytes (`Value` has no `Ref`, `Vector` or `DateTime` yet);
- inline edits;
- pushdown;
- "open as table" for other sources.

Prior art for what comes next (the Supabase table editor) will be studied for requirements only; what Moonkale rejects goes to [[Scrutiny]].

## Pushdown
Sort/filter/paginate are sent to the source when `Capabilities` allow; otherwise applied client-side with a "partial" indicator. Keyset pagination, never OFFSET on large tables.

## Why not AG Grid / a JS grid
The grid is where Rust-native pays off immediately: it is data-heavy, not text-input-heavy, and Dioxus signals + virtualisation are sufficient. Keeping it Rust also means it works identically in a wasm extension's `moonkale_shell::Tree` (the `Table` primitive is this component).
