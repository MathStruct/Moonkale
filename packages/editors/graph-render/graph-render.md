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

## Milestone 8: 3D
- `graph.rs`: `Node.z` from `layer_z(kind, i)` — one plane per kind (directory −2, file −1, page 0, symbol 1, block 1.5, database −1.5, table −0.5, columns/rows/vertices/keys 0.5, commit 2; ×140 units) plus a deterministic jitter.
- `camera.rs`: `three_d`, `yaw`/`pitch`/`dist`, `cz`; `view_proj()` (column-major perspective × look-at, y-down world), `project(x, y, z) → (sx, sy, w)`, `size_factor(w)`, `orbit`, 3D `pan` (target moves in the camera plane), `zoom_at` (dolly), `fit` (bounding sphere in the 50° field), `hit` in projected space; `uniform()` is 28 floats (2D fields, mode, viewport, dist, `mat4`).
- `shaders.wgsl`: `project()` branches on `camera.mode`; nodes are screen-space billboards with radius attenuated by `dist / w`; edges get a screen-space normal from projected endpoints; both carry depth and a fog factor toward the background.
- `render.rs`: instance positions are `vec3`; a `Depth24Plus` buffer with `LessEqual` (2D writes depth 0.5 everywhere, so order still wins there).
- `web.rs`: `set_mode("2d"|"3d")`, `mode()`, `Drag::Orbit` (right button or Shift), context menu suppressed on the canvas, labels and `node_screen_position` through `project`, far labels only when hovered. Rebuild with `build.sh`.
- Tested: `camera` unit tests (projection centre, attenuation, behind-camera) and `graph3d.mjs` in Chromium + SwiftShader (P-082) — the first suite that runs the renderer.

## Milestone 9
`Camera::basis()` (forward/right/up in the y-down world) and `unproject(sx, sy, w)`; in 3D a node under the pointer drags in its own depth plane (`web.rs` `Drag::Node`), pinned like in 2D. Test `unproject_inverts_project_at_the_same_depth`.

Android (P-089–P-091): `create(canvas, overlay, onEvent, prefer)` takes an optional backend hint — `"gl"` makes `Renderer::new(.., gl_only = true)` skip WebGPU, which the Android WebView advertises but never finishes creating a device for. `Renderer::new` prefers the first **non-sRGB** surface format from `get_capabilities` (the shaders write linear colours; sRGB on WebGL2 gave a grey canvas). `resize()` re-runs `Camera::fit` while `auto_fit` is still set, so a canvas that gets its real size after the layout settled (a hidden phone tile) — or a panel the user resizes before touching the graph — stays fitted. The 2D fit scale clamps at 12 so a five-node vault fills a phone screen.

Touch (spec 006): `State.touches` tracks touch pointers (captured on down, dropped on up/cancel); two fingers → `Drag::Pinch { last_dist, last_angle, last_mid }` — `zoom_at(dist / last_dist, mid)`, `pan(Δmid)`, and in 3D `yaw -= Δangle`; one finger left continues as a pan without a click. `pointerleave` ignores captured touches. `GraphView::camera_state()` for tests.

Incremental `set_graph` (spec 017): with ≥ 50 % shared node ids, positions/pins carry over, new nodes start at their known neighbours' mean, the layout is warmed (temperature 3) only if the node set or edge count changed, and the camera is not touched; otherwise a fresh layout + fit as before.

Graph quality (2026-09-20): `layout.rs` — attraction divided by `1 + 0.5·ln(1+max degree)` (hubs pull less), gravity 0.12 for degree-0 nodes; `graph.rs` — `fit_bounds()` over the connected majority; `web.rs` — labels placed by importance with a rectangle collision test (≤ 400), `layout_state()` export.
