//! The renderer's own graph: what the host sends, plus positions.

use serde::{Deserialize, Serialize};

/// Parallel edges bow apart in half-steps of this many world units
/// (spec 031 §2; the ideal edge length is ~28, so a pair sits visibly apart).
pub const BOW_SPACING: f32 = 9.0;

/// A node's silhouette (spec 031 §2): a circle everywhere, a rectangle for
/// a flow block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Circle,
    Rect,
    /// A decision/branch block (spec 031 §2): same half-extents as a
    /// rect, its lit area the inscribed rhombus — the bounding box's
    /// corners stay dark, which a pixel check can tell apart.
    Diamond,
}

/// A rect block's default half-height (world units); the host sizes it up
/// for many ports.
pub const RECT_HEIGHT: f32 = 22.0;
/// A rect block's default half-width (world units).
pub const RECT_WIDTH: f32 = 46.0;
/// The layered layout's column gap (world units) — a block's width plus
/// its wires' stubs (spec 031 §5, stage 7).
pub const HIER_COL: f32 = 160.0;
/// The layered layout's row gap (world units).
pub const HIER_ROW: f32 = 70.0;

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
    /// `"circle"` (default) or `"rect"` — a flow block (spec 031 §2).
    #[serde(default)]
    pub shape: Option<String>,
    /// Where the host wants it (world units) — a flow block's position is
    /// the document's (stage 7); absent = the spiral, as always.
    #[serde(default)]
    pub pos: Option<[f32; 2]>,
    /// Start pinned (a placed block does not drift).
    #[serde(default)]
    pub pinned: bool,
    /// A rect's half-width in world units, or a circle's radius (absent:
    /// the degree-derived radius). A flow panel sizes its circles — a
    /// factor graph's variable (ADR-0022) — so they stay a click target
    /// beside the blocks.
    #[serde(default)]
    pub width: Option<f32>,
    /// A rect's half-height in world units; absent = `RECT_HEIGHT`.
    #[serde(default)]
    pub height: Option<f32>,
    /// A rim colour `#rrggbb` (spec 031 §2, stage 8) — a ~2 px band along
    /// the shape's edge; absent = no stroke.
    #[serde(default)]
    pub stroke: Option<String>,
    /// A short glyph (spec 031 §2, stage 8) drawn on the label overlay,
    /// centered on the node — only while hovered or selected (§6 keeps
    /// per-node ink for the few).
    #[serde(default)]
    pub icon: Option<String>,
    /// A schematic symbol (ADR-0024): SVG path data in the node's box
    /// (`0 0 2·width 2·height`, world units), stroked on the label
    /// overlay in the node's colour. The body then draws as a faint
    /// tint — the symbol is the block — and the label sits below.
    #[serde(default)]
    pub symbol: Option<String>,
    /// The symbol's quarter turns clockwise (0–3) and its left–right
    /// mirror, applied in its own frame before the turn (ADR-0024 §5).
    /// The host sends the turned box (`width`/`height` swap on an odd
    /// turn) and the turned ports; only the drawing needs the transform.
    #[serde(default)]
    pub turn: u8,
    #[serde(default)]
    pub mirror: bool,
    /// A second line under the label (ADR-0024 §6): a part's value
    /// (`1.0 Ω`), smaller and dimmer.
    #[serde(default)]
    pub caption: Option<String>,
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
    /// A moving pulse along the stroke (spec 031 §2, stage 6): world
    /// units per second, presentation only — the phase is the clock's,
    /// but a host may drive one through deltas (speed 0 = off).
    #[serde(default)]
    pub pulse_speed: Option<f32>,
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
    /// Steps between the endpoints (flow diagrams, spec 031 SS2): out of
    /// the source's side, across, into the target's.
    Ortho {
        /// The first and last leg's length in world units; absent = half
        /// the gap.
        #[serde(default)]
        stub: Option<f32>,
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
    Bezier {
        offset: f32,
    },
    /// Left/right-then-over steps between the endpoints (flow diagrams,
    /// spec 031 SS2): the port sides decide the stubs, `stub` overrides.
    Ortho {
        stub: Option<f32>,
    },
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
#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    pub id: String,
    pub label: String,
    pub members: Vec<usize>,
    pub color: [f32; 4],
    pub visible: bool,
    pub highlight: bool,
}

/// The internal hyperedge (spec 031 §2): members as node indices; the
/// frame derives the junction and its spokes every build.
#[derive(Clone, Debug, PartialEq)]
pub struct Hyperedge {
    pub id: String,
    pub nodes: Vec<usize>,
    pub color: [f32; 4],
    pub width: f32,
    pub dash: f32,
    pub visible: bool,
}

/// The internal simplex (spec 031 §2): members as node indices; the frame
/// fans them into filled triangles.
#[derive(Clone, Debug, PartialEq)]
pub struct Simplex {
    pub id: String,
    pub nodes: Vec<usize>,
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
    /// Hyperedges (spec 031 §2, stage 5): connections over three or more
    /// nodes, drawn as a junction — a marker at the members' centroid
    /// with a spoke to each. A distinct class from groups and simplices.
    #[serde(default)]
    pub hyperedges: Vec<InHyperedge>,
    /// Simplicial faces (spec 031 §2, stage 5): filled triangles (and
    /// higher faces, fanned from their centroid) behind everything that
    /// draws over them.
    #[serde(default)]
    pub simplexes: Vec<InSimplex>,
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
    /// A highlighted clique/community (spec 031 §2, §4): still a visual
    /// membership over the same nodes — the hull wears the highlight
    /// style (a stronger tint and rim) instead of the group's wash.
    #[serde(default)]
    pub highlight: bool,
}

/// A hyperedge (spec 031 §2): one connection over three or more nodes,
/// drawn as a junction — members by id, a marker at their centroid, a
/// spoke to each. Not a group (a region of members) and not a simplex
/// (a filled face): three distinct classes.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct InHyperedge {
    pub id: String,
    #[serde(default)]
    pub nodes: Vec<String>,
    /// Spoke and marker colour `#rrggbb`.
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub width: Option<f32>,
    #[serde(default)]
    pub dash: Option<f32>,
    #[serde(default)]
    pub opacity: Option<f32>,
    /// Drawn while true (default).
    #[serde(default = "default_true")]
    pub visible: bool,
}

/// A simplicial face (spec 031 §2): a filled triangle over three nodes
/// (more than three: fanned from the centroid into triangles), by id.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct InSimplex {
    pub id: String,
    #[serde(default)]
    pub nodes: Vec<String>,
    /// Fill colour `#rrggbb`.
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub opacity: Option<f32>,
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
    /// Hyperedges and simplices by id, same replace-on-re-add contract as
    /// groups (spec 031 §2, stage 5); their members follow the remap.
    #[serde(default)]
    pub add_hyperedges: Vec<InHyperedge>,
    #[serde(default)]
    pub remove_hyperedges: Vec<String>,
    #[serde(default)]
    pub add_simplexes: Vec<InSimplex>,
    #[serde(default)]
    pub remove_simplexes: Vec<String>,
}

/// A node arriving through a delta (spec 031 §4: a collapsed group's
/// aggregate sits where the group was).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
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
    /// `"rect"` for a flow block; absent keeps the old silhouette on a
    /// replace (spec 031 §2, stage 7).
    #[serde(default)]
    pub shape: Option<String>,
    #[serde(default)]
    pub width: Option<f32>,
    #[serde(default)]
    pub height: Option<f32>,
    /// A rim colour; absent keeps the old one on a replace.
    #[serde(default)]
    pub stroke: Option<String>,
    /// An overlay glyph; absent keeps the old one on a replace.
    #[serde(default)]
    pub icon: Option<String>,
    /// A schematic symbol (ADR-0024); absent keeps the old one.
    #[serde(default)]
    pub symbol: Option<String>,
    /// The symbol's turn and mirror; absent keeps the old ones.
    #[serde(default)]
    pub turn: Option<u8>,
    #[serde(default)]
    pub mirror: Option<bool>,
    /// The caption line; absent keeps the old one.
    #[serde(default)]
    pub caption: Option<String>,
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
    #[serde(default)]
    pub pulse_speed: Option<f32>,
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

#[derive(Clone, Debug, PartialEq)]
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
    /// A rect's half-height; a circle ignores it (spec 031 §2, stage 7).
    pub height: f32,
    /// The silhouette (spec 031 §2): circles everywhere, rects for flow
    /// blocks, diamonds for decision blocks — a rect's `radius` is its
    /// half-width, and the degree recompute leaves it alone.
    pub shape: Shape,
    /// The host set this node's radius (`InNode.width`): the degree
    /// recompute leaves it alone, a circle included (ADR-0022).
    pub sized: bool,
    /// A rim colour (spec 031 §2, stage 8): alpha 0 = none.
    pub stroke: [f32; 4],
    /// An overlay glyph (spec 031 §2, stage 8): drawn only while hovered
    /// or selected.
    pub icon: Option<String>,
    /// A schematic symbol (ADR-0024): SVG path data in the node's box.
    pub symbol: Option<String>,
    /// The symbol's quarter turns clockwise and its mirror (ADR-0024 §5).
    pub turn: u8,
    pub mirror: bool,
    /// The caption under the label (ADR-0024 §6).
    pub caption: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
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
    /// `Graph::with_bows` on top of the host's route — an own field, so
    /// re-running the assignment never accumulates.
    pub bow: f32,
    /// A moving pulse along the stroke (spec 031 §2, stage 6): world
    /// units per second, 0 = none. Presentation only.
    pub pulse_speed: f32,
}

#[derive(Clone, Default)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub ports: Vec<Port>,
    pub layers: Vec<Layer>,
    pub traces: Vec<Trace>,
    pub groups: Vec<Group>,
    pub hyperedges: Vec<Hyperedge>,
    pub simplexes: Vec<Simplex>,
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
fn resolve_group(g: InGroup, ids: &std::collections::HashMap<String, usize>) -> Group {
    let members = distinct_members(&g.members, ids);
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
        highlight: g.highlight,
    }
}

/// A wire hyperedge with its members resolved to indices (missing ids
/// skip, like a group's; a repeated id counts once — two ids, one node
/// is not a connection). Purple is the neutral spoke colour.
fn resolve_hyperedge(h: InHyperedge, ids: &std::collections::HashMap<String, usize>) -> Hyperedge {
    let mut color = h
        .color
        .as_deref()
        .and_then(|c| parse_hex(c, 0.85))
        .unwrap_or([0.75, 0.55, 0.95, 0.85]);
    if let Some(o) = h.opacity {
        color[3] *= o.clamp(0.0, 1.0);
    }
    Hyperedge {
        id: h.id,
        nodes: distinct_members(&h.nodes, ids),
        color,
        width: h.width.unwrap_or(1.4).clamp(0.5, 12.0),
        dash: h.dash.unwrap_or(0.0).clamp(0.0, 64.0),
        visible: h.visible,
    }
}

/// A wire simplex with its members resolved to indices (missing ids and
/// repeats skip — three corners, three *distinct* nodes, or no face).
/// Orange is the neutral fill.
fn resolve_simplex(s: InSimplex, ids: &std::collections::HashMap<String, usize>) -> Simplex {
    let mut color = s
        .color
        .as_deref()
        .and_then(|c| parse_hex(c, 0.9))
        .unwrap_or([0.95, 0.60, 0.30, 0.9]);
    if let Some(o) = s.opacity {
        color[3] *= o.clamp(0.0, 1.0);
    }
    Simplex {
        id: s.id,
        nodes: distinct_members(&s.nodes, ids),
        color,
        visible: s.visible,
    }
}

/// Wire ids → first-occurrence node indices: a missing id skips (re-
/// addable through a later delta), a repeated id counts once — the same
/// rule an edge's endpoints follow.
fn distinct_members(ids: &[String], map: &std::collections::HashMap<String, usize>) -> Vec<usize> {
    let mut out: Vec<usize> = Vec::new();
    for id in ids {
        if let Some(&i) = map.get(id) {
            if !out.contains(&i) {
                out.push(i);
            }
        }
    }
    out
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
                let shape = match n.shape.as_deref() {
                    Some("rect") => Shape::Rect,
                    Some("diamond") => Shape::Diamond,
                    _ => Shape::Circle,
                };
                let (x, y) = match n.pos {
                    Some([x, y]) => (x, y),
                    None => (r * t.cos(), r * t.sin()),
                };
                Node {
                    radius: match (shape, n.width) {
                        (Shape::Rect | Shape::Diamond, w) => w.unwrap_or(RECT_WIDTH),
                        (Shape::Circle, Some(w)) => w,
                        (Shape::Circle, None) => 4.0 + (deg as f32).sqrt().min(6.0),
                    },
                    sized: n.width.is_some(),
                    height: n.height.unwrap_or(RECT_HEIGHT),
                    shape,
                    color: n
                        .color
                        .as_deref()
                        .and_then(|c| parse_hex(c, 1.0))
                        .unwrap_or_else(|| color_for(&n.kind)),
                    stroke: n
                        .stroke
                        .as_deref()
                        .and_then(|c| parse_hex(c, 1.0))
                        .unwrap_or([0.0, 0.0, 0.0, 0.0]),
                    icon: n.icon,
                    symbol: n.symbol,
                    turn: n.turn % 4,
                    mirror: n.mirror,
                    caption: n.caption,
                    id: n.id,
                    label: n.label,
                    kind: n.kind,
                    key: n.key,
                    x,
                    y,
                    z: layer_z(&n_kind, i),
                    degree: deg,
                    pinned: n.pinned,
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
                    Some(InRoute::Ortho { stub }) => Route::Ortho { stub },
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
                    pulse_speed: e.pulse_speed.unwrap_or(0.0).clamp(0.0, 2000.0),
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
        // One id → index map for the groups, hyperedges and simplices
        // (the P-166 review's #8: a linear scan per member is O(m·n)).
        let ids: std::collections::HashMap<String, usize> = nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.clone(), i))
            .collect();
        let groups = input
            .groups
            .into_iter()
            .map(|g| resolve_group(g, &ids))
            .collect();
        let hyperedges = input
            .hyperedges
            .into_iter()
            .map(|h| resolve_hyperedge(h, &ids))
            .collect();
        let simplexes = input
            .simplexes
            .into_iter()
            .map(|s| resolve_simplex(s, &ids))
            .collect();
        let mut g = Self {
            nodes,
            edges,
            ports,
            layers,
            traces,
            groups,
            hyperedges,
            simplexes,
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
                // A lone edge carries no parallel offset — its partner
                // may have gone with a delta (the P-166 review's #3).
                if let Some(&i) = idx.first() {
                    self.edges[i].bow = 0.0;
                }
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
    /// Apply an incremental change (spec 031 §4, stage 4a). Returns
    /// whether anything actually changed — a delta naming nothing that
    /// exists (or re-sending identical instances) must not redraw or warm
    /// the layout; a count-preserving collapse must (the P-166 review's
    /// #1: five equal counts used to skip the redraw entirely).
    pub fn apply_delta(&mut self, d: InDelta) -> bool {
        if d.remove_edges.is_empty()
            && d.remove_nodes.is_empty()
            && d.add_nodes.is_empty()
            && d.add_edges.is_empty()
            && d.add_groups.is_empty()
            && d.remove_groups.is_empty()
            && d.add_hyperedges.is_empty()
            && d.remove_hyperedges.is_empty()
            && d.add_simplexes.is_empty()
            && d.remove_simplexes.is_empty()
        {
            return false;
        }
        let mut changed = false;
        let remove_edges: std::collections::HashSet<&String> = d.remove_edges.iter().collect();
        let remove_nodes: std::collections::HashSet<&String> = d.remove_nodes.iter().collect();
        // Removal first: indices shift, so the survivors get a remap.
        let mut remap: Vec<Option<usize>> = Vec::with_capacity(self.nodes.len());
        let mut nodes = Vec::with_capacity(self.nodes.len());
        for n in self.nodes.iter() {
            if remove_nodes.contains(&n.id) {
                remap.push(None);
                changed = true;
            } else {
                remap.push(Some(nodes.len()));
                nodes.push(n.clone());
            }
        }
        self.nodes = nodes;
        let before = self.edges.len();
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
        changed |= self.edges.len() != before;
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
            let before = g.members.len();
            g.members = g.members.iter().filter_map(|&i| remap[i]).collect();
            changed |= g.members.len() != before;
        }
        // Hyperedges and simplices (spec 031 §2): the survivors follow the
        // remap too; a hyperedge under two members, a simplex under three,
        // has no shape to draw and is dropped (the host can re-send it).
        let before = self.hyperedges.len();
        for h in self.hyperedges.iter_mut() {
            h.nodes = h.nodes.iter().filter_map(|&i| remap[i]).collect();
        }
        self.hyperedges.retain(|h| h.nodes.len() >= 2);
        changed |= self.hyperedges.len() != before;
        let before = self.simplexes.len();
        for s in self.simplexes.iter_mut() {
            s.nodes = s.nodes.iter().filter_map(|&i| remap[i]).collect();
        }
        self.simplexes.retain(|s| s.nodes.len() >= 3);
        changed |= self.simplexes.len() != before;
        // One id → index map for the whole delta (the P-166 review's #8:
        // a linear scan per id is O(k·n) — a 2 000-edge delta at 100 k
        // nodes was ~4·10⁸ string comparisons). Owned, so it stays alive
        // across the mutations below; new ids join as they arrive.
        let mut ids: std::collections::HashMap<String, usize> = self
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.clone(), i))
            .collect();
        // Additions. A node id that already exists replaces its node; a
        // replace without `pos` keeps the old position — a relabel must
        // not teleport to a fresh spiral slot (the review's #7).
        for n in d.add_nodes {
            let color = n
                .color
                .as_deref()
                .and_then(|c| parse_hex(c, 1.0))
                .unwrap_or_else(|| color_for(&n.kind));
            if let Some(&i) = ids.get(&n.id) {
                let old = &self.nodes[i];
                let (x, y, z, pinned) = match n.pos {
                    Some([x, y]) => (x, y, layer_z(&n.kind, i), n.pinned),
                    None => (old.x, old.y, old.z, old.pinned),
                };
                let shape = match n.shape.as_deref() {
                    Some("rect") => Shape::Rect,
                    Some("circle") => Shape::Circle,
                    Some("diamond") => Shape::Diamond,
                    _ => old.shape, // absent keeps the silhouette
                };
                let new = Node {
                    id: n.id,
                    label: n.label,
                    kind: n.kind,
                    key: n.key,
                    x,
                    y,
                    z,
                    sized: n.width.is_some(),
                    radius: match (shape, n.width) {
                        (_, Some(w)) => w,
                        _ if shape != old.shape => {
                            if matches!(shape, Shape::Rect | Shape::Diamond) {
                                RECT_WIDTH
                            } else {
                                4.0 + (old.degree as f32).sqrt().min(6.0)
                            }
                        }
                        _ => old.radius,
                    },
                    height: n.height.unwrap_or(old.height),
                    shape,
                    color,
                    stroke: n
                        .stroke
                        .as_deref()
                        .and_then(|c| parse_hex(c, 1.0))
                        .unwrap_or(old.stroke),
                    icon: n.icon.or(old.icon.clone()),
                    symbol: n.symbol.or(old.symbol.clone()),
                    turn: n.turn.map(|t| t % 4).unwrap_or(old.turn),
                    mirror: n.mirror.unwrap_or(old.mirror),
                    caption: n.caption.or(old.caption.clone()),
                    degree: old.degree,
                    pinned,
                    layers: n.layers,
                };
                if self.nodes[i] != new {
                    self.nodes[i] = new;
                    changed = true;
                }
            } else {
                let t = self.nodes.len() as f32 * 2.399_963;
                let r = 12.0 * (self.nodes.len() as f32 + 1.0).sqrt();
                let (x, y) = match n.pos {
                    Some([x, y]) => (x, y),
                    None => (r * t.cos(), r * t.sin()),
                };
                let z = layer_z(&n.kind, self.nodes.len());
                ids.insert(n.id.clone(), self.nodes.len());
                let shape = match n.shape.as_deref() {
                    Some("rect") => Shape::Rect,
                    Some("diamond") => Shape::Diamond,
                    _ => Shape::Circle,
                };
                self.nodes.push(Node {
                    id: n.id,
                    label: n.label,
                    kind: n.kind,
                    key: n.key,
                    x,
                    y,
                    z,
                    radius: match (shape, n.width) {
                        (_, Some(w)) => w,
                        (Shape::Rect | Shape::Diamond, None) => RECT_WIDTH,
                        (Shape::Circle, None) => 4.0,
                    },
                    sized: n.width.is_some(),
                    height: n.height.unwrap_or(RECT_HEIGHT),
                    shape,
                    color,
                    stroke: n
                        .stroke
                        .as_deref()
                        .and_then(|c| parse_hex(c, 1.0))
                        .unwrap_or([0.0, 0.0, 0.0, 0.0]),
                    icon: n.icon,
                    symbol: n.symbol,
                    turn: n.turn.unwrap_or(0) % 4,
                    mirror: n.mirror.unwrap_or(false),
                    caption: n.caption,
                    degree: 0,
                    pinned: n.pinned,
                    layers: n.layers,
                });
                changed = true;
            }
        }
        // Edges: resolve the whole batch against the id map first, so the
        // edge-id map (for replace) borrows nothing while we mutate.
        if !d.add_edges.is_empty() {
            let resolved: Vec<Option<(Edge, Option<usize>)>> = {
                let edge_ids: std::collections::HashMap<&str, usize> = self
                    .edges
                    .iter()
                    .enumerate()
                    .map(|(i, e)| (e.id.as_str(), i))
                    .collect();
                d.add_edges
                    .into_iter()
                    .map(|e| {
                        let (Some(&a), Some(&b)) = (ids.get(&e.a), ids.get(&e.b)) else {
                            return None; // a missing endpoint skips; re-addable later
                        };
                        if a == b && e.a != e.b {
                            return None; // two ids, one node — nothing to draw
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
                            Some(InRoute::Ortho { stub }) => Route::Ortho { stub },
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
                        // Look the id up before the struct takes it.
                        let existing = edge_ids.get(e.id.as_str()).copied();
                        Some((
                            Edge {
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
                                pulse_speed: e.pulse_speed.unwrap_or(0.0).clamp(0.0, 2000.0),
                            },
                            // The existing index through the map — the
                            // replace lookup is O(1), not a scan per edge
                            // (the review's #8 again).
                            existing,
                        ))
                    })
                    .collect()
            };
            // Ids first added in this batch: a second add of the same new
            // id replaces the first, exactly as the nodes' `ids` map
            // does as they arrive — building the map once per batch let
            // both pushes through (the review's #9).
            let mut fresh: std::collections::HashMap<String, usize> =
                std::collections::HashMap::new();
            for (edge, existing) in resolved.into_iter().flatten() {
                match existing {
                    Some(i) => {
                        // The bow is re-assigned by `with_bows` below; it is
                        // not part of "did this edge change".
                        let mut cmp = edge.clone();
                        cmp.bow = self.edges[i].bow;
                        if self.edges[i] != cmp {
                            self.edges[i] = edge;
                            changed = true;
                        }
                    }
                    None => match fresh.get(&edge.id).copied() {
                        Some(i) => {
                            let mut cmp = edge.clone();
                            cmp.bow = self.edges[i].bow;
                            if self.edges[i] != cmp {
                                self.edges[i] = edge;
                                changed = true;
                            }
                        }
                        None => {
                            fresh.insert(edge.id.clone(), self.edges.len());
                            self.edges.push(edge);
                            changed = true;
                        }
                    },
                }
            }
        }
        let remove_groups: std::collections::HashSet<&String> = d.remove_groups.iter().collect();
        let before = self.groups.len();
        self.groups.retain(|g| !remove_groups.contains(&g.id));
        changed |= self.groups.len() != before;
        for g in d.add_groups {
            let group = resolve_group(g, &ids);
            match self.groups.iter().position(|x| x.id == group.id) {
                Some(i) => {
                    if self.groups[i] != group {
                        self.groups[i] = group;
                        changed = true;
                    }
                }
                None => {
                    self.groups.push(group);
                    changed = true;
                }
            }
        }
        let remove_hyper: std::collections::HashSet<&String> = d.remove_hyperedges.iter().collect();
        let before = self.hyperedges.len();
        self.hyperedges.retain(|h| !remove_hyper.contains(&h.id));
        changed |= self.hyperedges.len() != before;
        for h in d.add_hyperedges {
            let hyper = resolve_hyperedge(h, &ids);
            match self.hyperedges.iter().position(|x| x.id == hyper.id) {
                Some(i) => {
                    if self.hyperedges[i] != hyper {
                        self.hyperedges[i] = hyper;
                        changed = true;
                    }
                }
                None => {
                    self.hyperedges.push(hyper);
                    changed = true;
                }
            }
        }
        let remove_simp: std::collections::HashSet<&String> = d.remove_simplexes.iter().collect();
        let before = self.simplexes.len();
        self.simplexes.retain(|s| !remove_simp.contains(&s.id));
        changed |= self.simplexes.len() != before;
        for s in d.add_simplexes {
            let simp = resolve_simplex(s, &ids);
            match self.simplexes.iter().position(|x| x.id == simp.id) {
                Some(i) => {
                    if self.simplexes[i] != simp {
                        self.simplexes[i] = simp;
                        changed = true;
                    }
                }
                None => {
                    self.simplexes.push(simp);
                    changed = true;
                }
            }
        }
        // Canonical shape: a hyperedge under two members, a simplex under
        // three, has nothing to draw — kept out of the state entirely.
        self.hyperedges.retain(|h| h.nodes.len() >= 2);
        self.simplexes.retain(|s| s.nodes.len() >= 3);
        self.recompute_degrees();
        self.with_bows();
        changed
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
            if n.shape == Shape::Circle && !n.sized {
                n.radius = 4.0 + (deg as f32).sqrt().min(6.0);
            } // a rect's width, and a sized circle's, is the host's (stage 7)
        }
    }

    /// Axis-aligned bounds of all nodes (world units).
    pub fn bounds(&self) -> Option<(f32, f32, f32, f32)> {
        Self::bounds_of(self.nodes.iter())
    }

    /// The layered arrangement (spec 031 §5, stage 7): every wire goes
    /// from an earlier column to a later one — longest-path ranks, then
    /// columns left-to-right (`HIER_COL` apart) with each rank's rows
    /// stacked (`HIER_ROW` apart), keeping the nodes' vertical order so
    /// an arrangement survives a re-run. Every node lands **pinned**:
    /// a flow's positions are the host's document, not the physics'.
    /// Cycles cap at one pass per node — `validate` reports them; this
    /// still spreads the members apart instead of tangling them.
    pub fn hierarchize(&mut self) -> bool {
        // Longest-path ranks: rank(b) ≥ rank(a) + 1 per wire; enough
        // passes to settle any acyclic graph.
        let mut rank = vec![0u32; self.nodes.len()];
        for _ in 0..self.nodes.len() {
            let mut moved = false;
            for e in &self.edges {
                if e.a < rank.len() && e.b < rank.len() && rank[e.b] < rank[e.a] + 1 {
                    rank[e.b] = rank[e.a] + 1;
                    moved = true;
                }
            }
            if !moved {
                break;
            }
        }
        // Columns stack by rank; rows by the nodes' current order.
        let mut by_rank: std::collections::BTreeMap<u32, Vec<usize>> =
            std::collections::BTreeMap::new();
        for (i, r) in rank.iter().enumerate() {
            by_rank.entry(*r).or_default().push(i);
        }
        let mut moved = false;
        let cols = *by_rank.keys().next_back().unwrap_or(&0) as f32 + 1.0;
        for (r, members) in &by_rank {
            let mut members = members.clone();
            members.sort_by(|&a, &b| {
                self.nodes[a]
                    .y
                    .partial_cmp(&self.nodes[b].y)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            let rows = members.len() as f32;
            for (row, &i) in members.iter().enumerate() {
                let x = (*r as f32 - (cols - 1.0) / 2.0) * HIER_COL;
                let y = (row as f32 - (rows - 1.0) / 2.0) * HIER_ROW;
                let n = &mut self.nodes[i];
                if !n.pinned || n.x != x || n.y != y {
                    moved = true;
                }
                n.x = x;
                n.y = y;
                n.pinned = true;
            }
        }
        moved
    }

    /// Bounds for fitting: isolated nodes (degree 0) sit far out on the
    /// gravity ring and would shrink everything else, so when the connected
    /// part is the majority the fit is taken over it alone. The bounds
    /// include the nodes' rims (spec 031 §2: sizes vary) — a wide rect's
    /// ports sit on its rim, and a centre-only fit left them off the
    /// canvas (stage 7's flow blocks).
    /// A node with layers draws (and answers hits, and takes keyboard
    /// focus) only while one of its layers is visible (spec 031 §3).
    pub fn visible(&self, i: usize) -> bool {
        let n = &self.nodes[i];
        n.layers.is_empty()
            || n.layers
                .iter()
                .any(|id| self.layers.iter().any(|l| &l.id == id && l.visible))
    }

    /// Keyboard spatial navigation (spec 031, stage 10): the nearest node
    /// within a 45° cone of `dir` from node `from`, weighting distance
    /// against angular offset so a slightly-off near node beats a far
    /// straight one. `None` at the map's edge in that direction.
    pub fn nearest_in_direction(&self, from: usize, dir: [f32; 2]) -> Option<usize> {
        let origin = self.nodes.get(from)?;
        let (dx, dy) = (dir[0], dir[1]);
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1e-6 {
            return None;
        }
        let (ux, uy) = (dx / len, dy / len);
        let mut best: Option<(f32, usize)> = None;
        for (j, n) in self.nodes.iter().enumerate() {
            if j == from || !self.visible(j) {
                continue;
            }
            let (vx, vy) = (n.x - origin.x, n.y - origin.y);
            let d = (vx * vx + vy * vy).sqrt();
            if d < 1e-6 {
                continue;
            }
            let forward = (vx * ux + vy * uy) / d;
            if forward < std::f32::consts::FRAC_1_SQRT_2 {
                continue; // outside the cone
            }
            // A near node slightly off-axis wins over a far straight one;
            // the penalty is bounded so the walk never stalls.
            let score = d * (1.0 + (1.0 - forward) * 2.0);
            if best.is_none_or(|(s, _)| score < s) {
                best = Some((score, j));
            }
        }
        best.map(|(_, j)| j)
    }

    /// The fit's bounds. Once at least half the nodes are wired, only the
    /// wired ones count — far-away isolates must not blow the view up
    /// (spec 031). **A pinned node counts as wired**: it sits where a
    /// host or a user put it on purpose, so it belongs in the frame —
    /// without this, a flow's newly placed, still unwired blocks fell
    /// out of every fit the moment the flow was half wired (P-184, the
    /// MTK review's mechanical case: the mass landed below the canvas
    /// and no fit — the panel's refit, Layout, a manual fit — saw it).
    /// The hierarchical layout pins every node, so in hierarchy mode
    /// this exception covers its isolates too — harmless there, since
    /// that layout packs them into its grid; the rule is really for
    /// hosts that place nodes deliberately (the flow panel).
    pub fn fit_bounds(&self) -> Option<(f32, f32, f32, f32)> {
        let in_view = |n: &&Node| n.degree > 0 || n.pinned;
        let connected = self.nodes.iter().filter(in_view).count();
        if connected * 2 >= self.nodes.len() && connected > 0 {
            Self::bounds_with_rims(self.nodes.iter().filter(in_view))
        } else {
            Self::bounds_with_rims(self.nodes.iter())
        }
    }

    /// The fit's bounds: each node's centre expanded by its extent — a
    /// rect's half-width and half-height, a circle's radius.
    fn bounds_with_rims<'a>(
        mut it: impl Iterator<Item = &'a Node>,
    ) -> Option<(f32, f32, f32, f32)> {
        let first = it.next()?;
        let extent = |n: &Node| {
            let y = if matches!(n.shape, Shape::Rect | Shape::Diamond) {
                n.height
            } else {
                n.radius
            };
            (n.radius, y)
        };
        let (ex, ey) = extent(first);
        let mut b = (first.x - ex, first.y - ey, first.x + ex, first.y + ey);
        for n in it {
            let (ex, ey) = extent(n);
            b.0 = b.0.min(n.x - ex);
            b.1 = b.1.min(n.y - ey);
            b.2 = b.2.max(n.x + ex);
            b.3 = b.3.max(n.y + ey);
        }
        Some(b)
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
            ..Default::default()
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
                highlight: false,
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
                highlight: false,
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

    #[test]
    fn deltas_carry_hyperedges_and_simplexes_by_id() {
        // Spec 031 §2, stage 5: the same replace-on-re-add contract as
        // groups; members follow the survivor remap, and a hyperedge under
        // two members or a simplex under three has no shape and drops.
        let mut g = Graph::from_input(input());
        g.apply_delta(InDelta {
            add_hyperedges: vec![InHyperedge {
                id: "h".into(),
                nodes: vec!["a".into(), "b".into(), "c".into()],
                color: Some("#bb77ee".into()),
                ..Default::default()
            }],
            add_simplexes: vec![InSimplex {
                id: "s".into(),
                nodes: vec!["a".into(), "b".into(), "c".into()],
                color: Some("#ff9933".into()),
                ..Default::default()
            }],
            ..Default::default()
        });
        assert_eq!(g.hyperedges.len(), 1);
        assert_eq!(g.simplexes.len(), 1);
        assert_eq!(g.hyperedges[0].nodes.len(), 3);
        // Collapse b: its departure leaves both with two members — the
        // simplex drops, the hyperedge keeps its spoke pair (≥ 2 draws).
        g.apply_delta(InDelta {
            remove_nodes: vec!["b".into()],
            ..Default::default()
        });
        assert_eq!(g.hyperedges[0].nodes.len(), 2, "a spoke pair still draws");
        assert!(g.simplexes.is_empty(), "a face under three corners drops");
        // Removal by id; re-add replaces.
        g.apply_delta(InDelta {
            remove_hyperedges: vec!["h".into()],
            add_simplexes: vec![InSimplex {
                id: "s".into(),
                nodes: vec!["a".into(), "c".into(), "a".into()],
                color: None,
                ..Default::default()
            }],
            ..Default::default()
        });
        assert!(g.hyperedges.is_empty());
        // Two ids, one node: not a face (the same rule as an edge's).
        assert!(g.simplexes.is_empty());
    }

    #[test]
    fn a_delta_reports_whether_it_changed_anything() {
        // The P-166 review's #1: a count-preserving collapse must report
        // a change (five equal counts used to skip the redraw), and a
        // delta re-sending identical instances must not (it would warm
        // the layout and jiggle a settled graph for nothing).
        let mut g = Graph::from_input(input());
        let id_of = |g: &Graph, id: &str| g.nodes.iter().position(|n| n.id == id).unwrap();
        // Nothing named exists: no change.
        assert!(!g.apply_delta(InDelta {
            remove_nodes: vec!["ghost".into()],
            ..Default::default()
        }));
        // Re-sending an identical edge and node: no change.
        let (a, b) = (id_of(&g, "a"), id_of(&g, "b"));
        assert!(
            !g.apply_delta(InDelta {
                add_edges: vec![InDeltaEdge {
                    id: "0:1:0".into(),
                    a: "a".into(),
                    b: "b".into(),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            "the identical edge re-sent"
        );
        let old = g.nodes[a].clone();
        assert!(
            !g.apply_delta(InDelta {
                add_nodes: vec![InDeltaNode {
                    id: "a".into(),
                    label: "a".into(),
                    kind: "file".into(),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            "the identical node re-sent"
        );
        assert_eq!(g.nodes[a], old);
        // A relabel changes; a pulse_speed change changes; a collapse
        // with equal counts changes.
        assert!(g.apply_delta(InDelta {
            add_nodes: vec![InDeltaNode {
                id: "a".into(),
                label: "renamed".into(),
                kind: "file".into(),
                ..Default::default()
            }],
            ..Default::default()
        }));
        assert!(g.apply_delta(InDelta {
            add_edges: vec![InDeltaEdge {
                id: "0:1:0".into(),
                a: "a".into(),
                b: "b".into(),
                pulse_speed: Some(10.0),
                ..Default::default()
            }],
            ..Default::default()
        }));
        let before = (g.nodes.len(), g.edges.len());
        assert!(
            g.apply_delta(InDelta {
                remove_nodes: vec!["b".into()],
                add_nodes: vec![delta_node("agg", [10.0, 0.0])],
                add_edges: vec![delta_edge("p1", "a", "agg"), delta_edge("p2", "agg", "c")],
                ..Default::default()
            }),
            "the collapse kept the counts — it still redraws"
        );
        assert_eq!(g.nodes.len(), before.0);
        assert_eq!(g.edges.len(), before.1);
        // with_bows keeps the indices honest (a longer edge id list).
        let _ = b;
    }

    #[test]
    fn a_lone_edge_keeps_no_bow() {
        // The review's #3: an edge whose parallel partner a delta removed
        // must not stay curved for good.
        let mut g = Graph::from_input(InGraph {
            edges: vec![
                InEdge {
                    a: 0,
                    b: 1,
                    ..Default::default()
                },
                InEdge {
                    a: 0,
                    b: 1,
                    ..Default::default()
                },
            ],
            ..input()
        });
        assert!(g.edges.iter().any(|e| e.bow != 0.0), "the pair bows apart");
        g.apply_delta(InDelta {
            remove_edges: vec!["0:1:0".into()],
            ..Default::default()
        });
        assert!(
            g.edges.iter().all(|e| e.bow == 0.0),
            "the survivor straightened: {:?}",
            g.edges.iter().map(|e| e.bow).collect::<Vec<_>>()
        );
    }

    #[test]
    fn the_fit_bounds_carry_the_rects_rims() {
        // Stage 7: a flow block's half-width dwarfs the fit's pad — the
        // bounds must include the rims or the outermost ports land outside
        // the canvas (a centre-only fit cut them off).
        let g = Graph::from_input(InGraph {
            nodes: vec![
                InNode {
                    id: "blk".into(),
                    label: "Block".into(),
                    kind: "file".into(),
                    shape: Some("rect".into()),
                    width: Some(46.0),
                    height: Some(22.0),
                    pos: Some([30.0, 30.0]),
                    pinned: true,
                    ..Default::default()
                },
                InNode {
                    id: "dot".into(),
                    label: "dot".into(),
                    kind: "file".into(),
                    pos: Some([360.0, 370.0]),
                    pinned: true,
                    ..Default::default()
                },
            ],
            ..Default::default()
        });
        let (x0, y0, x1, y1) = g.fit_bounds().expect("two nodes have bounds");
        assert!((x0 - -16.0).abs() < 1e-4, "the rim goes left of the centre");
        assert!((x1 - 364.0).abs() < 1e-4, "the circle's radius rides along");
        assert!((y0 - 8.0).abs() < 1e-4, "the rect's half-height counts");
        assert!((y1 - 374.0).abs() < 1e-4);
    }

    #[test]
    fn the_arrows_walk_to_the_nearest_node_in_their_cone() {
        // Stage 10 (spec 031, the accessibility minimums): keyboard
        // spatial navigation — the nearest node inside a 45° cone,
        // with a slightly-off near node beating a far straight one.
        let g = Graph::from_input(InGraph {
            nodes: vec![
                InNode {
                    id: "a".into(),
                    label: "a".into(),
                    kind: "file".into(),
                    pos: Some([0.0, 0.0]),
                    pinned: true,
                    ..Default::default()
                },
                InNode {
                    id: "right".into(),
                    label: "r".into(),
                    kind: "file".into(),
                    pos: Some([40.0, 0.0]),
                    pinned: true,
                    ..Default::default()
                },
                InNode {
                    id: "near-off".into(),
                    label: "n".into(),
                    kind: "file".into(),
                    pos: Some([30.0, 12.0]),
                    pinned: true,
                    ..Default::default()
                },
                InNode {
                    id: "behind".into(),
                    label: "b".into(),
                    kind: "file".into(),
                    pos: Some([-40.0, 0.0]),
                    pinned: true,
                    ..Default::default()
                },
                InNode {
                    id: "far-straight".into(),
                    label: "f".into(),
                    kind: "file".into(),
                    pos: Some([200.0, 1.0]),
                    pinned: true,
                    ..Default::default()
                },
            ],
            edges: vec![],
            ..Default::default()
        });
        // Right: the near slightly-off node beats the far straight one.
        assert_eq!(g.nearest_in_direction(0, [1.0, 0.0]), Some(2));
        // Left: only "behind" is in the cone.
        assert_eq!(g.nearest_in_direction(0, [-1.0, 0.0]), Some(3));
        // Up: nothing above — the walk stalls at the map's edge.
        assert_eq!(g.nearest_in_direction(0, [0.0, -1.0]), None);
        // From "behind", right reaches "a".
        assert_eq!(g.nearest_in_direction(3, [1.0, 0.0]), Some(0));
    }

    #[test]
    fn a_host_placed_node_stays_where_the_host_put_it() {
        // Stage 7: a flow block's position is the document's — `pos` lands
        // it there and `pinned` keeps it out of the physics.
        let g = Graph::from_input(InGraph {
            nodes: vec![
                InNode {
                    id: "blk".into(),
                    label: "Block".into(),
                    kind: "file".into(),
                    shape: Some("rect".into()),
                    pos: Some([123.0, -45.0]),
                    pinned: true,
                    ..Default::default()
                },
                InNode {
                    id: "dot".into(),
                    label: "dot".into(),
                    kind: "file".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        });
        let blk = &g.nodes[0];
        assert!((blk.x - 123.0).abs() < 1e-4 && (blk.y + 45.0).abs() < 1e-4);
        assert!(blk.pinned, "the host placed it; the physics leave it");
        assert!(!g.nodes[1].pinned, "the spiral one is free to settle");
    }

    #[test]
    fn diamonds_carry_a_stroke_and_an_icon() {
        // Stage 8 (spec 031 §2): a third shape parses and sizes like a
        // block (half-extents, not the degree radius), a rim colour rides
        // along, and the icon survives a replace that names neither.
        let mut g = Graph::from_input(InGraph {
            nodes: vec![InNode {
                id: "dec".into(),
                label: "Decision".into(),
                kind: "file".into(),
                shape: Some("diamond".into()),
                width: Some(50.0),
                height: Some(24.0),
                stroke: Some("#ff8800".into()),
                icon: Some("◆".into()),
                pos: Some([10.0, 10.0]),
                pinned: true,
                ..Default::default()
            }],
            ..Default::default()
        });
        let n = &g.nodes[0];
        assert_eq!(n.shape, Shape::Diamond);
        assert!((n.radius - 50.0).abs() < 1e-4, "a block's half-width");
        assert_eq!(n.icon.as_deref(), Some("◆"));
        let stroke = n.stroke;
        assert!((stroke[0] - 1.0).abs() < 0.01 && stroke[3] > 0.99, "parsed");

        // A replace without stroke/icon keeps them (like the silhouette).
        g.apply_delta(InDelta {
            add_nodes: vec![InDeltaNode {
                id: "dec".into(),
                label: "Decision".into(),
                kind: "file".into(),
                pos: Some([30.0, 10.0]),
                ..Default::default()
            }],
            ..Default::default()
        });
        let n = &g.nodes[0];
        assert_eq!(n.shape, Shape::Diamond, "the silhouette survives");
        assert_eq!(n.stroke, stroke, "the stroke survives");
        assert_eq!(n.icon.as_deref(), Some("◆"), "the icon survives");
    }

    #[test]
    fn hierarchize_lines_the_wires_up_left_to_right() {
        // The flow arrangement (spec 031 SS5, stage 7): longest-path ranks
        // become columns, the rows stack, and every node lands pinned —
        // the host's document owns the positions, not the physics.
        let mut g = Graph::from_input(InGraph {
            nodes: vec![
                InNode {
                    id: "a".into(),
                    label: "a".into(),
                    kind: "file".into(),
                    ..Default::default()
                },
                InNode {
                    id: "d".into(),
                    label: "d".into(),
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
                    b: 2,
                    ..Default::default()
                },
                InEdge {
                    a: 1,
                    b: 2,
                    ..Default::default()
                },
                InEdge {
                    a: 2,
                    b: 3,
                    ..Default::default()
                },
            ],
            ..Default::default()
        });
        assert!(g.hierarchize());
        let at = |id: &str| g.nodes.iter().find(|n| n.id == id).unwrap();
        // Ranks: a,d = 0; b = 1; c = 2 — three columns centred on zero.
        assert_eq!(at("a").x, -HIER_COL, "column 0");
        assert_eq!(at("b").x, 0.0, "column 1");
        assert_eq!(at("c").x, HIER_COL, "column 2");
        assert_eq!(at("d").x, -HIER_COL);
        // The first column's two rows stack around its centre, keeping
        // the nodes' prior vertical order; everything is pinned.
        assert!((at("a").y + at("d").y).abs() < 1e-4, "rows centred");
        assert_eq!((at("a").y - at("d").y).abs() as u32, HIER_ROW as u32);
        assert!(g.nodes.iter().all(|n| n.pinned), "pinned for the host");
        // Idempotent: a re-run keeps the arrangement.
        assert!(!g.hierarchize());
    }

    #[test]
    fn a_replacement_without_pos_keeps_the_old_position() {
        // The review's #7: a relabel must not teleport the node to a
        // fresh spiral slot (nor reset its depth).
        let mut g = Graph::from_input(input());
        let old = g.nodes[0].clone();
        assert!(g.apply_delta(InDelta {
            add_nodes: vec![InDeltaNode {
                id: "a".into(),
                label: "renamed".into(),
                kind: "file".into(),
                ..Default::default()
            }],
            ..Default::default()
        }));
        let n = &g.nodes[0];
        assert_eq!(n.label, "renamed");
        assert_eq!((n.x, n.y, n.z), (old.x, old.y, old.z), "the position held");
        assert_eq!(n.pinned, old.pinned);
        // An explicit pos still moves it (an expand places its members).
        assert!(g.apply_delta(InDelta {
            add_nodes: vec![delta_node("a", [99.0, -7.0])],
            ..Default::default()
        }));
        assert_eq!((g.nodes[0].x, g.nodes[0].y), (99.0, -7.0));
    }
}
