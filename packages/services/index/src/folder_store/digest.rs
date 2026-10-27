//! Full-digest-v1 equivalence shared by the replay and the cache check.
use sophia_store::{sqlite::Sqlite, Store, Target};
use std::collections::{BTreeMap, BTreeSet};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub type State = BTreeMap<String, String>;
pub fn state(s: &Store<Sqlite>) -> Result<State> {
    let mut out = State::new();
    let mut ids = BTreeSet::new();
    for path in s.files()? {
        let m = s.file_manifest(&path)?.ok_or("missing manifest")?;
        out.insert(
            format!("manifest:{path}"),
            format!("{m:?} canonical={:?}", m.encode()),
        );
        if let Some(id) = m.source {
            ids.insert(id);
        }
        for (id, _, _) in m.spans {
            ids.insert(id);
        }
    }
    let names = s.read(|tx| tx.names_under("", usize::MAX))?;
    let mut nominal_names: BTreeMap<_, Vec<String>> = BTreeMap::new();
    for n in names {
        let target = match n.target {
            Target::Content(id) => {
                ids.insert(id);
                format!("content:{id}")
            }
            Target::Nominal(id) => {
                nominal_names.entry(id).or_default().push(n.path.clone());
                let mut members = s
                    .read(|tx| tx.relation(&id))?
                    .into_iter()
                    .map(|e| e.member.to_hex())
                    .collect::<Vec<_>>();
                members.sort();
                members.dedup();
                format!("generic:{members:?}")
            }
        };
        out.insert(format!("name:{}", n.path), target);
    }
    // Empty historical nominal allocations are append-only history. Any live
    // orphan relation is observable stale membership and must be reported.
    for nominal in s.read(|tx| tx.nominal_ids())? {
        let mut members = s
            .read(|tx| tx.relation(&nominal))?
            .into_iter()
            .map(|e| e.member.to_hex())
            .collect::<Vec<_>>();
        members.sort();
        members.dedup();
        if members.is_empty() {
            continue;
        }
        let mut bound = nominal_names.remove(&nominal).unwrap_or_default();
        bound.sort();
        out.insert(format!("membership:{bound:?}:{members:?}"), String::new());
    }
    for id in ids {
        // Compare canonical identity/bytes, excluding first-writer display metadata.
        let record = s
            .read(|tx| tx.record(&id))?
            .ok_or("missing active record")?;
        out.insert(
            format!("record:{id}"),
            format!(
                "{:?}:{:?}:{:?}",
                record.bytes,
                record.refs.iter().map(|id| id.to_hex()).collect::<Vec<_>>(),
                record.status
            ),
        );
        out.insert(
            format!("text:{id}"),
            format!(
                "{:?}",
                s.text_source(&id, None)?
                    .map(|t| (t.path, t.source.to_hex(), t.start, t.end))
            ),
        );
    }
    // Include facts on historical records too: stale facts must not hide behind
    // the current file manifests. Historical immutable records themselves stay.
    for id in s.read(|tx| tx.records_of_kind(None, usize::MAX))? {
        for f in s.read(|tx| tx.facts_of(&id))? {
            out.insert(format!("fact:{}", f.id), format!("{f:?}"));
        }
    }
    Ok(out)
}
pub fn differences(a: &State, b: &State) -> Vec<String> {
    a.keys()
        .chain(b.keys())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter(|k| a.get(*k) != b.get(*k))
        .map(|k| {
            format!(
                "{} {}",
                if !a.contains_key(k) {
                    "lost"
                } else if !b.contains_key(k) {
                    "extra"
                } else {
                    "stale"
                },
                k
            )
        })
        .collect()
}
