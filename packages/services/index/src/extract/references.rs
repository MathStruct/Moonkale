//! Conservative syntactic code dependencies. Uses the existing parsed tree;
//! no execution, dependency loading, compiler dispatch or cross-file guesses.

use super::Derived;
use crate::graph::derived_id;
use moonkale_core::{graph::Properties, Edge, EdgeKind, Node, NodeId, NodeKind, SourceId, Value};
use std::collections::HashMap;

pub(super) fn extract(
    mut derived: Derived,
    index: &SourceId,
    file: &Node,
    provenance: &Properties,
    root: arborium_tree_sitter::Node,
    text: &str,
) -> Derived {
    let definitions: Vec<_> = derived
        .nodes
        .iter()
        .filter(|n| n.kind == NodeKind::definition())
        .cloned()
        .collect();
    let occurrences: Vec<_> = derived
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::Defines)
        .cloned()
        .collect();
    let owners: HashMap<usize, NodeId> = occurrences
        .iter()
        .filter_map(|e| match e.props.get("start_byte") {
            Some(Value::Int(start)) => Some((*start as usize, e.to)),
            _ => None,
        })
        .collect();
    // A syntax index cannot prove dispatch through local bindings. Record
    // possible shadows first; over-approximation leaves an unresolved use
    // rather than drawing a false dependency on a same-named definition.
    let mut shadows: HashMap<NodeId, std::collections::HashSet<String>> = HashMap::new();
    let mut pending = vec![(root, file.id)];
    while let Some((node, inherited)) = pending.pop() {
        let owner = owners.get(&node.start_byte()).copied().unwrap_or(inherited);
        let mut cursor = node.walk();
        pending.extend(node.named_children(&mut cursor).map(|n| (n, owner)));
        let binding = match node.kind() {
            "parameters" => Some(node),
            "argument_list"
                if node.parent().is_some_and(|call| {
                    matches!(call.kind(), "call_expression")
                        && call.parent().is_some_and(|parent| {
                            matches!(parent.kind(), "signature" | "where_expression")
                                || (parent.kind() == "assignment"
                                    && parent.named_child(0).is_some_and(|n| n.id() == call.id()))
                        })
                }) =>
            {
                Some(node)
            }
            "let_declaration" => node.child_by_field_name("pattern"),
            "assignment" => node
                .child_by_field_name("left")
                .or_else(|| node.named_child(0))
                .filter(|n| !matches!(n.kind(), "call_expression" | "where_expression")),
            _ => None,
        };
        if let Some(binding) = binding {
            let mut bindings = vec![binding];
            while let Some(binding) = bindings.pop() {
                if binding.kind() == "identifier" {
                    if let Ok(name) = binding.utf8_text(text.as_bytes()) {
                        shadows.entry(owner).or_default().insert(name.into());
                    }
                }
                let mut cursor = binding.walk();
                bindings.extend(binding.named_children(&mut cursor));
            }
        }
    }
    let mut stack = vec![(root, file.id)];
    while let Some((node, inherited)) = stack.pop() {
        let caller = owners.get(&node.start_byte()).copied().unwrap_or(inherited);
        let mut cursor = node.walk();
        stack.extend(
            node.named_children(&mut cursor)
                .map(|child| (child, caller)),
        );
        let call = matches!(node.kind(), "call_expression" | "call");
        let import = matches!(
            node.kind(),
            "use_declaration" | "import_statement" | "import_from_statement" | "using_statement"
        );
        let qualified = matches!(
            node.kind(),
            "attribute" | "field_expression" | "scoped_identifier" | "scoped_type_identifier"
        );
        if !call
            && !import
            && !qualified
            && !matches!(node.kind(), "identifier" | "type_identifier")
        {
            continue;
        }
        if !call && !import {
            let mut ancestor = node.parent();
            let mut head = false;
            while let Some(parent) = ancestor {
                if matches!(
                    parent.kind(),
                    "attribute"
                        | "field_expression"
                        | "scoped_identifier"
                        | "scoped_type_identifier"
                ) {
                    head = true;
                    break;
                }
                if matches!(
                    parent.kind(),
                    "use_declaration"
                        | "import_statement"
                        | "import_from_statement"
                        | "using_statement"
                ) {
                    head = true;
                    break;
                }
                if matches!(parent.kind(), "call_expression" | "call") {
                    if let Some(target) = parent
                        .child_by_field_name("function")
                        .or_else(|| parent.named_child(0))
                    {
                        if target.start_byte() <= node.start_byte()
                            && target.end_byte() >= node.end_byte()
                        {
                            head = true;
                            break;
                        }
                    }
                }
                ancestor = parent.parent();
            }
            if head {
                continue;
            }
        }
        if !call
            && !import
            && node.parent().is_some_and(|p| {
                p.child_by_field_name("name")
                    .is_some_and(|n| n.id() == node.id())
                    || matches!(
                        p.kind(),
                        "parameters"
                            | "parameter"
                            | "typed_parameter"
                            | "call_expression"
                            | "call"
                            | "signature"
                            | "import_statement"
                            | "use_declaration"
                            | "using_statement"
                    )
            })
        {
            continue;
        }
        let target = if call {
            node.child_by_field_name("function")
                .or_else(|| node.named_child(0))
        } else if import {
            node.named_child(0)
        } else {
            Some(node)
        };
        let Some(target) = target else { continue };
        let Ok(name) = target.utf8_text(text.as_bytes()) else {
            continue;
        };
        // Do not interpret declaration signatures as calls.
        if call
            && node.parent().is_some_and(|p| {
                matches!(p.kind(), "signature" | "where_expression")
                    || (p.kind() == "assignment"
                        && p.named_child(0).is_some_and(|n| n.id() == node.id()))
            })
        {
            continue;
        }
        let caller_key = definitions
            .iter()
            .find(|d| d.id == caller)
            .map(|d| d.native_key.as_str())
            .unwrap_or(&file.native_key);
        let candidates: Vec<_> = definitions
            .iter()
            .filter(|d| {
                !shadows
                    .get(&caller)
                    .is_some_and(|names| names.contains(name))
                    && !shadows
                        .get(&file.id)
                        .is_some_and(|names| names.contains(name))
                    && d.props.get("name") == Some(&Value::Text(name.into()))
                    && {
                        let suffix = d
                            .native_key
                            .split_once('#')
                            .map(|(_, suffix)| suffix)
                            .unwrap_or("");
                        let scope = suffix
                            .rsplit_once('/')
                            .map(|(scope, _)| format!("{}#{scope}/", file.native_key));
                        scope.is_none_or(|scope| caller_key.starts_with(&scope))
                    }
            })
            .collect();
        let kind = if import {
            EdgeKind::Custom(moonkale_core::graph::code::IMPORTS.into())
        } else if call {
            EdgeKind::Calls
        } else {
            EdgeKind::References
        };
        let mut props = provenance.clone();
        props.insert("start_byte".into(), Value::Int(target.start_byte() as i64));
        props.insert("end_byte".into(), Value::Int(target.end_byte() as i64));
        props.insert("count".into(), Value::Int(1));
        let to = if candidates.len() == 1 {
            candidates[0].id
        } else {
            let key = format!(
                "{}#unresolved:{}:{}",
                file.native_key,
                target.start_byte(),
                name
            );
            let id = derived_id(index, &key);
            derived.nodes.push(Node {
                id,
                source: index.clone(),
                kind: NodeKind::Custom(moonkale_core::graph::code::UNRESOLVED.into()),
                label: name.into(),
                native_key: key,
                content: None,
                version: file.version,
                props: props.clone(),
            });
            derived.edges.push(Edge {
                source: index.clone(),
                from: caller,
                to: id,
                kind: EdgeKind::Custom(moonkale_core::graph::code::UNRESOLVED.into()),
                props: props.clone(),
            });
            id
        };
        derived.edges.push(Edge {
            source: index.clone(),
            from: caller,
            to,
            kind: kind.clone(),
            props: props.clone(),
        });
        if call {
            derived.edges.push(Edge {
                source: index.clone(),
                from: caller,
                to,
                kind: EdgeKind::References,
                props,
            });
        }
    }
    // One edge per (from, to, kind); retain every occurrence explicitly.
    let mut grouped: HashMap<(NodeId, NodeId, EdgeKind), usize> = HashMap::new();
    let mut edges: Vec<Edge> = Vec::new();
    for mut edge in derived.edges {
        if matches!(edge.kind, EdgeKind::Calls | EdgeKind::References) {
            let range = format!("{}:{}", edge.props["start_byte"], edge.props["end_byte"]);
            let key = (edge.from, edge.to, edge.kind.clone());
            if let Some(&i) = grouped.get(&key) {
                let previous = &mut edges[i];
                if let Some(Value::Int(count)) = previous.props.get_mut("count") {
                    *count += 1;
                }
                if let Some(Value::Text(ranges)) = previous.props.get_mut("ranges") {
                    ranges.push(',');
                    ranges.push_str(&range);
                }
                continue;
            }
            edge.props.insert("ranges".into(), Value::Text(range));
            grouped.insert(key, edges.len());
        }
        edges.push(edge);
    }
    derived.edges = edges;
    derived
}
