//! The renderer's own graph: what the host sends, plus positions.

use serde::{Deserialize, Serialize};

/// Parallel edges bow apart in half-steps of this many world units
/// (spec 031 §2; the ideal edge length is ~28, so a pair sits visibly apart).
pub const BOW_SPACING: f32 = 9.0;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct InNode {
    pub id: String,
    pub label: String,
    /// `"file" | "directory" | "page" | "symbol" | other` — picks the colour.
    pub kind: String,
    #[serde(default)]
    pub key: String,
    /// Optional host-chosen colour `#rrggbb` (per-label palettes); falls
    /// back to [`color_for`]`(kind)`.
    #[serde(default)]
    pub color: Option<String>,
    /// Visual layers this node belongs to (spec 031 §3); a node with layers
    /// draws only while one of them is visible.
    #[serde(default)]
    pub layers: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct InEdge {
    /// Indices into `nodes`.
    pub a: usize,
    pub b: usize,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub color: Option<String>,
    /// Stable instance id; when the host sends none, `{a}:{b}:{i}` is derived
    /// (unique per position in the input — hosts that reload graphs should
    /// send their own, spec 031 §1).
    #[serde(default)]
    pub id: Option<String>,
    /// A directed edge carries an arrowhead at its target unless `arrow`
    /// says otherwise.
    #[serde(default)]
    pub directed: bool,
    /// Stroke width in screen pixels (default 1.2).
    #[serde(default)]
    pub width: Option<f32>,
    /// Dash length in screen pixels; absent = solid.
    #[serde(default)]
    pub dash: Option<f32>,
    /// Multiplies the edge's alpha (0..1).
    #[serde(default)]
    pub opacity: Option<f32>,
    /// Arrowheads override `directed`'s default (target only).
    #[serde(default)]
    pub arrow: Option<InArrow>,
    /// Shown on hover (the panel's popup), stage 1 of spec 031.
    #[serde(default)]
    pub label: Option<String>,
    /// `straight` (default) or `bezier { offset }` — a perpendicular control
    /// offset in world units (spec 031 §2; orthogonal routes come with the
    /// flow editor, stage 7).
    #[serde(default)]
    pub route: Option<InRoute>,
    /// Anchor at a port of `a`/`b` by name (spec 031 §2, stage 2); absent =
    /// the node's centre.
    #[serde(default)]
    pub a_port: Option<String>,
    #[serde(default)]
    pub b_port: Option<String>,
    /// Visual layers this edge belongs to (spec 031 §3): the first visible
    /// layer styles it; two or more visible layers draw it once per layer,
    /// as offset parallel strokes.
    #[serde(default)]
    pub layers: Vec<String>,
}

/// Where arrowheads sit on an edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InArrow {
    Source,
    Target,
    Both,
}

/// How an edge reaches its target.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum InRoute {
    Straight,
    /// A quadratic Bézier whose control point sits `offset` world units off
    /// the midpoint, perpendicular to the endpoints' line.
    Bezier {
        offset: f32,
    },
}

/// The resolved form of [`InArrow`] plus the "none" case.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arrow {
    None,
    Source,
    Target,
    Both,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Route {
    Straight,
    Bezier { offset: f32 },
}

/// The internal port: the wire's [`InPort`] with the appearance resolved.
#[derive(Clone, Debug)]
pub struct Port {
    pub node: usize,
    pub name: String,
    pub color: [f32; 4],
    pub side: InSide,
    pub offset: f32,
    pub direction: InDirection,
}

impl Default for Port {
    fn default() -> Self {
        Self {
            node: 0,
            name: String::new(),
            color: [0.55, 0.60, 0.72, 1.0],
            side: InSide::Right,
            offset: 0.5,
            direction: InDirection::Both,
        }
    }
}

/// The internal layer: style resolved, visibility mutable at runtime
/// (`set_layer_visibility`) — presentation only (spec 031 §3).
#[derive(Clone, Debug)]
pub struct Layer {
    pub id: String,
    pub color: Option<[f32; 4]>,
    pub visible: bool,
    pub overlay: bool,
    pub width: Option<f32>,
    pub dash: Option<f32>,
    pub opacity: Option<f32>,
}

/// The internal trace (spec 031 §3): an ordered walk over node ids.
#[derive(Clone, Debug)]
pub struct Trace {
    pub id: String,
    pub layer: Option<String>,
    pub steps: Vec<String>,
    pub color: Option<[f32; 4]>,
}

/// The internal group (spec 031 §4): members as node indices, re-resolved
/// whenever a delta moves them. The colour's alpha is the hull tint's
/// strength cap; the shader fades tiny boxes out (LOD, spec §6).
#[derive(Clone, Debug)]
pub struct Group {
    pub id: String,
    pub label: String,
    pub members: Vec<usize>,
    pub color: [f32; 4],
    pub visible: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct InGraph {
    pub nodes: Vec<InNode>,
    pub edges: Vec<InEdge>,
    /// Boundary ports (spec 031 §2): mostly for port diagrams and the flow
    /// editor's edit mode; absent in plain index graphs.
    #[serde(default)]
    pub ports: Vec<InPort>,
    /// Toggleable visual layers (spec 031 §3); styling and visibility only —
    /// a layer never mutates topology.
    #[serde(default)]
    pub layers: Vec<InLayer>,
    /// Ordered occurrence walks (spec 031 §3): a stack trace is a sequence
    /// of node instance ids, not a set of edges — recursion revisits ids.
    #[serde(default)]
    pub traces: Vec<InTrace>,
    /// Structural groups (spec 031 §4): a group tints the region its
    /// members occupy — presentation over topology; membership never
    /// changes what the edges connect.
    #[serde(default)]
    pub groups: Vec<InGroup>,
}

/// A group: which nodes it gathers, by id — the host's collapse/expand
/// drives membership through deltas; the renderer only tints (spec 031 §4).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InGroup {
    pub id: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub members: Vec<String>,
    /// Tint `#rrggbb`; absent = the theme's neutral group colour.
    #[serde(default)]
    pub color: Option<String>,
    /// Drawn while true (default).
    #[serde(default = "default_true")]
    pub visible: bool,
}

/// One visual layer: style knobs apply to its members; `overlay` marks a
/// temporary layer (debugging, selection, search) — presentation state the
/// host may drop by id.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InLayer {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    /// Drawn while true (default).
    #[serde(default = "default_true")]
    pub visible: bool,
    #[serde(default)]
    pub overlay: bool,
    #[serde(default)]
    pub width: Option<f32>,
    #[serde(default)]
    pub dash: Option<f32>,
    #[serde(default)]
    pub opacity: Option<f32>,
}

fn default_true() -> bool {
    true
}

/// An incremental change to the scene, **by instance id** (spec 031 §3's
/// updates-at-scale note, ADR-0015's `changes_since`): collapse/expand and
/// live hosts send this instead of re-serialising 100 k nodes through
/// `set_graph`. Removed nodes take their edges with them; added edges
/// reference their endpoints by node id, and one that names a missing node
/// is skipped (re-addable when its node arrives).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct InDelta {
    #[serde(default)]
    pub add_nodes: Vec<InDeltaNode>,
    #[serde(default)]
    pub remove_nodes: Vec<String>,
    #[serde(default)]
    pub add_edges: Vec<InDeltaEdge>,
    #[serde(default)]
    pub remove_edges: Vec<String>,
    /// Groups to add or replace by id (spec 031 §4): a collapse re-sends
    /// the group with the aggregate among its members; an expand re-sends
    /// it with the members back. Members name nodes by id; missing ones
    /// skip, so the order of a delta's parts never matters.
    #[serde(default)]
    pub add_groups: Vec<InGroup>,
    #[serde(default)]
    pub remove_groups: Vec<String>,
}

/// A node arriving through a delta (spec 031 §4: a collapsed group's
/// aggregate sits where the group was).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InDeltaNode {
    pub id: String,
    pub label: String,
    pub kind: String,
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub layers: Vec<String>,
    /// Where it starts; absent = the spiral, as a fresh node.
    #[serde(default)]
    pub pos: Option<[f32; 2]>,
    /// Start pinned (a placed aggregate stays put).
    #[serde(default)]
    pub pinned: bool,
}

/// An edge arriving through a delta: endpoints by **node id**, not index —
/// indices shift with removals. The style knobs match [`InEdge`].
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct InDeltaEdge {
    pub id: String,
    pub a: String,
    pub b: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub directed: bool,
    #[serde(default)]
    pub width: Option<f32>,
    #[serde(default)]
    pub dash: Option<f32>,
    #[serde(default)]
    pub opacity: Option<f32>,
    #[serde(default)]
    pub arrow: Option<InArrow>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub route: Option<InRoute>,
    #[serde(default)]
    pub layers: Vec<String>,
    #[serde(default)]
    pub a_port: Option<String>,
    #[serde(default)]
    pub b_port: Option<String>,
}

/// An ordered walk over node instance ids (spec 031 §3).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InTrace {
    pub id: String,
    /// The layer the walk draws in (its visibility and colour); absent =
    /// always drawn in its own colour.
    #[serde(default)]
    pub layer: Option<String>,
    /// Node instance ids, in order — the same id may appear twice
    /// (recursion); ids that do not resolve are skipped.
    pub steps: Vec<String>,
    #[serde(default)]
    pub color: Option<String>,
}

/// One port on a node's boundary. Placement is side-based — the flow
/// editor's language (inputs left, outputs right) — with `offset` (0..1)
/// spreading several ports along the side. `direction` is presentation and
/// gesture affordance only; type checking stays with the host (spec §7).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InPort {
    /// Index into `nodes`.
    pub node: usize,
    pub name: String,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub side: Option<InSide>,
    /// 0..1 along the side's tangent; 0.5 (the default) is its centre.
    #[serde(default)]
    pub offset: Option<f32>,
    #[serde(default)]
    pub direction: Option<InDirection>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InSide {
    Left,
    Right,
    Top,
    Bottom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InDirection {
    In,
    Out,
    Both,
}

#[derive(Clone, Debug)]
pub struct Node {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub key: String,
    pub x: f32,
    pub y: f32,
    /// Depth for the 3D mode (Milestone 8): a layer per kind, so the
    /// structure reads as planes (directories below files below symbols).
    pub z: f32,
    pub radius: f32,
    pub color: [f32; 4],
    pub degree: u32,
    /// Pinned by the user (being dragged): the layout leaves it alone.
    pub pinned: bool,
    /// Layer membership (spec 031 §3): an empty list is the base scene.
    pub layers: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Edge {
    pub id: String,
    pub a: usize,
    pub b: usize,
    pub color: [f32; 4],
    /// Stroke width in screen pixels.
    pub width: f32,
    /// Dash length in screen pixels; 0 = solid.
    pub dash: f32,
    pub arrow: Arrow,
    pub label: String,
    /// Resolved route; parallel edges get their bow here (spec 031 §2).
    pub route: Route,
    /// Port anchors by name, resolved in `frame` (stage 2).
    pub a_port: Option<String>,
    pub b_port: Option<String>,
    /// Layer membership (spec 031 §3): an empty list is the base scene.
    pub layers: Vec<String>,
    /// The parallel-fan-out offset (world units), assigned by
    /// [`Graph::with_bows`] on top of the host's route — an own field, so
    /// re-running the assignment never accumulates.
    pub bow: f32,
}

#[derive(Clone, Default)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub ports: Vec<Port>,
    pub layers: Vec<Layer>,
    pub traces: Vec<Trace>,
    pub groups: Vec<Group>,
}

pub fn color_for(kind: &str) -> [f32; 4] {
    match kind {
        "file" => [0.43, 0.66, 1.0, 1.0],    // blue
        "page" => [0.79, 0.65, 0.37, 1.0],   // amber: phantom / unresolved
        "symbol" => [0.42, 0.70, 0.55, 1.0], // green
        "directory" => [0.50, 0.53, 0.58, 1.0],
        "table" => [0.80, 0.50, 0.75, 1.0],
        "database" => [0.90, 0.45, 0.45, 1.0],
        "column" => [0.60, 0.60, 0.80, 1.0],
        "vertex" => [0.95, 0.60, 0.30, 1.0], // orange: graph-database data
        _ => [0.64, 0.66, 0.72, 1.0],
    }
}

/// `#rrggbb` → linear-ish RGBA with the given alpha.
pub fn parse_hex(s: &str, alpha: f32) -> Option<[f32; 4]> {
    let h = s.strip_prefix('#')?;
    // Bytes, not chars: a multi-byte colour string must not slice inside a
    // character (#12).
    if h.len() != 6 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let c = |i: usize| {
        u8::from_str_radix(&h[i..i + 2], 16)
            .ok()
            .map(|v| v as f32 / 255.0)
    };
    Some([c(0)?, c(2)?, c(4)?, alpha])
}

/// A wire group with its members resolved to indices; ids that name no
/// node are skipped (a group is re-addable through a later delta, like an
/// edge whose endpoint is missing). Neutral tint: a soft blue-grey.
fn resolve_group(g: InGroup, nodes: &[Node]) -> Group {
    let members = g
        .members
        .iter()
        .filter_map(|id| nodes.iter().position(|n| &n.id == id))
        .collect();
    Group {
        id: g.id,
        label: g.label,
        members,
        color: g
            .color
            .as_deref()
            .and_then(|c| parse_hex(c, 1.0))
            .unwrap_or([0.35, 0.55, 0.85, 1.0]),
        visible: g.visible,
    }
}

pub fn edge_color(kind: &str) -> [f32; 4] {
    match kind {
        "links" => [0.79, 0.65, 0.37, 0.55],
        "defines" | "contains" => [0.45, 0.48, 0.55, 0.35],
        "custom" => [0.95, 0.60, 0.30, 0.6], // a named relation (Cypher rel table)
        _ => [0.55, 0.58, 0.65, 0.4],
    }
}

impl Graph {
    /// Build from host input; positions start on a deterministic spiral so
    /// the layout converges the same way every time for the same graph.
    pub fn from_input(input: InGraph) -> Self {
        let n = input.nodes.len().max(1) as f32;
        let mut degree = vec![0u32; input.nodes.len()];
        for e in &input.edges {
            if e.a < degree.len() && e.b < degree.len() {
                degree[e.a] += 1;
                degree[e.b] += 1;
            }
        }
        let spread = 12.0 * n.sqrt();
        let nodes: Vec<Node> = input
            .nodes
            .into_iter()
            .enumerate()
            .map(|(i, n)| {
                let t = i as f32 * 2.399_963; // golden angle
                let r = spread * ((i as f32 + 1.0) / (degree.len() as f32 + 1.0)).sqrt();
                let deg = degree[i];
                let n_kind = n.kind.clone();
                Node {
                    radius: 4.0 + (deg as f32).sqrt().min(6.0),
                    color: n
                        .color
                        .as_deref()
                        .and_then(|c| parse_hex(c, 1.0))
                        .unwrap_or_else(|| color_for(&n.kind)),
                    id: n.id,
                    label: n.label,
                    kind: n.kind,
                    key: n.key,
                    x: r * t.cos(),
                    y: r * t.sin(),
                    z: layer_z(&n_kind, i),
                    degree: deg,
                    pinned: false,
                    layers: n.layers,
                }
            })
            .collect();
        let edges = input
            .edges
            .into_iter()
            .enumerate()
            .filter(|(_, e)| e.a < degree.len() && e.b < degree.len())
            .map(|(i, e)| {
                let arrow = match (e.arrow, e.directed) {
                    (Some(InArrow::Source), _) => Arrow::Source,
                    (Some(InArrow::Target), _) => Arrow::Target,
                    (Some(InArrow::Both), _) => Arrow::Both,
                    (None, true) => Arrow::Target,
                    (None, false) => Arrow::None,
                };
                let route = match e.route {
                    Some(InRoute::Bezier { offset }) => Route::Bezier { offset },
                    _ => Route::Straight,
                };
                let mut color = e
                    .color
                    .as_deref()
                    .and_then(|c| parse_hex(c, 0.55))
                    .unwrap_or_else(|| edge_color(&e.kind));
                if let Some(o) = e.opacity {
                    color[3] *= o.clamp(0.0, 1.0);
                }
                Edge {
                    id: e.id.unwrap_or_else(|| format!("{}:{}:{}", e.a, e.b, i)),
                    a: e.a,
                    b: e.b,
                    color,
                    width: e.width.unwrap_or(1.2).clamp(0.5, 12.0),
                    dash: e.dash.unwrap_or(0.0).clamp(0.0, 64.0),
                    arrow,
                    label: e.label.unwrap_or_default(),
                    route,
                    a_port: e.a_port,
                    b_port: e.b_port,
                    layers: e.layers,
                    bow: 0.0,
                }
            })
            .collect();
        let layers = input
            .layers
            .into_iter()
            .map(|l| Layer {
                id: l.id,
                color: l.color.as_deref().and_then(|c| parse_hex(c, 0.9)),
                visible: l.visible,
                overlay: l.overlay,
                width: l.width,
                dash: l.dash,
                opacity: l.opacity,
            })
            .collect();
        let traces = input
            .traces
            .into_iter()
            .map(|t| Trace {
                id: t.id,
                layer: t.layer,
                steps: t.steps,
                color: t.color.as_deref().and_then(|c| parse_hex(c, 0.95)),
            })
            .collect();
        let ports = input
            .ports
            .into_iter()
            .filter(|p| p.node < nodes.len())
            .map(|p| {
                let mut color = p
                    .color
                    .as_deref()
                    .and_then(|c| parse_hex(c, 1.0))
                    .unwrap_or([0.55, 0.60, 0.72, 1.0]);
                color[3] = 1.0;
                Port {
                    node: p.node,
                    name: p.name,
                    color,
                    side: p.side.unwrap_or(InSide::Right),
                    offset: p.offset.unwrap_or(0.5).clamp(0.0, 1.0),
                    direction: p.direction.unwrap_or(InDirection::Both),
                }
            })
            .collect();
        let groups = input
            .groups
            .into_iter()
            .map(|g| resolve_group(g, &nodes))
            .collect();
        let mut g = Self {
            nodes,
            edges,
            ports,
            layers,
            traces,
            groups,
        };
        g.with_bows();
        g
    }

    /// Parallel edges (same endpoint pair, either direction) bow away from
    /// the straight line so they do not overdraw each other: the middle one
    /// stays straight, the others curve by their rank (spec 031 §2). A
    /// self-loop is its own group — its bow widens the loop. Idempotent:
    /// the bow is an own field set (not added) per rank.
    pub(crate) fn with_bows(&mut self) {
        use std::collections::HashMap;
        let mut groups: HashMap<(usize, usize), Vec<usize>> = HashMap::new();
        for (i, e) in self.edges.iter().enumerate() {
            let key = if e.a <= e.b { (e.a, e.b) } else { (e.b, e.a) };
            groups.entry(key).or_default().push(i);
        }
        for idx in groups.values() {
            let n = idx.len();
            if n < 2 {
                continue;
            }
            for (rank, &i) in idx.iter().enumerate() {
                // rank 0 of n → −(n−1)/2 … rank n−1 → +(n−1)/2, in half-steps
                // of BOW_SPACING so neighbours keep apart (stage 4's LOD
                // bundling takes over below the pixel threshold). The exact
                // middle keeps its straight line — a zero-offset Bézier
                // would tessellate into colinear segments for nothing.
                let bow = (rank as f32 - (n as f32 - 1.0) / 2.0) * BOW_SPACING;
                // The bow is measured on the perpendicular of a→b; an edge
                // stored the other way round (b→a, the two-way-link case)
                // has the opposite perpendicular, so its bow flips sign —
                // otherwise both parallels land on the same curve.
                let sign = if self.edges[i].a > self.edges[i].b {
                    -1.0
                } else {
                    1.0
                };
                self.edges[i].bow = bow * sign;
            }
        }
    }

    /// Apply an incremental change (spec 031 §4, stage 4a): remove the
    /// named nodes (their edges and port anchors go with them), add the new
    /// ones, resolve the new edges' endpoints by node id, recompute degrees
    /// and re-assign the bows of every parallel group a touched pair
    /// belongs to. Positions of survivors and their pins are untouched —
    /// the host's view (camera, selection by id) is stable across a delta.
    pub fn apply_delta(&mut self, d: InDelta) {
        if d.remove_edges.is_empty()
            && d.remove_nodes.is_empty()
            && d.add_nodes.is_empty()
            && d.add_edges.is_empty()
            && d.add_groups.is_empty()
            && d.remove_groups.is_empty()
        {
            return;
        }
        let remove_edges: std::collections::HashSet<&String> = d.remove_edges.iter().collect();
        let remove_nodes: std::collections::HashSet<&String> = d.remove_nodes.iter().collect();
        // Removal first: indices shift, so the survivors get a remap.
        let mut remap: Vec<Option<usize>> = Vec::with_capacity(self.nodes.len());
        let mut nodes = Vec::with_capacity(self.nodes.len());
        for n in self.nodes.iter() {
            if remove_nodes.contains(&n.id) {
                remap.push(None);
            } else {
                remap.push(Some(nodes.len()));
                nodes.push(n.clone());
            }
        }
        self.nodes = nodes;
        self.edges.retain_mut(|e| {
            if remove_edges.contains(&e.id) {
                return false;
            }
            let (Some(a), Some(b)) = (remap[e.a], remap[e.b]) else {
                return false; // its node went with the delta
            };
            e.a = a;
            e.b = b;
            true
        });
        // A port whose node went with the delta dangles; drop it.
        self.ports
            .retain_mut(|p| match remap.get(p.node).copied().flatten() {
                Some(i) => {
                    p.node = i;
                    true
                }
                None => false,
            });
        // A group's members are indices; the survivors follow the remap,
        // and a removed member simply leaves its group.
        for g in self.groups.iter_mut() {
            g.members = g.members.iter().filter_map(|&i| remap[i]).collect();
        }
        // Additions. A node id that already exists replaces its node —
        // a re-expand puts the members back where the aggregate was.
        fn index_of(nodes: &[Node], id: &str) -> Option<usize> {
            nodes.iter().position(|n| n.id == id)
        }
        for n in d.add_nodes {
            let color = n
                .color
                .as_deref()
                .and_then(|c| parse_hex(c, 1.0))
                .unwrap_or_else(|| color_for(&n.kind));
            let (x, y) = match n.pos {
                Some([x, y]) => (x, y),
                None => {
                    let t = self.nodes.len() as f32 * 2.399_963;
                    let r = 12.0 * (self.nodes.len() as f32 + 1.0).sqrt();
                    (r * t.cos(), r * t.sin())
                }
            };
            let z = layer_z(&n.kind, self.nodes.len());
            if let Some(i) = index_of(&self.nodes, &n.id) {
                let old = &mut self.nodes[i];
                old.label = n.label;
                old.kind = n.kind;
                old.key = n.key;
                old.color = color;
                old.layers = n.layers;
                old.x = x;
                old.y = y;
                old.z = z;
                old.pinned = n.pinned;
            } else {
                self.nodes.push(Node {
                    id: n.id,
                    label: n.label,
                    kind: n.kind,
                    key: n.key,
                    x,
                    y,
                    z,
                    radius: 4.0,
                    color,
                    degree: 0,
                    pinned: n.pinned,
                    layers: n.layers,
                });
            }
        }
        for e in d.add_edges {
            let (Some(a), Some(b)) = (index_of(&self.nodes, &e.a), index_of(&self.nodes, &e.b))
            else {
                continue; // a missing endpoint skips; re-addable later
            };
            if a == b && e.a != e.b {
                continue; // two ids, one node — nothing to draw
            }
            let arrow = match (e.arrow, e.directed) {
                (Some(InArrow::Source), _) => Arrow::Source,
                (Some(InArrow::Target), _) => Arrow::Target,
                (Some(InArrow::Both), _) => Arrow::Both,
                (None, true) => Arrow::Target,
                (None, false) => Arrow::None,
            };
            let route = match e.route {
                Some(InRoute::Bezier { offset }) => Route::Bezier { offset },
                _ => Route::Straight,
            };
            let mut color = e
                .color
                .as_deref()
                .and_then(|c| parse_hex(c, 0.55))
                .unwrap_or_else(|| edge_color(&e.kind));
            if let Some(o) = e.opacity {
                color[3] *= o.clamp(0.0, 1.0);
            }
            let edge = Edge {
                id: e.id,
                a,
                b,
                color,
                width: e.width.unwrap_or(1.2).clamp(0.5, 12.0),
                dash: e.dash.unwrap_or(0.0).clamp(0.0, 64.0),
                arrow,
                label: e.label.unwrap_or_default(),
                route,
                a_port: e.a_port,
                b_port: e.b_port,
                layers: e.layers,
                bow: 0.0,
            };
            match self.edges.iter().position(|x| x.id == edge.id) {
                Some(i) => self.edges[i] = edge, // re-add: replace
                None => self.edges.push(edge),
            }
        }
        let remove_groups: std::collections::HashSet<&String> = d.remove_groups.iter().collect();
        self.groups.retain(|g| !remove_groups.contains(&g.id));
        for g in d.add_groups {
            let group = resolve_group(g, &self.nodes);
            match self.groups.iter().position(|x| x.id == group.id) {
                Some(i) => self.groups[i] = group, // re-add: replace
                None => self.groups.push(group),
            }
        }
        self.recompute_degrees();
        self.with_bows();
    }

    /// Degree drives the radius; a delta changes it for the touched
    /// neighbourhood, so recompute for all — O(n + e), cheap next to the
    /// upload it prevents a stale radius from forcing.
    fn recompute_degrees(&mut self) {
        let mut degree = vec![0u32; self.nodes.len()];
        for e in &self.edges {
            if e.a < degree.len() && e.b < degree.len() {
                degree[e.a] += 1;
                degree[e.b] += 1;
            }
        }
        for (n, deg) in self.nodes.iter_mut().zip(degree) {
            n.degree = deg;
            n.radius = 4.0 + (deg as f32).sqrt().min(6.0);
        }
    }

    /// Axis-aligned bounds of all nodes (world units).
    pub fn bounds(&self) -> Option<(f32, f32, f32, f32)> {
        Self::bounds_of(self.nodes.iter())
    }

    /// Bounds for fitting: isolated nodes (degree 0) sit far out on the
    /// gravity ring and would shrink everything else, so when the connected
    /// part is the majority the fit is taken over it alone.
    pub fn fit_bounds(&self) -> Option<(f32, f32, f32, f32)> {
        let connected = self.nodes.iter().filter(|n| n.degree > 0).count();
        if connected * 2 >= self.nodes.len() && connected > 0 {
            Self::bounds_of(self.nodes.iter().filter(|n| n.degree > 0))
        } else {
            self.bounds()
        }
    }

    fn bounds_of<'a>(mut it: impl Iterator<Item = &'a Node>) -> Option<(f32, f32, f32, f32)> {
        let first = it.next()?;
        let mut b = (first.x, first.y, first.x, first.y);
        for n in it {
            b.0 = b.0.min(n.x);
            b.1 = b.1.min(n.y);
            b.2 = b.2.max(n.x);
            b.3 = b.3.max(n.y);
        }
        Some(b)
    }
}

/// Depth by kind: one plane per kind, 140 world units apart, with a small
/// deterministic jitter so coplanar nodes still read as separate.
pub fn layer_z(kind: &str, i: usize) -> f32 {
    let layer: f32 = match kind {
        "directory" => -2.0,
        "file" => -1.0,
        "page" => 0.0,
        "symbol" => 1.0,
        "block" => 1.5,
        "database" => -1.5,
        "table" => -0.5,
        "column" | "row" | "vertex" | "key" => 0.5,
        "commit" => 2.0,
        _ => 0.25,
    };
    let jitter = ((i as f32 * 0.618_034).fract() - 0.5) * 20.0;
    layer * 140.0 + jitter
}

#[cfg(test)]
mod tests {
    use super::parse_hex;

    #[test]
    fn hex_colours_parse_and_multibyte_strings_are_refused() {
        assert_eq!(parse_hex("#ff0000", 1.0), Some([1.0, 0.0, 0.0, 1.0]));
        // Six bytes, but `é` is two of them: used to slice inside it and panic (#12).
        assert_eq!(parse_hex("#éabcd", 1.0), None);
        assert_eq!(parse_hex("#gg0000", 1.0), None);
        assert_eq!(parse_hex("ff0000", 1.0), None);
    }
}

#[cfg(test)]
mod delta_tests {
    use super::*;

    fn input() -> InGraph {
        InGraph {
            nodes: vec![
                InNode {
                    id: "a".into(),
                    label: "a".into(),
                    kind: "file".into(),
                    ..Default::default()
                },
                InNode {
                    id: "b".into(),
                    label: "b".into(),
                    kind: "file".into(),
                    ..Default::default()
                },
                InNode {
                    id: "c".into(),
                    label: "c".into(),
                    kind: "file".into(),
                    ..Default::default()
                },
            ],
            edges: vec![
                InEdge {
                    a: 0,
                    b: 1,
                    ..Default::default()
                },
                InEdge {
                    a: 1,
                    b: 2,
                    ..Default::default()
                },
            ],
            ..Default::default()
        }
    }

    fn delta_node(id: &str, pos: [f32; 2]) -> InDeltaNode {
        InDeltaNode {
            id: id.into(),
            label: id.into(),
            kind: "directory".into(),
            key: String::new(),
            color: None,
            layers: Vec::new(),
            pos: Some(pos),
            pinned: false,
        }
    }

    fn delta_edge(id: &str, a: &str, b: &str) -> InDeltaEdge {
        InDeltaEdge {
            id: id.into(),
            a: a.into(),
            b: b.into(),
            ..Default::default()
        }
    }

    #[test]
    fn a_delta_removes_and_adds_by_id() {
        let mut g = Graph::from_input(input());
        // Collapse "b": its two edges go with it; an aggregate takes its
        // place with proxy edges to the outside.
        g.apply_delta(InDelta {
            remove_nodes: vec!["b".into()],
            remove_edges: vec![],
            add_nodes: vec![delta_node("agg", [10.0, 0.0])],
            add_edges: vec![delta_edge("p1", "a", "agg"), delta_edge("p2", "agg", "c")],
            ..Default::default()
        });
        assert_eq!(g.nodes.len(), 3, "b out, agg in");
        assert!(g.nodes.iter().all(|n| n.id != "b"));
        let agg = g.nodes.iter().position(|n| n.id == "agg").unwrap();
        assert_eq!(g.nodes[agg].x, 10.0, "the aggregate sits where placed");
        assert_eq!(g.edges.len(), 2, "b's edges went, two proxies arrived");
        assert!(g.edges.iter().all(|e| e.id != "0:1:0"));
        let ids: Vec<&str> = g.edges.iter().map(|e| e.id.as_str()).collect();
        assert!(ids.contains(&"p1") && ids.contains(&"p2"));
        // The survivors kept their positions (the layout warm happens in
        // the host command, not in the graph).
        let a = g.nodes.iter().position(|n| n.id == "a").unwrap();
        assert!(!g.nodes[a].pinned);
    }

    #[test]
    fn a_delta_edge_with_a_missing_endpoint_waits() {
        let mut g = Graph::from_input(input());
        g.apply_delta(InDelta {
            add_edges: vec![delta_edge("early", "a", "ghost")],
            ..Default::default()
        });
        assert!(
            g.edges.iter().all(|e| e.id != "early"),
            "an edge to a missing node skips"
        );
        // The node arrives later; re-adding the edge resolves it.
        g.apply_delta(InDelta {
            add_nodes: vec![delta_node("ghost", [0.0, 40.0])],
            add_edges: vec![delta_edge("early", "a", "ghost")],
            ..Default::default()
        });
        assert!(g.edges.iter().any(|e| e.id == "early"));
    }

    #[test]
    fn re_adding_an_id_replaces_its_instance() {
        let mut g = Graph::from_input(input());
        g.apply_delta(InDelta {
            add_nodes: vec![InDeltaNode {
                label: "renamed".into(),
                pos: Some([99.0, 99.0]),
                ..delta_node("a", [0.0, 0.0])
            }],
            ..Default::default()
        });
        assert_eq!(g.nodes.len(), 3, "no duplicate was created");
        let a = g.nodes.iter().position(|n| n.id == "a").unwrap();
        assert_eq!(g.nodes[a].label, "renamed");
        assert_eq!(g.nodes[a].x, 99.0);
        // The edge 0↔1 survived the replacement (indices remapped).
        assert_eq!(g.edges.len(), 2);
    }

    #[test]
    fn bows_are_assigned_idempotently() {
        let mut g = Graph::from_input(InGraph {
            edges: vec![
                InEdge {
                    a: 0,
                    b: 1,
                    ..Default::default()
                },
                InEdge {
                    a: 1,
                    b: 0,
                    ..Default::default()
                },
            ],
            ..input()
        });
        g.with_bows();
        let first: Vec<f32> = g.edges.iter().map(|e| e.bow).collect();
        g.with_bows();
        let second: Vec<f32> = g.edges.iter().map(|e| e.bow).collect();
        assert_eq!(first, second, "the assignment must not accumulate");
        // The two-way pair carries the same bow with the b→a sign flip —
        // equal bows on opposite perpendiculars land on opposite sides
        // (the frame test checks the rendered curve).
        assert!(first[0] != 0.0 && first[0] == first[1], "bows: {first:?}");
    }

    #[test]
    fn a_delta_moves_groups_with_their_members() {
        // Spec 031 §4: a collapse removes members (the group's hull shrinks
        // with them), an expand re-sends the group with the members back;
        // both directions are idempotent by group id.
        let mut g = Graph::from_input(input());
        g.apply_delta(InDelta {
            add_groups: vec![InGroup {
                id: "mod".into(),
                label: "module".into(),
                members: vec!["a".into(), "b".into()],
                color: Some("#596cff".into()),
                visible: true,
            }],
            ..Default::default()
        });
        assert_eq!(g.groups.len(), 1);
        assert_eq!(g.groups[0].members.len(), 2);
        // Collapse b: the re-sent group swaps it for the aggregate.
        g.apply_delta(InDelta {
            remove_nodes: vec!["b".into()],
            add_nodes: vec![delta_node("agg", [10.0, 0.0])],
            add_groups: vec![InGroup {
                id: "mod".into(),
                label: "module".into(),
                members: vec!["a".into(), "agg".into()],
                color: Some("#596cff".into()),
                visible: true,
            }],
            ..Default::default()
        });
        let ids: Vec<&str> = g.groups[0]
            .members
            .iter()
            .map(|&i| g.nodes[i].id.as_str())
            .collect();
        assert_eq!(ids, vec!["a", "agg"], "the group followed the delta");
        // A removal without a re-send drops the member that left; the
        // group stays for the host to re-send.
        g.apply_delta(InDelta {
            remove_nodes: vec!["agg".into()],
            ..Default::default()
        });
        assert_eq!(g.groups[0].members.len(), 1);
        // remove_groups by id.
        g.apply_delta(InDelta {
            remove_groups: vec!["mod".into()],
            ..Default::default()
        });
        assert!(g.groups.is_empty());
    }
}
