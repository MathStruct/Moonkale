---
title: "Milestone 2 — Implementation Log"
description: What was built for "Graph appears" so far, what deviated from the plan, and the problems hit.
tags: [milestone, log]
---
Plan: [[Milestone 2 - Graph Appears]].

> [!success] Done (2026-09-18)
> All seven steps are implemented. The **index** (wiki-links + tree-sitter Rust symbols) builds on folder open and refreshes on save; the **Graph panel** draws it with the **wgpu renderer wasm module** — confirmed rendering in Firefox by Daniel (WebGL/WebGPU are absent in the headless test browser, so the automated suite verifies everything up to the GPU and a legible failure beyond it); the **Links panel** shows backlinks/outgoing/unresolved for the active document; a **`.sqlite` file** opens as a database source whose tables open in a read-only **table editor** with a SQL box. Five Playwright suites pass (`packages/web/tests/e2e/`), 9 integration + 17 unit tests, clippy/fmt clean. **Desktop confirmed by Daniel (2026-09-18): the app runs and the graph renders inside the WebKitGTK webview** — ADR-0011 plan A works on Linux without the native-overlay fallback.

## Steps as executed

| # | step | outcome | notes |
|---|---|---|---|
| 1 | `core`: `Query::{Neighbours, All, Text}`, `Value`, `Table`, `TextDialect`, `Source::refresh` | ✅ 8 unit tests | folder source answers `Neighbours` as children, refuses `All`/`Text` |
| 2 | `moonkale-index` | ✅ 4 integration + 2 unit tests | [index.md](https://github.com/MathStruct/Moonkale/blob/master/packages/index/index.md) |
| 3 | platforms: `open_folder` → `[folder, index]`; server `refresh_source`; refresh after save; `graph_epoch` | ✅ all targets | web build gets the index as a second `RemoteSource` |
| 4 | `moonkale-graph-render` wasm module (wgpu 30, force layout, camera, labels overlay, pointer events) | ✅ builds, 4 native unit tests, 2.4 MB module | [graph-render.md](https://github.com/MathStruct/Moonkale/blob/master/packages/editors/graph-render/graph-render.md) |
| 5 | `moonkale-editor-graph` panel (host) | ✅ E2E `packages/web/tests/e2e/graph.mjs` | [graph.md](https://github.com/MathStruct/Moonkale/blob/master/packages/editors/graph/graph.md) |
| 6 | `moonkale-editor-markdown` Links panel | ✅ E2E `links-sqlite.mjs` | [markdown.md](https://github.com/MathStruct/Moonkale/blob/master/packages/editors/markdown/markdown.md) |
| 7 | `moonkale-sources-sql` SQLite + `moonkale-editor-table` | ✅ 3 tests + E2E | [sources-sql.md](https://github.com/MathStruct/Moonkale/blob/master/packages/sources-sql/sources-sql.md), [table.md](https://github.com/MathStruct/Moonkale/blob/master/packages/editors/table/table.md) |

## Deviations from the plan
1. **Index stats live in the descriptor's display name** ("index: 5 files · 5 links · 5 symbols") so the status bar shows them without a new API. Cheap; to be replaced by a proper stats query.
2. **Symbol keys carry the line number** (`src/lib.rs#fn:new@3`) so two `fn new` in different `impl`s stay distinct. Stable until code moves — good enough until the index persists.
3. **Graph nodes are indexed by position** in the JSON sent to the renderer (`edges: [{a, b}]`), not by id — halves the payload.
4. **No text on the GPU**: labels are a 2D-canvas overlay, capped at ~400 and hidden when zoomed out.
5. **Tables open as `Workspace::views`**, not documents: `open_node` routes `NodeKind::Table` into a separate list the table extension claims. Same mechanism will serve graph views and rows.
6. **`.sqlite` files are opened by clicking them in the Explorer** (path = folder root + relative key, so the same click works on web through the server's `MOONKALE_ROOT` check); no separate "connect to database" dialog yet.
7. **Columns are graph nodes** (`NodeKind::Column`, `Database` added to the open enum) so a database schema is a graph; the Graph panel does not show non-index sources yet.

## Problems hit (→ [[Problem Log]])
- **P-046 Random ids don't survive hydration.** The panel's canvas ids were derived from the (random) `WindowId`; the server rendered one id, hydration kept it, the client script looked for another and returned silently. Same root cause as P-034. Rule: element ids used from `eval` must be deterministic.
- **P-047 Eval messages are lost before the host awaits / after the script returns.** A message sent by JS before the Rust task has polled `recv()` was dropped, and a script that `return`s right after `dioxus.send` loses that message too. Pattern now: JS waits for an `init` handshake first, and a failing script parks on `recv()` until the host sends `destroy`. Also: don't mount an eval from a `use_effect` that writes a signal it reads — mount from the element's `onmounted` handler.
- **P-048 `dx serve` keeps missing non-rsx Rust changes** (third occurrence); every change to the eval script constant needed a restart. Costly: ~3 min per cycle.
- **P-049 Headless Firefox has no GL** on this machine (`FEATURE_FAILURE_WEBGL_EXHAUSTED_DRIVERS`, even with software-rendering prefs and Mesa env), and `wgpu` hangs rather than fails when a surface can't get a context. The panel now probes for WebGL2/WebGPU before calling `create()` and shows the reason.
- **P-051 Graph view jumped back after drags** (auto-fit on every settle; dragged nodes unpinned) — fixed after Daniel's test.
- **P-052 A new side panel stole the active tab**: panels attached to a tile become active; the default layout now lists `["explorer", "links"]` explicitly.
- **P-053 `dragend` not delivered after a drop on the workbench's drop zone** (Firefox): the session drag now also ends on any `drop` in the source window.
- **P-050 wgpu 30 API drift** vs. what I remembered (`InstanceDescriptor::new_without_display_handle`, `apply_limit_buckets`, `color_space`, `Option<&BindGroupLayout>`, `CurrentSurfaceTexture`, `queue.present`). Read the registry source, don't guess.

## Decisions worth keeping
- **Renderer as a standalone wasm module** loaded like `codemirror.js` — the same artefact works in the browser and inside the desktop webview (ADR-0011 plan A). Built by `packages/editors/graph-render/build.sh` with dx's `wasm-bindgen` into `editors/graph/assets/`; committed.
- **`wasm-release` profile** (`opt-level = "s"`, fat LTO, `panic = "abort"`, strip): 4.3 MB → 2.4 MB. `wasm-opt -Oz` would help further; not installed here.
- **`require_git(false)`**-style honesty for the index: `.git`, `target`, `node_modules` are skipped even when not ignored; ≤ 5000 files, ≤ 512 KB per text file.
- **Backlinks = incoming `Links` edges** from `Neighbours{depth 1, In}` — the parent directory's `Contains` edge is incoming too, so consumers filter by edge kind.

## Verified by hand
Web (Firefox) and desktop (Linux, WebKitGTK): the graph renders, pan/zoom/drag work after P-051.

## What to look at in a real browser / on desktop
1. Open a folder with markdown and Rust files → status bar shows `index: …`.
2. Graph tab → nodes and edges should draw; the info line shows the backend (`browserwebgpu` or `gl`). If it shows an error instead, that text is the diagnosis.
3. Hover → popup; double-click a file → opens in the editor; drag nodes; wheel zooms; *Local* follows the active document.
