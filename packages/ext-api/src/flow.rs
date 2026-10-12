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
        let mut indeg: BTreeMap<&str, usize> =
            self.blocks.iter().map(|b| (b.id.as_str(), 0)).collect();
        for w in &self.wires {
            if skip(w) {
                continue;
            }
            if let Some(d) = indeg.get_mut(w.to_block.as_str()) {
                *d += 1;
            }
        }
        let mut ready: Vec<&Block> = self
            .blocks
            .iter()
            .filter(|b| indeg.get(b.id.as_str()) == Some(&0))
            .collect();
        let mut out = Vec::new();
        while let Some(b) = ready.pop() {
            out.push(b);
            for w in self
                .wires
                .iter()
                .filter(|w| w.from_block == b.id && !skip(w))
            {
                if let Some(d) = indeg.get_mut(w.to_block.as_str()) {
                    *d -= 1;
                    if *d == 0 {
                        if let Some(n) = self.block(&w.to_block) {
                            ready.push(n);
                        }
                    }
                }
            }
        }
        if out.len() != self.blocks.len() {
            let stuck = self
                .blocks
                .iter()
                .find(|b| !out.iter().any(|o| o.id == b.id))
                .map(|b| b.id.clone())
                .unwrap_or_default();
            return Err(format!("cycle through block {stuck}"));
        }
        Ok(out)
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

/// One problem with a flow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Issue {
    /// Block id (or wire id) the issue is about.
    pub about: String,
    /// What is wrong.
    pub message: String,
}

/// Type-check every wire, check required inputs and cycles.
pub fn validate(flow: &Flow, libraries: &[FlowLibrary]) -> Vec<Issue> {
    let mut issues = Vec::new();
    for b in &flow.blocks {
        if find_kind(libraries, &b.kind).is_none() {
            issues.push(Issue {
                about: b.id.clone(),
                message: format!("unknown block kind {} (is its extension enabled?)", b.kind),
            });
        }
    }
    for w in &flow.wires {
        let from = flow
            .block(&w.from_block)
            .and_then(|b| find_kind(libraries, &b.kind));
        let to = flow
            .block(&w.to_block)
            .and_then(|b| find_kind(libraries, &b.kind));
        let (Some((_, fk)), Some((_, tk))) = (from, to) else {
            continue;
        };
        let out = fk.outputs.iter().find(|p| p.name == w.from_port);
        let inp = tk.inputs.iter().find(|p| p.name == w.to_port);
        let pin_f = fk.connections.iter().find(|p| p.name == w.from_port);
        let pin_t = tk.connections.iter().find(|p| p.name == w.to_port);
        // A wire is dataflow when it leaves an output and enters an input;
        // an equation when both ends are pins (ADR-0020) — either drag
        // order, and its types must unify symmetrically. A port that
        // exists but cannot pair is a mixed wire, not a missing one.
        if let (Some(o), Some(i)) = (out, inp) {
            if o.ty.unify(&i.ty).is_none() {
                issues.push(Issue {
                    about: w.id.clone(),
                    message: format!(
                        "{}.{} ({}) does not fit {}.{} ({})",
                        w.from_block,
                        w.from_port,
                        o.ty.describe(),
                        w.to_block,
                        w.to_port,
                        i.ty.describe()
                    ),
                });
            }
        } else if let (Some(a), Some(b)) = (pin_f, pin_t) {
            if a.ty.unify(&b.ty).or_else(|| b.ty.unify(&a.ty)).is_none() {
                issues.push(Issue {
                    about: w.id.clone(),
                    message: format!(
                        "{}.{} ({}) does not connect to {}.{} ({})",
                        w.from_block,
                        w.from_port,
                        a.ty.describe(),
                        w.to_block,
                        w.to_port,
                        b.ty.describe()
                    ),
                });
            }
        } else if out.is_some() || inp.is_some() || pin_f.is_some() || pin_t.is_some() {
            issues.push(Issue {
                about: w.id.clone(),
                message: "an output connects to an input, a pin to a pin".into(),
            });
        } else {
            issues.push(Issue {
                about: w.id.clone(),
                message: "wire references a port that does not exist".into(),
            });
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
    for b in &flow.blocks {
        if let Some((_, k)) = find_kind(libraries, &b.kind) {
            if duplicated_kinds.contains(&b.kind) && reported_kinds.insert(b.kind.clone()) {
                issues.push(Issue {
                    about: b.id.clone(),
                    message: format!("{} reuses a port name", b.kind),
                });
            }
            for p in k.inputs.iter().filter(|p| p.required) {
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
            for p in k.connections.iter().filter(|p| p.required) {
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
        }
    }
    // Cycles are a dataflow problem; equations may loop (ADR-0020).
    let pin_wires: std::collections::HashSet<&str> = flow
        .wires
        .iter()
        .filter(|w| {
            flow.block(&w.from_block)
                .and_then(|b| find_kind(libraries, &b.kind))
                .map(|(_, k)| k.connections.iter().any(|p| p.name == w.from_port))
                .unwrap_or(false)
                && flow
                    .block(&w.to_block)
                    .and_then(|b| find_kind(libraries, &b.kind))
                    .map(|(_, k)| k.connections.iter().any(|p| p.name == w.to_port))
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
                x: 0.0,
                y: 0.0,
                params: Default::default(),
            },
            Block {
                id: "g".into(),
                kind: "m/ground".into(),
                x: 0.0,
                y: 0.0,
                params: Default::default(),
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
            x: 0.0,
            y: 0.0,
            params: Default::default(),
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
                x: 0.0,
                y: 0.0,
                params: Default::default(),
            },
            Block {
                id: "r2".into(),
                kind: "m/resistor".into(),
                x: 0.0,
                y: 0.0,
                params: Default::default(),
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
        });
        let mut f = Flow::new();
        f.blocks = vec![
            Block {
                id: "d1".into(),
                kind: "m/dup".into(),
                x: 0.0,
                y: 0.0,
                params: Default::default(),
            },
            Block {
                id: "d2".into(),
                kind: "m/dup".into(),
                x: 0.0,
                y: 0.0,
                params: Default::default(),
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
                x: 0.0,
                y: 0.0,
                params: Default::default(),
            },
            Block {
                id: "r".into(),
                kind: "m/resistor".into(),
                x: 0.0,
                y: 0.0,
                params: Default::default(),
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
        });
        f.blocks.push(Block {
            id: "s".into(),
            kind: "m/src".into(),
            x: 0.0,
            y: 0.0,
            params: Default::default(),
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
}
