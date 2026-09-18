---
title: "editor-table — implementation notes"
tags: [crate-notes, milestone-2]
---
Notes for `moonkale-editor-table` (Milestone 2). Design: [[Table Editor]].

`TableExtension` contributes one closable panel per `Table` node in `Workspace::views` (non-text nodes opened via `open_node`, which routes `Table` there instead of into a `Document`). `TablePanel` holds a SQL textarea prefilled with `SELECT * FROM "table" LIMIT 200`, runs `Query::Text { dialect: "sql" }` on the node's source (Ctrl+Enter or the button), and renders the `Table` as a plain HTML grid (sticky header, NULL/number cell styles, truncation note). The source enforces read-only; the error text is shown verbatim. Not yet: virtualisation, pushdown sort/filter, editing.

**Milestone 3:** the query language follows the source's `capabilities.text_query` (`sql` or `cypher`; the default query is `MATCH (n:Table) RETURN n LIMIT 200` for Cypher). When a result carries nodes/edges (Ladybug path queries), a **Show in Graph** button sets `Workspace::graph_request`, and the Graph panel switches its source picker to that database and draws the query's result.
