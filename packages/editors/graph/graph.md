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

## Milestone 9
- Android WebView: asset URLs are resolved against `document.baseURI || location.href` before `import()` / `init` (an eval script has no base URL there, P-088); when the user agent is an Android WebView (`Android` + `wv`) `create()` gets `prefer: "gl"` (P-089). The stylesheet goes through `moonkale_ext_api::Stylesheet` (P-087).

## Spec 020 (2026-09-21)
Every open folder's index is drawn in one graph: `panel.rs` queries each `SourceFamily::Index` source (the same `Query::All` / `Query::Neighbours`), merges the results (ids are derived per index, so they never collide) and colours nodes per folder with `label_color(folder name)` + a legend when more than one is open. The source picker lists `folder: <name>` per index next to "all folders" (`picked_index` narrows the load to one); databases and traces are unchanged. Cross-folder edges do not exist yet (each index resolves links inside its own folder — [[Projects and Sources]]).

## P-224 — load profiling (2026-10-22)

Set `MOONKALE_GRAPH_PROFILE=1` when launching the desktop app to log numeric
query, payload-build, eval handoff and engine-stage timings. No graph contents
are logged. `load_stats()` in the renderer separates parsing/building/fit from
first layout and draw submission. Desktop handoff uses wall-clock timestamps
(the clocks share a machine); engine stages use `performance.now()`.

The ignored `profile_store_payload` test opens only an explicitly supplied store
copy (`MOONKALE_GRAPH_STORE`), times cold/warm `Query::All`, the actual panel
projection and Dioxus's JSON serialization stages, and writes the renderer
payload to `MOONKALE_GRAPH_PAYLOAD`. Run with `cargo test -p moonkale-editor-graph
profile_store_payload -- --ignored --nocapture`. The copied 100,000-node Sophia
store produces **14,188,850 bytes** of renderer JSON; the earlier 50 MB figure
was the complete source result, whose properties are not sent to the engine.

`ToJs::SetGraph.graph` is now a serialized JSON **string**. It is still ordinary
serde-escaped data in `Eval::send`, never interpolated into executable code. The
view passes it directly to the existing renderer JSON parser. This avoids
Dioxus's per-node `Value` object tree and a second JS stringify. Database nodes
move into `shown` after serialization instead of cloning all properties. No
source content or identity is discarded. A Unicode/quotes/script-like-content
round-trip test checks the changed boundary.

Identical in-flight database reads are coalesced using the source Arc identity,
query, graph epoch and current load generation. Changed queries/epochs and
replacement source handles still supersede old reads; navigating away invalidates
a pending generation, so returning cannot accidentally reuse a discarded load.
Completion or a current error releases the pending key. Stale errors do not
replace the current graph's error state. The key policy has a regression test.

## WP-07 — e-graph snapshots (2026-10-23)

The existing trace paste box also offers **Draw e-graph JSON**. The fixed eval
loop sends the string to the engine's bounded `set_egraph` parser; it never
interpolates graph text into script. This transient view pauses source reloads
and invalidates pending load generations. A class picker, per-class/all-class
collapse/expand, provenance visibility and **Back to sources** drive engine
methods. Engine count reports update the readout after each projection. Invalid
input reports a localised status and resumes normal source loading. Opening no
source is valid: a pasted snapshot suppresses the source-empty overlay.

The wire format, limits and fixture live in `packages/graph-render/graph-render.md`.
The engine keeps rewrite highlighting separate from the user's selection and
uses its existing group/layer/delta renderer. The panel adds no source, file
opener, persistence or Sophia-specific contract. `graph-egraph-ui.mjs` exercises
this production path; `graph-egraph.mjs` probes the committed engine separately.

## WP-06 source tier and region navigation

`tier.rs` reads the complete source count from the coarse query's table and
chooses a tier once at open/Reload. Panel-local per-source caches preserve the
choice across change-feed updates; explicit overrides clear the measurement.
Sources refusing hierarchy queries use capped detailed results with a lower
bound indicator. Schema, Local and explicit query views retain their semantics.

`coarse.js` is a static host controller, inserted into the existing eval script.
It uses existing node positions, `apply_delta` and visual groups, pins context,
and restores representatives and proxy boundaries on collapse. Only one region
is open. Contents remain serialized channel data, never executable source.
`graph-coarse.mjs` uses a separate test fixture crate to exercise this panel.

P-233: experimental structural controls resolve the visible selection through
`Workspace::index_node` before enabling edits. The renderer retains Sophia IDs;
reviews retain index provenance and the existing revision checks. Async mapping
results are keyed to the current selection. Shared content bodies with several
file locations have no automatic target.
The review dropdown marks its selected option explicitly when asynchronous
function options arrive, so the visible choice matches the review target.


WP-61 records camera state at pointer/wheel gesture boundaries, debounced
180 ms, into the workspace's bounded visit history. Restore uses the engine's
finite numeric API. Middle-click emits `openBeside`; file/symbol ownership is
resolved through the existing source/index paths and loaded without activation.
The left-hand preset gives the focused overlay WASD/QE/F/Space/Tab controls;
G and the rebindable F16 command toggle Local/Whole.
