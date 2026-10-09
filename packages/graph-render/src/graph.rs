//! The renderer's own graph: what the host sends, plus positions.

use serde::{Deserialize, Serialize};

/// Parallel edges bow apart in half-steps of this many world units
/// (spec 031 §2; the ideal edge length is ~28, so a pair sits visibly apart).
pub const BOW_SPACING: f32 = 9.0;

#[derive(Clone, Debug, Serialize, Deserialize)]
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
}

#[derive(Clone, Default)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub ports: Vec<Port>,
    pub layers: Vec<Layer>,
    pub traces: Vec<Trace>,
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
        Self {
            nodes,
            edges,
            ports,
            layers,
            traces,
        }
        .with_bows()
    }

    /// Parallel edges (same endpoint pair, either direction) bow away from
    /// the straight line so they do not overdraw each other: the middle one
    /// stays straight, the others curve by their rank (spec 031 §2). A
    /// self-loop is its own group — its bow widens the loop.
    pub(crate) fn with_bows(mut self) -> Self {
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
                if bow == 0.0 {
                    continue;
                }
                let e = &mut self.edges[i];
                // The bow is measured on the perpendicular of a→b; an edge
                // stored the other way round (b→a, the two-way-link case)
                // has the opposite perpendicular, so its bow flips sign —
                // otherwise both parallels land on the same curve.
                let sign = if e.a > e.b { -1.0 } else { 1.0 };
                e.route = match e.route {
                    Route::Straight => Route::Bezier { offset: bow * sign },
                    Route::Bezier { offset } => Route::Bezier {
                        offset: offset + bow * sign,
                    },
                };
            }
        }
        self
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
