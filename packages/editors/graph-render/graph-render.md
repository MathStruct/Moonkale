---
title: "graph-render — implementation notes"
tags: [crate-notes, milestone-2]
---
Notes for `moonkale-graph-render` (Milestone 2). Design: [[Graph View]], [[ADR-0011 Desktop graph surface strategy]] (plan A).

**A standalone wasm module**, not part of the app's wasm: built by `build.sh` (`cargo build --profile wasm-release --target wasm32-unknown-unknown` + `wasm-bindgen --target web`) into `../graph/assets/graph_render.js` + `graph_render_bg.wasm` (2.4 MB), loaded by the host panel via dynamic `import()` — the same way in the browser and inside the desktop webview.

- `graph.rs` — input JSON (`nodes[{id,label,kind,key}]`, `edges[{a,b,kind}]` by index), colours by kind, deterministic spiral start positions, degree-based radius.
- `layout.rs` — Fruchterman–Reingold with cooling, gravity, pinned nodes; O(n²); stops at temperature < 0.3 or 600 iterations. Native unit tests.
- `camera.rs` — world↔screen, pan, zoom-at-pointer, fit, nearest-node hit test. Native unit tests.
- `render.rs` — wgpu 30: `new_instance_with_webgpu_detection` (WebGPU when real, else WebGL2), surface on the canvas, two instanced pipelines (edges as thin quads, nodes as SDF circles), alpha blending, camera uniform. `shaders.wgsl`.
- `web.rs` — `create(canvas, overlay, onEvent)` → `GraphView { set_graph, fit, relayout, resize, backend, node_count, node_screen_position, destroy }`; rAF loop (more layout iterations per frame for small graphs); pointer handlers (pan, drag node, hover, click/dblclick, wheel zoom); labels on a 2D-canvas overlay with LOD.
- Camera policy: auto-fit after `set_graph`/`relayout` once the layout settles, **never** after the user has panned, zoomed or dragged (P-051). Dragged nodes stay pinned until Relayout.
- Events: `ready{backend}`, `hover{id,label,nodeKind,key,x,y}`, `click{id}`, `dblclick{id}`, `settled`.

Known: `create()` hangs if the canvas can't get a GL context — the host probes first (P-049). No GPU text, no picking buffer (CPU hit test), O(n²) layout: all listed in [[Problem Ranking]] P-22.

**P-068 (Milestone 4):** node attributes use explicit offsets (0, 8, 16) because `NodeInst` pads `radius`; `vertex_attr_array!` had put the colour at 12, dropping the red channel of every node since Milestone 2.

**Colours (Milestone 3):** `InNode.color` / `InEdge.color` (`#rrggbb`, optional) override the kind-based palette so the host can colour per database label; `parse_hex` in `graph.rs`.

## Milestone 6: scale
- `quadtree.rs` — Barnes–Hut: a quadtree rebuilt every step, `force(i, x, y, k2, theta)` approximates cells with `size / d < θ` by their centre of mass; θ = 0.8 (unit tests compare θ = 0 with the exact sum). `layout.rs` uses it from `BARNES_HUT_FROM = 1500` nodes; below that the exact O(n²) loop stays (cheaper for small graphs). Iteration caps: 600, 250 above 10 000 nodes, 120 above 50 000.
- `web.rs`: labels off above 20 000 nodes; fewer layout iterations per frame for big graphs; `bench_layout(n, steps)` export (synthetic graph, ms/step) used by `packages/web/tests/e2e/bench.mjs`; `examples/bench_layout.rs` is the native twin.
- Numbers (ms/step, Ryzen 7 7800X3D): native 1k 1.2 · 10k 12.9 · 50k 79.4 · 100k 169.3; wasm/Firefox 1.8 · 15.3 · 85.7 · 189.3. Recorded in [[Milestone 6 - Implementation Log]]. WebGPU compute stays deferred (not needed).
