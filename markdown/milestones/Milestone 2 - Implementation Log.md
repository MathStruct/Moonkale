---
title: "Milestone 2 — Implementation Log"
description: What was built for "Graph appears" so far, what deviated from the plan, and the problems hit.
tags: [milestone, log]
---
Plan: [[Milestone 2 - Graph Appears]]. Running log; this milestone is **in progress**.

> [!info] Status (2026-09-17, checkpoint)
> Steps 1–5 are implemented and verified on web as far as a headless browser allows: the **index** (wiki-links + tree-sitter Rust symbols) builds on folder open and refreshes on save; the **Graph panel** queries it, loads the **wgpu renderer wasm module**, applies filters, and falls back legibly where the browser has no GPU API. The test machine's headless Firefox has **no WebGL or WebGPU at all**, so actual rendering is unverified here and needs a look in a real browser / the desktop app. Steps 6 (backlinks) and 7 (SQLite + table editor) are next.

## Steps as executed

| # | step | outcome | notes |
|---|---|---|---|
| 1 | `core`: `Query::{Neighbours, All, Text}`, `Value`, `Table`, `TextDialect`, `Source::refresh` | ✅ 8 unit tests | folder source answers `Neighbours` as children, refuses `All`/`Text` |
| 2 | `moonkale-index` | ✅ 4 integration + 2 unit tests | [index.md](https://github.com/MathStruct/Moonkale/blob/master/packages/index/index.md) |
| 3 | platforms: `open_folder` → `[folder, index]`; server `refresh_source`; refresh after save; `graph_epoch` | ✅ all targets | web build gets the index as a second `RemoteSource` |
| 4 | `moonkale-graph-render` wasm module (wgpu 30, force layout, camera, labels overlay, pointer events) | ✅ builds, 4 native unit tests, 2.4 MB module | [graph-render.md](https://github.com/MathStruct/Moonkale/blob/master/packages/editors/graph-render/graph-render.md) |
| 5 | `moonkale-editor-graph` panel (host) | ✅ E2E `packages/web/tests/e2e/graph.mjs` | [graph.md](https://github.com/MathStruct/Moonkale/blob/master/packages/editors/graph/graph.md) |
| 6 | backlinks panel | ⏳ | |
| 7 | SQLite source + table editor | ⏳ | |

## Deviations from the plan
1. **Index stats live in the descriptor's display name** ("index: 5 files · 5 links · 5 symbols") so the status bar shows them without a new API. Cheap; to be replaced by a proper stats query.
2. **Symbol keys carry the line number** (`src/lib.rs#fn:new@3`) so two `fn new` in different `impl`s stay distinct. Stable until code moves — good enough until the index persists.
3. **Graph nodes are indexed by position** in the JSON sent to the renderer (`edges: [{a, b}]`), not by id — halves the payload.
4. **No text on the GPU**: labels are a 2D-canvas overlay, capped at ~400 and hidden when zoomed out.

## Problems hit (→ [[Problem Log]])
- **P-046 Random ids don't survive hydration.** The panel's canvas ids were derived from the (random) `WindowId`; the server rendered one id, hydration kept it, the client script looked for another and returned silently. Same root cause as P-034. Rule: element ids used from `eval` must be deterministic.
- **P-047 Eval messages are lost before the host awaits / after the script returns.** A message sent by JS before the Rust task has polled `recv()` was dropped, and a script that `return`s right after `dioxus.send` loses that message too. Pattern now: JS waits for an `init` handshake first, and a failing script parks on `recv()` until the host sends `destroy`. Also: don't mount an eval from a `use_effect` that writes a signal it reads — mount from the element's `onmounted` handler.
- **P-048 `dx serve` keeps missing non-rsx Rust changes** (third occurrence); every change to the eval script constant needed a restart. Costly: ~3 min per cycle.
- **P-049 Headless Firefox has no GL** on this machine (`FEATURE_FAILURE_WEBGL_EXHAUSTED_DRIVERS`, even with software-rendering prefs and Mesa env), and `wgpu` hangs rather than fails when a surface can't get a context. The panel now probes for WebGL2/WebGPU before calling `create()` and shows the reason.
- **P-050 wgpu 30 API drift** vs. what I remembered (`InstanceDescriptor::new_without_display_handle`, `apply_limit_buckets`, `color_space`, `Option<&BindGroupLayout>`, `CurrentSurfaceTexture`, `queue.present`). Read the registry source, don't guess.

## Decisions worth keeping
- **Renderer as a standalone wasm module** loaded like `codemirror.js` — the same artefact works in the browser and inside the desktop webview (ADR-0011 plan A). Built by `packages/editors/graph-render/build.sh` with dx's `wasm-bindgen` into `editors/graph/assets/`; committed.
- **`wasm-release` profile** (`opt-level = "s"`, fat LTO, `panic = "abort"`, strip): 4.3 MB → 2.4 MB. `wasm-opt -Oz` would help further; not installed here.
- **`require_git(false)`**-style honesty for the index: `.git`, `target`, `node_modules` are skipped even when not ignored; ≤ 5000 files, ≤ 512 KB per text file.
- **Backlinks = incoming `Links` edges** from `Neighbours{depth 1, In}` — the parent directory's `Contains` edge is incoming too, so consumers filter by edge kind.

## What to look at in a real browser / on desktop
1. Open a folder with markdown and Rust files → status bar shows `index: …`.
2. Graph tab → nodes and edges should draw; the info line shows the backend (`browserwebgpu` or `gl`). If it shows an error instead, that text is the diagnosis.
3. Hover → popup; double-click a file → opens in the editor; drag nodes; wheel zooms; *Local* follows the active document.
