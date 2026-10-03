# moonkale-graph-render

The graph view of [Moonkale](https://github.com/MathStruct/Moonkale): a force-directed layout (Barnes–Hut), a 2D/3D camera, hit-testing, and a `wgpu` renderer (WebGPU, or WebGL2 where WebGPU is missing). It depends on **no Moonkale crate** and knows nothing about Moonkale's model: a graph goes in, events come out.

Two ways to use it:

**From Rust** — the layout, camera and hit-testing, without a browser (`scene`):

```rust
use moonkale_graph_render::graph::{InEdge, InGraph, InNode};
use moonkale_graph_render::scene::{Event, Scene};

let mut scene = Scene::new(InGraph { nodes, edges });
scene.resize(800.0, 600.0);
scene.settle(2_000);          // run the force layout until it rests
scene.fit();                  // camera around the whole graph
if let Some(Event::Hover { id: Some(id), .. }) = scene.pointer_move(x, y) { /* … */ }
```

`cargo run -p moonkale-graph-render --example headless` lays out a small factor graph and prints positions and events.

**In a page** — the wasm module (`build.sh` builds `graph_render.js` + `graph_render_bg.wasm` with `wasm-bindgen --target web`): `create(canvas, overlay, on_event)` returns a `GraphView` with `set_graph(json)`, `set_graph_fresh(json)`, `fit()`, `relayout()`, `resize(w, h, dpr)`, `set_mode("2d" | "3d")`, `stash(name)` / `unstash(name)`, `node_screen_position(id)`, `destroy()`. `on_event` receives the same events `scene::Event` serializes to: `{"kind": "ready"}`, `{"kind": "hover", "id", "label", "nodeKind", "key", "x", "y"}` (`id: null` when the pointer leaves), `{"kind": "click" | "dblclick", "id"}`, `{"kind": "settled"}`.

## The graph

```json
{ "nodes": [{ "id": "a", "label": "A", "kind": "file", "key": "a.md", "color": "#6ea8fe" }],
  "edges": [{ "a": 0, "b": 1, "kind": "link", "color": null }] }
```

Edges refer to nodes by index; `kind` picks a default colour and (in 3D) a layer; `color` (hex) overrides it.

## Using it outside Moonkale

Depend on it by git tag (no crates.io release — Moonkale's decision of 2026-10-01):

```toml
moonkale-graph-render = { git = "https://github.com/MathStruct/Moonkale", tag = "lib-v1" }
```

`lib-vN` tags mark states where `moonkale-core`, `moonkale-ext-api` and this crate are compatible; see `packages/ext-api/CHANGELOG.md` in the repository.

License: as the Moonkale repository.
