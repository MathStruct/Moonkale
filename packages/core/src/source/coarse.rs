//! Shared hierarchy coarsening. Sources may replace this snapshot fallback with
//! indexed/cached cuts, or community detection for graphs without a hierarchy.
use super::{Query, QueryResult, Source};
use crate::{EdgeKind, NodeId, SourceError, Value};
use std::collections::{HashMap, VecDeque};

/// Hard output bound, independent of caller input.
pub const MAX_NODES: usize = 100_000;
/// The fallback refuses incomplete snapshots rather than inventing member counts.
pub const MAX_INPUT_NODES: usize = 2_000_000;
/// Bound temporary adjacency/aggregation work as well as node count.
pub const MAX_INPUT_EDGES: usize = 8_000_000;
pub const MAX_LEVEL: u32 = 64;

fn invalid() -> SourceError {
    SourceError::Invalid("invalid or oversized coarse hierarchy".into())
}

/// Default source implementation. Overrides should use source-native hierarchy
/// indexes to avoid materialising the full snapshot on each region expansion.
pub async fn query<S: Source + ?Sized>(
    source: &S,
    level: u32,
    region: Option<NodeId>,
) -> Result<QueryResult, SourceError> {
    if level > MAX_LEVEL {
        return Err(invalid());
    }
    let snapshot = match source
        .query(Query::All {
            limit: MAX_INPUT_NODES,
            kinds: None,
        })
        .await
    {
        Ok(snapshot) => snapshot,
        Err(SourceError::Unsupported(_)) => {
            // Folder sources deliberately expose no All query. Walk metadata
            // only, with separate bounds on nodes, edges and directory requests.
            let root = source.descriptor().root;
            let mut snapshot = source.query(Query::Node(root)).await?;
            let mut pending = VecDeque::from([root]);
            let mut visited = std::collections::HashSet::from([root]);
            while let Some(parent) = pending.pop_front() {
                let children = source.query(Query::Children(parent)).await?;
                if children.truncated
                    || snapshot.nodes.len() + children.nodes.len() > MAX_INPUT_NODES
                    || snapshot.edges.len() + children.edges.len() > MAX_INPUT_EDGES
                {
                    return Err(invalid());
                }
                for child in children.nodes {
                    if visited.insert(child.id) {
                        if child.kind == crate::NodeKind::Directory {
                            pending.push_back(child.id);
                        }
                        snapshot.nodes.push(child);
                    }
                }
                snapshot.edges.extend(children.edges);
            }
            snapshot
        }
        Err(error) => return Err(error),
    };
    project(snapshot, level, region)
}

/// Project a complete hierarchy forest in linear traversal time. Multiple
/// parents of equal precedence and cycles are refused: silently choosing one would miscount members.
/// Non-hierarchy edges retain kind and direction, summing their multiplicity.
pub fn project(
    snapshot: QueryResult,
    level: u32,
    region: Option<NodeId>,
) -> Result<QueryResult, SourceError> {
    if snapshot.truncated
        || snapshot.nodes.len() > MAX_INPUT_NODES
        || snapshot.edges.len() > MAX_INPUT_EDGES
        || level > MAX_LEVEL
    {
        return Err(invalid());
    }
    let ids: HashMap<_, _> = snapshot
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id, i))
        .collect();
    if ids.len() != snapshot.nodes.len() {
        return Err(invalid());
    }
    let mut children = vec![Vec::new(); ids.len()];
    let mut parents: Vec<Option<(usize, bool)>> = vec![None; ids.len()];
    let mut hierarchy = false;
    // Contains is lexical ownership; Defines may additionally point from a
    // file to every nested declaration. Prefer the more specific ownership.
    for kind in [EdgeKind::Contains, EdgeKind::Defines] {
        for e in snapshot.edges.iter().filter(|e| e.kind == kind) {
            let (Some(&a), Some(&b)) = (ids.get(&e.from), ids.get(&e.to)) else {
                return Err(invalid());
            };
            if kind == EdgeKind::Defines && parents[b].is_some_and(|(_, contains)| contains) {
                continue;
            }
            if parents[b].is_some_and(|(parent, _)| parent == a) {
                continue;
            }
            if parents[b]
                .replace((a, kind == EdgeKind::Contains))
                .is_some()
            {
                return Err(invalid());
            }
            children[a].push(b);
            hierarchy = true;
        }
    }
    if !hierarchy {
        return Err(SourceError::Unsupported("hierarchy coarsening".into()));
    }
    // Validate the whole forest, including cycles outside a requested region.
    let roots: Vec<_> = parents
        .iter()
        .enumerate()
        .filter_map(|(i, p)| p.is_none().then_some(i))
        .collect();
    let mut queue: VecDeque<_> = roots.iter().copied().collect();
    let mut seen = 0;
    while let Some(i) = queue.pop_front() {
        seen += 1;
        queue.extend(children[i].iter().copied());
    }
    if seen != ids.len() {
        return Err(invalid());
    }
    let starts = match region {
        Some(id) => vec![*ids.get(&id).ok_or(SourceError::NotFound)?],
        None => roots,
    };
    // Every descendant maps to exactly one frontier representative. Leaves
    // above the requested depth remain visible, so uneven trees lose no nodes.
    let mut owner = vec![None; ids.len()];
    let mut counts = vec![0_i64; ids.len()];
    let mut frontier = Vec::new();
    let mut queue: VecDeque<_> = starts.into_iter().map(|i| (i, 0, None)).collect();
    while let Some((i, depth, inherited)) = queue.pop_front() {
        let representative = if let Some(r) = inherited {
            Some(r)
        } else if depth >= level || children[i].is_empty() {
            frontier.push(i);
            if frontier.len() > MAX_NODES {
                return Err(invalid());
            }
            Some(i)
        } else {
            None
        };
        owner[i] = representative;
        if let Some(r) = representative {
            counts[r] += 1;
        }
        queue.extend(children[i].iter().map(|&c| (c, depth + 1, representative)));
    }
    let mut out = QueryResult {
        table: Some(super::Table {
            columns: vec!["source.nodes".into(), "source.edges".into()],
            rows: vec![vec![
                Value::Int(ids.len() as i64),
                Value::Int(snapshot.edges.len() as i64),
            ]],
            truncated: false,
        }),
        ..Default::default()
    };
    let mut aggregated = HashMap::new();
    for mut edge in snapshot.edges {
        let (Some(&a), Some(&b)) = (ids.get(&edge.from), ids.get(&edge.to)) else {
            continue;
        };
        let (Some(a), Some(b)) = (owner[a], owner[b]) else {
            continue;
        };
        if a == b {
            continue;
        }
        let key = (edge.source.clone(), a, b, edge.kind.clone());
        let index = *aggregated.entry(key).or_insert_with(|| {
            edge.from = snapshot.nodes[a].id;
            edge.to = snapshot.nodes[b].id;
            // Source properties are not meaningful on a quotient edge.
            edge.props.clear();
            edge.props.insert("coarse.weight".into(), Value::Int(0));
            out.edges.push(edge);
            out.edges.len() - 1
        });
        if let Some(Value::Int(weight)) = out.edges[index].props.get_mut("coarse.weight") {
            *weight += 1;
        }
    }
    for (i, mut node) in snapshot.nodes.into_iter().enumerate() {
        if owner[i] == Some(i) {
            node.props
                .insert("coarse.members".into(), Value::Int(counts[i] - 1));
            out.nodes.push(node);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Edge, Node, NodeKind, SourceId, Version};

    fn fixture(folders: usize, files: usize) -> QueryResult {
        let source = SourceId::new("coarse-test");
        let mut graph = QueryResult::default();
        let mut add = |key: usize, kind| {
            // Avoid hashing a million strings in the scale fixture.
            let id = NodeId(uuid::Uuid::from_u128(key as u128 + 1));
            graph.nodes.push(Node {
                id,
                source: source.clone(),
                kind,
                label: key.to_string(),
                native_key: key.to_string(),
                content: None,
                version: Version(0),
                props: Default::default(),
            });
            id
        };
        let root = add(0, NodeKind::Directory);
        let mut first = Vec::new();
        for folder in 0..folders {
            let parent = add(1 + folder * (files + 1), NodeKind::Directory);
            graph.edges.push(Edge::contains(&source, root, parent));
            for file in 0..files {
                let id = add(2 + folder * (files + 1) + file, NodeKind::File);
                graph.edges.push(Edge::contains(&source, parent, id));
                if file == 0 {
                    first.push(id);
                }
            }
        }
        for pair in first.windows(2) {
            for _ in 0..3 {
                graph.edges.push(Edge {
                    source: source.clone(),
                    from: pair[0],
                    to: pair[1],
                    kind: EdgeKind::References,
                    props: Default::default(),
                });
            }
        }
        graph
    }

    #[test]
    fn counts_weights_and_region_members() {
        let graph = fixture(3, 5);
        let coarse = project(graph.clone(), 1, None).unwrap();
        assert_eq!(coarse.nodes.len(), 3);
        assert!(coarse
            .nodes
            .iter()
            .all(|n| n.props["coarse.members"] == Value::Int(5)));
        assert_eq!(coarse.edges.len(), 2);
        assert!(coarse
            .edges
            .iter()
            .all(|e| e.props["coarse.weight"] == Value::Int(3)));
        let region = coarse.nodes[0].id;
        let members = project(graph, 1, Some(region)).unwrap();
        assert_eq!(members.nodes.len(), 5);
        assert!(members.nodes.iter().all(|n| n.kind == NodeKind::File));
    }

    #[test]
    fn projected_edges_retain_distinct_source_ownership() {
        let mut graph = fixture(2, 2);
        let mut other = graph.edges.last().unwrap().clone();
        other.source = SourceId::new("other-owner");
        graph.edges.push(other);
        let result = project(graph, 1, None).unwrap();
        assert_eq!(result.edges.len(), 2);
        let weights: HashMap<_, _> = result
            .edges
            .iter()
            .map(|edge| (edge.source.as_str(), edge.props["coarse.weight"].clone()))
            .collect();
        assert_eq!(weights["coarse-test"], Value::Int(3));
        assert_eq!(weights["other-owner"], Value::Int(1));
    }

    #[test]
    fn refuses_incomplete_cycles_multiple_parents_and_excess_depth() {
        let mut graph = fixture(2, 2);
        graph.truncated = true;
        assert!(project(graph.clone(), 1, None).is_err());
        graph.truncated = false;
        assert!(project(graph.clone(), MAX_LEVEL + 1, None).is_err());
        graph.edges.push(Edge::contains(
            &graph.nodes[0].source,
            graph.nodes[2].id,
            graph.nodes[0].id,
        ));
        assert!(project(graph, 1, None).is_err());
        let mut graph = fixture(2, 2);
        graph.edges.push(Edge::contains(
            &graph.nodes[0].source,
            graph.nodes[4].id,
            graph.nodes[2].id,
        ));
        assert!(project(graph, 1, None).is_err());
    }

    #[test]
    fn refuses_a_cut_over_the_output_bound() {
        assert!(project(fixture(1, MAX_NODES + 1), 2, None).is_err());
    }

    #[test]
    #[ignore = "million-node scale fixture; run explicitly with --ignored --nocapture"]
    fn million_node_hierarchy() {
        let graph = fixture(1000, 999);
        assert_eq!(graph.nodes.len(), 1_000_001);
        let start = std::time::Instant::now();
        let coarse = project(graph, 1, None).unwrap();
        eprintln!("million-node hierarchy projection: {:?}", start.elapsed());
        assert_eq!(coarse.nodes.len(), 1000);
        assert_eq!(coarse.edges.len(), 999);
        assert!(coarse
            .nodes
            .iter()
            .all(|n| n.props["coarse.members"] == Value::Int(999)));
    }
}
