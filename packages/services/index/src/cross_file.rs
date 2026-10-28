//! Folder-only syntactic resolution. Reconcile cached facts after each batch;
//! never reread/reparse callers or consult the filesystem/dependency loader.
use crate::extract::Derived;
use moonkale_core::{EdgeKind, Node, NodeId, NodeKind, Value};
use std::collections::{HashMap, HashSet, VecDeque};

type Exports = HashMap<(String, String, String), Vec<(NodeId, NodeId)>>;

fn text<'a>(n: &'a Node, key: &str) -> Option<&'a str> {
    match n.props.get(key) {
        Some(Value::Text(s)) => Some(s),
        _ => None,
    }
}

fn modules(key: &str) -> Option<Vec<&str>> {
    let mut parts: Vec<_> = key.split('/').collect();
    parts.pop();
    parts
        .into_iter()
        .map(|s| s.strip_prefix("mod:").or_else(|| s.strip_prefix("module:")))
        .collect()
}

/// Returns candidate projections; the publisher replaces only changed origins.
/// Base facts retain unresolved uses, including their original ranges/provenance.
/// The walk's file/byte limits bound this linear reconciliation over admitted facts.
pub(crate) fn resolve(
    files: &HashMap<NodeId, Node>,
    base: &HashMap<NodeId, Derived>,
) -> HashMap<NodeId, Derived> {
    if files.len() > crate::walk::Limits::default().max_files {
        // Incremental additions can outgrow the initial walk. Retract cross-file
        // edges rather than resolving against an arbitrary subset.
        return base.clone();
    }
    let unsupported_modules: HashMap<_, Vec<_>> = base
        .iter()
        .map(|(id, d)| {
            (
                *id,
                d.nodes
                    .iter()
                    .filter(|n| n.props.get("module_path_supported") == Some(&Value::Bool(false)))
                    .map(|n| format!("{}/", n.native_key))
                    .collect(),
            )
        })
        .collect();
    let mut rust: HashMap<NodeId, Vec<(String, String)>> = HashMap::new();
    let paths: HashMap<_, _> = files
        .values()
        .map(|f| (f.native_key.as_str(), f.id))
        .collect();
    let mut pending = VecDeque::new();
    for f in files.values().filter(|f| {
        f.native_key.ends_with("/lib.rs")
            || f.native_key.ends_with("/main.rs")
            || matches!(f.native_key.as_str(), "lib.rs" | "main.rs")
    }) {
        pending.push_back((f.id, f.native_key.clone(), String::new()));
    }
    let mut visited = HashSet::new();
    while let Some((id, root, prefix)) = pending.pop_front() {
        if !visited.insert((id, root.clone())) {
            continue;
        }
        rust.entry(id)
            .or_default()
            .push((root.clone(), prefix.clone()));
        let Some(f) = files.get(&id) else { continue };
        let dir = f.native_key.rsplit_once('/').map_or("", |(p, _)| p);
        let stem = crate::graph::stem_of(&f.native_key);
        let dir = if matches!(stem, "lib" | "main" | "mod") {
            dir.to_string()
        } else if dir.is_empty() {
            stem.into()
        } else {
            format!("{dir}/{stem}")
        };
        for n in base
            .get(&id)
            .into_iter()
            .flat_map(|d| &d.nodes)
            .filter(|n| n.props.get("external_module") == Some(&Value::Bool(true)))
        {
            if unsupported_modules[&id]
                .iter()
                .any(|prefix| n.native_key.starts_with(prefix))
            {
                continue;
            }
            let Some((_, key)) = n.native_key.split_once('#') else {
                continue;
            };
            let Some(mut scope) = modules(key) else {
                continue;
            };
            let Some(name) = text(n, "name") else {
                continue;
            };
            scope.push(name);
            let suffix = scope.join("/");
            let path = if dir.is_empty() {
                suffix
            } else {
                format!("{dir}/{suffix}")
            };
            let candidates: Vec<_> = [format!("{path}.rs"), format!("{path}/mod.rs")]
                .iter()
                .filter_map(|p| paths.get(p.as_str()).copied())
                .collect();
            if candidates.len() == 1 {
                let module = scope.join("::");
                pending.push_back((
                    candidates[0],
                    root.clone(),
                    if prefix.is_empty() {
                        module
                    } else {
                        format!("{prefix}::{module}")
                    },
                ));
            }
        }
    }
    // Two files can spell the same Python namespace (`m.py`, `m/__init__.py`),
    // or declare the same Julia module. A unique leaf name cannot disambiguate
    // its owner in either case.
    let mut module_counts = HashMap::new();
    for (origin, d) in base {
        let Some(f) = files.get(origin) else { continue };
        if f.language_hint() == Some("python") {
            let module = f
                .native_key
                .trim_end_matches(".py")
                .trim_end_matches("/__init__")
                .replace('/', ".");
            *module_counts
                .entry(("python".to_string(), module))
                .or_insert(0usize) += 1;
        }
        for n in &d.nodes {
            if text(n, "lang") != Some("julia") || text(n, "def_kind") != Some("module") {
                continue;
            }
            let Some((_, key)) = n.native_key.split_once('#') else {
                continue;
            };
            let Some(mut scope) = modules(key) else {
                continue;
            };
            let Some(name) = text(n, "name") else {
                continue;
            };
            scope.push(name);
            *module_counts
                .entry(("julia".to_string(), scope.join(".")))
                .or_insert(0usize) += 1;
        }
    }
    let mut exports = Exports::new();
    for (origin, d) in base {
        let Some(f) = files.get(origin) else { continue };
        for n in d.nodes.iter().filter(|n| n.kind == NodeKind::definition()) {
            if unsupported_modules[origin]
                .iter()
                .any(|prefix| n.native_key.starts_with(prefix))
            {
                continue;
            }
            let Some((_, key)) = n.native_key.split_once('#') else {
                continue;
            };
            let Some(scope) = modules(key) else { continue };
            let Some(name) = text(n, "name") else {
                continue;
            };
            let lang = text(n, "lang").unwrap_or("");
            let keys: Vec<_> = match lang {
                "rust" => rust
                    .get(origin)
                    .into_iter()
                    .flatten()
                    .map(|(root, module)| {
                        let mut path = vec![module.as_str()];
                        path.extend(scope.iter().copied());
                        path.push(name);
                        (
                            root.clone(),
                            path.into_iter()
                                .filter(|p| !p.is_empty())
                                .collect::<Vec<_>>()
                                .join("::"),
                        )
                    })
                    .collect(),
                "python" if scope.is_empty() => {
                    let module = f
                        .native_key
                        .strip_suffix(".py")
                        .unwrap_or("")
                        .trim_end_matches("/__init__")
                        .replace('/', ".");
                    vec![(String::new(), format!("{module}.{name}"))]
                }
                "julia" if !scope.is_empty() => {
                    let mut path = scope;
                    path.push(name);
                    vec![(String::new(), path.join("."))]
                }
                _ => Vec::new(),
            };
            for (root, path) in keys {
                exports
                    .entry((lang.into(), root, path))
                    .or_default()
                    .push((*origin, n.id));
            }
        }
    }
    let mut out = HashMap::new();
    for (origin, d) in base {
        let mut targets = HashMap::new();
        for n in &d.nodes {
            let (Some(path), Some(lang)) = (text(n, "import_target"), text(n, "lang")) else {
                continue;
            };
            let root = if lang == "rust" {
                let Some(roots) = rust.get(origin).filter(|r| r.len() == 1) else {
                    continue;
                };
                roots[0].0.as_str()
            } else {
                ""
            };
            let path = if lang == "rust" {
                path.strip_prefix("crate::").unwrap_or(path)
            } else {
                path
            };
            if lang != "rust" {
                let mut namespace = path;
                let mut ambiguous = false;
                while let Some((parent, _)) = namespace.rsplit_once('.') {
                    if module_counts
                        .get(&(lang.into(), parent.into()))
                        .is_some_and(|n| *n > 1)
                    {
                        ambiguous = true;
                        break;
                    }
                    namespace = parent;
                }
                if ambiguous {
                    continue;
                }
            }
            let Some(candidates) = exports.get(&(lang.into(), root.into(), path.into())) else {
                continue;
            };
            if candidates.len() == 1 && candidates[0].0 != *origin {
                targets.insert(n.id, candidates[0].1);
            }
        }
        let mut result = d.clone();
        result.nodes.retain(|n| !targets.contains_key(&n.id));
        result.edges.retain(|e| !(targets.contains_key(&e.to) && matches!(&e.kind, EdgeKind::Custom(k) if k == moonkale_core::graph::code::UNRESOLVED)));
        for e in &mut result.edges {
            if let Some(target) = targets.get(&e.to) {
                e.to = *target;
                e.props
                    .insert("resolved_via".into(), Value::Text("import".into()));
            }
        }
        // Qualified and bare spellings can reach the same target. Preserve
        // all occurrence ranges and contributing import statements in one edge.
        let mut grouped = HashMap::new();
        let mut edges: Vec<moonkale_core::Edge> = Vec::new();
        for mut edge in result.edges {
            if edge.props.contains_key("resolved_via") {
                let key = (edge.from, edge.to, edge.kind.clone());
                let statement = match edge.props.get("import") {
                    Some(Value::Text(s)) => s.clone(),
                    _ => String::new(),
                };
                if let Some(&i) = grouped.get(&key) {
                    let previous: &mut moonkale_core::Edge = &mut edges[i];
                    if let (Some(Value::Int(count)), Some(Value::Int(more))) =
                        (previous.props.get_mut("count"), edge.props.get("count"))
                    {
                        *count += more;
                    }
                    if let (Some(Value::Text(ranges)), Some(Value::Text(more))) =
                        (previous.props.get_mut("ranges"), edge.props.get("ranges"))
                    {
                        ranges.push(',');
                        ranges.push_str(more);
                    }
                    if let Some(Value::Text(imports)) = previous.props.get_mut("imports") {
                        let mut list: Vec<String> =
                            serde_json::from_str(imports).unwrap_or_default();
                        if !list.contains(&statement) {
                            list.push(statement);
                            list.sort();
                        }
                        *imports = serde_json::to_string(&list).unwrap();
                    }
                    continue;
                }
                edge.props.insert(
                    "imports".into(),
                    Value::Text(serde_json::to_string(&vec![statement]).unwrap()),
                );
                grouped.insert(key, edges.len());
            }
            edges.push(edge);
        }
        result.edges = edges;
        out.insert(*origin, result);
    }
    out
}
