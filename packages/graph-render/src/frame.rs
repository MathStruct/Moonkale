//! Spec 031, stage 1: the pure scene → instance bridge. [`build_frame`]
//! turns a [`Graph`] plus its edge styles into draw lists — edge segments
//! (parallels bow apart, self-loops circle the node), arrowheads — with no
//! GPU, no browser and no Moonkale types, so the geometry is unit-testable
//! natively. Stage 4 adds LOD filtering and culling here; stage 7 adds
//! orthogonal routes. [`edge_at`] picks an edge from a screen point over the
//! same segments.

use crate::camera::Camera;
use crate::graph::{Arrow, Edge, Graph, InSide, Port, Route};
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
    pub pos_rev: u64,
    pub attr_rev: u64,
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

#[derive(Default)]
pub struct Frame {
    pub segs: Vec<SegPos>,
    pub seg_attrs: Vec<SegAttr>,
    /// Which edge each segment belongs to (picking; not uploaded).
    pub seg_edge: Vec<usize>,
    pub arrows: Vec<ArrowPos>,
    pub arrow_attrs: Vec<ArrowAttr>,
    /// World positions of the graph's ports, same order as `graph.ports`
    /// (drawn in edit mode, picked always — spec 031 §2, stage 2).
    pub port_pos: Vec<[f32; 3]>,
}

/// The scene → instance lists for one frame.
pub fn build_frame(graph: &Graph, three_d: bool) -> Frame {
    let mut f = Frame {
        port_pos: graph.ports.iter().map(|p| port_world(graph, p)).collect(),
        ..Default::default()
    };
    for (i, e) in graph.edges.iter().enumerate() {
        let line = polyline(graph, e);
        for w in line.windows(2) {
            f.segs.push(SegPos { a: w[0], b: w[1] });
            f.seg_edge.push(i);
            f.seg_attrs.push(SegAttr {
                color: e.color,
                width: e.width,
                dash: e.dash,
            });
        }
        if three_d {
            // 3D keeps plain edges (spec 031 mapping): no arrowheads there.
            continue;
        }
        let n = line.len();
        if n < 2 {
            continue;
        }
        // The tangent each end travels along, from the tessellation, so
        // straight, curved and loop edges place their heads the same way.
        let into_b = normalize2(sub2(line[n - 1], line[n - 2]));
        let out_of_a = normalize2(sub2(line[1], line[0]));
        let a_pos = line[0];
        let b_pos = line[n - 1];
        let a_radius = graph.nodes[e.a].radius;
        let b_radius = graph.nodes[e.b].radius;
        // Arrowheads sit at their end's rim, pointing into its node; the
        // head's colour is the edge's with the alpha lifted, so it reads
        // over its own stroke.
        let mut head_color = e.color;
        head_color[3] = head_color[3].max(0.8);
        let head = |tip_at: [f32; 3], dir: [f32; 2], radius: f32| -> (ArrowPos, ArrowAttr) {
            let d = normalize2(dir);
            let pull = radius + 1.0 + ARROW_SIZE * 0.5;
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
    f
}

/// The edge's world-space polyline (its tessellation): `[a, …, b]`.
fn polyline(graph: &Graph, e: &Edge) -> Vec<[f32; 3]> {
    let na = &graph.nodes[e.a];
    let a_end = anchored(graph, e.a, &e.a_port);
    let b_end = anchored(graph, e.b, &e.b_port);
    if e.a == e.b {
        // A self-loop: a circle resting on the node, starting and ending at
        // its centre so it reads as attached; the bow widens the ring.
        let offset = match e.route {
            Route::Bezier { offset } => offset,
            Route::Straight => 0.0,
        };
        let side = if offset < 0.0 { -1.0 } else { 1.0 };
        let r = na.radius + 6.0 + offset.abs() * 0.35;
        let (cx, cy) = (na.x, na.y - side * r);
        (0..=LOOP_SEGS)
            .map(|k| {
                let t = std::f32::consts::FRAC_PI_2
                    + k as f32 * std::f32::consts::TAU / LOOP_SEGS as f32;
                [cx + r * t.cos(), cy + r * t.sin(), na.z]
            })
            .collect()
    } else {
        match e.route {
            Route::Straight => vec![a_end, b_end],
            Route::Bezier { offset } => {
                let (dx, dy) = (b_end[0] - a_end[0], b_end[1] - a_end[1]);
                let len = (dx * dx + dy * dy).sqrt().max(1e-3);
                // Perpendicular of the endpoint line, unit length.
                let (px, py) = (-dy / len, dx / len);
                let (mx, my) = (
                    (a_end[0] + b_end[0]) / 2.0 + px * offset,
                    (a_end[1] + b_end[1]) / 2.0 + py * offset,
                );
                (0..=CURVE_SEGS)
                    .map(|k| {
                        let t = k as f32 / CURVE_SEGS as f32;
                        let s = 1.0 - t;
                        bezier(a_end, [mx, my, (a_end[2] + b_end[2]) / 2.0], b_end, t, s)
                    })
                    .collect()
            }
        }
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
    let n = &graph.nodes[port.node];
    let (dir, tangent) = match port.side {
        InSide::Right => ([1.0, 0.0], [0.0, 1.0]),
        InSide::Left => ([-1.0, 0.0], [0.0, 1.0]),
        InSide::Top => ([0.0, -1.0], [1.0, 0.0]),
        InSide::Bottom => ([0.0, 1.0], [1.0, 0.0]),
    };
    let along = (port.offset - 0.5) * n.radius * 2.0;
    [
        n.x + dir[0] * (n.radius + PORT_STANDOFF) + tangent[0] * along,
        n.y + dir[1] * (n.radius + PORT_STANDOFF) + tangent[1] * along,
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
        let (Some((ax, ay)), Some((bx, by))) = (project(seg.a), project(seg.b)) else {
            continue;
        };
        let d = dist_to_segment(x, y, ax, ay, bx, by);
        let reach = (frame.seg_attrs[i].width * 0.5 + 2.5).max(4.0);
        if d <= reach && best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, frame.seg_edge[i]));
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
        }
    }

    fn graph(nodes: Vec<Node>, edges: Vec<Edge>) -> Graph {
        Graph {
            nodes,
            edges,
            ports: Vec::new(),
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
        assert_eq!(f.seg_edge, vec![0]);
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
            .map(|e| f.seg_edge.iter().filter(|&&x| x == e).count())
            .collect();
        assert_eq!(counts, vec![CURVE_SEGS, 1, CURVE_SEGS]);
        // The bows sit on opposite sides of the endpoints' line (y = 0):
        // the point at t = 0.5 of each curve is half its control offset out.
        let y_at_mid = |e: usize| {
            let first = f.seg_edge.iter().position(|&x| x == e).unwrap();
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
        };
        let f = build_frame(&g, false);
        let c = cam(100.0, 100.0);
        // World (7.5, 0) → screen (57.5, 50).
        assert_eq!(port_at(&f, &c, 57.5, 50.0), Some(0));
        assert_eq!(port_at(&f, &c, 57.5, 54.0), Some(0), "within the reach");
        assert_eq!(port_at(&f, &c, 57.5, 62.0), None, "too far");
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
}
