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
- Events: `ready{backend}`, `hover{id,label,nodeKind,key,x,y}`, `click{id}`, `dblclick{id}`, `settled`.

Known: `create()` hangs if the canvas can't get a GL context — the host probes first (P-049). No GPU text, no picking buffer (CPU hit test), O(n²) layout: all listed in [[Problem Ranking]] P-22.
