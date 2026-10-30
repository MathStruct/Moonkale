---
title: "editor-table — implementation notes"
tags: [crate-notes, milestone-2]
---
Notes for `moonkale-editor-table` (Milestone 2). Design: [[Table Editor]].

`TableExtension` contributes one closable panel per `Table` node in `Workspace::docs.views` (non-text nodes opened via `open_node`, which routes `Table` there instead of into a `Document`). `TablePanel` holds a SQL textarea prefilled with `SELECT * FROM "table" LIMIT 1000`, runs `Query::Text { dialect: "sql" }` on the node's source (Ctrl+Enter or the button), and renders the `Table` in a **virtualised grid** (2026-10-14). Only the rows and columns under the viewport, plus a little overscan, are in the DOM, so the SQL sources' cap could rise from 500 to 50 000 rows. The grid has a sticky header and row numbers, NULL/number/bytes cell styles, and a truncation note. Clicking a header sorts (ascending, descending, off; NULLs last), dragging its edge resizes, and a filter box narrows the rows. A keyboard cursor moves with the arrows, Page Up/Down, Home/End and Ctrl+Home/End, scrolling the cell into view. Sort and filter apply to the loaded rows. The arithmetic (windows, widths, order, moves, reveal) is `grid.rs`, unit-tested without a DOM; `table-grid.mjs` scrolls a 50 000-row result in the browser. Raw query text is read-only; the error text is shown verbatim. A source that answers two dialects (`Capabilities::text_dialects`, a Sophia store's `sql` and `query`) gets one button per dialect before the box. Switching runs the query in the new dialect, and an untouched default text follows it (`select p, d where name(p, d)` for `query`). Not yet: pushdown sort/filter/pagination.

**Milestone 3:** the query language follows the source's `capabilities.text_query` (`sql` or `cypher`; the default query is `MATCH (n:Table) RETURN n LIMIT 200` for Cypher). When a result carries nodes/edges (Ladybug path queries), a **Show in Graph** button sets `Workspace::graph_request`, and the Graph panel switches its source picker to that database and draws the query's result.


WP-61's left-hand preset moves the focused grid cursor with WASD, pages
with Q/E, focuses the local filter with F and advances cells with Tab/Shift+Tab.
Filter/query fields retain letters. F19 reruns only the active table query.
Middle-clicking a Ref opens its source-known target in a background tab.


WP-63 (2026-10-28): panel actions use `icons::ToolbarActions` for the Simple/Detailed density and the extension’s user-selected chrome. Secondary controls stay reachable through a flat ⋯ disclosure. Terminal session tabs and document/query content remain navigation/content. See [[ADR-0027 Simple by default]].


WP-59 publishes the current result subgraph to its linked graph and synchronizes Ref-valued row selections with group selection. Query results remain derived, non-persisted state.


WP-14 (2026-10-30): the unchanged default SQL table query uses
`Source::read_rows` to obtain authoritative keys and row versions. Arbitrary
query results stay derived read-only tables. `edit.rs` renders typed cell drafts,
Enter/save, Escape/cancel, Tab/Shift+Tab navigation with viewport reveal, per-cell refusal/conflict and a bounded
checked undo stack. Sorting/filtering observe edited values. Keep mine prepares
another checked save; keep current reloads the cell. Session/source identities
are retained across asynchronous requests. All labels are localized in en/de/zh-CN.


Inline contexts retain the reader's source handle and reject replaced/closed
sources. Rendered cells share one pointer-compared row snapshot per visible row;
draft keystrokes do not clone every column. Native originals in `stored` restore
SQLite date text exactly during undo. A single active draft and a per-table busy
guard prevent overlapping control actions; scope cancellation releases the guard.
