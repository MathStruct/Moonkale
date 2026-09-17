---
title: "Table Editor"
tags: [editor, table, sql]
---
Crate: `editors/table`. Fully Rust/Dioxus — no JS dependency. Two panels:

**Query panel** — a [[Code Editor]] embedding with `lang: sql | cypher | typeql`, a source picker, Run/Cancel, history (stored as nodes → linkable from wiki pages), and a warning ribbon when the statement is classified as a write ([[LLM and RAG]] uses the same classifier).

**Grid** — virtualised rows *and* columns; only the visible window is in the DOM. Typed cells from `Value`: `Ref` cells are links into the graph, `Vector` cells show a sparkline, `DateTime` localised. Inline edits accumulate into one `Transaction` with expected versions; refused ops show per-cell.

Because the grid renders any `QueryResult::Rows`, it is also "open as table" for a `GraphView`, a Redis hash, or a CSV in a folder (via DuckDB, [[Data Sources]]).

## Pushdown
Sort/filter/paginate are sent to the source when `Capabilities` allow; otherwise applied client-side with a "partial" indicator. Keyset pagination, never OFFSET on large tables.

## Why not AG Grid / a JS grid
The grid is where Rust-native pays off immediately: it is data-heavy, not text-input-heavy, and Dioxus signals + virtualisation are sufficient. Keeping it Rust also means it works identically in a wasm extension's `ui::Tree` (the `Table` primitive is this component).
