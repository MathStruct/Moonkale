//! The type and plate rules of one scope (ADR-0032 §2–4): a block's
//! pin refined by its `space`/`unit` parameters, unit variables bound
//! per block, a variable's spaces met across all its wires, and a
//! factor inside every plate of the variables it touches. `validate`
//! reports them; the wire gesture asks [`wire_refusal`], so both say
//! the same.

use super::units::{unit_term, Unit, UnitTerm};
use super::Block;
use super::{
    inner_ports, BlockPorts, Flow, FlowLibrary, Issue, ParamKind, Plate, Port, PortType, Role,
    Space, Subflow, Wire, BOUNDARY,
};
use std::collections::{BTreeMap, BTreeSet};

/// One scope as the rules see it: the root's or a subflow's lists, and
/// how an id resolves to ports there.
#[derive(Clone, Copy)]
pub(super) struct View<'a> {
    pub flow: &'a Flow,
    pub sub: Option<&'a Subflow>,
    pub libs: &'a [FlowLibrary],
    pub blocks: &'a [Block],
    pub wires: &'a [Wire],
    pub plates: &'a [Plate],
}

impl<'a> View<'a> {
    /// The root scope.
    pub fn root(flow: &'a Flow, libs: &'a [FlowLibrary]) -> Self {
        View {
            flow,
            sub: None,
            libs,
            blocks: &flow.blocks,
            wires: &flow.wires,
            plates: &flow.plates,
        }
    }

    /// A subflow's scope.
    pub fn inside(flow: &'a Flow, sub: &'a Subflow, libs: &'a [FlowLibrary]) -> Self {
        View {
            flow,
            sub: Some(sub),
            libs,
            blocks: &sub.blocks,
            wires: &sub.wires,
            plates: &sub.plates,
        }
    }

    /// The scope a path addresses (ADR-0021 §4).
    fn at(flow: &'a Flow, path: &[String], libs: &'a [FlowLibrary]) -> Option<Self> {
        flow.scope(path)?;
        Some(match path.last() {
            None => View::root(flow, libs),
            Some(id) => View::inside(flow, flow.subflow(id)?, libs),
        })
    }

    fn ports(&self, id: &str) -> Option<BlockPorts<'a>> {
        match self.sub {
            Some(sub) => inner_ports(self.flow, sub, self.libs, id),
            None => self
                .blocks
                .iter()
                .find(|b| b.id == id)
                .and_then(|b| self.flow.ports_of(self.libs, b)),
        }
    }

    fn block(&self, id: &str) -> Option<&'a Block> {
        self.blocks.iter().find(|b| b.id == id)
    }

    fn about(&self, x: &str) -> String {
        match self.sub {
            Some(s) => format!("{}/{x}", s.id),
            None => x.to_string(),
        }
    }
}

/// A block's plates, innermost first (ADR-0032 §1). A dangling or
/// cyclic parent ends the chain (`validate` reports both).
pub fn plates_of<'p>(plates: &'p [Plate], block: &Block) -> Vec<&'p Plate> {
    let mut out: Vec<&Plate> = Vec::new();
    let mut next = block.plate.as_deref();
    while let Some(id) = next {
        let Some(p) = plates.iter().find(|p| p.id == id) else {
            break;
        };
        if out.iter().any(|q| q.id == p.id) {
            break;
        }
        out.push(p);
        next = p.parent.as_deref();
    }
    out
}

/// The type of pin `port` of the block `id`, refined by the block's
/// `space`/`unit` parameters (ADR-0032 §4). A boundary pseudo-block has
/// no parameters: its declared type is its type.
fn refined(block: Option<&Block>, ports: &BlockPorts, port: &Port) -> Result<PortType, String> {
    let mut ty = port.ty.clone();
    let Some(b) = block else {
        return Ok(ty);
    };
    let value = |name: &str, default: &str| -> String {
        b.params
            .get(name)
            .map(String::as_str)
            .unwrap_or(default)
            .trim()
            .to_string()
    };
    for p in ports.params {
        if let ParamKind::Space { pin } = &p.kind {
            if pin != &port.name {
                continue;
            }
            let v = value(&p.name, &p.default);
            if v.is_empty() {
                continue;
            }
            let space: Space = v.parse().map_err(|e| format!("{}: {e}", b.id))?;
            ty = match ty {
                PortType::Any | PortType::Scalar => PortType::value(space),
                other => other.unify_or_why(&PortType::value(space)).map_err(|why| {
                    format!(
                        "{}: the space {space} does not fit {}: {why}",
                        b.id, port.name
                    )
                })?,
            };
        }
    }
    for p in ports.params {
        if let ParamKind::Unit { pin } = &p.kind {
            if pin != &port.name {
                continue;
            }
            let v = value(&p.name, &p.default);
            if v.is_empty() {
                continue;
            }
            Unit::parse(&v).map_err(|e| format!("{}: {e}", b.id))?;
            ty = match ty {
                PortType::Any | PortType::Scalar => PortType::value_in(Space::Real, &v),
                PortType::Value { space, unit: None } => PortType::value_in(space, &v),
                PortType::Value {
                    space,
                    unit: Some(u),
                } => {
                    if let Some(why) = super::units::unit_refusal(&u, &v) {
                        return Err(format!("{}: {why}", b.id));
                    }
                    PortType::value_in(space, &v)
                }
                other => {
                    return Err(format!(
                        "{}: {} ({}) takes no unit",
                        b.id,
                        port.name,
                        other.describe()
                    ))
                }
            };
        }
    }
    Ok(ty)
}

/// The type with its unit dropped (units are checked over nets first).
fn without_unit(t: &PortType) -> PortType {
    match t {
        PortType::Value { space, .. } => PortType::value(*space),
        other => other.clone(),
    }
}

/// A pin end in the scope.
struct PinEnd<'a> {
    id: &'a str,
    /// How messages name it: a variable by its own name (`y`, as the
    /// canvas shows it), anything else as `block.pin`.
    shown: String,
    ty: PortType,
    role: Option<Role>,
}

fn find(parent: &mut [usize], i: usize) -> usize {
    let mut r = i;
    while parent[r] != r {
        r = parent[r];
    }
    let mut x = i;
    while parent[x] != r {
        let n = parent[x];
        parent[x] = r;
        x = n;
    }
    r
}

/// The issues of one scope's types and plates. `about` is the block or
/// wire id, prefixed with the subflow's id inside one.
pub(super) fn scope_issues<'a>(v: View<'a>) -> Vec<Issue> {
    let mut issues: Vec<Issue> = Vec::new();
    let mut said: BTreeSet<String> = BTreeSet::new();
    let mut push = |about: String, message: String, issues: &mut Vec<Issue>| {
        if said.insert(message.clone()) {
            issues.push(Issue { about, message });
        }
    };

    // ---- plates (ADR-0032 §1) ------------------------------------------
    let mut ids: BTreeSet<&str> = BTreeSet::new();
    for p in v.plates {
        if !ids.insert(p.id.as_str()) {
            push(
                v.about(&p.id),
                format!("plate id {} is not unique", p.id),
                &mut issues,
            );
        }
        let ident = p
            .index
            .chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_')
            && p.index.chars().all(|c| c.is_alphanumeric() || c == '_')
            && p.index.chars().any(char::is_alphanumeric)
            && p.index.chars().count() <= 32;
        if !ident {
            push(
                v.about(&p.id),
                format!(
                    "plate {}: its index {:?} is not a name (i, t, layer)",
                    p.id, p.index
                ),
                &mut issues,
            );
        }
        if let Err(e) = p.size() {
            push(v.about(&p.id), e, &mut issues);
        }
        if let Some(parent) = &p.parent {
            if !v.plates.iter().any(|q| &q.id == parent) {
                push(
                    v.about(&p.id),
                    format!(
                        "plate {} nests in {parent}, which this scope does not hold",
                        p.id
                    ),
                    &mut issues,
                );
            }
        }
        // A parent chain that comes back is a cycle.
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        let mut next = Some(p.id.as_str());
        while let Some(id) = next {
            if !seen.insert(id) {
                push(
                    v.about(&p.id),
                    format!("plate {} nests in itself", p.id),
                    &mut issues,
                );
                break;
            }
            next = v
                .plates
                .iter()
                .find(|q| q.id == id)
                .and_then(|q| q.parent.as_deref());
        }
    }
    for b in v.blocks {
        if let Some(pid) = &b.plate {
            if !v.plates.iter().any(|p| &p.id == pid) {
                push(
                    v.about(&b.id),
                    format!(
                        "{} is in the plate {pid}, which this scope does not hold",
                        b.id
                    ),
                    &mut issues,
                );
            }
        }
    }

    // ---- refined pin types ---------------------------------------------
    for b in v.blocks {
        let Some(ports) = v.ports(&b.id) else {
            continue;
        };
        // Each parameter on its own, so a bad space and a bad unit are
        // both reported.
        for p in ports.params {
            let text = b.params.get(&p.name).unwrap_or(&p.default).trim();
            let bad = match &p.kind {
                ParamKind::Space { .. } if !text.is_empty() => text.parse::<Space>().err(),
                ParamKind::Unit { .. } if !text.is_empty() => Unit::parse(text).err(),
                _ => None,
            };
            if let Some(e) = bad {
                push(v.about(&b.id), format!("{}: {e}", b.id), &mut issues);
            }
        }
        for port in ports.connections {
            if let Err(e) = refined(Some(b), &ports, port) {
                push(v.about(&b.id), e, &mut issues);
            }
        }
    }

    // The pin ends, and which wires join them into nets.
    let mut ends: Vec<PinEnd> = Vec::new();
    let mut index: BTreeMap<(&str, &str), usize> = BTreeMap::new();
    let mut pin_wires: Vec<(&Wire, usize, usize)> = Vec::new();
    // A wire whose kinds' own types already refuse is `wire_issue`'s.
    let mut raw_refused: BTreeSet<usize> = BTreeSet::new();
    for w in v.wires {
        let (Some(fp), Some(tp)) = (v.ports(&w.from_block), v.ports(&w.to_block)) else {
            continue;
        };
        let (Some(a), Some(b)) = (
            fp.connections.iter().find(|p| p.name == w.from_port),
            tp.connections.iter().find(|p| p.name == w.to_port),
        ) else {
            continue;
        };
        let ia = intern(&mut ends, &mut index, &v, &w.from_block, &fp, a);
        let ib = intern(&mut ends, &mut index, &v, &w.to_block, &tp, b);
        let (Some(ia), Some(ib)) = (ia, ib) else {
            continue;
        };
        if a.ty.unify(&b.ty).or_else(|| b.ty.unify(&a.ty)).is_none() {
            raw_refused.insert(ia);
            raw_refused.insert(ib);
        }
        pin_wires.push((w, ia, ib));

        // A factor is in every plate of the variables it touches
        // (ADR-0032 §2).
        if let Some(why) = plate_rule(&v, &fp, &w.from_block, &tp, &w.to_block) {
            push(v.about(&w.id), why, &mut issues);
        }
    }
    let mut parent: Vec<usize> = (0..ends.len()).collect();
    for (_, a, b) in &pin_wires {
        let (ra, rb) = (find(&mut parent, *a), find(&mut parent, *b));
        if ra != rb {
            parent[ra] = rb;
        }
    }
    let nets: Vec<usize> = (0..ends.len()).map(|i| find(&mut parent, i)).collect();
    // A net is named after its variable (the junction), else its first end.
    let net_about = |n: usize| -> String {
        let i = (0..ends.len())
            .find(|i| nets[*i] == n && ends[*i].role == Some(Role::Variable))
            .or_else(|| (0..ends.len()).find(|i| nets[*i] == n))
            .unwrap_or(0);
        v.about(ends.get(i).map(|e| e.id).unwrap_or(""))
    };

    // ---- units: unit variables bound per block, one unit per net -------
    let terms: Vec<Option<(UnitTerm, String)>> = ends
        .iter()
        .map(|e| match &e.ty {
            PortType::Value { unit: Some(u), .. } => match unit_term(u) {
                Ok(t) => Some((t, u.clone())),
                Err(why) => {
                    push(v.about(e.id), format!("{}: {why}", e.shown), &mut issues);
                    None
                }
            },
            _ => None,
        })
        .collect();
    let show =
        |u: &Unit, text: Option<&str>| text.map(String::from).unwrap_or_else(|| u.to_string());
    // (net) → (unit, its text, which end set it); (block, var) → (unit, text, end).
    let mut net_unit: BTreeMap<usize, (Unit, String, String)> = BTreeMap::new();
    let mut bound: BTreeMap<(&str, String), (Unit, String, String)> = BTreeMap::new();
    let mut broken: BTreeSet<usize> = BTreeSet::new();
    for _ in 0..=ends.len() + 1 {
        let mut changed = false;
        for (i, e) in ends.iter().enumerate() {
            let n = nets[i];
            if broken.contains(&n) {
                continue;
            }
            let Some((term, text)) = &terms[i] else {
                continue;
            };
            let here = e.shown.clone();
            let (u, utext, why) = match term {
                UnitTerm::Fixed(u) => (*u, text.clone(), here.clone()),
                UnitTerm::Var(name, k) => match bound.get(&(e.id, name.clone())) {
                    Some((b, btext, from)) => {
                        let t = if *k == 1 {
                            btext.clone()
                        } else {
                            format!("({btext})^{k}")
                        };
                        let Some(u) = b.pow(*k) else {
                            broken.insert(n);
                            push(
                                net_about(n),
                                format!("{here}: {btext} to the power {k} is too large a unit"),
                                &mut issues,
                            );
                            continue;
                        };
                        (u, t, format!("{here} (like {from})"))
                    }
                    None => {
                        if let Some((nu, ntext, _)) = net_unit.get(&n) {
                            match nu.root(*k) {
                                Some(b) => {
                                    let btext = if *k == 1 {
                                        ntext.clone()
                                    } else {
                                        show(&b, None)
                                    };
                                    bound.insert((e.id, name.clone()), (b, btext, here));
                                    changed = true;
                                }
                                None => {
                                    broken.insert(n);
                                    push(
                                        net_about(n),
                                        format!("{here} is in a unit to the power {k}, but {ntext} is not one"),
                                        &mut issues,
                                    );
                                }
                            }
                        }
                        continue;
                    }
                },
            };
            match net_unit.get(&n) {
                None => {
                    net_unit.insert(n, (u, utext, why));
                    changed = true;
                }
                Some((nu, ntext, nwhy)) => {
                    if !nu.same(&u) {
                        broken.insert(n);
                        let reason = if nu.same_dimension(&u) {
                            "the same dimension at another scale — convert explicitly"
                        } else {
                            "different dimensions"
                        };
                        push(
                            net_about(n),
                            format!("units disagree: {nwhy} is in {ntext}, but {why} is in {utext} ({reason})"),
                            &mut issues,
                        );
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }

    // ---- spaces: one space per net -------------------------------------
    let mut done: BTreeSet<usize> = BTreeSet::new();
    for i in 0..ends.len() {
        let n = nets[i];
        if !done.insert(n) || (0..ends.len()).any(|j| nets[j] == n && raw_refused.contains(&j)) {
            continue;
        }
        let members: Vec<usize> = (0..ends.len()).filter(|j| nets[*j] == n).collect();
        let mut acc = PortType::Any;
        let mut by: Option<usize> = None;
        for j in members {
            let t = without_unit(&ends[j].ty);
            match acc.unify_or_why(&t) {
                Ok(next) => {
                    if next != acc {
                        by = Some(j);
                    }
                    acc = next;
                }
                Err(why) => {
                    let e = &ends[j];
                    let msg = match by {
                        Some(k) => format!(
                            "{} ({}) does not connect to {} ({}): {why}",
                            e.shown,
                            t.describe(),
                            ends[k].shown,
                            acc.describe()
                        ),
                        None => format!("{}: {why}", e.shown),
                    };
                    push(net_about(n), msg, &mut issues);
                    break;
                }
            }
        }
    }
    issues
}

fn intern<'a>(
    ends: &mut Vec<PinEnd<'a>>,
    index: &mut BTreeMap<(&'a str, &'a str), usize>,
    v: &View<'a>,
    id: &'a str,
    ports: &BlockPorts,
    port: &'a Port,
) -> Option<usize> {
    if let Some(i) = index.get(&(id, port.name.as_str())) {
        return Some(*i);
    }
    // A refinement error is reported per block; the kind's own type
    // stands in so the rest still checks.
    let ty = refined(v.block(id), ports, port).unwrap_or_else(|_| port.ty.clone());
    let i = ends.len();
    ends.push(PinEnd {
        id,
        shown: match ports.role {
            Some(Role::Variable) => shown(v, id),
            _ => format!("{id}.{}", port.name),
        },
        ty,
        role: ports.role,
    });
    index.insert((id, port.name.as_str()), i);
    Some(i)
}

/// Why a pin wire between `a` and `b` breaks the plate rule (ADR-0032
/// §2): the factor end must be in every plate of the variable end. A
/// boundary pseudo-block is outside every plate of its scope.
fn plate_rule(v: &View, fa: &BlockPorts, a: &str, fb: &BlockPorts, b: &str) -> Option<String> {
    let (var, fac) = match (fa.role, fb.role) {
        (Some(Role::Variable), Some(Role::Factor)) => (a, b),
        (Some(Role::Factor), Some(Role::Variable)) => (b, a),
        _ => return None,
    };
    let plates = |id: &str| -> Vec<&Plate> {
        if id.starts_with(BOUNDARY) {
            return Vec::new();
        }
        v.block(id)
            .map(|b| plates_of(v.plates, b))
            .unwrap_or_default()
    };
    let (vp, fp) = (plates(var), plates(fac));
    let missing = vp.iter().find(|p| !fp.iter().any(|q| q.id == p.id))?;
    Some(format!(
        "{} is one per {i}; {fac} is outside plate {i} and cannot take it — move the factor into the plate",
        shown(v, var),
        i = missing.index
    ))
}

/// A variable as messages name it: its `name` parameter when it has one
/// (the canvas shows that), else its id; a boundary pin by its name.
fn shown(v: &View, id: &str) -> String {
    if let Some(pin) = id.strip_prefix(BOUNDARY) {
        return pin.to_string();
    }
    v.block(id)
        .and_then(|b| b.params.get("name"))
        .map(|n| n.trim())
        .filter(|n| !n.is_empty())
        .unwrap_or(id)
        .to_string()
}

/// The scope's type and plate issues (ADR-0032), as `validate` reports
/// them for that scope. `None` when the path names no scope.
pub fn type_issues(flow: &Flow, path: &[String], libs: &[FlowLibrary]) -> Option<Vec<Issue>> {
    View::at(flow, path, libs).map(scope_issues)
}

/// Why adding `wire` to the scope at `path` breaks a type or plate rule
/// (ADR-0032): the first issue the wire adds. The flow panel's wire
/// gesture asks this, so its refusal and `validate` say the same.
pub fn wire_refusal(
    flow: &Flow,
    path: &[String],
    libs: &[FlowLibrary],
    wire: &Wire,
) -> Option<String> {
    let before = View::at(flow, path, libs)?;
    // The kinds' own types first: `validate` reports their mismatch
    // through `wire_issue`, which the scope pass leaves to it (review,
    // Codex #4).
    if let (Some(fp), Some(tp)) = (before.ports(&wire.from_block), before.ports(&wire.to_block)) {
        let a = fp.connections.iter().find(|p| p.name == wire.from_port);
        let b = tp.connections.iter().find(|p| p.name == wire.to_port);
        if let (Some(a), Some(b)) = (a, b) {
            if let Some(why) = super::bipartite_refusal(&fp, &tp) {
                return Some(why);
            }
            if let Err(why) = a.ty.unify_or_why(&b.ty) {
                return Some(format!(
                    "{}.{} ({}) does not connect to {}.{} ({}): {why}",
                    wire.from_block,
                    wire.from_port,
                    a.ty.describe(),
                    wire.to_block,
                    wire.to_port,
                    b.ty.describe()
                ));
            }
        }
    }
    let had: BTreeSet<String> = scope_issues(before)
        .into_iter()
        .map(|i| i.message)
        .collect();
    let mut wires = before.wires.to_vec();
    wires.push(wire.clone());
    let after = View {
        wires: &wires,
        ..before
    };
    scope_issues(after)
        .into_iter()
        .find(|i| !had.contains(&i.message))
        .map(|i| i.message)
}

#[cfg(test)]
mod tests {
    use super::super::{validate, BlockKind, Param, Wire};
    use super::*;

    fn pin(n: &str, ty: PortType) -> Port {
        Port {
            name: n.into(),
            ty,
            required: false,
        }
    }

    fn val(space: &str, unit: Option<&str>) -> PortType {
        PortType::Value {
            space: space.parse().unwrap(),
            unit: unit.map(String::from),
        }
    }

    fn factor(id: &str, pins: &[(&str, PortType)]) -> BlockKind {
        BlockKind {
            id: id.into(),
            name: id.into(),
            connections: pins.iter().map(|(n, t)| pin(n, t.clone())).collect(),
            role: Some(Role::Factor),
            ..Default::default()
        }
    }

    fn lib() -> FlowLibrary {
        let p = |name: &str, kind: ParamKind| Param {
            name: name.into(),
            kind,
            default: String::new(),
        };
        FlowLibrary {
            id: "rx",
            name: "rx",
            blocks: vec![
                BlockKind {
                    id: "variable".into(),
                    name: "variable".into(),
                    connections: vec![pin("x", PortType::Any)],
                    params: vec![
                        p("space", ParamKind::Space { pin: "x".into() }),
                        p("unit", ParamKind::Unit { pin: "x".into() }),
                    ],
                    role: Some(Role::Variable),
                    ..Default::default()
                },
                factor(
                    "normal",
                    &[
                        ("out", val("real", Some("'u"))),
                        ("mean", val("real", Some("'u"))),
                        ("variance", val("positive", Some("'u^2"))),
                    ],
                ),
                factor(
                    "beta",
                    &[
                        ("out", val("unit interval", None)),
                        ("a", val("positive", None)),
                    ],
                ),
                factor(
                    "bernoulli",
                    &[
                        ("out", val("binary", None)),
                        ("p", val("unit interval", None)),
                    ],
                ),
            ],
            codegen: None,
            language: "julia",
        }
    }

    fn var(id: &str, space: &str, unit: &str) -> Block {
        let mut b = Block {
            id: id.into(),
            kind: "rx/variable".into(),
            ..Default::default()
        };
        if !space.is_empty() {
            b.params.insert("space".into(), space.into());
        }
        if !unit.is_empty() {
            b.params.insert("unit".into(), unit.into());
        }
        b
    }

    fn fac(id: &str, kind: &str) -> Block {
        Block {
            id: id.into(),
            kind: format!("rx/{kind}"),
            ..Default::default()
        }
    }

    fn w(id: &str, a: &str, b: &str, bp: &str) -> Wire {
        Wire {
            id: id.into(),
            from_block: a.into(),
            from_port: "x".into(),
            to_block: b.into(),
            to_port: bp.into(),
        }
    }

    fn issues(f: &Flow) -> Vec<String> {
        validate(f, &[lib()])
            .into_iter()
            .map(|i| i.message)
            .collect()
    }

    fn normal(xu: &str, yu: &str) -> Flow {
        Flow {
            blocks: vec![var("x", "", xu), var("y", "", yu), fac("n", "normal")],
            wires: vec![w("w1", "x", "n", "mean"), w("w2", "y", "n", "out")],
            ..Default::default()
        }
    }

    #[test]
    fn a_unit_mismatch_through_a_factor_is_refused() {
        assert!(
            issues(&normal("m", "m")).is_empty(),
            "{:?}",
            issues(&normal("m", "m"))
        );
        assert!(issues(&normal("m/s", "m*s^-1")).is_empty());
        assert!(issues(&normal("m", "")).is_empty());
        let bad = issues(&normal("m", "s"));
        assert_eq!(bad.len(), 1, "{bad:?}");
        assert!(
            bad[0].contains("units disagree") && bad[0].contains("different dimensions"),
            "{bad:?}"
        );
        let scale = issues(&normal("km", "m"));
        assert!(scale[0].contains("convert explicitly"), "{scale:?}");
        // The gesture says the same as validate.
        let mut f = normal("m", "s");
        let second = f.wires.pop().unwrap();
        assert!(issues(&f).is_empty());
        let why = wire_refusal(&f, &[], &[lib()], &second).unwrap();
        assert_eq!(why, bad[0]);
    }

    #[test]
    fn a_unit_variable_raised_binds_through_its_root() {
        let mut f = normal("m", "m");
        f.blocks.push(var("v", "", "m^2"));
        f.wires.push(w("w3", "v", "n", "variance"));
        assert!(issues(&f).is_empty(), "{:?}", issues(&f));
        f.blocks[3].params.insert("unit".into(), "m".into());
        assert!(issues(&f)[0].contains("units disagree"), "{:?}", issues(&f));
        // Bound from the variance alone: mean must be in m.
        let f = Flow {
            blocks: vec![var("v", "", "s^2"), var("x", "", "m"), fac("n", "normal")],
            wires: vec![w("w1", "v", "n", "variance"), w("w2", "x", "n", "mean")],
            ..Default::default()
        };
        assert!(issues(&f)[0].contains("units disagree"), "{:?}", issues(&f));
        // Not a square: refused, not panicking.
        let f = Flow {
            blocks: vec![var("v", "", "m"), var("x", "", ""), fac("n", "normal")],
            wires: vec![w("w1", "v", "n", "variance"), w("w2", "x", "n", "mean")],
            ..Default::default()
        };
        assert!(issues(&f)[0].contains("power 2"), "{:?}", issues(&f));
    }

    #[test]
    fn spaces_meet_across_every_wire_of_a_variable() {
        // A count is not a Beta draw.
        let f = Flow {
            blocks: vec![var("t", "count", ""), fac("b", "beta")],
            wires: vec![w("w1", "t", "b", "out")],
            ..Default::default()
        };
        assert!(issues(&f)[0].contains("do not meet"), "{:?}", issues(&f));
        // Undeclared: the channels meet through it — binary ⊂ unit interval.
        let f = Flow {
            blocks: vec![var("t", "", ""), fac("b", "beta"), fac("c", "bernoulli")],
            wires: vec![w("w1", "t", "b", "out"), w("w2", "t", "c", "p")],
            ..Default::default()
        };
        assert!(issues(&f).is_empty(), "{:?}", issues(&f));
        // A probability is not a variance (no containment either way).
        let f = Flow {
            blocks: vec![var("t", "", ""), fac("b", "beta"), fac("n", "normal")],
            wires: vec![w("w1", "t", "b", "out"), w("w2", "t", "n", "variance")],
            ..Default::default()
        };
        let got = issues(&f);
        assert!(
            got[0].contains("n.variance") && got[0].contains("b.out"),
            "{got:?}"
        );
        // A bad parameter is reported on the block.
        let f = Flow {
            blocks: vec![var("t", "reals", "furlong")],
            ..Default::default()
        };
        let got = issues(&f);
        assert_eq!(got.len(), 2, "{got:?}");
    }

    fn plated() -> Flow {
        let mut y = var("y", "", "");
        y.plate = Some("p".into());
        let mut c = fac("c", "bernoulli");
        c.plate = Some("p".into());
        Flow {
            blocks: vec![var("θ", "", ""), fac("b", "beta"), y, c],
            wires: vec![
                w("w1", "θ", "b", "out"),
                w("w2", "θ", "c", "p"),
                w("w3", "y", "c", "out"),
            ],
            plates: vec![Plate {
                id: "p".into(),
                index: "i".into(),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn a_factor_is_in_every_plate_of_its_variables() {
        assert!(issues(&plated()).is_empty(), "{:?}", issues(&plated()));
        let mut f = plated();
        f.blocks[3].plate = None; // the likelihood leaves the plate
        let got = issues(&f);
        assert_eq!(got.len(), 1, "{got:?}");
        assert!(got[0].contains("y is one per i"), "{got:?}");
        let mut g = plated();
        let w3 = g.wires.pop().unwrap();
        g.blocks[3].plate = None;
        assert_eq!(
            wire_refusal(&g, &[], &[lib()], &w3).as_deref(),
            Some(got[0].as_str())
        );
    }

    #[test]
    fn plates_themselves_are_checked() {
        let mut f = plated();
        f.plates[0].size = "0".into();
        f.plates[0].index = "1i".into();
        let mut under = plated();
        under.plates[0].index = "__".into();
        assert!(issues(&under).join("\n").contains("not a name"));
        f.plates.push(Plate {
            id: "q".into(),
            index: "j".into(),
            size: "3".into(),
            parent: Some("r".into()),
        });
        f.plates.push(Plate {
            id: "p".into(),
            index: "k".into(),
            ..Default::default()
        });
        f.blocks[0].plate = Some("nowhere".into());
        let got = issues(&f).join("\n");
        for needle in [
            "not a whole number",
            "not a name",
            "does not hold",
            "not unique",
            "nowhere",
        ] {
            assert!(got.contains(needle), "{needle}: {got}");
        }
        let mut f = plated();
        f.plates[0].parent = Some("p".into());
        assert!(issues(&f).join("\n").contains("nests in itself"));
        // A cycle does not hang the chain.
        assert_eq!(plates_of(&f.plates, &f.blocks[2]).len(), 1);
    }

    #[test]
    fn plates_round_trip_and_stay_out_of_plain_files() {
        let plain = normal("m", "s");
        let json = plain.to_json();
        assert!(!json.contains("plate"), "{json}");
        let f = plated();
        let back = Flow::parse(&f.to_json()).unwrap();
        assert_eq!(back, f);
        assert_eq!(f.plates[0].label(), "i = 1…n");
        let mut g = f.clone();
        g.plates[0].size = "32".into();
        assert_eq!(g.plates[0].label(), "i = 1…32");
    }

    #[test]
    fn inside_a_subflow_the_rules_hold_too() {
        let mut f = plated();
        let sub = Subflow {
            id: "s".into(),
            name: "s".into(),
            blocks: std::mem::take(&mut f.blocks),
            wires: std::mem::take(&mut f.wires),
            plates: std::mem::take(&mut f.plates),
            ..Default::default()
        };
        f.subflows.push(sub);
        f.blocks.push(Block {
            id: "inst".into(),
            kind: "subflow".into(),
            subflow: Some("s".into()),
            ..Default::default()
        });
        assert!(issues(&f).is_empty(), "{:?}", issues(&f));
        f.subflows[0].blocks[3].plate = None;
        let got = validate(&f, &[lib()]);
        assert!(got.iter().any(|i| i.about == "s/w3"), "{got:?}");
        assert!(type_issues(&f, &["s".into()], &[lib()]).unwrap().len() == 1);
    }

    #[test]
    fn wire_refusal_also_refuses_what_the_kinds_types_refuse() {
        // Review (Codex) #4: a variable kind declaring `positive` against
        // a `unit interval` channel — validate reports it through
        // `wire_issue`, so the gesture must refuse it too.
        let mut l = lib();
        l.blocks[0].connections[0].ty = val("positive", None);
        let f = Flow {
            blocks: vec![var("t", "", ""), fac("c", "bernoulli")],
            ..Default::default()
        };
        let w = w("w", "t", "c", "p");
        let mut g = f.clone();
        g.wires.push(w.clone());
        let reported: Vec<String> = validate(&g, std::slice::from_ref(&l))
            .into_iter()
            .map(|i| i.message)
            .collect();
        assert!(
            reported.iter().any(|m| m.contains("does not connect")),
            "{reported:?}"
        );
        let why = wire_refusal(&f, &[], std::slice::from_ref(&l), &w).unwrap();
        assert!(why.contains("do not meet"), "{why}");
    }
}
