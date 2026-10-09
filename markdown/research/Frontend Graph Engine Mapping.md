---
title: "Frontend Graph Engine — mapping onto graph-render"
description: Maps the Frontend Graph Engine specification onto the existing Rust/wgpu renderer (packages/graph-render + editors/graph) and proposes minimal interfaces and a staged plan. No code yet.
tags: [research, graph, flow]
---
Spec [[031]] asks for one Rust frontend graph engine over the existing WebGL renderer: layered, hierarchical, port- and route-aware, LOD-disciplined, and shared by graph exploration and flow editing (the flow editor's move onto it is ADR-0018). This note maps every requirement onto what `packages/graph-render` (wasm, wgpu 30) and `packages/editors/graph` (Dioxus host) have today, proposes the smallest set of interfaces, and stages the work in testable steps. Nothing here rewrites the backend or touches storage, semantics or simulation.

## 1. What exists today

| Piece | File(s) | State |
|---|---|---|
| Wire format | `graph.rs` `InGraph { nodes: Vec<InNode{id,label,kind,key,color}>, edges: Vec<InEdge{a,b,kind,color}> }` | flat nodes + straight edges by index; JSON from the panel, incremental `set_graph` keeps ≥50 % shared positions (spec 017) |
| Internal graph | `graph.rs` `Node { id, x, y, z, radius, color, degree, pinned }`, `Edge { a, b, color }` | one instance per node; `a == b` **filtered out** (no self-loops); no ports, shapes, selection, hierarchy, layers |
| Layout | `layout.rs` FR + Barnes–Hut (`quadtree.rs`) | force-directed only; pinning exists (drag); 100 k nodes measured |
| Camera/picking | `camera.rs` | 2D/3D, `hit` = nearest node, CPU only; no edge/port picking, no GPU id buffer |
| Rendering | `render.rs` + `shaders.wgsl` | 2 instanced pipelines: SDF-circle node billboards, edge **quads** (straight, uniform width); depth buffer; alpha blend; instance buffers rebuilt and re-uploaded every frame |
| Labels | `web.rs` on a 2D-canvas overlay | importance-ordered, collision-tested, ≤ 400, LOD off above 20 k nodes |
| Rust API | `scene.rs` (Milestone 18) | `Scene` graph-in/events-out (`hover/click/dblclick/settled`), same protocol as wasm — the natural seam for extension |
| Interaction | `web.rs` | pan/zoom, node drag (id-stable since P-155), hover, click/dblclick, touch pinch, 3D orbit; no selection, multiselection, edge clicks, collapse |
| Host protocol | `panel.rs` eval script | commands `setGraph/stash/unstash/fit/relayout/setMode/destroy`; events `ready/hover/click/dblclick/settled` |
| Flow editor | `editors/flow` on `dioxus-flow` (DOM/SVG, ~1 k nodes, typed handles, layered auto-layout) | a second rendering stack — exactly what the spec asks to eventually unify, and what [[dioxus-flow]] recommended *against* unifying at Milestone 6 scale |

Key numbers to respect: 100 k nodes / 170 ms layout step native (Milestone 6 bench, `bench.mjs`); the engine must stay fluid at that scale after each stage.

## 2. Requirement → architecture mapping

**§1 Architecture (input / scene / layout / render separation; instance ≠ semantic id).**
The split already exists in embryo: `InGraph` (input) → `Graph` + `Layout` + `Camera` (scene, exposed as `Scene`) → `Renderer` (draw). Two changes, both additive:
- *Instance vs entity*: today `Node.id` is both. Add `InNode.entity: Option<String>` — instances keep unique ids (`trace[3]`, `frame@main#2`), the entity names what they reify (the same function appearing twice in a stack trace). `set_graph`'s incremental matching stays keyed on instance ids.
- *Styles/viewport vs topology*: node/edge presentation fields (below) are data, never topology — the renderer already keeps camera state out of `Graph`, and layers (§3) will follow the same rule: a layer hides or styles, it never removes.

**§2 Visual primitives.** All gaps; the renderer has circles and straight quads only.
- *Shapes/stroke*: extend the billboard SDF (circle → also rect, diamond, arbitrary convex polygon by per-instance shape id + half-extents); stroke = second SDF ring. Icons: draw on the existing 2D label overlay for hovered/selected/expanded nodes only (§6 forbids rich UI per node anyway).
- *Ports*: new `InPort { node, name, color, side, offset }`; edges gain `a_port/b_port`. Ports are instances on the node pipeline plus independent hit targets (`port_hit`, next to `camera.hit`).
- *Edges*: `InEdge` grows optional fields — `directed`, `width`, `dash`, `opacity`, `arrow`, `label`, `route: Straight | Bezier { tension } | Orthogonal { bends }`. Internally one edge becomes **N segment instances** (route tessellation; straight stays N = 1) plus optional arrowhead instances on the glyph pipeline. Parallel edges: at build time, edges sharing a pair get a curvature fan-out by index. Self-loops: stop filtering `a == b`, render as a small circle route at the node's rim.
- *Animated edges*: `InPulse { edge, speed, color }` → shader-animated particles along the segment range (time uniform; no CPU per-frame cost). LOD-gated (§6).
- *Compound nodes*: see §4.

**§3 Layers.** Nothing exists. `InLayer { id, name, color, visible, overlay }`; nodes/edges carry `layers: Vec<String>`. A shared edge drawn in two visible layers becomes two segment instances with a per-layer offset (parallel strokes) — built at frame-build time, so layers are pure presentation. Persistent vs temporary is host bookkeeping (an overlay layer can be dropped by id).
- *Stack traces*: an **ordered occurrence walk**, not an edge-id set: `InTrace { id, layer, steps: Vec<instance-id> }`. The renderer draws it as one polyline + hop markers over the base graph — order and revisits survive by construction because each step is an occurrence instance (entity ids repeat, instance ids don't).

**§4 Hierarchy, grouping, zoom.**
- *Structural expansion* is host-driven: the host sends the group's internals or doesn't (collapse = re-query with the group folded to one node). The renderer's job is only to draw **group hulls** (`InGroup { id, members, label, collapsed }` → a rounded-rect fill instance) and to keep `set_graph`'s incremental positioning so a collapse/expand doesn't scatter the map. Selection survives because selection is by instance id and the host keeps the group node's id stable across the fold.
- *Visual aggregation* is renderer-side LOD (§6 + the designed [[Case Selector]] coarse tier): below a pixel threshold, dense groups swap for aggregate nodes and bundled proxy edges. Substituting instances at frame-build time keeps this out of topology.
- *Focal layouts*: `fit`-to-subset already exists in embryo (`fit_bounds`); add `focus { ids }` that frames and centres on member bounds.

**§5 Layout and interaction.**
- *Pluggable layouts*: `Layout` becomes an enum (`Force`, later `Hierarchical`); `InNode.fixed: [f32; 2]` opts a node out of any layout (spatial hardware layouts, §7, keep their coordinates — the layout must never move fixed nodes; today only *pinned* drags are respected).
- *Interaction*: add selection (click = single, Ctrl = toggle, later marquee) as `HashSet<instance-id>` in state + `select` events; edge and port picking via CPU distance tests over viewport-culled segments first (GPU id picking stays an option, see staged plan).
- *Modes over one renderer*: `set_mode("explore" | "edit")` on `GraphView`. Edit mode: ports snap the hover, a drag from port to port emits `connect { from_port, to_port }`, new wires route orthogonally, optional grid snap. The flow editor's migration onto this is the final stage, not a prerequisite — dioxus-flow stays until parity (see staged plan).

**§6 LOD and performance.**
Today: label LOD, 2 px min node radius, iteration caps. Missing: the general rule. Add a `Lod` table computed from projected pixel size + per-tile density (quadtree exists) each frame: thresholds for ports, icons, edge labels, dashes, pulses, hull fills; below them, instances are not built (not merely hidden — never uploaded). Frustum-cull instances. Replace `draw`'s per-frame `create_buffer_init` with persistent buffers + partial `write_buffer` updates (positions change every layout step; styles rarely). Keep `bench.mjs` as the regression gate per stage.

**§7 Domain compatibility.** The wire format is the config surface: typed edges = `kind` + per-layer styles (RDF: one layer per predicate); call graphs = layers for call vs trace; flows = ports + orthogonal routes + edit mode; MTK/factor graphs = port sides + directed edges; hypergraphs/simplicial = `InHyperedge { members, style: Junction | Region }` and `InSimplex { members }` (junction = a small node + spokes, region = tinted hull — deliberately distinct instance classes so they can't be conflated); spatial layouts = `fixed` positions. Validation/simulation stay in the domains, outside the renderer, as the spec demands.

**3D.** The renderer has a 3D mode (`set_mode`, orbit camera, kind planes). Everything this note adds is 2D-first: routes, ports, hulls, orthogonal edges and edit mode are 2D-only — in 3D every edge stays a straight quad between endpoints, groups show as their members, and edit mode is refused (the mode stays 2D while it is active). Straight segments and pulses work in both (a pulse travels the projected path).

**Accessibility.** A canvas graph is invisible to keyboards and screen readers today. Two minimums, tracked as requirements rather than a stage: keyboard focus moves between nodes (arrows/Tab over the instance order, Enter = click), and the selection is described in text (an `aria-live` line naming the selected nodes/edges, mirroring what the events already carry).

## 3. Minimal interfaces (proposal)

All wire additions are `#[serde(default)]`, so the current panel — and every existing e2e suite — keeps working unchanged.

```rust
// graph.rs — the wire format grows, nothing breaks
pub struct InNode  { .., entity: Option<String>, shape: Option<InShape>,
                     size: Option<[f32; 2]>, stroke: Option<InStroke>,
                     icon: Option<String>, fixed: Option<[f32; 2]>,
                     layers: Vec<String> }
pub struct InEdge  { .., a_port: Option<String>, b_port: Option<String>,
                     directed: bool, width: Option<f32>, dash: Option<f32>,
                     opacity: Option<f32>, arrow: Option<InArrow>,
                     label: Option<String>, route: Option<InRoute>,
                     layers: Vec<String> }
pub struct InGraph { .., ports: Vec<InPort>, groups: Vec<InGroup>,
                     layers: Vec<InLayer>, traces: Vec<InTrace>,
                     hyperedges: Vec<InHyperedge>, simplices: Vec<InSimplex>,
                     pulses: Vec<InPulse> }

// frame.rs — new: pure, unit-testable scene → instance lists (LOD, culling,
// parallel fan-out, route tessellation, layer duplication all live here)
pub struct Frame { nodes: Vec<NodeInst>, segs: Vec<SegInst>,
                   glyphs: Vec<GlyphInst>,   // arrowheads, ports, junction dots
                   fills: Vec<FillInst>,     // hulls, simplex faces, regions
                   pulses: Vec<PulseInst> }
pub fn build_frame(graph: &Graph, pres: &Presentation, lod: &Lod) -> Frame

// scene.rs / web.rs — protocol additions
// in:  set_layer_visibility, toggle_group, set_mode("explore"|"edit"), focus(ids)
// out: select { ids }, port_hover { node, port }, connect { from, to }
```

`render.rs` then draws one pipeline per instance class and uploads ranges into persistent buffers. Native unit tests target `build_frame` (counts, fan-out geometry, LOD cutoffs) — the same style as the existing `scene.rs`/`camera.rs` tests — and `graph3d.mjs`/`graph.mjs` gain per-stage e2e steps.

*Updates at scale:* every update re-sends the whole graph as JSON today, which will not stay fluid at 100 k nodes under edit-mode-frequency changes. A delta form — add/remove/change **by instance id**, next to the incremental `set_graph` that matches whole graphs — belongs in the same command set; it is the wire-level twin of ADR-0015's `changes_since` model and can be introduced per stage without changing the shapes above.

## 4. Staged plan

| Stage | Delivers | Acceptance | Tests |
|---|---|---|---|
| 0 — Persistent buffers | replace `draw`'s per-frame `create_buffer_init` with persistent buffers + partial `write_buffer` updates (positions change every layout step, styles rarely) | none by itself, but it removes today's per-frame cost at 100 k and depends on nothing else — do it first and measure | **Done 2026-10-08**: geometry/appearance split per pipeline, revision-gated uploads; `bench-draw.mjs` (new) on SwiftShader: settling 1.91 → 1.14 ms/frame, hover 1.88 → 0.89 ms; graph, graph-local, graph3d suites pass |
| 1 — Edges & selection | segment instances, width/dash/opacity/arrow, parallel fan-out, self-loops, edge labels on hover, CPU edge/port-free picking, selection + `select` events | §8.1 (partly: edges selectable), §8.2 (routes minus orthogonal) | **Done 2026-10-08**: `frame.rs` (build_frame + edge_at, 8 native tests), three pipelines (segments with width/dash, arrowheads, nodes), id-based selection with `select` events and a drag guard; `graph-edges.mjs` picks parallels apart, toggles with Ctrl, drags nothing, picks a self-loop; a naga shader-validation test gates WGSL typos natively. Edge hover labels wait for the panel's popup work |
| 2 — Ports & edit mode | `InPort`, port hit targets, `set_interaction("edit")` (`set_mode` stays 2D/3D), connect gesture + `connect` event | §8.1 (ports), §8.7 (mode switch on the same renderer) | **Done 2026-10-09**: ports draw on the node pipeline (edit mode), edges anchor at ports (stale names fall back to the node), `Drag::Wire` emits `connect` — the host adds the edge, never the renderer; 4 native tests, `graph-edit.mjs` (ports only in edit, camera preserved, connect + cancel); all five graph suites pass |
| 3 — Layers & traces | `InLayer` visibility/commands, offset parallel strokes, `InTrace` ordered walks, overlays | §8.3 | **Done 2026-10-09**: shared edges draw one offset stroke per visible layer, traces walk node ids in order with a head per hop (never picked), hidden layers drop their strokes/nodes and answer no hits; 4 native tests + `graph-layers.mjs` (both layers on: 21 segments, 4 heads; toggles remove exactly each layer's share); all six graph suites pass |
| 4 — Groups & LOD | group hulls, collapse/expand via host + incremental set_graph, LOD table (ports/icons/labels/dashes/pulses), frustum culling, `focus` | §8.5, §8.6 | bench.mjs stays within budget; e2e collapse keeps selection + external edges |
| 5 — Higher-order | hyperedge junctions/regions, simplices, clique hulls | §8.4 | unit: distinct instance classes; e2e shows all three differently |
| 6 — Pulses | shader-animated edge particles, LOD-gated | §8.2 (variable speed) | e2e + a GPU-frame sanity check |
| 7 — Flow migration | hierarchical layout, orthogonal routing, port-typed wiring parity — including **parameter editing**: an HTML editor shown only for the selected node, positioned from `Scene::screen_position` (the spec forbids rich per-node UI). The flow editor drives the wgpu engine in edit mode; dioxus-flow retired | §8.7 complete | flow.mjs rewritten against the engine; only then does the second stack die |

Order notes: stages 1–3 are additive and independently shippable; 4 is where performance work lands (it gates 5–6 visually); 7 is the only stage that removes code and is explicitly gated on a parity checklist (ports, typed validation hooks, layered layout, undo are the flow editor's today — [[Flow Editor]] phase list). GPU id-picking is deliberately *not* scheduled: CPU picking over culled segments is O(visible), and the pick-buffer only wins if measurements say otherwise ([[Graph Rendering Options]] lists it; decide with data).

Decided with the spec's registration: **stage 7 is in** — the flow editor moves onto this engine and `dioxus-flow` is gradually replaced, because we do not own that repository and cannot make the substantial changes the spec needs (ADR-0018). Still to confirm along the way: whether hulls/regions are worth their own pipeline early, or wait for stage 5. The specification is registered as [[031]]; the mapping above was written against it section by section.
