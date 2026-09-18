---
title: "editor-graph — implementation notes"
tags: [crate-notes, milestone-2]
---
Notes for `moonkale-editor-graph` (Milestone 2), the Dioxus host of the renderer module.

- `extension.rs` — one non-closable "Graph" panel, home `main`.
- `panel.rs` — toolbar (Whole / Local, kind filters, Fit, Relayout, counts + backend readout), two stacked canvases (wgpu + label overlay), hover popup, empty/error states. Mounted from the host div's **`onmounted`** (not an effect — P-047) with a deterministic element id (P-046). The eval script: handshake → `import()` the module and init the wasm (reports `loaded`) → probe WebGL2/WebGPU → `create()` with a 15 s timeout → `ResizeObserver` → command loop (`setGraph`, `fit`, `relayout`, `destroy`). Errors park the script until `destroy` so the message isn't lost.
- Data: `Query::All{limit 3000, kinds}` (Whole) or `Query::Neighbours{active, depth 2, Both}` (Local) on the index; nodes → JSON by position; the panel keeps an id → `Node` map so `dblclick` can open files (symbols open their defining file via an incoming `Defines` neighbour). Re-queries on: index appears, mode/filters, active document (Local), `ws.graph_epoch` (bumped after save+refresh).
- **Source picker (Milestone 3):** when a database source is open (`SourceFamily::Graph | Sql`), a `<select>` chooses what to draw — the index (default, with the kind filters) or a database (its schema graph via `Query::All`; Local = one hop around the active node). A **query** checkbox appears once a table editor sent `Workspace::graph_request` ("Show in Graph"): the panel then runs that `Query::Text` on the database and draws the returned nodes/edges (Cypher paths). A new request switches the picker automatically. Renderer colours: `vertex` orange, `database` red, `column` lilac, `custom` edges orange. Double-click on a `Table` opens the table editor. E2E: `packages/web/tests/e2e/ladybug.mjs`.
- Diagnostics in the DOM: `.mk-graph-info[data-nodes][data-backend][data-module]` and `.mk-graph-error` — what `packages/web/tests/e2e/graph.mjs` asserts.
