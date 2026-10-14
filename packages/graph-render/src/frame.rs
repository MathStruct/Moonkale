//! Spec 031, stage 1: the pure scene → instance bridge. [`build_frame`]
//! turns a [`Graph`] plus its edge styles into draw lists — edge segments
//! (parallels bow apart, self-loops circle the node), arrowheads — with no
//! GPU, no browser and no Moonkale types, so the geometry is unit-testable
//! natively. Stage 4 adds LOD filtering and culling here; stage 7 adds
//! orthogonal routes. [`edge_at`] picks an edge from a screen point over the
//! same segments.

use crate::camera::Camera;
use crate::graph::{Arrow, Edge, Graph, InSide, Layer, Port, Route};
use bytemuck::{Pod, Zeroable};

/// Segment endpoints (instance slot 0 — changes while the layout runs).
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct SegPos {
    pub a: [f32; 3],
    pub b: [f32; 3],
}

/// Segment appearance (instance slot 1 — changes on a swap or a selection).
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct SegAttr {
    pub color: [f32; 4],
    /// Stroke width in screen pixels.
    pub width: f32,
    /// Dash length in screen pixels; 0 = solid.
    pub dash: f32,
    /// Arc length (world units) at this segment's start: the dash pattern
    /// continues across a curve's sub-segments instead of restarting.
    pub phase: f32,
    /// A moving pulse along the whole stroke (spec 031 §2, stage 6):
    /// world units per second, 0 = none.
    pub pulse: f32,
    /// The stroke's total world length — the pulse wraps over it.
    pub edge_len: f32,
}

/// An arrowhead: a billboard at `pos`, rotated to point along `angle`.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct ArrowPos {
    pub pos: [f32; 3],
    /// Radians — atan2 of the pointing direction in the world plane.
    pub angle: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct ArrowAttr {
    pub color: [f32; 4],
    /// World units; the shader clamps it to a screen minimum.
    pub size: f32,
}

/// Selection by instance id (spec 031 §1): ids survive an incremental
/// `set_graph` — indices would not — and stale ids simply match nothing.
#[derive(Clone, Default, PartialEq)]
pub struct Selection {
    pub nodes: std::collections::HashSet<String>,
    pub edges: std::collections::HashSet<String>,
}

impl Selection {
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty() && self.edges.is_empty()
    }
}

/// What one frame needs besides the graph (spec 031, stage 2: the draw
/// call's inputs grew past the inline style). `render::draw` takes it; a
/// native host could run the same numbers through it later.
pub struct DrawInput<'a> {
    pub camera: &'a Camera,
    pub hovered: Option<usize>,
    pub selected: &'a Selection,
    /// Edit mode draws the ports and the pending wire (spec 031 §5).
    pub edit: bool,
    /// The wire being dragged, world endpoints — not yet an edge.
    pub pending: Option<([f32; 3], [f32; 3])>,
    /// Geometry revision: a layout step, a drag, a graph swap.
    pub pos_rev: u64,
    /// Style revision: a graph swap, a layer toggle, a wire start or end.
    pub attr_rev: u64,
    /// Hover revision — hover recolours nodes only; segment styles must
    /// not rebuild for it (the stage 0–3 review's perf note).
    pub hover_rev: u64,
    /// Selection revision — the ring on nodes, the brightening on edges.
    pub sel_rev: u64,
}

/// Segments per curved edge (a parallel's bow is a shallow arc; stage 4's
/// LOD replaces distant curves with fewer).
pub const CURVE_SEGS: usize = 8;
/// Segments per self-loop.
pub const LOOP_SEGS: usize = 16;
/// Arrowhead size in world units (the nodes' own radius is 4–10).
pub const ARROW_SIZE: f32 = 6.0;
/// Port marker radius in world units; a port sits this far outside the rim.
pub const PORT_STANDOFF: f32 = 2.5;
pub const PORT_RADIUS: f32 = 2.5;
/// An edge shared by two visible layers bows its strokes apart by this many
/// world units per rank (spec 031 §3: offset parallel strokes).
pub const LAYER_STROKE_SPACING: f32 = 3.0;
/// A hull clears its outermost member's rim by this many world units
/// (spec 031 §4: the tinted region reads as *around* the group).
pub const HULL_PAD: f32 = 7.0;
/// Hull corner radius in world units (rounded-rect SDF).
pub const HULL_CORNER: f32 = 12.0;
/// The hull tint's strength (spec 031 §4: a tinted region, not a block).
pub const HULL_FILL: f32 = 0.16;
/// A stroke of the group's colour traces the hull's rim, this many px.
pub const HULL_RIM: f32 = 1.5;
/// Junction marker radius in world units (spec 031 §2: hyperedges).
pub const JUNCTION_RADIUS: f32 = 3.0;
/// A highlight clique's hull tints twice as strong as a group's wash.
pub const HULL_HIGHLIGHT_FILL: f32 = 0.32;
/// The pulse's glow radius in world units (spec 031 §2, stage 6).
pub const PULSE_SIZE: f32 = 8.0;

/// A group hull: one rounded-rect instance (instance slot 0 — the box
/// follows its members' positions every layout step).
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct HullPos {
    pub center: [f32; 3],
    /// World half-extents, padding included.
    pub half: [f32; 2],
}

/// Hull appearance (instance slot 1 — the tint, set on a graph swap).
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct HullAttr {
    pub color: [f32; 4],
    /// Corner radius in world units.
    pub corner: f32,
    /// 1.0 = a highlighted clique's rim (spec 031 §2): stronger than a
    /// group's wash, so the two read apart over the same nodes.
    pub highlight: f32,
}

/// One filled triangle of a simplicial face (spec 031 §2) — instance
/// slot 0; a face over more than three nodes is fanned into triangles.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct SimplexPos {
    pub a: [f32; 3],
    pub b: [f32; 3],
    pub c: [f32; 3],
}

/// Simplex appearance (instance slot 1).
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct SimplexAttr {
    pub color: [f32; 4],
}

/// A hyperedge's junction marker (spec 031 §2) — a small circle riding
/// the node pipeline at the members' centroid; the spokes ride the
/// segment pipeline. A distinct class from a group hull and a simplex.
#[derive(Clone, Copy)]
pub struct Junction {
    pub pos: [f32; 3],
    pub color: [f32; 4],
}

#[derive(Default)]
pub struct Frame {
    pub segs: Vec<SegPos>,
    pub seg_attrs: Vec<SegAttr>,
    /// Which edge each segment belongs to (picking); `None` marks a trace
    /// hop — drawn like any stroke, but never picked as an edge: layers and
    /// traces add presentation, not topology (spec 031 §3).
    pub seg_edge: Vec<Option<usize>>,
    pub arrows: Vec<ArrowPos>,
    pub arrow_attrs: Vec<ArrowAttr>,
    /// World positions of the graph's ports, same order as `graph.ports`
    /// (drawn in edit mode, picked always — spec 031 §2, stage 2).
    pub port_pos: Vec<[f32; 3]>,
    /// Node visibility by graph index (spec 031 §3): a node with layers
    /// draws only while one is visible. The instance keeps its slot (hover
    /// and selection stay index-aligned); the renderer fades it out.
    pub node_visible: Vec<bool>,
    /// Group hulls (spec 031 §4), one per visible group with visible
    /// members — 2D only; 3D keeps plain nodes and edges.
    pub hulls: Vec<HullPos>,
    pub hull_attrs: Vec<HullAttr>,
    /// Simplicial faces (spec 031 §2, stage 5), fanned into triangles —
    /// 2D only, drawn with the hulls behind everything.
    pub simplex_tris: Vec<SimplexPos>,
    pub simplex_attrs: Vec<SimplexAttr>,
    /// Hyperedge junctions (spec 031 §2, stage 5): the markers ride the
    /// node pipeline; the spokes are already among the segments.
    pub junctions: Vec<Junction>,
}

/// The scene → instance lists for one frame.
pub fn build_frame(graph: &Graph, three_d: bool) -> Frame {
    let mut f = Frame::default();
    let mut scratch = Vec::new();
    build_frame_into(graph, three_d, &mut f, &mut scratch);
    f
}

/// The reusing form the renderer drives: every frame of a running layout
/// rebuilds the scene, so the buffers' capacities are kept instead of
/// reallocating ~120k polylines per frame (the stage 0–3 review's bench).
pub fn build_frame_into(graph: &Graph, three_d: bool, f: &mut Frame, scratch: &mut Vec<[f32; 3]>) {
    f.segs.clear();
    f.seg_attrs.clear();
    f.seg_edge.clear();
    f.arrows.clear();
    f.arrow_attrs.clear();
    f.port_pos.clear();
    f.node_visible.clear();
    f.hulls.clear();
    f.hull_attrs.clear();
    f.simplex_tris.clear();
    f.simplex_attrs.clear();
    f.junctions.clear();
    f.port_pos
        .extend(graph.ports.iter().map(|p| port_world(graph, p)));
    f.node_visible.extend(graph.nodes.iter().map(|n| {
        n.layers.is_empty()
            || n.layers
                .iter()
                .any(|id| graph.layers.iter().any(|l| &l.id == id && l.visible))
    }));
    for (i, e) in graph.edges.iter().enumerate() {
        // Fast path (the 100k-node case): no layers on the edge — the base
        // scene, one stroke, no per-edge allocation. Layered edges (flows,
        // traces) live in small graphs; the collect is for them.
        let visible: Vec<&Layer> = if e.layers.is_empty() {
            Vec::new()
        } else {
            e.layers
                .iter()
                .filter_map(|id| graph.layers.iter().find(|l| &l.id == id))
                .filter(|l| l.visible)
                .collect()
        };
        // Layer visibility (spec 031 §3): an instance with layers draws
        // only while one of them is visible; no layers = the base scene.
        if !e.layers.is_empty() && visible.is_empty() {
            continue;
        }
        // One stroke per visible layer — a shared edge shows both, as
        // offset parallel strokes; a single (or no) layer draws once, in
        // the first visible layer's style when there is one.
        let strokes = visible.len().max(1);
        for rank in 0..strokes {
            let layer = visible
                .get(rank.min(visible.len().saturating_sub(1)))
                .copied();
            polyline_into(graph, e, stroke_offset(rank, strokes), scratch);
            // The stroke's total length — the pulse wraps over it, and the
            // dash LOD gates on it (the glow and the gaps are positioned in
            // world units / screen pixels over the whole stroke, crossing a
            // curve's sub-segments as one line). Plain strokes skip the
            // pass (the stage 4–6 review's #10: every edge ran it on every
            // layout step) — a layer may dash a stroke whose edge isn't.
            let needs_len =
                e.pulse_speed > 0.0 || e.dash > 0.0 || visible.iter().any(|l| l.dash.is_some());
            let edge_len: f32 = if needs_len {
                scratch
                    .windows(2)
                    .map(|w| ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2)).sqrt())
                    .sum()
            } else {
                0.0
            };
            let mut attr = SegAttr {
                color: e.color,
                width: e.width,
                dash: e.dash,
                phase: 0.0,
                pulse: e.pulse_speed,
                edge_len,
            };
            if let Some(l) = layer {
                if let Some(c) = l.color {
                    attr.color = c;
                }
                if let Some(w) = l.width {
                    attr.width = w;
                }
                if let Some(d) = l.dash {
                    attr.dash = d;
                }
                if let Some(o) = l.opacity {
                    attr.color[3] *= o.clamp(0.0, 1.0);
                }
            }
            let mut arc = 0.0f32;
            for w in scratch.windows(2) {
                f.segs.push(SegPos { a: w[0], b: w[1] });
                f.seg_edge.push(Some(i));
                f.seg_attrs.push(SegAttr { phase: arc, ..attr });
                arc += ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2)).sqrt();
            }
            if three_d {
                // 3D keeps plain edges (spec 031 mapping): no arrowheads.
                continue;
            }
            let n = scratch.len();
            if n < 2 {
                continue;
            }
            // The tangent each end travels along, from the tessellation, so
            // straight, curved and loop edges place their heads the same
            // way.
            let into_b = normalize2(sub2(scratch[n - 1], scratch[n - 2]));
            let out_of_a = normalize2(sub2(scratch[1], scratch[0]));
            let a_pos = scratch[0];
            let b_pos = scratch[n - 1];
            // A head pulls back by its endpoint's own size: a port anchor
            // sits outside the rim already, so pulling by the node's radius
            // would bury the head inside the node (nodes draw on top).
            let a_radius = anchor_pull(graph, e.a, &e.a_port);
            let b_radius = anchor_pull(graph, e.b, &e.b_port);
            let mut head_color = attr.color;
            head_color[3] = head_color[3].max(0.8);
            let head = |tip_at: [f32; 3], dir: [f32; 2], pull: f32| -> (ArrowPos, ArrowAttr) {
                let d = normalize2(dir);
                let pull = pull + ARROW_SIZE * 0.5;
                (
                    ArrowPos {
                        pos: [tip_at[0] - d[0] * pull, tip_at[1] - d[1] * pull, tip_at[2]],
                        angle: d[1].atan2(d[0]),
                    },
                    ArrowAttr {
                        color: head_color,
                        size: ARROW_SIZE,
                    },
                )
            };
            if matches!(e.arrow, Arrow::Target | Arrow::Both) {
                let (p, a) = head(b_pos, into_b, b_radius);
                f.arrows.push(p);
                f.arrow_attrs.push(a);
            }
            if matches!(e.arrow, Arrow::Source | Arrow::Both) {
                // The head at the source points into `a` — against the travel.
                let (p, a) = head(a_pos, [-out_of_a[0], -out_of_a[1]], a_radius);
                f.arrows.push(p);
                f.arrow_attrs.push(a);
            }
        }
    }
    build_traces(graph, three_d, f);
    build_hyperedges(graph, f);
    if !three_d {
        build_hulls(graph, f);
        build_simplexes(graph, f);
    }
}

/// One rounded-rect hull per visible group (spec 031 §4): the axis-aligned
/// box of its visible members' rims, padded, tinted by the group's colour.
/// A group may nest (a member of the inner is a member of the outer) or
/// overlap (members shared) — each emits its own instance, drawn in
/// declaration order; alpha blending stacks the tints.
fn build_hulls(graph: &Graph, f: &mut Frame) {
    for g in &graph.groups {
        if !g.visible {
            continue;
        }
        let (mut x0, mut y0) = (f32::MAX, f32::MAX);
        let (mut x1, mut y1) = (f32::MIN, f32::MIN);
        let mut rim = 0.0f32;
        let mut any = false;
        for &i in &g.members {
            let visible = f.node_visible.get(i).copied().unwrap_or(true);
            if !visible {
                continue; // an invisible member does not stretch the tint
            }
            let n = &graph.nodes[i];
            x0 = x0.min(n.x - n.radius);
            y0 = y0.min(n.y - n.radius);
            x1 = x1.max(n.x + n.radius);
            y1 = y1.max(n.y + n.radius);
            rim = rim.max(n.radius);
            any = true;
        }
        if !any {
            continue; // every member left with a collapse or a hidden layer
        }
        let pad = rim + HULL_PAD;
        f.hulls.push(HullPos {
            center: [(x0 + x1) / 2.0, (y0 + y1) / 2.0, 0.0],
            half: [(x1 - x0) / 2.0 + pad, (y1 - y0) / 2.0 + pad],
        });
        let mut color = g.color;
        // A highlighted clique tints twice as strong as a group's wash
        // and wears the brighter rim (spec 031 §2: three classes apart).
        color[3] *= if g.highlight {
            HULL_HIGHLIGHT_FILL
        } else {
            HULL_FILL
        };
        f.hull_attrs.push(HullAttr {
            color,
            corner: HULL_CORNER,
            highlight: g.highlight as u32 as f32,
        });
    }
}

/// Hyperedges (spec 031 §2, stage 5): one junction per hyperedge — a
/// marker at the members' centroid, a spoke to each member. The spokes
/// are segments owned by `None` (never picked as an edge — a hyperedge
/// is presentation over the same nodes, never new topology), the markers
/// ride the node pipeline behind this builder's back.
fn build_hyperedges(graph: &Graph, f: &mut Frame) {
    for h in &graph.hyperedges {
        if !h.visible {
            continue;
        }
        // An invisible member (a hidden layer) draws nothing — its spoke
        // and the centroid must not reach for it (the stage 4–6 review's
        // #6, unlike a hull whose box only shrinks).
        let members: Vec<usize> = h
            .nodes
            .iter()
            .copied()
            .filter(|&i| f.node_visible.get(i).copied().unwrap_or(true))
            .collect();
        if members.len() < 2 {
            continue;
        }
        let mut center = [0.0f32; 3];
        for &i in &members {
            let n = &graph.nodes[i];
            center[0] += n.x;
            center[1] += n.y;
            center[2] += n.z;
        }
        let k = members.len() as f32;
        center[0] /= k;
        center[1] /= k;
        center[2] /= k;
        for &i in &members {
            f.segs.push(SegPos {
                a: center,
                b: node_pos(graph, i),
            });
            f.seg_edge.push(None); // never picked as an edge (§2, §3)
            f.seg_attrs.push(SegAttr {
                color: h.color,
                width: h.width,
                dash: h.dash,
                phase: 0.0,
                pulse: 0.0, // no pulse knob on a hyperedge (yet, §2)
                edge_len: 0.0,
            });
        }
        f.junctions.push(Junction {
            pos: center,
            color: h.color,
        });
    }
}

/// Simplicial faces (spec 031 §2, stage 5): a filled triangle over three
/// nodes; a face over more is fanned from its centroid into triangles.
fn build_simplexes(graph: &Graph, f: &mut Frame) {
    for s in &graph.simplexes {
        if !s.visible {
            continue;
        }
        // Invisible members (hidden layers) leave the face — it must not
        // stretch to a node that draws nothing (the review's #6).
        let pts: Vec<[f32; 3]> = s
            .nodes
            .iter()
            .filter(|&&i| f.node_visible.get(i).copied().unwrap_or(true))
            .map(|&i| node_pos(graph, i))
            .collect();
        if pts.len() < 3 {
            continue;
        }
        if pts.len() == 3 {
            f.simplex_tris.push(SimplexPos {
                a: pts[0],
                b: pts[1],
                c: pts[2],
            });
            f.simplex_attrs.push(SimplexAttr { color: s.color });
            continue;
        }
        let mut center = [0.0f32; 3];
        for p in &pts {
            center[0] += p[0];
            center[1] += p[1];
            center[2] += p[2];
        }
        let k = pts.len() as f32;
        center[0] /= k;
        center[1] /= k;
        center[2] /= k;
        for w in pts.windows(2) {
            f.simplex_tris.push(SimplexPos {
                a: center,
                b: w[0],
                c: w[1],
            });
            f.simplex_attrs.push(SimplexAttr { color: s.color });
        }
        // Close the fan: last → first.
        let n = pts.len();
        f.simplex_tris.push(SimplexPos {
            a: center,
            b: pts[n - 1],
            c: pts[0],
        });
        f.simplex_attrs.push(SimplexAttr { color: s.color });
    }
}

/// Traces (spec 031 §3): ordered walks over node ids — recursion revisits
/// ids, and each hop carries a head at its end so the sequence reads as a
/// directed walk. A trace in a hidden layer disappears with it.
fn build_traces(graph: &Graph, three_d: bool, f: &mut Frame) {
    for t in &graph.traces {
        let layer = t
            .layer
            .as_ref()
            .and_then(|id| graph.layers.iter().find(|l| &l.id == id));
        if layer.is_some_and(|l| !l.visible) {
            continue;
        }
        let color = t
            .color
            .or_else(|| layer.and_then(|l| l.color))
            .unwrap_or([0.92, 0.70, 0.28, 0.95]);
        let mut prev: Option<[f32; 3]> = None;
        for step in &t.steps {
            let Some(pos) = graph
                .nodes
                .iter()
                .find(|n| &n.id == step)
                .map(|n| [n.x, n.y, n.z])
            else {
                continue; // a stale id never breaks the walk
            };
            if let Some(a) = prev {
                // A degenerate hop (a frame recurring into itself) draws
                // nothing — its order is the walk's, not a stroke's.
                if (a[0] - pos[0]).abs() < 1e-3 && (a[1] - pos[1]).abs() < 1e-3 {
                    prev = Some(pos);
                    continue;
                }
                f.segs.push(SegPos { a, b: pos });
                f.seg_edge.push(None); // never picked as an edge (§3)
                f.seg_attrs.push(SegAttr {
                    color,
                    width: 2.2,
                    dash: 0.0,
                    phase: 0.0,
                    pulse: 0.0, // traces are ordered, not flowing (§3)
                    edge_len: 0.0,
                });
                if !three_d {
                    let into = normalize2([pos[0] - a[0], pos[1] - a[1]]);
                    let radius = graph
                        .nodes
                        .iter()
                        .find(|n| &n.id == step)
                        .map(|n| n.radius)
                        .unwrap_or(5.0);
                    let pull = radius + 1.0 + ARROW_SIZE * 0.5;
                    f.arrows.push(ArrowPos {
                        pos: [pos[0] - into[0] * pull, pos[1] - into[1] * pull, pos[2]],
                        angle: into[1].atan2(into[0]),
                    });
                    f.arrow_attrs.push(ArrowAttr {
                        color,
                        size: ARROW_SIZE,
                    });
                }
            }
            prev = Some(pos);
        }
    }
}

/// How far the `rank`th of `total` strokes bows off the edge's own route
/// (spec 031 §3): shared edges keep their parallels apart. Only the layer
/// rank — `polyline` adds the route's own offset on top, once.
fn stroke_offset(rank: usize, total: usize) -> f32 {
    if total < 2 {
        return 0.0;
    }
    (rank as f32 - (total as f32 - 1.0) / 2.0) * LAYER_STROKE_SPACING
}

/// The edge's world-space polyline (its tessellation): `[a, …, b]`, written
/// into `out` (cleared first — the renderer reuses one buffer across every
/// frame of a layout). The `stroke` bows a parallel copy off the route
/// (spec 031 §3) — 0 is the edge's own line.
fn polyline_into(graph: &Graph, e: &Edge, extra: f32, out: &mut Vec<[f32; 3]>) {
    out.clear();
    let na = &graph.nodes[e.a];
    let a_end = anchored(graph, e.a, &e.a_port);
    let b_end = anchored(graph, e.b, &e.b_port);
    if e.a == e.b {
        // A self-loop: a circle resting on the node, starting and ending at
        // its centre so it reads as attached; the bow widens the ring.
        let offset = match e.route {
            Route::Bezier { offset } => offset,
            Route::Straight | Route::Ortho { .. } => 0.0,
        } + e.bow;
        let side = if offset < 0.0 { -1.0 } else { 1.0 };
        let r = na.radius + 6.0 + offset.abs() * 0.35;
        let (cx, cy) = (na.x, na.y - side * r);
        // The ring starts at its point nearest the node: π/2 for a ring
        // above (side +), −π/2 for one below — otherwise the loop's ends
        // sit two radii away from the node it belongs to.
        let start = if side < 0.0 {
            -std::f32::consts::FRAC_PI_2
        } else {
            std::f32::consts::FRAC_PI_2
        };
        for k in 0..=LOOP_SEGS {
            let t = start + k as f32 * std::f32::consts::TAU / LOOP_SEGS as f32;
            out.push([cx + r * t.cos(), cy + r * t.sin(), na.z]);
        }
    } else {
        // A bowed parallel of a straight line is a shallow Bézier; a
        // curve's control point takes the extra offset (the edge's own bow
        // plus the layer rank) on top of its own.
        let route = match (e.route, e.bow + extra) {
            (Route::Straight, s) if s.abs() > 1e-6 => Route::Bezier { offset: s },
            (Route::Straight, _) => Route::Straight,
            (Route::Bezier { offset }, s) => Route::Bezier { offset: offset + s },
            (Route::Ortho { stub }, s) => Route::Ortho {
                stub: stub.map(|st| st + s * 0.5), // parallels step apart
            },
        };
        match route {
            Route::Straight => {
                out.push(a_end);
                out.push(b_end);
            }
            Route::Ortho { stub } => ortho_into(graph, e, a_end, b_end, stub, out),
            Route::Bezier { offset } => {
                let (dx, dy) = (b_end[0] - a_end[0], b_end[1] - a_end[1]);
                let len = (dx * dx + dy * dy).sqrt().max(1e-3);
                // Perpendicular of the endpoint line, unit length.
                let (px, py) = (-dy / len, dx / len);
                let (mx, my) = (
                    (a_end[0] + b_end[0]) / 2.0 + px * offset,
                    (a_end[1] + b_end[1]) / 2.0 + py * offset,
                );
                for k in 0..=CURVE_SEGS {
                    let t = k as f32 / CURVE_SEGS as f32;
                    let s = 1.0 - t;
                    out.push(bezier(
                        a_end,
                        [mx, my, (a_end[2] + b_end[2]) / 2.0],
                        b_end,
                        t,
                        s,
                    ));
                }
            }
        }
    }
}

/// How far a wire leaves its port before it turns (world units): a
/// short stub, so the wire visibly comes out of the pin's side.
const ORTHO_STUB: f32 = 12.0;

/// The flow route (spec 031 §2; ADR-0024 made it side-aware): out of
/// each port along its side's normal, square corners between. Pins that
/// face each other meet half-way across — the classic three segments.
/// Pins facing the same way meet beyond the farther one; pins facing
/// away (a capacitor's right pin back to a source's left) detour round
/// the parts instead of running through them. `stub` overrides the
/// facing case's corner, as before; a parallel's extra steps the route.
fn ortho_into(
    graph: &Graph,
    e: &Edge,
    a_end: [f32; 3],
    b_end: [f32; 3],
    stub: Option<f32>,
    out: &mut Vec<[f32; 3]>,
) {
    let side = |node: usize, port: &Option<String>| {
        port.as_deref().and_then(|name| {
            graph
                .ports
                .iter()
                .find(|p| p.node == node && p.name == name)
                .map(|p| p.side)
        })
    };
    let toward = |from: [f32; 3], to: [f32; 3]| {
        if to[0] >= from[0] {
            [1.0, 0.0]
        } else {
            [-1.0, 0.0]
        }
    };
    let dir = |s: InSide| match s {
        InSide::Right => [1.0f32, 0.0f32],
        InSide::Left => [-1.0, 0.0],
        InSide::Top => [0.0, -1.0],
        InSide::Bottom => [0.0, 1.0],
    };
    let d0 = side(e.a, &e.a_port)
        .map(dir)
        .unwrap_or_else(|| toward(a_end, b_end));
    let d1 = side(e.b, &e.b_port)
        .map(dir)
        .unwrap_or_else(|| toward(b_end, a_end));
    let z = a_end[2];
    let mut pts: Vec<[f32; 2]> = Vec::with_capacity(6);
    // Work along the first end's axis: `u` is the axis it leaves on, `v`
    // the other — one code path for horizontal and vertical pins.
    let horizontal = d0[1] == 0.0;
    let (u, v) = if horizontal { (0, 1) } else { (1, 0) };
    let at = |pu: f32, pv: f32| {
        let mut q = [0.0; 2];
        q[u] = pu;
        q[v] = pv;
        q
    };
    let (p0, p1) = ([a_end[0], a_end[1]], [b_end[0], b_end[1]]);
    let extent = |i: usize| {
        let n = &graph.nodes[i];
        let half = if n.shape == crate::graph::Shape::Circle {
            [n.radius, n.radius]
        } else {
            [n.radius, n.height]
        };
        (n.x, n.y, half)
    };
    pts.push(p0);
    if d1[v] == 0.0 {
        // Both ends leave along `u`.
        let gap = (p1[u] - p0[u]).abs();
        let facing = d0[u] * (p1[u] - p0[u]) > 0.0 && d1[u] * (p0[u] - p1[u]) > 0.0;
        if facing {
            let st = stub.unwrap_or((gap * 0.5).max(8.0));
            let c = p0[u] + d0[u] * st;
            pts.push(at(c, p0[v]));
            pts.push(at(c, p1[v]));
        } else if d0[u] == d1[u] {
            let c = if d0[u] > 0.0 {
                p0[u].max(p1[u]) + ORTHO_STUB
            } else {
                p0[u].min(p1[u]) - ORTHO_STUB
            };
            pts.push(at(c, p0[v]));
            pts.push(at(c, p1[v]));
        } else {
            // Facing away: out, round, in. The detour runs between the
            // two parts when there is room, else past both.
            let s0 = p0[u] + d0[u] * ORTHO_STUB;
            let s1 = p1[u] + d1[u] * ORTHO_STUB;
            let (ax, ay, ah) = extent(e.a);
            let (bx, by, bh) = extent(e.b);
            let (ac, bc) = ([ax, ay][v], [bx, by][v]);
            let (a_lo, a_hi) = (ac - ah[v], ac + ah[v]);
            let (b_lo, b_hi) = (bc - bh[v], bc + bh[v]);
            let detour = if a_hi + 2.0 * ORTHO_STUB < b_lo {
                (a_hi + b_lo) / 2.0
            } else if b_hi + 2.0 * ORTHO_STUB < a_lo {
                (b_hi + a_lo) / 2.0
            } else {
                a_hi.max(b_hi) + 2.0 * ORTHO_STUB
            };
            pts.push(at(s0, p0[v]));
            pts.push(at(s0, detour));
            pts.push(at(s1, detour));
            pts.push(at(s1, p1[v]));
        }
    } else {
        // One end leaves along `u`, the other along `v`: one corner when
        // each points toward the other, else out of both and across.
        let reach0 = d0[u] * (p1[u] - p0[u]) > 0.0;
        let reach1 = d1[v] * (p0[v] - p1[v]) > 0.0;
        if reach0 && reach1 {
            pts.push(at(p1[u], p0[v]));
        } else {
            let s0 = p0[u] + d0[u] * ORTHO_STUB;
            let s1 = p1[v] + d1[v] * ORTHO_STUB;
            pts.push(at(s0, p0[v]));
            pts.push(at(s0, s1));
            pts.push(at(p1[u], s1));
        }
    }
    pts.push(p1);
    pts.dedup_by(|a, b| (a[0] - b[0]).abs() < 1e-4 && (a[1] - b[1]).abs() < 1e-4);
    out.extend(pts.into_iter().map(|q| [q[0], q[1], z]));
    // The last point carries the target's depth.
    if let Some(last) = out.last_mut() {
        last[2] = b_end[2];
    }
}

/// Quadratic Bézier at `t` (`s = 1 − t`).
fn bezier(a: [f32; 3], c: [f32; 3], b: [f32; 3], t: f32, s: f32) -> [f32; 3] {
    [
        s * s * a[0] + 2.0 * s * t * c[0] + t * t * b[0],
        s * s * a[1] + 2.0 * s * t * c[1] + t * t * b[1],
        s * s * a[2] + 2.0 * s * t * c[2] + t * t * b[2],
    ]
}

/// One arrowhead: its tip a unit's width off the node's rim, the billboard
/// centred half a head behind it, pointing along `dir`.
/// A port's world position: outside the node's rim on its side, spread
/// along the side by `offset` (0..1). Circles for now — stage 7's port
/// diagrams on rectangles re-derives this from the node's shape.
pub fn port_world(graph: &Graph, port: &Port) -> [f32; 3] {
    use crate::graph::Shape;
    let n = &graph.nodes[port.node];
    let (dir, tangent) = match port.side {
        InSide::Right => ([1.0, 0.0], [0.0, 1.0]),
        InSide::Left => ([-1.0, 0.0], [0.0, 1.0]),
        InSide::Top => ([0.0, -1.0], [1.0, 0.0]),
        InSide::Bottom => ([0.0, 1.0], [1.0, 0.0]),
    };
    // A block's ports ride its rectangle — a diamond block too (spec 031
    // §2, stage 8): the side's own half-extent pushes out, the other axis
    // spreads along it (spec 031 §2, stage 7).
    let (out, along_half) = if n.shape == Shape::Rect || n.shape == Shape::Diamond {
        match port.side {
            InSide::Left | InSide::Right => (n.radius, n.height),
            InSide::Top | InSide::Bottom => (n.height, n.radius),
        }
    } else {
        (n.radius, n.radius)
    };
    let along = (port.offset - 0.5) * along_half * 2.0;
    [
        n.x + dir[0] * (out + PORT_STANDOFF) + tangent[0] * along,
        n.y + dir[1] * (out + PORT_STANDOFF) + tangent[1] * along,
        n.z,
    ]
}

/// An edge endpoint: the named port's position, or the node's centre when
/// the port does not exist (a stale name never breaks the geometry).
fn anchored(graph: &Graph, node: usize, port: &Option<String>) -> [f32; 3] {
    match port {
        Some(name) => graph
            .ports
            .iter()
            .find(|p| p.node == node && &p.name == name)
            .map(|p| port_world(graph, p))
            .unwrap_or_else(|| node_pos(graph, node)),
        None => node_pos(graph, node),
    }
}

fn node_pos(graph: &Graph, node: usize) -> [f32; 3] {
    let n = &graph.nodes[node];
    [n.x, n.y, n.z]
}

/// How far a head at this endpoint pulls back from its anchor: a port's own
/// radius when the edge anchors there (the port already sits outside the
/// rim), the node's radius plus one otherwise.
fn anchor_pull(graph: &Graph, node: usize, port: &Option<String>) -> f32 {
    match port.as_deref() {
        Some(name) if graph.ports.iter().any(|p| p.node == node && p.name == name) => PORT_RADIUS,
        _ => graph.nodes[node].radius + 1.0,
    }
}

/// The port under a screen point, nearest within its marker's reach
/// (spec 031 §2: ports are independent hit targets).
pub fn port_at(frame: &Frame, camera: &Camera, x: f32, y: f32) -> Option<usize> {
    let project = |p: [f32; 3]| -> Option<(f32, f32)> {
        if camera.three_d {
            camera.project(p[0], p[1], p[2]).map(|(sx, sy, _)| (sx, sy))
        } else {
            Some(camera.world_to_screen(p[0], p[1]))
        }
    };
    let mut best: Option<(f32, usize)> = None;
    for (i, p) in frame.port_pos.iter().enumerate() {
        let Some((px, py)) = project(*p) else {
            continue;
        };
        let d = ((px - x).powi(2) + (py - y).powi(2)).sqrt();
        let reach = (PORT_RADIUS * camera.scale).max(4.0) + 2.0;
        if d <= reach && best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, i));
        }
    }
    best.map(|(_, i)| i)
}

fn sub2(a: [f32; 3], b: [f32; 3]) -> [f32; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn normalize2(v: [f32; 2]) -> [f32; 2] {
    let len = (v[0] * v[0] + v[1] * v[1]).sqrt().max(1e-6);
    [v[0] / len, v[1] / len]
}

/// The edge under a screen point, over the frame's segments: the nearest
/// segment within its stroke's reach wins; nodes win over edges in the
/// caller (the camera's node hit runs first).
pub fn edge_at(frame: &Frame, camera: &Camera, x: f32, y: f32) -> Option<usize> {
    let project = |p: [f32; 3]| -> Option<(f32, f32)> {
        if camera.three_d {
            camera.project(p[0], p[1], p[2]).map(|(sx, sy, _)| (sx, sy))
        } else {
            Some(camera.world_to_screen(p[0], p[1]))
        }
    };
    let mut best: Option<(f32, usize)> = None;
    for (i, seg) in frame.segs.iter().enumerate() {
        // Trace hops are never picked (spec 031 §3: presentation, not
        // topology).
        let Some(owner) = frame.seg_edge[i] else {
            continue;
        };
        let (Some((ax, ay)), Some((bx, by))) = (project(seg.a), project(seg.b)) else {
            continue;
        };
        let d = dist_to_segment(x, y, ax, ay, bx, by);
        let reach = (frame.seg_attrs[i].width * 0.5 + 2.5).max(4.0);
        if d <= reach && best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, owner));
        }
    }
    best.map(|(_, e)| e)
}

fn dist_to_segment(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let (abx, aby) = (bx - ax, by - ay);
    let len2 = abx * abx + aby * aby;
    let t = if len2 < 1e-6 {
        0.0
    } else {
        (((px - ax) * abx + (py - ay) * aby) / len2).clamp(0.0, 1.0)
    };
    let (cx, cy) = (ax + abx * t, ay + aby * t);
    ((px - cx).powi(2) + (py - cy).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{Edge, Node, BOW_SPACING};

    fn node(id: &str, x: f32, y: f32) -> Node {
        Node {
            id: id.into(),
            label: id.into(),
            kind: "file".into(),
            key: String::new(),
            x,
            y,
            z: 0.0,
            radius: 5.0,
            color: [0.5, 0.5, 0.5, 1.0],
            degree: 1,
            pinned: false,
            layers: Vec::new(),
            height: crate::graph::RECT_HEIGHT,
            shape: crate::graph::Shape::Circle,
            sized: false,
            stroke: [0.0, 0.0, 0.0, 0.0],
            icon: None,
            symbol: None,
            turn: 0,
            mirror: false,
            caption: None,
        }
    }

    fn edge(a: usize, b: usize) -> Edge {
        Edge {
            id: format!("e{a}-{b}"),
            a,
            b,
            color: [0.6, 0.6, 0.6, 0.55],
            width: 1.2,
            dash: 0.0,
            arrow: Arrow::None,
            label: String::new(),
            route: Route::Straight,
            a_port: None,
            b_port: None,
            layers: Vec::new(),
            bow: 0.0,
            pulse_speed: 0.0,
        }
    }

    fn graph(nodes: Vec<Node>, edges: Vec<Edge>) -> Graph {
        Graph {
            nodes,
            edges,
            ports: Vec::new(),
            layers: Vec::new(),
            traces: Vec::new(),
            groups: Vec::new(),
            hyperedges: Vec::new(),
            simplexes: Vec::new(),
        }
    }

    fn layer(id: &str, color: [f32; 4], visible: bool) -> Layer {
        Layer {
            id: id.into(),
            color: Some(color),
            visible,
            overlay: false,
            width: None,
            dash: None,
            opacity: None,
        }
    }

    fn port(node: usize, name: &str, side: InSide, offset: f32) -> Port {
        Port {
            node,
            name: name.into(),
            side,
            offset,
            ..Default::default()
        }
    }

    fn cam(w: f32, h: f32) -> Camera {
        Camera {
            width: w,
            height: h,
            scale: 1.0,
            ..Default::default()
        }
    }

    #[test]
    fn straight_edge_is_one_segment_without_heads() {
        let g = graph(
            vec![node("a", 0.0, 0.0), node("b", 40.0, 0.0)],
            vec![edge(0, 1)],
        );
        let f = build_frame(&g, false);
        assert_eq!(f.segs.len(), 1);
        assert!(f.arrows.is_empty());
        assert_eq!(f.segs[0].a, [0.0, 0.0, 0.0]);
        assert_eq!(f.segs[0].b, [40.0, 0.0, 0.0]);
        assert_eq!(f.seg_edge, vec![Some(0)]);
    }

    #[test]
    fn parallels_bow_apart_and_the_middle_stays_straight() {
        // What `Graph::from_input` decides for three parallels: rank 0 bows
        // below, rank 1 stays straight, rank 2 bows above.
        let mut es = vec![edge(0, 1), edge(0, 1), edge(0, 1)];
        es[0].route = Route::Bezier {
            offset: -BOW_SPACING,
        };
        es[2].route = Route::Bezier {
            offset: BOW_SPACING,
        };
        let g = graph(vec![node("a", -20.0, 0.0), node("b", 20.0, 0.0)], es);
        let f = build_frame(&g, false);
        // Middle: 1 segment; the outer two: one per curve step.
        let counts: Vec<usize> = (0..3)
            .map(|e| f.seg_edge.iter().filter(|&&x| x == Some(e)).count())
            .collect();
        assert_eq!(counts, vec![CURVE_SEGS, 1, CURVE_SEGS]);
        // The bows sit on opposite sides of the endpoints' line (y = 0):
        // the point at t = 0.5 of each curve is half its control offset out.
        let y_at_mid = |e: usize| {
            let first = f.seg_edge.iter().position(|&x| x == Some(e)).unwrap();
            f.segs[first + counts[e] / 2].a[1]
        };
        assert!(y_at_mid(0) < 0.0, "rank 0 bows below");
        assert!(y_at_mid(2) > 0.0, "rank 2 bows above");
        assert_eq!(y_at_mid(1), 0.0, "middle stays on the line");
    }

    #[test]
    fn self_loop_circles_the_node() {
        let mut e = edge(0, 0);
        e.route = Route::Bezier { offset: 9.0 };
        let g = graph(vec![node("a", 0.0, 0.0)], vec![e]);
        let f = build_frame(&g, false);
        assert_eq!(f.segs.len(), LOOP_SEGS);
        // Starts and ends at the node itself (cos π/2 leaves ~1e-7).
        let near = |p: [f32; 3]| p.iter().all(|c| c.abs() < 1e-4);
        assert!(near(f.segs[0].a));
        assert!(near(f.segs[LOOP_SEGS - 1].b));
        // Every point sits on the ring: r = 5 + 6 + 9·0.35 ≈ 14.15, centred
        // above the node.
        for s in &f.segs {
            for p in [s.a, s.b] {
                let d = ((p[0]).powi(2) + (p[1] + 14.15).powi(2)).sqrt();
                assert!((d - 14.15).abs() < 0.7, "off the ring: {d}");
            }
        }
    }

    #[test]
    fn directed_edges_get_a_target_head_away_from_the_node() {
        let mut e = edge(0, 1);
        e.arrow = Arrow::Target;
        let g = graph(vec![node("a", 0.0, 0.0), node("b", 40.0, 0.0)], vec![e]);
        let f = build_frame(&g, false);
        assert_eq!(f.arrows.len(), 1);
        let a = f.arrows[0];
        // Points along +x, centred short of b's rim: pull = 5 + 1 + 3 = 9.
        assert!((a.angle).abs() < 1e-4);
        assert!((a.pos[0] - 31.0).abs() < 1e-4, "x {}", a.pos[0]);
        assert_eq!(f.arrow_attrs.len(), 1);
        // 3D refuses the heads (the mapping's 2D-only rule).
        assert!(build_frame(&g, true).arrows.is_empty());
    }

    #[test]
    fn both_heads_and_dash_pass_through() {
        let mut e = edge(0, 1);
        e.arrow = Arrow::Both;
        e.dash = 6.0;
        e.width = 2.5;
        let g = graph(vec![node("a", 0.0, 0.0), node("b", 0.0, 40.0)], vec![e]);
        let f = build_frame(&g, false);
        assert_eq!(f.arrows.len(), 2);
        // One points up (into b), one down (out of a).
        let angles: Vec<f32> = f.arrows.iter().map(|a| a.angle).collect();
        assert!(angles.contains(&std::f32::consts::FRAC_PI_2));
        assert!(angles.contains(&-std::f32::consts::FRAC_PI_2));
        assert_eq!(f.seg_attrs[0].dash, 6.0);
        assert_eq!(f.seg_attrs[0].width, 2.5);
    }

    #[test]
    fn edge_at_picks_the_nearest_stroke() {
        let g = graph(
            vec![node("a", 0.0, 0.0), node("b", 40.0, 0.0)],
            vec![edge(0, 1)],
        );
        let f = build_frame(&g, false);
        let c = cam(100.0, 100.0);
        // world (20, 0) is the segment's midpoint → screen (70, 50).
        assert_eq!(edge_at(&f, &c, 70.0, 50.0), Some(0));
        assert_eq!(edge_at(&f, &c, 70.0, 54.0), Some(0), "within the reach");
        assert_eq!(edge_at(&f, &c, 70.0, 60.0), None, "too far");
        assert_eq!(edge_at(&f, &c, 5.0, 5.0), None);
    }

    #[test]
    fn edge_at_tells_parallels_apart() {
        let mut straight = edge(0, 1);
        let mut bowed = edge(0, 1);
        bowed.route = Route::Bezier { offset: 15.0 };
        straight.id = "straight".into();
        bowed.id = "bowed".into();
        let g = graph(
            vec![node("a", 0.0, 0.0), node("b", 40.0, 0.0)],
            vec![straight, bowed],
        );
        let f = build_frame(&g, false);
        let c = cam(100.0, 100.0);
        // The bowed edge's apex sits half its control offset (7.5 world
        // units) off the line → screen y = 50 + 7.5 = 57.5 (screen y tracks
        // world y; `world_to_screen` does not flip).
        let hit = edge_at(&f, &c, 70.0, 57.5);
        assert_eq!(hit, Some(1), "the bowed edge, not the straight one");
        assert_eq!(edge_at(&f, &c, 70.0, 50.0), Some(0), "on the line");
    }

    #[test]
    fn ports_sit_outside_the_rim_on_their_side() {
        let n = node("a", 0.0, 0.0); // radius 5
        let g = Graph {
            nodes: vec![n],
            edges: Vec::new(),
            layers: Vec::new(),
            traces: Vec::new(),
            groups: Vec::new(),
            hyperedges: Vec::new(),
            simplexes: Vec::new(),
            ports: vec![
                port(0, "out", InSide::Right, 0.5),
                port(0, "in", InSide::Left, 0.5),
                port(0, "top", InSide::Top, 0.5),
                port(0, "bot", InSide::Bottom, 0.75),
            ],
        };
        let f = build_frame(&g, false);
        assert_eq!(f.port_pos.len(), 4);
        let near = |a: [f32; 3], x: f32, y: f32| (a[0] - x).abs() < 1e-4 && (a[1] - y).abs() < 1e-4;
        // Standoff 2.5 beyond the radius-5 rim; the offsets spread along the
        // side's tangent (offset 0.75 of 1 → (0.75 − 0.5) · 2 · 5 = 2.5).
        assert!(near(f.port_pos[0], 7.5, 0.0), "right: {:?}", f.port_pos[0]);
        assert!(near(f.port_pos[1], -7.5, 0.0), "left: {:?}", f.port_pos[1]);
        assert!(near(f.port_pos[2], 0.0, -7.5), "top: {:?}", f.port_pos[2]);
        assert!(
            near(f.port_pos[3], 2.5, 7.5),
            "bottom at 0.75: {:?}",
            f.port_pos[3]
        );
    }

    #[test]
    fn edges_anchor_at_ports_and_stale_names_fall_back() {
        let mut e = edge(0, 1);
        e.a_port = Some("out".into());
        e.b_port = Some("in".into());
        let g = Graph {
            nodes: vec![node("a", 0.0, 0.0), node("b", 40.0, 0.0)],
            edges: vec![e],
            ports: vec![
                port(0, "out", InSide::Right, 0.5),
                port(1, "in", InSide::Left, 0.5),
            ],
            layers: Vec::new(),
            traces: Vec::new(),
            groups: Vec::new(),
            hyperedges: Vec::new(),
            simplexes: Vec::new(),
        };
        let f = build_frame(&g, false);
        assert_eq!(f.segs.len(), 1);
        assert_eq!(f.segs[0].a, [7.5, 0.0, 0.0], "starts at a's port");
        assert_eq!(f.segs[0].b, [32.5, 0.0, 0.0], "ends at b's port");
        // A port name that does not exist anchors at the node's centre.
        let mut stale = edge(0, 1);
        stale.a_port = Some("nope".into());
        let g2 = graph(vec![node("a", 0.0, 0.0), node("b", 40.0, 0.0)], vec![stale]);
        let f2 = build_frame(&g2, false);
        assert_eq!(f2.segs[0].a, [0.0, 0.0, 0.0]);
    }

    #[test]
    fn port_at_picks_the_marker() {
        let g = Graph {
            nodes: vec![node("a", 0.0, 0.0)],
            edges: Vec::new(),
            ports: vec![port(0, "out", InSide::Right, 0.5)],
            layers: Vec::new(),
            traces: Vec::new(),
            groups: Vec::new(),
            hyperedges: Vec::new(),
            simplexes: Vec::new(),
        };
        let f = build_frame(&g, false);
        let c = cam(100.0, 100.0);
        // World (7.5, 0) → screen (57.5, 50).
        assert_eq!(port_at(&f, &c, 57.5, 50.0), Some(0));
        assert_eq!(port_at(&f, &c, 57.5, 54.0), Some(0), "within the reach");
        assert_eq!(port_at(&f, &c, 57.5, 62.0), None, "too far");
    }

    #[test]
    fn layers_style_and_hide_edges() {
        let mut e = edge(0, 1);
        e.layers = vec!["calls".into()];
        let mut hidden = edge(0, 1);
        hidden.id = "ghost".into();
        hidden.layers = vec!["ghost".into()];
        let mut g = graph(
            vec![node("a", 0.0, 0.0), node("b", 40.0, 0.0)],
            vec![e, hidden],
        );
        g.layers = vec![
            layer("calls", [0.10, 0.20, 0.90, 0.9], true),
            layer("ghost", [0.90, 0.20, 0.20, 0.9], false),
        ];
        g.layers[0].width = Some(2.0);
        let f = build_frame(&g, false);
        // The styled edge draws once in the layer's colour and width; the
        // one in a hidden layer is gone.
        assert_eq!(f.segs.len(), 1);
        assert_eq!(f.seg_attrs[0].color, [0.10, 0.20, 0.90, 0.9]);
        assert_eq!(f.seg_attrs[0].width, 2.0);
        assert_eq!(f.seg_edge[0], Some(0));
    }

    #[test]
    fn a_shared_edge_draws_one_stroke_per_visible_layer() {
        let mut e = edge(0, 1);
        e.layers = vec!["calls".into(), "trace".into()];
        let mut g = graph(vec![node("a", 0.0, 0.0), node("b", 40.0, 0.0)], vec![e]);
        g.layers = vec![
            layer("calls", [0.10, 0.20, 0.90, 0.9], true),
            layer("trace", [0.90, 0.70, 0.20, 0.95], true),
        ];
        let f = build_frame(&g, false);
        // Two strokes, one per layer, bowed apart (spec 031 §3).
        assert_eq!(f.segs.len(), 2 * CURVE_SEGS);
        let colors: Vec<[f32; 4]> = f.seg_attrs.iter().map(|a| a.color).collect();
        assert!(colors.contains(&[0.10, 0.20, 0.90, 0.9]), "calls stroke");
        assert!(colors.contains(&[0.90, 0.70, 0.20, 0.95]), "trace stroke");
        // Every stroke picks as the same edge.
        let c = cam(100.0, 100.0);
        assert_eq!(edge_at(&f, &c, 70.0, 50.0), Some(0));
        // Hiding one layer leaves the other's single, straight stroke.
        g.layers[0].visible = false;
        let f2 = build_frame(&g, false);
        assert_eq!(f2.segs.len(), 1);
        assert_eq!(f2.seg_attrs[0].color, [0.90, 0.70, 0.20, 0.95]);
    }

    #[test]
    fn traces_walk_in_order_and_never_pick() {
        let mut g = graph(
            vec![
                node("f", 0.0, 0.0),
                node("g", 40.0, 10.0),
                node("h", 10.0, 40.0),
            ],
            Vec::new(),
        );
        g.layers = vec![layer("trace", [0.90, 0.70, 0.20, 0.95], true)];
        g.traces = vec![crate::graph::Trace {
            id: "stack".into(),
            layer: Some("trace".into()),
            steps: vec!["f".into(), "g".into(), "h".into(), "g".into(), "f".into()],
            color: None,
        }];
        let f = build_frame(&g, false);
        // f→g→h→g→f: four hops with a head each, revisiting g and f — the
        // walk's order is the emission's (spec 031 §3).
        assert_eq!(f.segs.len(), 4);
        assert_eq!(f.arrows.len(), 4);
        assert_eq!(f.segs[0].a, [0.0, 0.0, 0.0], "starts at f");
        assert_eq!(f.segs[0].b, [40.0, 10.0, 0.0], "then g");
        assert_eq!(f.segs[3].b, [0.0, 0.0, 0.0], "returns to f");
        assert_eq!(f.seg_attrs[0].color, [0.90, 0.70, 0.20, 0.95]);
        // Trace hops are never picked as edges.
        let c = cam(100.0, 100.0);
        assert_eq!(edge_at(&f, &c, 70.0, 55.0), None, "the f→g midpoint");
        // Hiding the layer hides the walk.
        g.layers[0].visible = false;
        let f2 = build_frame(&g, false);
        assert!(f2.segs.is_empty() && f2.arrows.is_empty());
    }

    #[test]
    fn two_way_links_bow_apart() {
        // The vault's most common parallel: a→b and b→a. Their bows must
        // land on opposite sides (the b→a edge's perpendicular flips), or
        // they overdraw each other and only one is ever picked.
        let mut g = graph(
            vec![node("a", -20.0, 0.0), node("b", 20.0, 0.0)],
            vec![edge(0, 1), {
                let mut e = edge(1, 0);
                e.id = "back".into();
                e
            }],
        );
        g.with_bows();
        let f = build_frame(&g, false);
        assert_eq!(f.segs.len(), 2 * CURVE_SEGS);
        let y_at_mid = |e: usize| {
            let first = f.seg_edge.iter().position(|&x| x == Some(e)).unwrap();
            f.segs[first + CURVE_SEGS / 2].a[1]
        };
        let (y0, y1) = (y_at_mid(0), y_at_mid(1));
        assert!(
            y0 * y1 < 0.0,
            "opposite sides, not the same curve: {y0} vs {y1}"
        );
        // Picking on the a→b curve finds the a→b edge, not the other.
        let c = cam(100.0, 100.0);
        let hit = edge_at(&f, &c, 50.0 + 0.0 + (y0 / 2.0), 50.0 + y0 / 2.0);
        assert_eq!(hit, Some(0), "the a→b curve picks the a→b edge");
    }

    #[test]
    fn a_shared_curved_edge_does_not_double_its_own_bow() {
        // The layer rank adds to the route's offset exactly once (B5 of the
        // review): control points at 9∓3, not 9∓3 plus the route's own 9.
        let mut e = edge(0, 1);
        e.route = Route::Bezier { offset: 9.0 };
        e.layers = vec!["x".into(), "y".into()];
        let mut g = graph(vec![node("a", 0.0, 0.0), node("b", 40.0, 0.0)], vec![e]);
        g.layers = vec![
            layer("x", [0.1, 0.1, 0.9, 0.9], true),
            layer("y", [0.9, 0.9, 0.1, 0.9], true),
        ];
        let f = build_frame(&g, false);
        assert_eq!(f.segs.len(), 2 * CURVE_SEGS);
        let y_at_mid = |stroke: usize| {
            let first = stroke * CURVE_SEGS;
            f.segs[first + CURVE_SEGS / 2].a[1]
        };
        let (y0, y1) = (y_at_mid(0), y_at_mid(1));
        // Control points at 7.5 and 10.5 → apexes (half the control
        // offset) at 3.75 and 5.25 — the route's own 9 counted once.
        assert!((y0 - 3.75).abs() < 0.1, "stroke 0 apex {y0}, wanted 3.75");
        assert!((y1 - 5.25).abs() < 0.1, "stroke 1 apex {y1}, wanted 5.25");
    }

    #[test]
    fn dash_phases_accumulate_along_a_curve() {
        // The pattern must continue across sub-segments (B7): each phase is
        // the arc length before it, not zero.
        let mut e = edge(0, 1);
        e.route = Route::Bezier { offset: 9.0 };
        e.dash = 8.0;
        let g = graph(vec![node("a", 0.0, 0.0), node("b", 40.0, 0.0)], vec![e]);
        let f = build_frame(&g, false);
        assert_eq!(f.seg_attrs.len(), CURVE_SEGS);
        for k in 1..CURVE_SEGS {
            let prev = &f.segs[k - 1];
            let len = ((prev.b[0] - prev.a[0]).powi(2) + (prev.b[1] - prev.a[1]).powi(2)).sqrt();
            assert!(
                (f.seg_attrs[k].phase - f.seg_attrs[k - 1].phase - len).abs() < 1e-3,
                "phase {} does not continue the arc",
                k
            );
        }
        assert_eq!(f.seg_attrs[0].phase, 0.0);
    }

    #[test]
    fn a_negative_bow_loop_still_touches_its_node() {
        // The ring below the node (a negative bow) must start and end at the
        // node itself, like the one above (B9).
        let mut e = edge(0, 0);
        e.route = Route::Bezier { offset: -9.0 };
        let g = graph(vec![node("a", 0.0, 0.0)], vec![e]);
        let f = build_frame(&g, false);
        let near = |p: [f32; 3]| p.iter().all(|c| c.abs() < 1e-4);
        assert!(near(f.segs[0].a), "starts at the node: {:?}", f.segs[0].a);
        assert!(
            near(f.segs[LOOP_SEGS - 1].b),
            "ends at the node: {:?}",
            f.segs[LOOP_SEGS - 1].b
        );
    }

    #[test]
    fn port_anchored_heads_pull_back_by_the_port() {
        // A head at a port anchor sits outside it, not buried inside the
        // node the port belongs to (B10).
        let mut e = edge(0, 1);
        e.arrow = Arrow::Target;
        e.b_port = Some("in".into());
        let g = Graph {
            nodes: vec![node("a", 0.0, 0.0), node("b", 40.0, 0.0)],
            edges: vec![e],
            ports: vec![port(1, "in", InSide::Left, 0.5)],
            layers: Vec::new(),
            traces: Vec::new(),
            groups: Vec::new(),
            hyperedges: Vec::new(),
            simplexes: Vec::new(),
        };
        let f = build_frame(&g, false);
        assert_eq!(f.arrows.len(), 1);
        let head = f.arrows[0];
        // The port sits at x 32.5; the head pulls back PORT_RADIUS + half a
        // head: 32.5 − 2.5 − 3 = 27.
        assert!((head.pos[0] - 27.0).abs() < 1e-4, "x {}", head.pos[0]);
    }

    #[test]
    fn build_frame_into_reuses_without_accumulating() {
        // The renderer drives the reusing form every layout frame; a buffer
        // that is not cleared first accumulates silently (the review found
        // it as 63 ports for 3).
        let g = Graph {
            nodes: vec![node("a", 0.0, 0.0), node("b", 40.0, 0.0)],
            edges: vec![edge(0, 1)],
            ports: vec![port(0, "out", InSide::Right, 0.5)],
            layers: Vec::new(),
            traces: Vec::new(),
            groups: Vec::new(),
            hyperedges: Vec::new(),
            simplexes: Vec::new(),
        };
        let mut f = Frame::default();
        let mut scratch = Vec::new();
        for _ in 0..5 {
            build_frame_into(&g, false, &mut f, &mut scratch);
        }
        assert_eq!(f.port_pos.len(), 1, "ports accumulated");
        assert_eq!(f.node_visible.len(), 2, "visibility accumulated");
        assert_eq!(f.segs.len(), 1, "segments accumulated");
        assert_eq!(f.seg_attrs.len(), 1);
        assert_eq!(f.seg_edge.len(), 1);
    }

    #[test]
    fn edge_at_hits_a_self_loop() {
        let mut e = edge(0, 0);
        e.route = Route::Bezier { offset: 9.0 };
        let g = graph(vec![node("a", 20.0, 20.0)], vec![e]);
        let f = build_frame(&g, false);
        let c = cam(100.0, 100.0);
        // The ring's centre is 14.15 above the node (world y −8.3), so its
        // top sits at world y ≈ −22.4 → screen y = 50 − 22.4 + 14.15·2…
        // simply: top = centre − r → world (20, 5.85 − 14.15) = (20, −8.3)
        // → screen (70, 41.7).
        let hit = edge_at(&f, &c, 70.0, 41.7);
        assert_eq!(hit, Some(0));
    }

    fn group(id: &str, members: &[usize], color: &str) -> crate::graph::Group {
        crate::graph::Group {
            id: id.into(),
            label: id.into(),
            members: members.to_vec(),
            color: crate::graph::parse_hex(color, 1.0).unwrap(),
            visible: true,
            highlight: false,
        }
    }

    #[test]
    fn a_group_tints_the_box_of_its_members() {
        // Spec 031 §4: one hull per group — the axis-aligned box of the
        // members' rims (radius 5 here), padded by rim + HULL_PAD (12).
        let mut g = graph(
            vec![
                node("a", 0.0, 0.0),
                node("b", 40.0, 10.0),
                node("c", -60.0, 50.0),
            ],
            vec![edge(0, 1)],
        );
        g.groups = vec![group("g", &[0, 1], "#596cff")];
        let f = build_frame(&g, false);
        assert_eq!(f.hulls.len(), 1);
        let h = f.hulls[0];
        assert!(
            (h.center[0] - 20.0).abs() < 1e-4,
            "centre x {}",
            h.center[0]
        );
        assert!((h.center[1] - 5.0).abs() < 1e-4, "centre y {}", h.center[1]);
        let pad = 5.0 + HULL_PAD;
        assert!(
            (h.half[0] - (25.0 + pad)).abs() < 1e-4,
            "half x {}",
            h.half[0]
        );
        assert!(
            (h.half[1] - (10.0 + pad)).abs() < 1e-4,
            "half y {}",
            h.half[1]
        );
        // The tint: the group colour with HULL_FILL strength.
        let c = crate::graph::parse_hex("#596cff", HULL_FILL).unwrap();
        assert!((f.hull_attrs[0].color[0] - c[0]).abs() < 1e-4);
        assert_eq!(f.hull_attrs[0].corner, HULL_CORNER);
        // 3D keeps plain nodes and edges: no hulls (the mapping's rule).
        assert!(build_frame(&g, true).hulls.is_empty());
    }

    #[test]
    fn hidden_or_gone_members_do_not_stretch_the_tint() {
        // An invisible member (a hidden layer) leaves no mark; a group with
        // no visible member draws nothing at all.
        let mut g = graph(
            vec![node("a", 0.0, 0.0), node("b", 200.0, 0.0)],
            vec![edge(0, 1)],
        );
        g.nodes[1].layers = vec!["ghost".into()];
        g.layers = vec![layer("ghost", [0.9, 0.2, 0.2, 0.9], false)];
        g.groups = vec![
            group("near", &[0], "#596cff"),
            group("far", &[1], "#596cff"),
        ];
        let f = build_frame(&g, false);
        assert_eq!(f.hulls.len(), 1, "only the visible member's hull");
        assert!(
            f.hulls[0].half[0] < 30.0,
            "the hidden member did not stretch it"
        );
        // The group itself hidden: nothing.
        g.groups[0].visible = false;
        assert!(build_frame(&g, false).hulls.is_empty());
    }

    #[test]
    fn nested_and_overlapping_groups_each_get_their_own_hull() {
        // Spec 031 §4: nesting and overlapping visual memberships are not
        // conflated — two groups sharing a member both tint.
        let mut g = graph(
            vec![
                node("a", 0.0, 0.0),
                node("b", 30.0, 0.0),
                node("c", 60.0, 0.0),
            ],
            vec![edge(0, 1), edge(1, 2)],
        );
        g.groups = vec![
            group("outer", &[0, 1, 2], "#596cff"),
            group("pair", &[0, 1], "#ff5959"),
        ];
        let f = build_frame(&g, false);
        assert_eq!(f.hulls.len(), 2);
        let widths: Vec<f32> = f.hulls.iter().map(|h| h.half[0]).collect();
        assert!(widths[0] > widths[1], "the outer box is the wider one");
        // The shared member sits in both boxes.
        for h in &f.hulls {
            assert!(h.half[0] >= 30.0 && h.center[0] - h.half[0] < 0.0 + 1e-4);
        }
    }

    #[test]
    fn a_hyperedge_draws_a_junction_with_spokes_never_picked() {
        // Spec 031 §2: a hyperedge is a junction — a marker at the
        // members' centroid, a spoke to each — and never new topology:
        // no spoke picks as an edge.
        let mut g = graph(
            vec![
                node("a", 0.0, 0.0),
                node("b", 30.0, 0.0),
                node("c", 0.0, 30.0),
            ],
            vec![],
        );
        g.hyperedges = vec![crate::graph::Hyperedge {
            id: "h".into(),
            nodes: vec![0, 1, 2],
            color: [0.75, 0.55, 0.95, 0.85],
            width: 1.4,
            dash: 0.0,
            visible: true,
        }];
        let f = build_frame(&g, false);
        assert_eq!(f.segs.len(), 3, "one spoke per member");
        assert!(f.seg_edge.iter().all(|o| o.is_none()), "never an edge");
        assert_eq!(f.junctions.len(), 1);
        let j = f.junctions[0];
        assert!((j.pos[0] - 10.0).abs() < 1e-4 && (j.pos[1] - 10.0).abs() < 1e-4);
        assert_eq!(j.color, [0.75, 0.55, 0.95, 0.85]);
        // A spoke midpoint never picks (presentation, not topology — §2).
        let c = cam(100.0, 100.0);
        let mid = [
            (f.segs[0].a[0] + f.segs[0].b[0]) / 2.0,
            (f.segs[0].a[1] + f.segs[0].b[1]) / 2.0,
        ];
        let s = c.world_to_screen(mid[0], mid[1]);
        assert_eq!(edge_at(&f, &c, s.0, s.1), None);
        // Hidden: no spokes, no marker. 3D keeps the junction and spokes.
        g.hyperedges[0].visible = false;
        let f2 = build_frame(&g, false);
        assert!(f2.segs.is_empty() && f2.junctions.is_empty());
        g.hyperedges[0].visible = true;
        let f3 = build_frame(&g, true);
        assert_eq!(f3.junctions.len(), 1, "3D keeps the junction");
    }

    #[test]
    fn a_simplex_fills_one_triangle_or_a_fan() {
        // Spec 031 §2: three members → one filled triangle; more → a fan
        // from the centroid, closing the loop; 2D only.
        let mut g = graph(
            vec![
                node("a", 0.0, 0.0),
                node("b", 40.0, 0.0),
                node("c", 0.0, 40.0),
                node("d", 40.0, 40.0),
            ],
            vec![],
        );
        g.simplexes = vec![crate::graph::Simplex {
            id: "s".into(),
            nodes: vec![0, 1, 2],
            color: [0.95, 0.60, 0.30, 0.9],
            visible: true,
        }];
        let f = build_frame(&g, false);
        assert_eq!(f.simplex_tris.len(), 1);
        let t = f.simplex_tris[0];
        assert_eq!(t.a, [0.0, 0.0, 0.0]);
        assert_eq!(t.b, [40.0, 0.0, 0.0]);
        assert_eq!(t.c, [0.0, 40.0, 0.0]);
        assert_eq!(f.simplex_attrs[0].color, [0.95, 0.60, 0.30, 0.9]);
        // 3D draws no faces (like the hulls).
        assert!(build_frame(&g, true).simplex_tris.is_empty());
        // Four members fan into four triangles around the centroid (20, 20).
        g.simplexes[0].nodes = vec![0, 1, 3, 2];
        let f2 = build_frame(&g, false);
        assert_eq!(f2.simplex_tris.len(), 4, "a fan of n triangles");
        for t in &f2.simplex_tris {
            assert_eq!(
                t.a,
                [20.0, 20.0, 0.0],
                "each fan triangle starts at the centroid"
            );
        }
    }

    #[test]
    fn a_highlighted_clique_wears_a_stronger_tint_than_a_group() {
        // Spec 031 §2: a highlighted clique is not a group — same hull
        // mechanism, twice the tint and the highlight rim flag.
        let mut g = graph(vec![node("a", 0.0, 0.0), node("b", 30.0, 0.0)], vec![]);
        g.groups = vec![group("plain", &[0, 1], "#59ff8c")];
        let plain = build_frame(&g, false).hull_attrs[0];
        g.groups[0].highlight = true;
        let lit = build_frame(&g, false).hull_attrs[0];
        assert!(
            (plain.color[3] - crate::graph::parse_hex("#59ff8c", HULL_FILL).unwrap()[3]).abs()
                < 1e-4
        );
        assert!(
            (lit.color[3] - crate::graph::parse_hex("#59ff8c", HULL_HIGHLIGHT_FILL).unwrap()[3])
                .abs()
                < 1e-4,
            "the highlight tints twice as strong"
        );
        assert_eq!(plain.highlight, 0.0);
        assert_eq!(lit.highlight, 1.0);
    }

    #[test]
    fn a_pulsed_edge_carries_speed_and_its_full_length() {
        // Spec 031 §2, stage 6: the glow is positioned in world units over
        // the whole stroke, so every sub-segment of a curve carries the
        // speed and the stroke's total length — it crosses them as one
        // continuous line, and each sub-segment's phase finds it.
        let mut e = edge(0, 1);
        e.route = Route::Bezier { offset: 15.0 };
        e.pulse_speed = 30.0;
        let g = graph(vec![node("a", 0.0, 0.0), node("b", 40.0, 0.0)], vec![e]);
        let f = build_frame(&g, false);
        assert_eq!(f.seg_attrs.len(), CURVE_SEGS);
        for a in &f.seg_attrs {
            assert_eq!(a.pulse, 30.0);
            assert!(a.edge_len > 40.0, "a bowed stroke is longer than its chord");
        }
        // The straight stroke: the chord exactly. A plain edge carries no
        // length at all — the pass is gated to pulsed and dashed strokes
        // (the review's #10).
        let mut plain = edge(0, 1);
        plain.pulse_speed = 20.0;
        let f2 = build_frame(
            &graph(vec![node("a", 0.0, 0.0), node("b", 40.0, 0.0)], vec![plain]),
            false,
        );
        assert!((f2.seg_attrs[0].edge_len - 40.0).abs() < 1e-4);
        let f3 = build_frame(
            &graph(
                vec![node("a", 0.0, 0.0), node("b", 40.0, 0.0)],
                vec![edge(0, 1)],
            ),
            false,
        );
        assert_eq!(f3.seg_attrs[0].pulse, 0.0);
        assert_eq!(
            f3.seg_attrs[0].edge_len, 0.0,
            "a plain stroke skips the pass"
        );
    }

    #[test]
    fn an_ortho_route_steps_between_the_ports() {
        // The flow route (spec 031 SS2, stage 7): out of the source's
        // port, across, into the target's — three square-cornered
        // segments, and the head rides the last tangent.
        let mut e = edge(0, 1);
        e.route = Route::Ortho { stub: None };
        e.a_port = Some("out".into());
        e.b_port = Some("in".into());
        e.arrow = Arrow::Target;
        let mut g = graph(vec![node("a", 0.0, 0.0), node("b", 60.0, 40.0)], vec![e]);
        g.ports = vec![
            port(0, "out", InSide::Right, 0.5),
            port(1, "in", InSide::Left, 0.5),
        ];
        let f = build_frame(&g, false);
        assert_eq!(f.segs.len(), 3, "out, across, in");
        let a_end = f.segs[0].a;
        let b_end = f.segs[2].b;
        // The stub is half the gap: 7.5 world off a's port (x 7.5).
        assert!((a_end[0] - 7.5).abs() < 1e-4, "starts at a's port");
        // The middle segment is the vertical leg: one column of corners.
        assert_eq!(f.segs[0].b[0], f.segs[1].a[0], "square corners");
        // The stub is half the gap: a's port at 7.5, b's at 52.5 → 22.5.
        assert!(
            (f.segs[1].a[0] - (a_end[0] + 22.5)).abs() < 1e-4,
            "half-way across"
        );
        assert!(
            (f.segs[1].a[1] - a_end[1]).abs() < 1e-4,
            "across at a's height"
        );
        assert!(
            (f.segs[1].b[1] - b_end[1]).abs() < 1e-4,
            "down to b's height"
        );
        assert!(
            (b_end[0] - 52.5).abs() < 1e-4 && (b_end[1] - 40.0).abs() < 1e-4,
            "ends at b's port"
        );
        // The head points into the target along the last tangent (+x:
        // b's left port faces the corner from the left).
        assert_eq!(f.arrows.len(), 1);
        assert!(
            f.arrows[0].angle.abs() < 1e-4,
            "angle {}",
            f.arrows[0].angle
        );
    }

    #[test]
    fn an_ortho_route_detours_round_pins_that_face_away() {
        // ADR-0024: a capacitor's right pin back to a source's left pin,
        // both on one line — the route must not run through the parts
        // between them. It leaves right, drops below both, crosses, and
        // comes up into the left pin from the left.
        let rect = |id: &str, x: f32| {
            let mut n = node(id, x, 0.0);
            n.shape = crate::graph::Shape::Rect;
            n.radius = 20.0;
            n.height = 10.0;
            n
        };
        let mut e = edge(0, 1);
        e.route = Route::Ortho { stub: None };
        e.a_port = Some("n".into());
        e.b_port = Some("p".into());
        let mut g = graph(vec![rect("c", 200.0), rect("v", 0.0)], vec![e]);
        g.ports = vec![
            port(0, "n", InSide::Right, 0.5),
            port(1, "p", InSide::Left, 0.5),
        ];
        let f = build_frame(&g, false);
        let pts: Vec<[f32; 3]> = std::iter::once(f.segs[0].a)
            .chain(f.segs.iter().map(|s| s.b))
            .collect();
        assert_eq!(pts.len(), 6, "out, down, across, up, in: {pts:?}");
        let (start, end) = (pts[0], pts[5]);
        assert!(pts[1][0] > start[0], "leaves to the right");
        assert!(pts[4][0] < end[0], "enters from the left");
        let below = pts[2][1];
        assert!(below > 10.0, "the crossing runs below both parts: {below}");
        assert_eq!(pts[2][1], pts[3][1], "one level across");
        for w in pts.windows(2) {
            assert!(w[0][0] == w[1][0] || w[0][1] == w[1][1], "square corners");
        }
    }

    #[test]
    fn an_ortho_route_meets_same_facing_pins_beyond_the_farther() {
        let mut e = edge(0, 1);
        e.route = Route::Ortho { stub: None };
        e.a_port = Some("a".into());
        e.b_port = Some("b".into());
        let mut g = graph(vec![node("a", 0.0, 0.0), node("b", 50.0, 60.0)], vec![e]);
        g.ports = vec![
            port(0, "a", InSide::Right, 0.5),
            port(1, "b", InSide::Right, 0.5),
        ];
        let f = build_frame(&g, false);
        assert_eq!(f.segs.len(), 3);
        let b_port = f.segs[2].b[0];
        assert!(f.segs[1].a[0] > b_port, "the turn lies past b's pin");
        assert_eq!(f.segs[1].a[0], f.segs[1].b[0], "a vertical leg");
    }

    #[test]
    fn a_rect_node_puts_its_ports_on_its_sides() {
        // A flow block (spec 031 SS2, stage 7): the port rides the
        // rectangle — pushed out by the side's half-extent, spread along
        // the other axis.
        let mut n = node("blk", 0.0, 0.0);
        n.shape = crate::graph::Shape::Rect;
        n.radius = 46.0; // its half-width
        n.height = 22.0;
        let mut g = graph(vec![n], vec![]);
        g.ports = vec![
            port(0, "in", InSide::Left, 0.25),
            port(0, "out", InSide::Right, 0.75),
        ];
        let f = build_frame(&g, false);
        let near = |a: [f32; 3], x: f32, y: f32| (a[0] - x).abs() < 1e-4 && (a[1] - y).abs() < 1e-4;
        // Left: x −(46 + 2.5), y at a quarter of the height's span.
        assert!(
            near(f.port_pos[0], -(46.0 + PORT_STANDOFF), (0.25 - 0.5) * 44.0),
            "in: {:?}",
            f.port_pos[0]
        );
        assert!(
            near(f.port_pos[1], 46.0 + PORT_STANDOFF, (0.75 - 0.5) * 44.0),
            "out: {:?}",
            f.port_pos[1]
        );
    }

    #[test]
    fn a_diamond_node_puts_its_ports_on_its_sides() {
        // Stage 8 (spec 031 §2): a decision block's ports ride the same
        // half-extents as a rect's.
        let mut n = node("dec", 0.0, 0.0);
        n.shape = crate::graph::Shape::Diamond;
        n.radius = 50.0;
        n.height = 24.0;
        let mut g = graph(vec![n], vec![]);
        g.ports = vec![
            port(0, "in", InSide::Left, 0.5),
            port(0, "out", InSide::Right, 0.5),
        ];
        let f = build_frame(&g, false);
        let near = |a: [f32; 3], x: f32, y: f32| (a[0] - x).abs() < 1e-4 && (a[1] - y).abs() < 1e-4;
        assert!(
            near(f.port_pos[0], -(50.0 + PORT_STANDOFF), 0.0),
            "in: {:?}",
            f.port_pos[0]
        );
        assert!(
            near(f.port_pos[1], 50.0 + PORT_STANDOFF, 0.0),
            "out: {:?}",
            f.port_pos[1]
        );
    }

    #[test]
    fn hyperedges_and_simplices_do_not_reach_hidden_nodes() {
        // The P-166 review's #6: an invisible member (a hidden layer)
        // draws nothing — its spoke must not point at it, the junction
        // must not lean toward it, and no face may stretch to it.
        let mut g = graph(
            vec![
                node("a", 0.0, 0.0),
                node("b", 40.0, 0.0),
                node("c", 0.0, 40.0),
            ],
            vec![],
        );
        g.nodes[2].layers = vec!["ghost".into()];
        g.layers = vec![layer("ghost", [0.9, 0.2, 0.2, 0.9], false)];
        g.hyperedges = vec![crate::graph::Hyperedge {
            id: "h".into(),
            nodes: vec![0, 1, 2],
            color: [0.75, 0.55, 0.95, 0.85],
            width: 1.4,
            dash: 0.0,
            visible: true,
        }];
        g.simplexes = vec![crate::graph::Simplex {
            id: "s".into(),
            nodes: vec![0, 1, 2],
            color: [0.95, 0.6, 0.3, 0.9],
            visible: true,
        }];
        let f = build_frame(&g, false);
        // Two visible members: one spoke each, the centroid over them
        // (20, 0) — the hidden member pulled neither.
        assert_eq!(f.junctions.len(), 1);
        assert_eq!(f.segs.len(), 2, "no spoke to the hidden node");
        assert!((f.junctions[0].pos[0] - 20.0).abs() < 1e-4);
        assert!((f.junctions[0].pos[1] - 0.0).abs() < 1e-4);
        // Two visible corners: no face.
        assert!(f.simplex_tris.is_empty());
    }
}
