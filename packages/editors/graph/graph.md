---
title: "editor-graph — implementation notes"
tags: [crate-notes, milestone-2]
---
Notes for `moonkale-editor-graph` (Milestone 2), the Dioxus host of the renderer module.

- `extension.rs` — one non-closable "Graph" panel, home `main`.
- `panel.rs` — toolbar (Whole / Local, kind filters, Fit, Relayout, counts + backend readout), two stacked canvases (wgpu + label overlay), hover popup, empty/error states. Mounted from the host div's **`onmounted`** (not an effect — P-047) with a deterministic element id (P-046). The eval script: handshake → `import()` the module and init the wasm (reports `loaded`) → probe WebGL2/WebGPU → `create()` with a 15 s timeout → `ResizeObserver` → command loop (`setGraph`, `fit`, `relayout`, `destroy`). Errors park the script until `destroy` so the message isn't lost.
- Data: `Query::All{limit 3000, kinds}` (Whole) or `Query::Neighbours{active, depth 2, Both}` (Local) on the index; nodes → JSON by position; the panel keeps an id → `Node` map so `dblclick` can open files (symbols open their defining file via an incoming `Defines` neighbour). Re-queries on: index appears, mode/filters, active document (Local), `ws.graph_epoch` (bumped after save+refresh).
- **Database modes (P-062):** with a database picked, the Whole/Local buttons become **Data** (default: `Query::All { limit 3000, kinds: [Vertex] }` — the stored nodes and relations), **Schema** (`Query::All { kinds: None }`) and **Query** (the last *Show in Graph* request). Vertices are coloured by node table and relations by name through a 12-colour hash palette (`label_color`), sent to the renderer as `color: "#rrggbb"` per node/edge; a legend lists the labels. The group comes from the vertex key `v:<Label>:…` written by `sources-graph::ladybug`.
- **Source picker (Milestone 3):** when a database source is open (`SourceFamily::Graph | Sql`), a `<select>` chooses what to draw — the index (default, with the kind filters) or a database (its schema graph via `Query::All`; Local = one hop around the active node). A **query** checkbox appears once a table editor sent `Workspace::graph_request` ("Show in Graph"): the panel then runs that `Query::Text` on the database and draws the returned nodes/edges (Cypher paths). A new request switches the picker automatically, and opening a graph database (`SourceFamily::Graph`) selects it too — otherwise the picker sits on *index* and the database's internals are invisible (P-060). The table editor's *Show in Graph* also dispatches `Command::ShowPanel("graph")` so the tab comes forward. Renderer colours: `vertex` orange, `database` red, `column` lilac, `custom` edges orange. Double-click on a `Table` opens the table editor. E2E: `packages/web/tests/e2e/ladybug.mjs`.
- **Teardown (P-061):** the eval script drops the WebGL2 probe context right after probing and loses the renderer's context on `pagehide`. That did not stop the NVIDIA exit crash (WebKit's own teardown is what crashes); the desktop launcher mutes those dumps instead.
- Diagnostics in the DOM: `.mk-graph-info[data-nodes][data-backend][data-module]` and `.mk-graph-error` — what `packages/web/tests/e2e/graph.mjs` asserts.

## Milestone 4
- **Trace sources** (`SourceFamily::Custom("trace")`) are pickable and auto-picked like graph databases; the mode buttons give way to a "files → frames → call chain" hint. Double-click on a trace node opens `path[:line[:col]]` in the folder via `open_relative_path` + `reveal`.
- **Trace…** toolbar button: a paste box; *Draw* parses the text with `moonkale-trace` and adds one source per trace.
- Loads carry a generation counter (`load_gen`): an older async reply (a remote index) can no longer overwrite a newer one (a local trace) — P-063.

## Milestone 6
- `GRAPH_LIMIT = 100_000` nodes per query (`Query::All { limit }`), up from 3 000; the renderer's LOD handles the rest. The `truncated` flag still shows in the info line.

## Milestone 8
- **3D** toolbar toggle → `ToJs::SetMode`; `.mk-graph-info[data-mode]` reports `2d`/`3d`.
