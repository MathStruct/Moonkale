//! Flow schema and model (Milestone 6): the contract between the flow
//! editor (`editors/flow`) and the extensions that contribute block
//! libraries (e.g. the optional Lux.jl extension). Dioxus-free; stored as
//! JSON in `*.flow.json` files.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A tensor shape with unknown dimensions allowed (`None`).
pub type Shape = Vec<Option<u64>>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PortType {
    /// Connects to anything.
    Any,
    Scalar,
    /// A tensor whose shape may be partially known (batch dim excluded).
    Tensor { shape: Shape },
    /// Opaque, must match by name (e.g. `"optimiser"`, `"loss"`).
    Named { name: String },
}

impl PortType {
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Port {
    pub name: String,
    pub ty: PortType,
    /// Inputs that must be wired for the flow to be valid.
    #[serde(default)]
    pub required: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ParamKind {
    Int,
    Float,
    Text,
    Bool,
    Choice { options: Vec<String> },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Param {
    pub name: String,
    pub kind: ParamKind,
    pub default: String,
}

/// A block type a library offers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BlockKind {
    /// Unique within the library (`"dense"`).
    pub id: String,
    pub name: String,
    pub category: String,
    pub description: String,
    pub inputs: Vec<Port>,
    pub outputs: Vec<Port>,
    pub params: Vec<Param>,
}

/// Generated code for a flow: file name + text.
pub type Codegen = fn(&Flow, &FlowLibrary) -> Result<Generated, String>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Generated {
    pub file_name: String,
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
    pub name: &'static str,
    pub blocks: Vec<BlockKind>,
    pub codegen: Option<Codegen>,
    pub language: &'static str,
}

impl FlowLibrary {
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

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Block {
    pub id: String,
    /// `"<library>/<kind>"`.
    pub kind: String,
    pub x: f64,
    pub y: f64,
    #[serde(default)]
    pub params: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Wire {
    pub id: String,
    pub from_block: String,
    pub from_port: String,
    pub to_block: String,
    pub to_port: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Flow {
    pub version: u32,
    pub blocks: Vec<Block>,
    pub wires: Vec<Wire>,
}

impl Flow {
    pub const VERSION: u32 = 1;
    pub const EXTENSION: &'static str = ".flow.json";

    pub fn parse(json: &str) -> Result<Self, String> {
        if json.trim().is_empty() {
            return Ok(Self::new());
        }
        serde_json::from_str(json).map_err(|e| e.to_string())
    }

    pub fn new() -> Self {
        Self {
            version: Self::VERSION,
            ..Default::default()
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".into())
    }

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
        let mut indeg: BTreeMap<&str, usize> = self.blocks.iter().map(|b| (b.id.as_str(), 0)).collect();
        for w in &self.wires {
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
            for w in self.wires.iter().filter(|w| w.from_block == b.id) {
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

pub fn find_kind<'a>(libraries: &'a [FlowLibrary], kind: &str) -> Option<(&'a FlowLibrary, &'a BlockKind)> {
    let (lib, id) = kind.split_once('/')?;
    let library = libraries.iter().find(|l| l.id == lib)?;
    library.block(id).map(|k| (library, k))
}

/// One problem with a flow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Issue {
    /// Block id (or wire id) the issue is about.
    pub about: String,
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
        let from = flow.block(&w.from_block).and_then(|b| find_kind(libraries, &b.kind));
        let to = flow.block(&w.to_block).and_then(|b| find_kind(libraries, &b.kind));
        let (Some((_, fk)), Some((_, tk))) = (from, to) else { continue };
        let out = fk.outputs.iter().find(|p| p.name == w.from_port);
        let inp = tk.inputs.iter().find(|p| p.name == w.to_port);
        match (out, inp) {
            (Some(o), Some(i)) => {
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
            }
            _ => issues.push(Issue {
                about: w.id.clone(),
                message: "wire references a port that does not exist".into(),
            }),
        }
    }
    for b in &flow.blocks {
        if let Some((_, k)) = find_kind(libraries, &b.kind) {
            for p in k.inputs.iter().filter(|p| p.required) {
                if !flow.wires.iter().any(|w| w.to_block == b.id && w.to_port == p.name) {
                    issues.push(Issue {
                        about: b.id.clone(),
                        message: format!("input {} of {} is not connected", p.name, b.id),
                    });
                }
            }
        }
    }
    if let Err(e) = flow.topological() {
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
        let port = |n: &str, ty: PortType, required: bool| Port { name: n.into(), ty, required };
        FlowLibrary {
            id: "t",
            name: "Test",
            language: "none",
            codegen: None,
            blocks: vec![
                BlockKind { id: "src".into(), name: "Src".into(), category: "io".into(), description: String::new(),
                    inputs: vec![], outputs: vec![port("out", PortType::tensor(vec![Some(28), Some(28), Some(1)]), false)], params: vec![] },
                BlockKind { id: "dense".into(), name: "Dense".into(), category: "layer".into(), description: String::new(),
                    inputs: vec![port("in", PortType::tensor(vec![None]), true)], outputs: vec![port("out", PortType::tensor(vec![None]), false)], params: vec![] },
                BlockKind { id: "flat".into(), name: "Flatten".into(), category: "layer".into(), description: String::new(),
                    inputs: vec![port("in", PortType::tensor(vec![]), true)], outputs: vec![port("out", PortType::tensor(vec![None]), false)], params: vec![] },
            ],
        }
    }

    fn wire(id: &str, a: &str, b: &str) -> Wire {
        Wire { id: id.into(), from_block: a.into(), from_port: "out".into(), to_block: b.into(), to_port: "in".into() }
    }

    #[test]
    fn unification() {
        let a = PortType::tensor(vec![Some(28), None]);
        let b = PortType::tensor(vec![None, Some(3)]);
        assert_eq!(a.unify(&b), Some(PortType::tensor(vec![Some(28), Some(3)])));
        assert_eq!(a.unify(&PortType::tensor(vec![Some(1)])), None);
        assert_eq!(PortType::Any.unify(&PortType::Scalar), Some(PortType::Scalar));
        assert_eq!(PortType::tensor(vec![]).unify(&a), Some(a.clone()));
    }

    #[test]
    fn validation_finds_mismatch_missing_and_cycles() {
        let libs = vec![lib()];
        let mut f = Flow::new();
        f.blocks = vec![
            Block { id: "s".into(), kind: "t/src".into(), ..Default::default() },
            Block { id: "d".into(), kind: "t/dense".into(), ..Default::default() },
            Block { id: "fl".into(), kind: "t/flat".into(), ..Default::default() },
        ];
        // src (rank 3) → dense (rank 1): mismatch; flatten unconnected: missing.
        f.wires = vec![wire("w1", "s", "d")];
        let issues = validate(&f, &libs);
        assert!(issues.iter().any(|i| i.about == "w1" && i.message.contains("does not fit")));
        assert!(issues.iter().any(|i| i.about == "fl" && i.message.contains("not connected")));
        // src → flatten → dense: fine.
        f.wires = vec![wire("w1", "s", "fl"), wire("w2", "fl", "d")];
        assert!(validate(&f, &libs).is_empty(), "{:?}", validate(&f, &libs));
        let order: Vec<&str> = f.topological().unwrap().iter().map(|b| b.id.as_str()).collect();
        assert_eq!(order, ["s", "fl", "d"]);
        f.wires.push(wire("w3", "d", "fl"));
        assert!(validate(&f, &libs).iter().any(|i| i.message.contains("cycle")));
        let json = f.to_json();
        assert_eq!(Flow::parse(&json).unwrap(), f);
    }
}
