//! Resolve a visible cache definition without choosing an arbitrary shared body.
use moonkale_core::{Edge, EdgeKind, Node, NodeKind, Value};

pub(crate) fn index_node<'a>(
    selected: &Node,
    nodes: &'a [Node],
    edges: &[Edge],
    locations: &[Vec<Value>],
) -> Option<&'a Node> {
    let unique = |matches: Vec<&'a Node>| {
        if matches.len() == 1 {
            matches.into_iter().next()
        } else {
            None
        }
    };
    let native: Vec<_> = nodes
        .iter()
        .filter(|n| n.kind.is_definition() && n.native_key == selected.native_key)
        .collect();
    if !native.is_empty() {
        return unique(native);
    }
    // Content identities can be shared by several declarations. Do not guess.
    if locations.len() != 1 {
        return None;
    }
    let [Value::Text(path), start @ Value::Int(_), end @ Value::Int(_), input @ Value::Text(_)] =
        locations[0].as_slice()
    else {
        return None;
    };
    unique(
        nodes
            .iter()
            .filter(|n| {
                n.kind.is_definition()
                    && n.props.get("input") == Some(input)
                    && edges.iter().any(|e| {
                        e.to == n.id
                            && e.kind == EdgeKind::Defines
                            && e.props.get("start_byte") == Some(start)
                            && e.props.get("end_byte") == Some(end)
                            && nodes.iter().any(|f| {
                                f.id == e.from && f.kind == NodeKind::File && &f.native_key == path
                            })
                    })
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use moonkale_core::{NodeId, SourceId, Version};

    fn node(key: &str, kind: NodeKind) -> Node {
        let source = SourceId::new("index:test");
        Node {
            id: NodeId::derive(&source, key),
            source,
            kind,
            native_key: key.into(),
            label: key.into(),
            props: Default::default(),
            content: None,
            version: Version(0),
        }
    }

    #[test]
    fn native_key_precedes_locations_and_duplicates_are_refused() {
        let selected = node("a.rs#fn:f", NodeKind::definition());
        let nodes = vec![selected.clone()];
        assert_eq!(
            index_node(&selected, &nodes, &[], &[]).unwrap().id,
            selected.id
        );
        assert!(index_node(&selected, &[selected.clone(), selected.clone()], &[], &[]).is_none());
    }

    #[test]
    fn span_requires_exact_file_and_both_endpoints() {
        let file = node("a.rs", NodeKind::File);
        let mut target = node("a.rs#fn:f", NodeKind::definition());
        target
            .props
            .insert("input".into(), Value::Text("digest".into()));
        let selected = node("#content", NodeKind::definition());
        let edge = Edge {
            source: target.source.clone(),
            from: file.id,
            to: target.id,
            kind: EdgeKind::Defines,
            props: [
                ("start_byte".into(), Value::Int(4)),
                ("end_byte".into(), Value::Int(20)),
            ]
            .into(),
        };
        let nodes = vec![file, target.clone()];
        let location = vec![
            Value::Text("a.rs".into()),
            Value::Int(4),
            Value::Int(20),
            Value::Text("digest".into()),
        ];
        assert_eq!(
            index_node(
                &selected,
                &nodes,
                std::slice::from_ref(&edge),
                std::slice::from_ref(&location)
            )
            .unwrap()
            .id,
            target.id
        );
        for bad in [
            vec![
                Value::Text("b.rs".into()),
                Value::Int(4),
                Value::Int(20),
                Value::Text("digest".into()),
            ],
            vec![
                Value::Text("a.rs".into()),
                Value::Int(4),
                Value::Int(21),
                Value::Text("digest".into()),
            ],
        ] {
            assert!(index_node(&selected, &nodes, std::slice::from_ref(&edge), &[bad]).is_none());
        }
        let mut stale = location.clone();
        stale[3] = Value::Text("older-input".into());
        assert!(index_node(&selected, &nodes, std::slice::from_ref(&edge), &[stale]).is_none());
        assert!(index_node(&selected, &nodes, &[edge], &[location.clone(), location]).is_none());
    }
}
