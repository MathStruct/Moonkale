//! Flow schema and model (Milestone 6): the contract between the flow
//! editor (`editors/flow`) and the extensions that contribute block
//! libraries (e.g. the optional Lux.jl extension). Dioxus-free; stored as
//! JSON in `*.flow.json` files.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A tensor shape with unknown dimensions allowed (`None`).
pub type Shape = Vec<Option<u64>>;

/// What a port carries; wires connect compatible types.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PortType {
    /// Connects to anything.
    Any,
    /// A single number.
    Scalar,
    /// A tensor whose shape may be partially known (batch dim excluded).
    Tensor {
        /// Its shape (`None` = unknown dimension).
        shape: Shape,
    },
    /// Opaque, must match by name (e.g. `"optimiser"`, `"loss"`).
    Named {
        /// The name both ends must share.
        name: String,
    },
}

impl PortType {
    /// A tensor port of `shape`.
    pub fn tensor(shape: impl Into<Shape>) -> Self {
        PortType::Tensor {
            shape: shape.into(),
        }
    }

    /// The most specific type both accept, or `None` on a mismatch.
    pub fn unify(&self, other: &PortType) -> Option<PortType> {
        use PortType::*;
        match (self, other) {
            (Any, t) | (t, Any) => Some(t.clone()),
            (Scalar, Scalar) => Some(Scalar),
            (Named { name: a }, Named { name: b }) if a == b => Some(Named { name: a.clone() }),
            (Tensor { shape: a }, Tensor { shape: b }) => {
                // An empty shape means "any rank".
                if a.is_empty() {
                    return Some(Tensor { shape: b.clone() });
                }
                if b.is_empty() {
                    return Some(Tensor { shape: a.clone() });
                }
                if a.len() != b.len() {
                    return None;
                }
                let mut out = Vec::with_capacity(a.len());
                for (x, y) in a.iter().zip(b) {
                    out.push(match (x, y) {
                        (Some(x), Some(y)) if x != y => return None,
                        (Some(x), _) => Some(*x),
                        (None, y) => *y,
                    });
                }
                Some(Tensor { shape: out })
            }
            _ => None,
        }
    }

    /// A short human form (`tensor[?, 784]`).
    pub fn describe(&self) -> String {
        match self {
            PortType::Any => "any".into(),
            PortType::Scalar => "scalar".into(),
            PortType::Named { name } => name.clone(),
            PortType::Tensor { shape } if shape.is_empty() => "tensor".into(),
            PortType::Tensor { shape } => format!(
                "tensor[{}]",
                shape
                    .iter()
                    .map(|d| d.map(|d| d.to_string()).unwrap_or_else(|| "?".into()))
                    .collect::<Vec<_>>()
                    .join("×")
            ),
        }
    }
}

/// An input or output of a block.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Port {
    /// The port's name, unique within the block's inputs or outputs.
    pub name: String,
    /// What it carries.
    pub ty: PortType,
    /// Inputs that must be wired for the flow to be valid.
    #[serde(default)]
    pub required: bool,
}

/// The kind of value a block parameter takes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ParamKind {
    /// An integer.
    Int,
    /// A number.
    Float,
    /// Free text.
    Text,
    /// On/off.
    Bool,
    /// One of `options`.
    Choice {
        /// The choices.
        options: Vec<String>,
    },
}

/// A block kind's side of a bipartite factor graph (ADR-0022): RxInfer
/// and Lenticulum models are made of variables and factors, and a wire
/// always joins one of each. A kind without a role (Lux, ModelingToolkit,
/// a subflow, a boundary pseudo-block) is outside the rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// A variable: no behaviour of its own; any number of factors attach
    /// to it (a variable of degree d is a copier).
    Variable,
    /// A factor: its pins are named channels, each attached to exactly
    /// one variable — a channel is a port, not a bus.
    Factor,
}

impl Role {
    fn plural(self) -> &'static str {
        match self {
            Role::Variable => "variables",
            Role::Factor => "factors",
        }
    }
    fn other(self) -> &'static str {
        match self {
            Role::Variable => "factors",
            Role::Factor => "variables",
        }
    }
}

/// A block's drawing (ADR-0024): a schematic symbol instead of a plain
/// box — a capacitor's plates, a resistor's zigzag, ground's bars — and
/// where its pins sit on it. A kind without one draws as a box, its pins
/// placed by the editor's default rule.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Symbol {
    /// The symbol's width in canvas units (a default box is 92 wide).
    pub width: f32,
    /// Its height in canvas units (a default box is 44 tall).
    pub height: f32,
    /// SVG path data in the box `0 0 width height`, stroked in the
    /// block's colour (`M 0 11 H 14 M 14 3 V 19 …`). Only the path
    /// syntax — no fills, no other SVG elements.
    pub path: String,
    /// Where each port sits on the symbol's box. A port not named here
    /// falls back to the default rule.
    #[serde(default)]
    pub anchors: Vec<Anchor>,
}

/// One port's place on a [`Symbol`]'s box: a side, and how far along it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Anchor {
    /// The port's name.
    pub port: String,
    /// The side it sits on.
    pub side: Side,
    /// 0..1 along the side, left to right or top to bottom; 0.5 is its
    /// middle.
    #[serde(default = "half")]
    pub offset: f32,
}

fn half() -> f32 {
    0.5
}

/// A side of a block's box.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    /// The left edge.
    Left,
    /// The right edge.
    Right,
    /// The top edge.
    Top,
    /// The bottom edge.
    Bottom,
}

impl Side {
    /// The side's name as the graph engine spells it.
    pub fn as_str(self) -> &'static str {
        match self {
            Side::Left => "left",
            Side::Right => "right",
            Side::Top => "top",
            Side::Bottom => "bottom",
        }
    }
}

/// A parameter of a block (units, activation, …).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Param {
    /// The parameter's name.
    pub name: String,
    /// What it takes.
    pub kind: ParamKind,
    /// Its value when a block is created, as text.
    pub default: String,
}

/// A block type a library offers. `Default` keeps future fields
/// non-breaking: a library builds the fields it cares about and finishes
/// with `..Default::default()`, so a new field no longer needs a new
/// ext-api tag in lockstep (the pins review's #2).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BlockKind {
    /// Unique within the library (`"dense"`).
    pub id: String,
    /// The name shown in the palette.
    pub name: String,
    /// The palette group (`"layers"`).
    pub category: String,
    /// One line for the palette.
    pub description: String,
    /// Its input ports.
    pub inputs: Vec<Port>,
    /// Its output ports.
    pub outputs: Vec<Port>,
    /// Its undirected pins (ADR-0020): ModelingToolkit-style `connect`
    /// ports. A pin's type names the **domain** both ends must share
    /// (`Named { name: "electrical" }`). Absent in directed libraries
    /// (Lux), so the field is defaulted — old flows and libraries parse
    /// unchanged. Port names must stay unique across inputs, outputs and
    /// connections within one kind.
    #[serde(default)]
    pub connections: Vec<Port>,
    /// Its parameters.
    pub params: Vec<Param>,
    /// Its side of a bipartite factor graph (ADR-0022): pin wires join a
    /// variable and a factor, never two of a kind, and a factor's
    /// channel carries one wire. `None` — every library before RxInfer —
    /// is outside the rule.
    #[serde(default)]
    pub role: Option<Role>,
    /// Its drawing (ADR-0024); `None` draws a box.
    #[serde(default)]
    pub symbol: Option<Symbol>,
}

/// Generated code for a flow: file name + text.
pub type Codegen = fn(&Flow, &FlowLibrary) -> Result<Generated, String>;

/// What a [`Codegen`] produces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Generated {
    /// The file to write (`model.jl`).
    pub file_name: String,
    /// Its text.
    pub text: String,
    /// How to run it, shown to the user (`"julia model.jl"`).
    pub run_hint: String,
}

/// A block library, contributed by an extension through
/// `Extension::flow_libraries`.
#[derive(Clone)]
pub struct FlowLibrary {
    /// Namespace for block ids (`"lux"`): blocks are referenced as `lux/dense`.
    pub id: &'static str,
    /// The library's display name.
    pub name: &'static str,
    /// Its block types.
    pub blocks: Vec<BlockKind>,
    /// Turns a flow into code, if the library can.
    pub codegen: Option<Codegen>,
    /// The language of that code (`"julia"`).
    pub language: &'static str,
}

impl FlowLibrary {
    /// A block type by id (within this library).
    pub fn block(&self, id: &str) -> Option<&BlockKind> {
        self.blocks.iter().find(|b| b.id == id)
    }
}

impl PartialEq for FlowLibrary {
    fn eq(&self, o: &Self) -> bool {
        self.id == o.id && self.blocks == o.blocks
    }
}

// ---- the stored model ------------------------------------------------------

/// A block placed on the canvas.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Block {
    /// Unique within the flow.
    pub id: String,
    /// `"<library>/<kind>"`.
    pub kind: String,
    /// Canvas position.
    pub x: f64,
    /// Canvas position.
    pub y: f64,
    /// Parameter values by name, as text.
    #[serde(default)]
    pub params: BTreeMap<String, String>,
    /// The subflow this block renders as one block (ADR-0021): its id
    /// in the same document. `None` for a library block. Defaulted, so
    /// every existing flow parses; a literal that lists every field of
    /// `Block` gains `subflow: None` (the `ext-api` tag with this).
    #[serde(default)]
    pub subflow: Option<String>,
}

/// A wire from an output port to an input port.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Wire {
    /// Unique within the flow.
    pub id: String,
    /// The block it leaves.
    pub from_block: String,
    /// Its output port.
    pub from_port: String,
    /// The block it enters.
    pub to_block: String,
    /// Its input port.
    pub to_port: String,
}

/// A subflow (ADR-0021): a named subsystem that lives in the same
/// document as the flow that shows it. A block with
/// [`Block::subflow`] set renders one as a single block whose ports
/// are the subflow's declared boundary ports; the declaration mirrors
/// a [`BlockKind`]'s three port lists, so a boundary port is a
/// directed input, an output or a domain-typed pin exactly as before.
/// The inner `blocks`/`wires` (stage B: boundary ports appear inside
/// as pseudo-blocks) are ordinary [`Block`]s and [`Wire`]s.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Subflow {
    /// Unique within the flow (`"rc_filter"`).
    pub id: String,
    /// The name the parent block shows.
    pub name: String,
    /// The subflow's own blocks.
    pub blocks: Vec<Block>,
    /// The subflow's own wires.
    pub wires: Vec<Wire>,
    /// Declared boundary inputs — the parent block's input ports.
    pub inputs: Vec<Port>,
    /// Declared boundary outputs — the parent block's output ports.
    pub outputs: Vec<Port>,
    /// Declared boundary pins — the parent block's undirected pins
    /// (ADR-0020).
    pub connections: Vec<Port>,
}

/// A `*.flow.json` file: blocks and wires.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Flow {
    /// The file format's version ([`Flow::VERSION`]).
    pub version: u32,
    /// The blocks.
    pub blocks: Vec<Block>,
    /// The wires.
    pub wires: Vec<Wire>,
    /// The subflows (ADR-0021): subsystems referenced by
    /// [`Block::subflow`]. Empty in every pre-subflow flow.
    pub subflows: Vec<Subflow>,
}

/// The id prefix of a boundary pseudo-block (ADR-0021 §3): inside a
/// subflow, each declared port appears as a block whose id is
/// `boundary/<port>` — synthesized by the editor when the scope is
/// open, never stored in the document. The declaration is the truth;
/// the pseudo-block is the illusion the panel owns.
pub const BOUNDARY: &str = "boundary/";

/// The blocks and wires a scope stack addresses (ADR-0021 §4): the
/// flow's own at the root, a subflow's once entered. The panel's
/// mutators and its scene build on this; the engine never knows a
/// "level" exists.
#[derive(Clone, Copy)]
pub struct Scope<'a> {
    /// The scope's blocks (a boundary pseudo-block is not among them).
    pub blocks: &'a [Block],
    /// The scope's wires — they may name boundary pseudo-blocks.
    pub wires: &'a [Wire],
}

impl Flow {
    /// The version this build writes.
    pub const VERSION: u32 = 1;
    /// The file name suffix of flows.
    pub const EXTENSION: &'static str = ".flow.json";

    /// Read a flow; an unparseable file is an error, never an empty flow.
    pub fn parse(json: &str) -> Result<Self, String> {
        if json.trim().is_empty() {
            return Ok(Self::new());
        }
        serde_json::from_str(json).map_err(|e| e.to_string())
    }

    /// An empty flow.
    pub fn new() -> Self {
        Self {
            version: Self::VERSION,
            ..Default::default()
        }
    }

    /// The file's text.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".into())
    }

    /// A block by id.
    pub fn block(&self, id: &str) -> Option<&Block> {
        self.blocks.iter().find(|b| b.id == id)
    }

    /// A subflow by id (ADR-0021).
    pub fn subflow(&self, id: &str) -> Option<&Subflow> {
        self.subflows.iter().find(|s| s.id == id)
    }

    /// The scope a stack of subflow ids addresses (ADR-0021 §4): the
    /// flow's own blocks at `[]`, a subflow's once entered. Each id
    /// must be referenced by a block of the scope it is entered from —
    /// a dangling id names no scope.
    pub fn scope(&self, path: &[String]) -> Option<Scope<'_>> {
        let mut blocks = &self.blocks[..];
        let mut wires = &self.wires[..];
        for id in path {
            let sub = self.subflow(id)?;
            if !blocks
                .iter()
                .any(|b| b.subflow.as_deref() == Some(id.as_str()))
            {
                return None;
            }
            blocks = &sub.blocks[..];
            wires = &sub.wires[..];
        }
        Some(Scope { blocks, wires })
    }

    /// Run `f` on the scope's blocks and wires (ADR-0021 §4) — the
    /// mutators of an open scope write through this. `None` if the
    /// path names no scope (a dangling id). Each step takes the next
    /// subflow's lists out and puts them back on unwind, so one frame
    /// never borrows through two levels.
    pub fn with_scope<R>(
        &mut self,
        path: &[String],
        mut f: impl FnMut(&mut Vec<Block>, &mut Vec<Wire>) -> R,
    ) -> Option<R> {
        fn run<R, F: FnMut(&mut Vec<Block>, &mut Vec<Wire>) -> R>(
            blocks: &mut Vec<Block>,
            wires: &mut Vec<Wire>,
            subs: &mut Vec<Subflow>,
            path: &[String],
            f: &mut F,
        ) -> Option<R> {
            match path.split_first() {
                None => Some(f(blocks, wires)),
                Some((id, rest)) => {
                    if !blocks
                        .iter()
                        .any(|b| b.subflow.as_deref() == Some(id.as_str()))
                    {
                        return None;
                    }
                    let i = subs.iter().position(|s| &s.id == id)?;
                    let mut sub = std::mem::take(&mut subs[i]);
                    let mut inner_blocks = std::mem::take(&mut sub.blocks);
                    let mut inner_wires = std::mem::take(&mut sub.wires);
                    let r = run(&mut inner_blocks, &mut inner_wires, subs, rest, f);
                    sub.blocks = inner_blocks;
                    sub.wires = inner_wires;
                    subs[i] = sub;
                    r
                }
            }
        }
        run(
            &mut self.blocks,
            &mut self.wires,
            &mut self.subflows,
            path,
            &mut f,
        )
    }

    /// A block's ports: the subflow's declared boundary ports when the
    /// block references one (ADR-0021 §2 — they *are* the parent
    /// block's ports), else its library kind's. `None` = the block
    /// resolves to nothing (`validate` reports it).
    pub fn ports_of<'a>(
        &'a self,
        libraries: &'a [FlowLibrary],
        block: &Block,
    ) -> Option<BlockPorts<'a>> {
        if let Some(id) = &block.subflow {
            let s = self.subflow(id)?;
            return Some(BlockPorts {
                name: &s.name,
                inputs: &s.inputs,
                outputs: &s.outputs,
                connections: &s.connections,
                params: &[],
                role: None,
                symbol: None,
            });
        }
        find_kind(libraries, &block.kind).map(|(_, k)| BlockPorts {
            name: &k.name,
            inputs: &k.inputs,
            outputs: &k.outputs,
            connections: &k.connections,
            params: &k.params,
            role: k.role,
            symbol: k.symbol.as_ref(),
        })
    }

    /// Kinds resolved against the libraries: `(block, library, kind)`.
    pub fn resolve<'a>(
        &'a self,
        libraries: &'a [FlowLibrary],
    ) -> Vec<(&'a Block, Option<(&'a FlowLibrary, &'a BlockKind)>)> {
        self.blocks
            .iter()
            .map(|b| (b, find_kind(libraries, &b.kind)))
            .collect()
    }

    /// Blocks in an order where every wire goes from earlier to later
    /// (Kahn); `Err` names a block on a cycle.
    pub fn topological(&self) -> Result<Vec<&Block>, String> {
        self.topological_skipping(&|_| false)
    }

    /// [`topological`](Self::topological) over the wires `skip` does not
    /// name — a pin wire is an equation, not dataflow (ADR-0020), so
    /// cycles through pins are legal and must not trap the walk.
    pub fn topological_skipping(
        &self,
        skip: &dyn Fn(&Wire) -> bool,
    ) -> Result<Vec<&Block>, String> {
        topological_over(&self.blocks, &self.wires, skip)
    }
}

/// The library and block type of a `"<library>/<kind>"` reference.
pub fn find_kind<'a>(
    libraries: &'a [FlowLibrary],
    kind: &str,
) -> Option<(&'a FlowLibrary, &'a BlockKind)> {
    let (lib, id) = kind.split_once('/')?;
    let library = libraries.iter().find(|l| l.id == lib)?;
    library.block(id).map(|k| (library, k))
}

/// What a block resolves to through [`Flow::ports_of`]: its library
/// kind's ports, or — when the block references a subflow — the
/// subflow's declared boundary ports (ADR-0021 §2). The panel and
/// [`validate`] both draw on this; the engine never learns what a
/// "subflow" is.
#[derive(Clone, Copy)]
pub struct BlockPorts<'a> {
    /// The name the block shows.
    pub name: &'a str,
    /// Its input ports.
    pub inputs: &'a [Port],
    /// Its output ports.
    pub outputs: &'a [Port],
    /// Its undirected pins (ADR-0020).
    pub connections: &'a [Port],
    /// Its parameters — a subflow declares none through the model.
    pub params: &'a [Param],
    /// Its bipartite role (ADR-0022) — a subflow and a boundary
    /// pseudo-block have none.
    pub role: Option<Role>,
    /// Its drawing (ADR-0024) — a subflow and a boundary pseudo-block
    /// have none.
    pub symbol: Option<&'a Symbol>,
}

/// Why a pin wire between `a` and `b` breaks the bipartite rule
/// (ADR-0022), if it does: two variables or two factors. Ends without a
/// role are outside the rule. `validate` and the flow panel's wire
/// gesture share it, so the refusal and the issue say the same.
pub fn bipartite_refusal(a: &BlockPorts, b: &BlockPorts) -> Option<String> {
    match (a.role, b.role) {
        (Some(x), Some(y)) if x == y => Some(format!(
            "{} connect to {}, not to {}",
            x.plural(),
            x.other(),
            x.plural()
        )),
        _ => None,
    }
}

/// Why one more wire on pin `pin` of a block with ports `ports` is
/// refused (ADR-0022): a factor's channel attaches to one variable, so
/// fan-out goes through a shared variable. `wires` is how many the pin
/// already carries.
pub fn channel_refusal(ports: &BlockPorts, block: &str, pin: &str, wires: usize) -> Option<String> {
    (ports.role == Some(Role::Factor)
        && wires >= 1
        && ports.connections.iter().any(|p| p.name == pin))
    .then(|| {
        format!(
            "channel {pin} of {block} already has its variable — a channel is a port, not a bus; connect the factors through one variable"
        )
    })
}

/// Blocks in an order where every wire goes from earlier to later
/// (Kahn); `Err` names a block on a cycle. Shared by the root flow and
/// every subflow — a scope's dataflow is checked exactly like the
/// root's (ADR-0021 §4).
fn topological_over<'a>(
    blocks: &'a [Block],
    wires: &[Wire],
    skip: &dyn Fn(&Wire) -> bool,
) -> Result<Vec<&'a Block>, String> {
    let mut indeg: BTreeMap<&str, usize> = blocks.iter().map(|b| (b.id.as_str(), 0)).collect();
    for w in wires {
        if skip(w) {
            continue;
        }
        if let Some(d) = indeg.get_mut(w.to_block.as_str()) {
            *d += 1;
        }
    }
    let mut ready: Vec<&Block> = blocks
        .iter()
        .filter(|b| indeg.get(b.id.as_str()) == Some(&0))
        .collect();
    let mut out = Vec::new();
    while let Some(b) = ready.pop() {
        out.push(b);
        for w in wires.iter().filter(|w| w.from_block == b.id && !skip(w)) {
            if let Some(d) = indeg.get_mut(w.to_block.as_str()) {
                *d -= 1;
                if *d == 0 {
                    if let Some(n) = blocks.iter().find(|b| b.id == w.to_block) {
                        ready.push(n);
                    }
                }
            }
        }
    }
    if out.len() != blocks.len() {
        let stuck = blocks
            .iter()
            .find(|b| !out.iter().any(|o| o.id == b.id))
            .map(|b| b.id.clone())
            .unwrap_or_default();
        return Err(format!("cycle through block {stuck}"));
    }
    Ok(out)
}

/// One problem with a flow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Issue {
    /// Block id (or wire id) the issue is about.
    pub about: String,
    /// What is wrong.
    pub message: String,
}

/// The wire classification and type check shared by the root flow and
/// every subflow (ADR-0021 §4): dataflow fits or mismatches, equations
/// unify symmetrically or refuse, and a port that exists but cannot
/// pair is a mixed wire, not a missing one (ADR-0020). `about` prefixes
/// an inner issue with its subflow's id.
fn wire_issue(
    fk: &BlockPorts,
    tk: &BlockPorts,
    from_block: &str,
    from_port: &str,
    to_block: &str,
    to_port: &str,
    about: String,
) -> Option<Issue> {
    let out = fk.outputs.iter().find(|p| p.name == from_port);
    let inp = tk.inputs.iter().find(|p| p.name == to_port);
    let pin_f = fk.connections.iter().find(|p| p.name == from_port);
    let pin_t = tk.connections.iter().find(|p| p.name == to_port);
    // A wire is dataflow when it leaves an output and enters an input;
    // an equation when both ends are pins (ADR-0020) — either drag
    // order, and its types must unify symmetrically.
    if let (Some(o), Some(i)) = (out, inp) {
        (o.ty.unify(&i.ty).is_none()).then(|| Issue {
            about,
            message: format!(
                "{}.{} ({}) does not fit {}.{} ({})",
                from_block,
                from_port,
                o.ty.describe(),
                to_block,
                to_port,
                i.ty.describe()
            ),
        })
    } else if let (Some(a), Some(b)) = (pin_f, pin_t) {
        if a.ty.unify(&b.ty).or_else(|| b.ty.unify(&a.ty)).is_none() {
            return Some(Issue {
                about,
                message: format!(
                    "{}.{} ({}) does not connect to {}.{} ({})",
                    from_block,
                    from_port,
                    a.ty.describe(),
                    to_block,
                    to_port,
                    b.ty.describe()
                ),
            });
        }
        bipartite_refusal(fk, tk).map(|why| Issue {
            about,
            message: format!("{from_block}.{from_port} – {to_block}.{to_port}: {why}"),
        })
    } else if out.is_some() || inp.is_some() || pin_f.is_some() || pin_t.is_some() {
        Some(Issue {
            about,
            message: "an output connects to an input, a pin to a pin".into(),
        })
    } else {
        Some(Issue {
            about,
            message: "wire references a port that does not exist".into(),
        })
    }
}

/// A block's ports inside a subflow (ADR-0021 §3): a boundary
/// pseudo-block — `boundary/<port>` — resolves through the subflow's
/// declaration, where a declared *input* is a source inside (an
/// output), a declared *output* is a sink (an input), and a pin stays
/// a pin; every other block resolves exactly as at the root (its
/// library kind, or the subflow it references).
pub fn inner_ports<'a>(
    flow: &'a Flow,
    sub: &'a Subflow,
    libraries: &'a [FlowLibrary],
    id: &str,
) -> Option<BlockPorts<'a>> {
    if let Some(port) = id.strip_prefix(BOUNDARY) {
        let one = |p: &'a Port| std::slice::from_ref(p);
        if let Some(p) = sub.inputs.iter().find(|p| p.name == port) {
            return Some(BlockPorts {
                name: &p.name,
                inputs: &[],
                outputs: one(p),
                connections: &[],
                params: &[],
                role: None,
                symbol: None,
            });
        }
        if let Some(p) = sub.outputs.iter().find(|p| p.name == port) {
            return Some(BlockPorts {
                name: &p.name,
                inputs: one(p),
                outputs: &[],
                connections: &[],
                params: &[],
                role: None,
                symbol: None,
            });
        }
        if let Some(p) = sub.connections.iter().find(|p| p.name == port) {
            return Some(BlockPorts {
                name: &p.name,
                inputs: &[],
                outputs: &[],
                connections: one(p),
                params: &[],
                role: None,
                symbol: None,
            });
        }
        return None;
    }
    flow.ports_of(libraries, sub.blocks.iter().find(|b| b.id == id)?)
}

/// A factor's channels carry one wire each (ADR-0022): two wires on one
/// channel are reported once, about the block.
fn channel_issues(
    b: &Block,
    ports: &BlockPorts,
    wires: &[Wire],
    about: &str,
    issues: &mut Vec<Issue>,
) {
    if ports.role != Some(Role::Factor) {
        return;
    }
    for p in ports.connections {
        let n = wires
            .iter()
            .filter(|w| {
                (w.from_block == b.id && w.from_port == p.name)
                    || (w.to_block == b.id && w.to_port == p.name)
            })
            .count();
        if n > 1 {
            issues.push(Issue {
                about: about.to_string(),
                message: format!(
                    "channel {} of {} carries {n} wires — a channel attaches to one variable",
                    p.name, b.id
                ),
            });
        }
    }
}

/// Type-check every wire, check required inputs and cycles.
pub fn validate(flow: &Flow, libraries: &[FlowLibrary]) -> Vec<Issue> {
    let mut issues = Vec::new();
    for b in &flow.blocks {
        // A subflow block resolves through its subflow, not a library
        // (ADR-0021 §2) — the message names which one is missing.
        if let Some(id) = &b.subflow {
            if flow.subflow(id).is_none() {
                issues.push(Issue {
                    about: b.id.clone(),
                    message: format!(
                        "block {} references the subflow {id}, which this document does not hold",
                        b.id
                    ),
                });
            }
        } else if find_kind(libraries, &b.kind).is_none() {
            issues.push(Issue {
                about: b.id.clone(),
                message: format!("unknown block kind {} (is its extension enabled?)", b.kind),
            });
        }
    }
    for w in &flow.wires {
        let from = flow
            .block(&w.from_block)
            .and_then(|b| flow.ports_of(libraries, b));
        let to = flow
            .block(&w.to_block)
            .and_then(|b| flow.ports_of(libraries, b));
        let (Some(fk), Some(tk)) = (from, to) else {
            continue;
        };
        if let Some(issue) = wire_issue(
            &fk,
            &tk,
            &w.from_block,
            &w.from_port,
            &w.to_block,
            &w.to_port,
            w.id.clone(),
        ) {
            issues.push(issue);
        }
    }
    // Port names are unique across a kind's inputs, outputs and
    // connections — the wire classification depends on it (the pins
    // review's #4). Reported once per kind, on the first block using it.
    let duplicated_kinds: std::collections::HashSet<String> = libraries
        .iter()
        .flat_map(|l| l.blocks.iter().map(move |k| (l.id, k)))
        .filter(|(_, k)| {
            let mut seen: Vec<&str> = Vec::new();
            k.inputs
                .iter()
                .chain(&k.outputs)
                .chain(&k.connections)
                .any(|p| {
                    if seen.contains(&p.name.as_str()) {
                        true
                    } else {
                        seen.push(p.name.as_str());
                        false
                    }
                })
        })
        .map(|(lib, k)| format!("{lib}/{}", k.id))
        .collect();
    let mut reported_kinds: std::collections::HashSet<String> = std::collections::HashSet::new();
    // A subflow's boundary ports obey the same uniqueness rule — the
    // wire classification depends on it — reported once per subflow on
    // the first block referencing it, mirroring the kind check.
    let duplicated_subflows: std::collections::HashSet<&str> = flow
        .subflows
        .iter()
        .filter(|s| {
            let mut seen: Vec<&str> = Vec::new();
            s.inputs
                .iter()
                .chain(&s.outputs)
                .chain(&s.connections)
                .any(|p| {
                    if seen.contains(&p.name.as_str()) {
                        true
                    } else {
                        seen.push(p.name.as_str());
                        false
                    }
                })
        })
        .map(|s| s.id.as_str())
        .collect();
    let mut reported_subflows: std::collections::HashSet<String> = std::collections::HashSet::new();
    for b in &flow.blocks {
        let Some(ports) = flow.ports_of(libraries, b) else {
            continue;
        };
        if let Some(id) = &b.subflow {
            if duplicated_subflows.contains(id.as_str()) && reported_subflows.insert(id.clone()) {
                issues.push(Issue {
                    about: b.id.clone(),
                    message: format!("subflow {id} reuses a port name"),
                });
            }
        } else if duplicated_kinds.contains(&b.kind) && reported_kinds.insert(b.kind.clone()) {
            issues.push(Issue {
                about: b.id.clone(),
                message: format!("{} reuses a port name", b.kind),
            });
        }
        for p in ports.inputs.iter().filter(|p| p.required) {
            if !flow
                .wires
                .iter()
                .any(|w| w.to_block == b.id && w.to_port == p.name)
            {
                issues.push(Issue {
                    about: b.id.clone(),
                    message: format!("input {} of {} is not connected", p.name, b.id),
                });
            }
        }
        // A required pin is satisfied from either direction — a pin
        // wire's from/to are just the two ends (ADR-0020).
        for p in ports.connections.iter().filter(|p| p.required) {
            if !flow.wires.iter().any(|w| {
                (w.from_block == b.id && w.from_port == p.name)
                    || (w.to_block == b.id && w.to_port == p.name)
            }) {
                issues.push(Issue {
                    about: b.id.clone(),
                    message: format!("pin {} of {} is not connected", p.name, b.id),
                });
            }
        }
        // A factor's channel attaches to one variable (ADR-0022).
        channel_issues(b, &ports, &flow.wires, &b.id, &mut issues);
    }
    // Cycles are a dataflow problem; equations may loop (ADR-0020).
    let pin_wires: std::collections::HashSet<&str> = flow
        .wires
        .iter()
        .filter(|w| {
            flow.block(&w.from_block)
                .and_then(|b| flow.ports_of(libraries, b))
                .map(|k| k.connections.iter().any(|p| p.name == w.from_port))
                .unwrap_or(false)
                && flow
                    .block(&w.to_block)
                    .and_then(|b| flow.ports_of(libraries, b))
                    .map(|k| k.connections.iter().any(|p| p.name == w.to_port))
                    .unwrap_or(false)
        })
        .map(|w| w.id.as_str())
        .collect();
    if let Err(e) = flow.topological_skipping(&|w| pin_wires.contains(w.id.as_str())) {
        issues.push(Issue {
            about: String::new(),
            message: e,
        });
    }
    // Subflow ids are unique in the document: the scope stack and the
    // inner issue paths name them (ADR-0021 §4).
    {
        let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for s in &flow.subflows {
            if !seen.insert(s.id.as_str()) {
                issues.push(Issue {
                    about: s.id.clone(),
                    message: format!("subflow id {} is not unique", s.id),
                });
            }
        }
    }
    // Inside a subflow (ADR-0021 §3-4): its wires type-check against
    // its blocks and its boundary pseudo-blocks, its dataflow must be
    // acyclic, and a required boundary port must be fed from inside —
    // a broken inner wire otherwise only shows up at codegen. An
    // inner issue is about "{subflow id}/{block or wire id}".
    for sub in &flow.subflows {
        for w in &sub.wires {
            let (Some(fk), Some(tk)) = (
                inner_ports(flow, sub, libraries, &w.from_block),
                inner_ports(flow, sub, libraries, &w.to_block),
            ) else {
                continue;
            };
            if let Some(issue) = wire_issue(
                &fk,
                &tk,
                &w.from_block,
                &w.from_port,
                &w.to_block,
                &w.to_port,
                format!("{}/{}", sub.id, w.id),
            ) {
                issues.push(issue);
            }
        }
        for b in &sub.blocks {
            let about = format!("{}/{}", sub.id, b.id);
            let Some(ports) = flow.ports_of(libraries, b) else {
                if b.subflow.is_some() {
                    if flow.subflow(b.subflow.as_deref().unwrap()).is_none() {
                        issues.push(Issue {
                            about,
                            message: format!(
                                "block {} references the subflow {}, which this document does not hold",
                                b.id,
                                b.subflow.as_deref().unwrap_or_default()
                            ),
                        });
                    }
                } else if find_kind(libraries, &b.kind).is_none() {
                    issues.push(Issue {
                        about,
                        message: format!(
                            "unknown block kind {} (is its extension enabled?)",
                            b.kind
                        ),
                    });
                }
                continue;
            };
            for p in ports.inputs.iter().filter(|p| p.required) {
                if !sub
                    .wires
                    .iter()
                    .any(|w| w.to_block == b.id && w.to_port == p.name)
                {
                    issues.push(Issue {
                        about: about.clone(),
                        message: format!("input {} of {} is not connected", p.name, b.id),
                    });
                }
            }
            for p in ports.connections.iter().filter(|p| p.required) {
                if !sub.wires.iter().any(|w| {
                    (w.from_block == b.id && w.from_port == p.name)
                        || (w.to_block == b.id && w.to_port == p.name)
                }) {
                    issues.push(Issue {
                        about: about.clone(),
                        message: format!("pin {} of {} is not connected", p.name, b.id),
                    });
                }
            }
            channel_issues(b, &ports, &sub.wires, &about, &mut issues);
        }
        // A required boundary port must also be fed from inside: an
        // output that nothing inside drives, a pin nothing inside
        // joins.
        for p in sub.outputs.iter().filter(|p| p.required) {
            let id = format!("{BOUNDARY}{}", p.name);
            if !sub
                .wires
                .iter()
                .any(|w| w.to_block == id && w.to_port == p.name)
            {
                issues.push(Issue {
                    about: sub.id.clone(),
                    message: format!(
                        "boundary output {} of {} is not fed from inside",
                        p.name, sub.id
                    ),
                });
            }
        }
        for p in sub.connections.iter().filter(|p| p.required) {
            let id = format!("{BOUNDARY}{}", p.name);
            if !sub.wires.iter().any(|w| {
                (w.from_block == id && w.from_port == p.name)
                    || (w.to_block == id && w.to_port == p.name)
            }) {
                issues.push(Issue {
                    about: sub.id.clone(),
                    message: format!(
                        "boundary pin {} of {} is not connected inside",
                        p.name, sub.id
                    ),
                });
            }
        }
        // A scope's dataflow is checked exactly like the root's:
        // cycles through pins are legal equations (ADR-0020).
        let inner_pin_wires: std::collections::HashSet<&str> = sub
            .wires
            .iter()
            .filter(|w| {
                inner_ports(flow, sub, libraries, &w.from_block)
                    .is_some_and(|k| k.connections.iter().any(|p| p.name == w.from_port))
                    && inner_ports(flow, sub, libraries, &w.to_block)
                        .is_some_and(|k| k.connections.iter().any(|p| p.name == w.to_port))
            })
            .map(|w| w.id.as_str())
            .collect();
        if let Err(e) = topological_over(&sub.blocks, &sub.wires, &|w| {
            inner_pin_wires.contains(w.id.as_str())
        }) {
            issues.push(Issue {
                about: sub.id.clone(),
                message: format!("{subid}: {e}", subid = sub.id),
            });
        }
    }
    // Subflows nest, but a subflow may not reference itself — directly
    // or through another (flattening recurses; a cycle would never
    // end). Reported once per subflow on a cycle.
    for start in &flow.subflows {
        let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
        let mut stack = vec![start.id.as_str()];
        while let Some(id) = stack.pop() {
            if !seen.insert(id) {
                continue;
            }
            for next in flow
                .subflow(id)
                .into_iter()
                .flat_map(|s| s.blocks.iter())
                .filter_map(|b| b.subflow.as_deref())
            {
                if next == start.id.as_str() {
                    issues.push(Issue {
                        about: start.id.clone(),
                        message: format!("subflow {} references itself through subflows", start.id),
                    });
                    stack.clear();
                    break;
                }
                stack.push(next);
            }
            if stack.is_empty() {
                break;
            }
        }
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lib() -> FlowLibrary {
        let port = |n: &str, ty: PortType, required: bool| Port {
            name: n.into(),
            ty,
            required,
        };
        FlowLibrary {
            id: "t",
            name: "Test",
            language: "none",
            codegen: None,
            blocks: vec![
                BlockKind {
                    id: "src".into(),
                    name: "Src".into(),
                    category: "io".into(),
                    description: String::new(),
                    inputs: vec![],
                    outputs: vec![port(
                        "out",
                        PortType::tensor(vec![Some(28), Some(28), Some(1)]),
                        false,
                    )],
                    connections: vec![],
                    params: vec![],
                    role: None,
                    symbol: None,
                },
                BlockKind {
                    id: "dense".into(),
                    name: "Dense".into(),
                    category: "layer".into(),
                    description: String::new(),
                    inputs: vec![port("in", PortType::tensor(vec![None]), true)],
                    outputs: vec![port("out", PortType::tensor(vec![None]), false)],
                    connections: vec![],
                    params: vec![],
                    role: None,
                    symbol: None,
                },
                BlockKind {
                    id: "flat".into(),
                    name: "Flatten".into(),
                    category: "layer".into(),
                    description: String::new(),
                    inputs: vec![port("in", PortType::tensor(vec![]), true)],
                    outputs: vec![port("out", PortType::tensor(vec![None]), false)],
                    connections: vec![],
                    params: vec![],
                    role: None,
                    symbol: None,
                },
            ],
        }
    }

    fn wire(id: &str, a: &str, b: &str) -> Wire {
        Wire {
            id: id.into(),
            from_block: a.into(),
            from_port: "out".into(),
            to_block: b.into(),
            to_port: "in".into(),
        }
    }

    #[test]
    fn unification() {
        let a = PortType::tensor(vec![Some(28), None]);
        let b = PortType::tensor(vec![None, Some(3)]);
        assert_eq!(a.unify(&b), Some(PortType::tensor(vec![Some(28), Some(3)])));
        assert_eq!(a.unify(&PortType::tensor(vec![Some(1)])), None);
        assert_eq!(
            PortType::Any.unify(&PortType::Scalar),
            Some(PortType::Scalar)
        );
        assert_eq!(PortType::tensor(vec![]).unify(&a), Some(a.clone()));
    }

    #[test]
    fn validation_finds_mismatch_missing_and_cycles() {
        let libs = vec![lib()];
        let mut f = Flow::new();
        f.blocks = vec![
            Block {
                id: "s".into(),
                kind: "t/src".into(),
                ..Default::default()
            },
            Block {
                id: "d".into(),
                kind: "t/dense".into(),
                ..Default::default()
            },
            Block {
                id: "fl".into(),
                kind: "t/flat".into(),
                ..Default::default()
            },
        ];
        // src (rank 3) → dense (rank 1): mismatch; flatten unconnected: missing.
        f.wires = vec![wire("w1", "s", "d")];
        let issues = validate(&f, &libs);
        assert!(issues
            .iter()
            .any(|i| i.about == "w1" && i.message.contains("does not fit")));
        assert!(issues
            .iter()
            .any(|i| i.about == "fl" && i.message.contains("not connected")));
        // src → flatten → dense: fine.
        f.wires = vec![wire("w1", "s", "fl"), wire("w2", "fl", "d")];
        assert!(validate(&f, &libs).is_empty(), "{:?}", validate(&f, &libs));
        let order: Vec<&str> = f
            .topological()
            .unwrap()
            .iter()
            .map(|b| b.id.as_str())
            .collect();
        assert_eq!(order, ["s", "fl", "d"]);
        f.wires.push(wire("w3", "d", "fl"));
        assert!(validate(&f, &libs)
            .iter()
            .any(|i| i.message.contains("cycle")));
        let json = f.to_json();
        assert_eq!(Flow::parse(&json).unwrap(), f);
    }

    /// A pin library (ADR-0020): two acausal components whose `connect`
    /// ports share a domain, plus one of another domain.
    fn pin_lib() -> FlowLibrary {
        let pin = |n: &str, domain: &str, required: bool| Port {
            name: n.into(),
            ty: PortType::Named {
                name: domain.into(),
            },
            required,
        };
        FlowLibrary {
            id: "m",
            name: "MTK",
            language: "none",
            codegen: None,
            blocks: vec![
                BlockKind {
                    id: "resistor".into(),
                    name: "Resistor".into(),
                    category: "electrical".into(),
                    description: String::new(),
                    inputs: vec![],
                    outputs: vec![],
                    connections: vec![pin("p", "electrical", true), pin("n", "electrical", true)],
                    params: vec![],
                    role: None,
                    symbol: None,
                },
                BlockKind {
                    id: "ground".into(),
                    name: "Ground".into(),
                    category: "electrical".into(),
                    description: String::new(),
                    inputs: vec![],
                    outputs: vec![],
                    connections: vec![pin("g", "electrical", false)],
                    params: vec![],
                    role: None,
                    symbol: None,
                },
                BlockKind {
                    id: "mass".into(),
                    name: "Mass".into(),
                    category: "mechanical".into(),
                    description: String::new(),
                    inputs: vec![],
                    outputs: vec![],
                    connections: vec![pin("flange", "mechanical", false)],
                    params: vec![],
                    role: None,
                    symbol: None,
                },
            ],
        }
    }

    #[test]
    fn pin_wires_connect_in_either_order_and_refuse_other_domains() {
        let libs = vec![pin_lib()];
        let mut f = Flow::new();
        f.blocks = vec![
            Block {
                id: "r".into(),
                kind: "m/resistor".into(),
                ..Default::default()
            },
            Block {
                id: "g".into(),
                kind: "m/ground".into(),
                ..Default::default()
            },
        ];
        // Dragged r.p → g.g: an equation, accepted either way round.
        f.wires = vec![Wire {
            id: "w1".into(),
            from_block: "r".into(),
            from_port: "p".into(),
            to_block: "g".into(),
            to_port: "g".into(),
        }];
        let issues = validate(&f, &libs);
        assert!(
            issues
                .iter()
                .all(|i| !i.message.contains("does not connect")
                    && !i.message.contains("port that does not exist")),
            "the pin wire is an equation: {issues:?}"
        );
        // The same wire dragged the other way: also fine.
        f.wires = vec![Wire {
            id: "w1".into(),
            from_block: "g".into(),
            from_port: "g".into(),
            to_block: "r".into(),
            to_port: "n".into(),
        }];
        let issues = validate(&f, &libs);
        assert!(
            !issues.iter().any(|i| i.message.contains("does not connect")
                || i.message.contains("port that does not exist")),
            "either drag order connects: {issues:?}"
        );
        // A mechanical flange onto an electrical pin: refused.
        f.blocks.push(Block {
            id: "m".into(),
            kind: "m/mass".into(),
            ..Default::default()
        });
        f.wires = vec![Wire {
            id: "w2".into(),
            from_block: "m".into(),
            from_port: "flange".into(),
            to_block: "r".into(),
            to_port: "p".into(),
        }];
        assert!(
            validate(&f, &libs)
                .iter()
                .any(|i| i.about == "w2" && i.message.contains("does not connect")),
            "a mechanical flange does not connect to an electrical pin"
        );
    }

    #[test]
    fn pin_cycles_are_legal_and_required_pins_must_wire() {
        let libs = vec![pin_lib()];
        let mut f = Flow::new();
        f.blocks = vec![
            Block {
                id: "r1".into(),
                kind: "m/resistor".into(),
                ..Default::default()
            },
            Block {
                id: "r2".into(),
                kind: "m/resistor".into(),
                ..Default::default()
            },
        ];
        // A loop of equations — legal (an algebraic loop is a model, not
        // a wiring mistake), and the unwired required pins are issues.
        f.wires = vec![
            Wire {
                id: "w1".into(),
                from_block: "r1".into(),
                from_port: "p".into(),
                to_block: "r2".into(),
                to_port: "p".into(),
            },
            Wire {
                id: "w2".into(),
                from_block: "r2".into(),
                from_port: "n".into(),
                to_block: "r1".into(),
                to_port: "n".into(),
            },
        ];
        let issues = validate(&f, &libs);
        assert!(
            !issues.iter().any(|i| i.message.contains("cycle")),
            "a cycle through pins is not a dataflow cycle: {issues:?}"
        );
        assert_eq!(
            issues
                .iter()
                .filter(|i| i.message.contains("not connected"))
                .count(),
            0,
            "both pins of both resistors are wired: {issues:?}"
        );
        // Wire one required pin the other way round: satisfied too.
        f.wires = vec![Wire {
            id: "w1".into(),
            from_block: "r2".into(),
            from_port: "p".into(),
            to_block: "r1".into(),
            to_port: "p".into(),
        }];
        let issues = validate(&f, &libs);
        assert_eq!(
            issues
                .iter()
                .filter(|i| i.message.contains("not connected"))
                .count(),
            2,
            "r2's pins and r1's n are still open: {issues:?}"
        );
        // topological() itself still counts every wire — the Lux codegen
        // walks it — while topological_skipping frees the loop.
        f.wires = vec![
            Wire {
                id: "w1".into(),
                from_block: "r1".into(),
                from_port: "p".into(),
                to_block: "r2".into(),
                to_port: "p".into(),
            },
            Wire {
                id: "w2".into(),
                from_block: "r2".into(),
                from_port: "n".into(),
                to_block: "r1".into(),
                to_port: "n".into(),
            },
        ];
        assert!(f.clone().topological().is_err());
        let skip = |w: &Wire| w.id == "w1";
        assert!(f.topological_skipping(&skip).is_ok());
    }

    #[test]
    fn a_kind_that_reuses_a_port_name_is_reported_once() {
        let mut libs = vec![pin_lib()];
        libs[0].blocks.push(BlockKind {
            id: "dup".into(),
            name: "Dup".into(),
            category: "electrical".into(),
            description: String::new(),
            inputs: vec![Port {
                name: "p".into(),
                ty: PortType::Scalar,
                required: false,
            }],
            outputs: vec![],
            connections: vec![Port {
                name: "p".into(),
                ty: PortType::Named {
                    name: "electrical".into(),
                },
                required: false,
            }],
            params: vec![],
            role: None,
            symbol: None,
        });
        let mut f = Flow::new();
        f.blocks = vec![
            Block {
                id: "d1".into(),
                kind: "m/dup".into(),
                ..Default::default()
            },
            Block {
                id: "d2".into(),
                kind: "m/dup".into(),
                ..Default::default()
            },
        ];
        let issues = validate(&f, &libs);
        assert_eq!(
            issues
                .iter()
                .filter(|i| i.message.contains("reuses a port name"))
                .count(),
            1,
            "one issue per kind, not per block: {issues:?}"
        );
        assert!(
            issues.iter().any(|i| i.about == "d1"),
            "reported on the first block of the kind: {issues:?}"
        );
    }

    #[test]
    fn a_mixed_wire_is_told_what_pairs_with_what() {
        let mut libs = vec![pin_lib()];
        let mut f = Flow::new();
        f.blocks = vec![
            Block {
                id: "m".into(),
                kind: "m/mass".into(),
                ..Default::default()
            },
            Block {
                id: "r".into(),
                kind: "m/resistor".into(),
                ..Default::default()
            },
        ];
        // mass's flange (a pin) onto resistor's p (also a pin) but of
        // another domain: refused as a domain mismatch. The mixed wire —
        // an output against a pin — names its rule instead.
        f.wires = vec![Wire {
            id: "w1".into(),
            from_block: "m".into(),
            from_port: "flange".into(),
            to_block: "r".into(),
            to_port: "p".into(),
        }];
        let issues = validate(&f, &libs);
        assert!(
            issues
                .iter()
                .any(|i| i.about == "w1" && i.message.contains("does not connect")),
            "the domain mismatch keeps its message: {issues:?}"
        );
        libs[0].blocks.push(BlockKind {
            id: "src".into(),
            name: "Source".into(),
            category: "io".into(),
            description: String::new(),
            inputs: vec![],
            outputs: vec![Port {
                name: "out".into(),
                ty: PortType::Scalar,
                required: false,
            }],
            connections: vec![],
            params: vec![],
            role: None,
            symbol: None,
        });
        f.blocks.push(Block {
            id: "s".into(),
            kind: "m/src".into(),
            ..Default::default()
        });
        f.wires = vec![Wire {
            id: "w2".into(),
            from_block: "s".into(),
            from_port: "out".into(),
            to_block: "r".into(),
            to_port: "p".into(),
        }];
        let issues = validate(&f, &libs);
        assert!(
            issues.iter().any(|i| i.about == "w2"
                && i.message == "an output connects to an input, a pin to a pin"),
            "the mixed wire says which port pairs with which: {issues:?}"
        );
    }

    // ---- ADR-0021: subflows ---------------------------------------------

    /// A subflow with a declared boundary: an input, an output and an
    /// electrical pin — the shapes a ModelingToolkit subsystem carries.
    fn sub_amp() -> Subflow {
        let port = |n: &str, ty: PortType, required: bool| Port {
            name: n.into(),
            ty,
            required,
        };
        Subflow {
            id: "amp".into(),
            name: "Amplifier".into(),
            blocks: vec![],
            wires: vec![],
            inputs: vec![port("x", PortType::Any, true)],
            outputs: vec![port("y", PortType::Any, false)],
            connections: vec![port(
                "p",
                PortType::Named {
                    name: "electrical".into(),
                },
                false,
            )],
        }
    }

    fn sub_block(id: &str, sub: &str) -> Block {
        Block {
            id: id.into(),
            kind: "subflow".into(),
            subflow: Some(sub.into()),
            ..Default::default()
        }
    }

    #[test]
    fn a_subflow_block_resolves_its_declared_boundary_ports() {
        let mut f = Flow::new();
        f.subflows = vec![sub_amp()];
        f.blocks = vec![sub_block("sub_1", "amp")];
        // The boundary ports are the parent block's ports (ADR-0021 §2).
        let ports = f
            .ports_of(&[], f.block("sub_1").unwrap())
            .expect("a subflow block resolves without any library");
        assert_eq!(ports.name, "Amplifier");
        assert_eq!(
            ports
                .inputs
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>(),
            ["x"]
        );
        assert_eq!(
            ports
                .outputs
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>(),
            ["y"]
        );
        assert_eq!(
            ports
                .connections
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>(),
            ["p"]
        );
        assert!(ports.params.is_empty());
        // Every pre-subflow file parses unchanged, and the new field
        // round-trips through the file's own JSON.
        let plain = Flow::parse(r#"{"version":1,"blocks":[],"wires":[]}"#).unwrap();
        assert_eq!(plain, Flow::new());
        let json = f.to_json();
        assert_eq!(Flow::parse(&json).unwrap(), f);
        // A reference to a subflow the document does not hold is an
        // issue, not a panic.
        f.subflows.clear();
        let issues = validate(&f, &[]);
        assert!(
            issues
                .iter()
                .any(|i| i.about == "sub_1"
                    && i.message.contains("which this document does not hold")),
            "{issues:?}"
        );
    }

    #[test]
    fn boundary_wiring_validates_like_any_block() {
        let libs = vec![pin_lib()];
        let mut f = Flow::new();
        f.subflows = vec![sub_amp()];
        f.blocks = vec![
            sub_block("sub_1", "amp"),
            Block {
                id: "r".into(),
                kind: "m/resistor".into(),
                ..Default::default()
            },
        ];
        // A pin wire onto the subsystem's declared pin: an equation,
        // either end first — the domain unifies through the boundary.
        f.wires = vec![Wire {
            id: "w1".into(),
            from_block: "sub_1".into(),
            from_port: "p".into(),
            to_block: "r".into(),
            to_port: "p".into(),
        }];
        let issues = validate(&f, &libs);
        assert!(
            !issues
                .iter()
                .any(|i| i.message.contains("does not connect")),
            "a declared pin connects: {issues:?}"
        );
        // A declared input is required exactly like a kind's.
        assert!(
            issues
                .iter()
                .any(|i| i.about == "sub_1" && i.message.contains("input x of sub_1")),
            "{issues:?}"
        );
        // A wrong domain through the boundary: refused.
        f.blocks.push(Block {
            id: "mm".into(),
            kind: "m/mass".into(),
            ..Default::default()
        });
        f.wires = vec![Wire {
            id: "w3".into(),
            from_block: "mm".into(),
            from_port: "flange".into(),
            to_block: "sub_1".into(),
            to_port: "p".into(),
        }];
        assert!(
            validate(&f, &libs)
                .iter()
                .any(|i| i.about == "w3" && i.message.contains("does not connect")),
            "a mechanical flange does not connect through the boundary"
        );
    }

    #[test]
    fn boundary_dataflow_wires_type_check() {
        let libs = vec![lib()];
        let mut f = Flow::new();
        f.subflows = vec![Subflow {
            id: "s1".into(),
            name: "Stage".into(),
            inputs: vec![Port {
                name: "x".into(),
                ty: PortType::tensor(vec![Some(1)]),
                required: false,
            }],
            ..Default::default()
        }];
        f.blocks = vec![
            sub_block("stage", "s1"),
            Block {
                id: "src".into(),
                kind: "t/src".into(),
                ..Default::default()
            },
            Block {
                id: "fl".into(),
                kind: "t/flat".into(),
                ..Default::default()
            },
        ];
        // src (rank 3) into the subsystem's rank-1 input: does not fit.
        f.wires = vec![Wire {
            id: "w1".into(),
            from_block: "src".into(),
            from_port: "out".into(),
            to_block: "stage".into(),
            to_port: "x".into(),
        }];
        assert!(
            validate(&f, &libs)
                .iter()
                .any(|i| i.about == "w1" && i.message.contains("does not fit")),
            "the boundary type-checks like a kind's input"
        );
        // flat (rank 1, dimension unknown) into it: fits.
        f.wires = vec![Wire {
            id: "w2".into(),
            from_block: "fl".into(),
            from_port: "out".into(),
            to_block: "stage".into(),
            to_port: "x".into(),
        }];
        assert!(
            !validate(&f, &libs)
                .iter()
                .any(|i| i.message.contains("does not fit")),
            "an unknown dimension unifies through the boundary"
        );
    }

    #[test]
    fn scopes_follow_the_stack_and_dangling_ids_name_no_scope() {
        let mut f = Flow::new();
        f.subflows = vec![
            sub_amp(),
            Subflow {
                id: "outer".into(),
                name: "Outer".into(),
                blocks: vec![sub_block("inner_ref", "amp")],
                ..Default::default()
            },
        ];
        f.blocks = vec![sub_block("sub_1", "amp"), sub_block("o_1", "outer")];
        // The root scope is the flow's own lists.
        assert_eq!(f.scope(&[]).unwrap().blocks.len(), 2);
        // Entering follows the references: o_1 -> outer, its inner_ref
        // -> amp.
        assert!(f.scope(&["outer".into()]).is_some());
        let inner = f.scope(&["outer".into(), "amp".into()]).unwrap();
        assert!(inner.blocks.is_empty(), "amp holds nothing yet");
        // A dangling id — referenced by no block of the scope it is
        // "entered" from — names no scope.
        assert!(f.scope(&["amp".into(), "outer".into()]).is_none());
        // with_scope writes the addressed scope and nothing else.
        f.with_scope(&["outer".into(), "amp".into()], |blocks, wires| {
            blocks.push(Block {
                id: "deep".into(),
                kind: "t/dense".into(),
                ..Default::default()
            });
            wires.push(Wire {
                id: "w9".into(),
                from_block: "boundary/x".into(),
                from_port: "x".into(),
                to_block: "deep".into(),
                to_port: "in".into(),
            });
        })
        .expect("the nested scope resolves");
        assert_eq!(f.subflow("amp").unwrap().blocks.len(), 1);
        assert_eq!(
            f.scope(&[]).unwrap().blocks.len(),
            2,
            "the root is untouched"
        );
        assert_eq!(
            f.scope(&["outer".into()]).unwrap().blocks.len(),
            1,
            "the middle scope is untouched"
        );
        // A dangling path writes nothing and answers None.
        assert!(f
            .with_scope(&["amp".into(), "outer".into()], |_, _| ())
            .is_none());
    }

    #[test]
    fn validation_looks_inside_subflows() {
        let libs = vec![lib()];
        let mut f = Flow::new();
        let mut amp = sub_amp();
        // Inside: a dense fed by the boundary input x, feeding the
        // boundary output y. The boundary pseudo-blocks are the
        // declared ports seen from within (ADR-0021 §3).
        amp.blocks = vec![Block {
            id: "d1".into(),
            kind: "t/dense".into(),
            ..Default::default()
        }];
        amp.wires = vec![
            Wire {
                id: "w1".into(),
                from_block: "boundary/x".into(),
                from_port: "x".into(),
                to_block: "d1".into(),
                to_port: "in".into(),
            },
            Wire {
                id: "w2".into(),
                from_block: "d1".into(),
                from_port: "out".into(),
                to_block: "boundary/y".into(),
                to_port: "y".into(),
            },
        ];
        f.subflows = vec![amp];
        f.blocks = vec![sub_block("sub_1", "amp")];
        let issues = validate(&f, &libs);
        // The inner wiring is sound: no issue about amp's inside.
        assert!(
            !issues.iter().any(|i| i.about.starts_with("amp/")
                && (i.message.contains("does not fit")
                    || i.message.contains("port that does not exist"))),
            "the boundary wires type-check: {issues:?}"
        );
        // The subflow block's required input x is unwired at the
        // parent — the root-level check still fires.
        assert!(
            issues
                .iter()
                .any(|i| i.about == "sub_1" && i.message.contains("input x")),
            "{issues:?}"
        );
        // A mismatch through the boundary is an inner issue, about
        // "amp/w3": the declared input is any-rank, a rank-2 tensor
        // does not fit the dense's rank-1 input.
        f.subflows[0].inputs[0].ty = PortType::tensor(vec![Some(1), Some(2)]);
        f.subflows[0].wires.push(Wire {
            id: "w3".into(),
            from_block: "boundary/x".into(),
            from_port: "x".into(),
            to_block: "d1".into(),
            to_port: "in".into(),
        });
        let issues = validate(&f, &libs);
        assert!(
            issues
                .iter()
                .any(|i| i.about == "amp/w3" && i.message.contains("does not fit")),
            "an inner mismatch is about the subflow's wire: {issues:?}"
        );
        // A required boundary output nobody inside feeds.
        f.subflows[0].outputs[0].required = true;
        f.subflows[0].wires.retain(|w| w.id != "w2");
        let issues = validate(&f, &libs);
        assert!(
            issues
                .iter()
                .any(|i| i.about == "amp" && i.message.contains("not fed from inside")),
            "{issues:?}"
        );
        // An inner dataflow cycle is the subflow's issue; a pin loop
        // inside would be legal, but this one is dataflow.
        f.subflows[0].wires.push(Wire {
            id: "w4".into(),
            from_block: "d1".into(),
            from_port: "out".into(),
            to_block: "d1".into(),
            to_port: "in".into(),
        });
        let issues = validate(&f, &libs);
        assert!(
            issues
                .iter()
                .any(|i| i.about == "amp" && i.message.contains("cycle")),
            "{issues:?}"
        );
        // Duplicate subflow ids mangle the scope paths: an issue.
        f.subflows.push(f.subflows[0].clone());
        let issues = validate(&f, &libs);
        assert!(
            issues.iter().any(|i| i.message.contains("is not unique")),
            "{issues:?}"
        );
    }

    #[test]
    fn subflow_port_names_are_unique_and_nesting_is_acyclic() {
        let mut f = Flow::new();
        f.subflows = vec![Subflow {
            id: "dup".into(),
            name: "Dup".into(),
            inputs: vec![Port {
                name: "x".into(),
                ty: PortType::Any,
                required: false,
            }],
            outputs: vec![Port {
                name: "x".into(),
                ty: PortType::Any,
                required: false,
            }],
            ..Default::default()
        }];
        f.blocks = vec![sub_block("d1", "dup"), sub_block("d2", "dup")];
        let issues = validate(&f, &[]);
        assert_eq!(
            issues
                .iter()
                .filter(|i| i.message.contains("reuses a port name"))
                .count(),
            1,
            "reported once per subflow, on the first block referencing it: {issues:?}"
        );
        assert_eq!(issues[0].about, "d1");
        // A subflow may nest, but not into itself — directly or through
        // another (flattening recurses).
        f.subflows = vec![sub_amp()];
        f.subflows[0].blocks = vec![sub_block("inner", "amp")];
        f.blocks = vec![sub_block("a", "amp")];
        let issues = validate(&f, &[]);
        assert!(
            issues
                .iter()
                .any(|i| i.about == "amp" && i.message.contains("references itself")),
            "a self-referencing subflow is an issue: {issues:?}"
        );
    }

    // ---- ADR-0022: the bipartite rule -------------------------------

    /// A factor-graph library (ADR-0022): a variable with one pin of any
    /// degree, a Normal factor with three channels, and a role-less
    /// component from outside the rule.
    fn factor_lib() -> FlowLibrary {
        let pin = |n: &str| Port {
            name: n.into(),
            ty: PortType::Any,
            required: false,
        };
        FlowLibrary {
            id: "rx",
            name: "RxInfer",
            language: "none",
            codegen: None,
            blocks: vec![
                BlockKind {
                    id: "variable".into(),
                    name: "Variable".into(),
                    connections: vec![pin("x")],
                    role: Some(Role::Variable),
                    ..Default::default()
                },
                BlockKind {
                    id: "normal".into(),
                    name: "Normal".into(),
                    connections: vec![pin("out"), pin("mean"), pin("var")],
                    role: Some(Role::Factor),
                    ..Default::default()
                },
                BlockKind {
                    id: "plain".into(),
                    name: "Plain".into(),
                    connections: vec![pin("p")],
                    ..Default::default()
                },
            ],
        }
    }

    fn rx_flow(blocks: &[(&str, &str)], wires: &[(&str, &str, &str, &str, &str)]) -> Flow {
        let mut f = Flow::new();
        f.blocks = blocks
            .iter()
            .map(|(id, kind)| Block {
                id: (*id).into(),
                kind: format!("rx/{kind}"),
                ..Default::default()
            })
            .collect();
        f.wires = wires
            .iter()
            .map(|(id, a, ap, b, bp)| Wire {
                id: (*id).into(),
                from_block: (*a).into(),
                from_port: (*ap).into(),
                to_block: (*b).into(),
                to_port: (*bp).into(),
            })
            .collect();
        f
    }

    #[test]
    fn a_factor_graph_joins_a_variable_and_a_factor_never_two_of_a_kind() {
        let libs = vec![factor_lib()];
        let blocks = [
            ("x", "variable"),
            ("y", "variable"),
            ("f", "normal"),
            ("g", "normal"),
        ];
        // Variable – factor, in either drag order: clean.
        let f = rx_flow(
            &blocks,
            &[("w1", "x", "x", "f", "mean"), ("w2", "f", "out", "y", "x")],
        );
        assert_eq!(
            validate(&f, &libs),
            vec![],
            "variable–factor wires are clean"
        );
        // Factor – factor and variable – variable: both refused, on the wire.
        let f = rx_flow(
            &blocks,
            &[("w1", "f", "out", "g", "mean"), ("w2", "x", "x", "y", "x")],
        );
        let issues = validate(&f, &libs);
        assert!(
            issues.iter().any(|i| i.about == "w1"
                && i.message
                    .contains("factors connect to variables, not to factors")),
            "{issues:?}"
        );
        assert!(
            issues.iter().any(|i| i.about == "w2"
                && i.message
                    .contains("variables connect to factors, not to variables")),
            "{issues:?}"
        );
        // A block without a role is outside the rule.
        let mut f = rx_flow(&blocks, &[("w1", "f", "out", "p", "p")]);
        f.blocks.push(Block {
            id: "p".into(),
            kind: "rx/plain".into(),
            ..Default::default()
        });
        assert_eq!(
            validate(&f, &libs),
            vec![],
            "a role-less end is outside the rule"
        );
    }

    #[test]
    fn a_channel_holds_one_variable_and_a_variable_any_number_of_factors() {
        let libs = vec![factor_lib()];
        let blocks = [
            ("x", "variable"),
            ("y", "variable"),
            ("f", "normal"),
            ("g", "normal"),
        ];
        // x feeds two factors' means and is g's output too: degree 3, fine.
        let f = rx_flow(
            &blocks,
            &[
                ("w1", "x", "x", "f", "mean"),
                ("w2", "x", "x", "g", "mean"),
                ("w3", "g", "out", "x", "x"),
            ],
        );
        assert_eq!(validate(&f, &libs), vec![], "a variable is a copier");
        // f.out on two variables: the channel is a port, not a bus.
        let f = rx_flow(
            &blocks,
            &[("w1", "f", "out", "x", "x"), ("w2", "f", "out", "y", "x")],
        );
        let issues = validate(&f, &libs);
        assert!(
            issues
                .iter()
                .any(|i| i.about == "f" && i.message.contains("channel out of f carries 2 wires")),
            "{issues:?}"
        );
        // The panel's refusal says why before the second wire exists.
        let fk = f.ports_of(&libs, f.block("f").unwrap()).unwrap();
        let xk = f.ports_of(&libs, f.block("x").unwrap()).unwrap();
        assert!(channel_refusal(&fk, "f", "out", 1).is_some());
        assert!(channel_refusal(&fk, "f", "out", 0).is_none());
        assert!(
            channel_refusal(&xk, "x", "x", 5).is_none(),
            "variables fan out"
        );
    }

    #[test]
    fn the_bipartite_rule_holds_inside_a_subflow() {
        let libs = vec![factor_lib()];
        let mut f = rx_flow(&[("sub-1", "variable")], &[]);
        f.blocks[0].kind = "subflow".into();
        f.blocks[0].subflow = Some("s".into());
        let inner = rx_flow(
            &[
                ("f", "normal"),
                ("g", "normal"),
                ("x", "variable"),
                ("y", "variable"),
            ],
            &[
                ("iw1", "f", "out", "g", "mean"),
                ("iw2", "f", "mean", "x", "x"),
                ("iw3", "f", "mean", "y", "x"),
            ],
        );
        f.subflows = vec![Subflow {
            id: "s".into(),
            name: "S".into(),
            blocks: inner.blocks,
            wires: inner.wires,
            ..Default::default()
        }];
        let issues = validate(&f, &libs);
        assert!(
            issues
                .iter()
                .any(|i| i.about == "s/iw1" && i.message.contains("not to factors")),
            "{issues:?}"
        );
        assert!(
            issues
                .iter()
                .any(|i| i.about == "s/f" && i.message.contains("channel mean of f carries 2")),
            "{issues:?}"
        );
    }
}
