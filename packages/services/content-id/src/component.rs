//! Ordered-port graph canonicalization. Every candidate root is compared;
//! automorphic members use the smallest index among equal encodings.
use super::{field, Budget, ContentIdError, Identity, Part, Result, NORMALIZER_VERSION};
use std::collections::{HashMap, HashSet};
use std::hash::Hash;

fn targets<K: Copy>(parts: &[Part<K>]) -> impl DoubleEndedIterator<Item = K> + '_ {
    parts.iter().filter_map(|p| match p {
        Part::Reference(id) => Some(*id),
        _ => None,
    })
}

/// Identities of every definition in `units` (each definition's encoding
/// from [`definition`](crate::definition)). Every referenced key must be a
/// unit too; the result does not depend on the map's iteration order.
pub fn identities<K: Copy + Eq + Hash>(
    units: &HashMap<K, Vec<Part<K>>>,
    budget: &mut Budget,
) -> Result<HashMap<K, Identity>> {
    // Iterative Kosaraju: no native stack use proportional to component size.
    let mut visited = HashSet::new();
    let mut finish = Vec::new();
    let mut reverse: HashMap<K, Vec<K>> = HashMap::new();
    for (&id, parts) in units {
        for target in targets(parts) {
            reverse.entry(target).or_default().push(id);
        }
    }
    for &root in units.keys() {
        let mut stack = vec![(root, false)];
        while let Some((id, exit)) = stack.pop() {
            budget.step()?;
            if exit {
                finish.push(id);
                continue;
            }
            if !visited.insert(id) {
                continue;
            }
            stack.push((id, true));
            for target in targets(&units[&id]).rev() {
                stack.push((target, false));
            }
        }
    }
    let mut groups = Vec::new();
    visited.clear();
    while let Some(root) = finish.pop() {
        if visited.contains(&root) {
            continue;
        }
        let mut group = Vec::new();
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            budget.step()?;
            if !visited.insert(id) {
                continue;
            }
            group.push(id);
            stack.extend(reverse.get(&id).into_iter().flatten().copied());
        }
        groups.push(group);
    }
    // SCCs are source-first; dependencies are needed first.
    let mut done: HashMap<K, Identity> = HashMap::new();
    for group in groups.into_iter().rev() {
        let members: HashSet<_> = group.iter().copied().collect();
        let mut best: Option<Vec<u8>> = None;
        let mut indices = HashMap::new();
        for &root in &group {
            let mut order = Vec::new();
            let mut numbered = HashMap::new();
            let mut stack = vec![root];
            while let Some(id) = stack.pop() {
                budget.step()?;
                if numbered.contains_key(&id) {
                    continue;
                }
                numbered.insert(id, order.len() as u32);
                order.push(id);
                // The syntax/reference order, never HashMap iteration order.
                for target in targets(&units[&id]).rev() {
                    if members.contains(&target) {
                        stack.push(target);
                    }
                }
            }
            let mut bytes = Vec::new();
            field(
                &mut bytes,
                b"moonkale-component",
                &NORMALIZER_VERSION.to_le_bytes(),
            );
            field(&mut bytes, b"size", &(group.len() as u32).to_le_bytes());
            for id in order {
                field(&mut bytes, b"member", &[]);
                for part in &units[&id] {
                    budget.step()?;
                    match part {
                        Part::Bytes(value) => field(&mut bytes, b"syntax", value),
                        Part::Reference(target) => {
                            if let Some(index) = numbered.get(target) {
                                field(&mut bytes, b"internal", &index.to_le_bytes());
                            } else {
                                let target =
                                    done.get(target).ok_or(ContentIdError::MalformedSyntax)?;
                                field(&mut bytes, b"external", &target.digest);
                            }
                        }
                    }
                }
                field(&mut bytes, b"end-member", &[]);
            }
            match &best {
                Some(previous) if bytes > *previous => {}
                Some(previous) if bytes == *previous => {
                    for (id, index) in numbered {
                        let entry = indices.entry(id).or_insert(index);
                        *entry = (*entry).min(index);
                    }
                }
                _ => {
                    best = Some(bytes);
                    indices = numbered;
                }
            }
        }
        let digest = *blake3::hash(&best.ok_or(ContentIdError::MalformedSyntax)?).as_bytes();
        for id in group.iter() {
            let index = indices[id];
            let mut bytes = Vec::new();
            field(&mut bytes, b"moonkale-member", &digest);
            field(&mut bytes, b"index", &index.to_le_bytes());
            let member_digest = *blake3::hash(&bytes).as_bytes();
            done.insert(
                *id,
                Identity {
                    digest: member_digest,
                    normalizer_version: NORMALIZER_VERSION,
                    component_digest: digest,
                    component_index: index,
                    component_size: group.len() as u32,
                },
            );
        }
    }
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;
    type Units = HashMap<u8, Vec<Part<u8>>>;
    fn id(n: u8) -> u8 {
        n
    }
    fn bytes(n: u8) -> Part<u8> {
        Part::Bytes(vec![n])
    }
    #[test]
    fn dependency_diamond_and_disconnected_units_do_not_depend_on_insertion_order() {
        let mut graph = Units::new();
        graph.insert(
            id(1),
            vec![bytes(1), Part::Reference(id(2)), Part::Reference(id(3))],
        );
        graph.insert(id(2), vec![bytes(2), Part::Reference(id(4))]);
        graph.insert(id(3), vec![bytes(3), Part::Reference(id(4))]);
        graph.insert(id(4), vec![bytes(4)]);
        graph.insert(id(5), vec![bytes(5)]);
        let first = identities(&graph, &mut Budget::default()).unwrap();
        let reversed = graph
            .clone()
            .into_iter()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        assert_eq!(
            first,
            identities(&reversed, &mut Budget::default()).unwrap()
        );
        graph.insert(id(4), vec![bytes(8)]);
        let changed = identities(&graph, &mut Budget::default()).unwrap();
        for n in 1..5 {
            assert_ne!(first[&id(n)].digest, changed[&id(n)].digest);
        }
        assert_eq!(first[&id(5)], changed[&id(5)]);
    }
    #[test]
    fn ordered_ports_distinguish_different_wiring_and_component_members() {
        let graph = Units::from([
            (
                id(1),
                vec![bytes(1), Part::Reference(id(2)), Part::Reference(id(1))],
            ),
            (id(2), vec![bytes(2), Part::Reference(id(1))]),
        ]);
        let first = identities(&graph, &mut Budget::default()).unwrap();
        assert_ne!(first[&id(1)].digest, first[&id(2)].digest);
        assert_eq!(
            first[&id(1)].component_digest,
            first[&id(2)].component_digest
        );
        let mut other = graph.clone();
        other.insert(
            id(1),
            vec![bytes(1), Part::Reference(id(1)), Part::Reference(id(2))],
        );
        assert_ne!(first, identities(&other, &mut Budget::default()).unwrap());
    }
    #[test]
    fn canonicalization_budget_has_no_partial_success() {
        let graph = Units::from([(id(1), vec![Part::Reference(id(1))])]);
        assert_eq!(
            identities(&graph, &mut Budget(crate::MAX_STEPS, 0)),
            Err(ContentIdError::LimitExceeded)
        );
    }
}
