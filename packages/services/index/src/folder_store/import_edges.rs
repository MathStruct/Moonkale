//! Import edges are file-owned annotations, separate from normalized identity.
//! Targets use checked manifest locations, so they cannot retain dead records.
use super::{error, manifest, CodeFile};
use crate::extract::Derived;
use moonkale_core::{EdgeKind, NodeId, SourceError, SourceId, Value};
use sophia_store::{sqlite::Sqlite, Fact, FileIngest, Store};
use std::collections::{BTreeMap, BTreeSet, HashMap};

pub(super) type SyntaxCache = HashMap<NodeId, (String, Derived)>;

pub(super) fn reconcile(
    store: &Store<Sqlite>,
    files: &[CodeFile],
    produced: &mut BTreeMap<String, FileIngest>,
    changed: &mut BTreeSet<String>,
    cache: &mut SyntaxCache,
) -> Result<(), SourceError> {
    let index = SourceId::new("index:folder-imports");
    let present: BTreeSet<_> = files.iter().map(|f| f.node.id).collect();
    cache.retain(|id, _| present.contains(id));
    for f in files {
        let input = crate::extract::provenance(&f.text)["input"].to_string();
        if cache.get(&f.node.id).is_some_and(|(old, _)| old == &input) {
            continue;
        }
        let d = match f.language() {
            "rust" => crate::extract::symbols_rust::extract(&index, &f.node, &f.text),
            "python" => crate::extract::symbols_python::extract(&index, &f.node, &f.text),
            "julia" => crate::extract::symbols_julia::extract(&index, &f.node, &f.text),
            _ => continue,
        };
        cache.insert(f.node.id, (input, d));
    }
    // A selected debounce batch can observe another dirty file whose event
    // has not arrived yet. Never combine its new text with old normalized
    // records/spans, or republish it while updating a caller's annotations.
    let mut active = BTreeSet::new();
    for f in files {
        if produced.contains_key(&f.node.native_key)
            || store
                .file_manifest(&f.node.native_key)
                .map_err(error)?
                .is_some_and(|m| {
                    m.source
                        == Some(sophia_store::folder::source_record(&f.node.native_key, &f.text).id)
                })
        {
            active.insert(f.node.id);
        }
    }
    let base = cache
        .iter()
        .filter(|(id, _)| active.contains(id))
        .map(|(id, (_, d))| (*id, d.clone()))
        .collect();
    let nodes = files
        .iter()
        .filter(|f| active.contains(&f.node.id))
        .map(|f| (f.node.id, f.node.clone()))
        .collect();
    let resolved = crate::cross_file::resolve(&nodes, &base);
    let mut locations = HashMap::new();
    let mut manifests = HashMap::new();
    for f in files.iter().filter(|f| active.contains(&f.node.id)) {
        let m = if let Some(ingest) = produced.get(&f.node.native_key) {
            Some(manifest(ingest))
        } else {
            store.file_manifest(&f.node.native_key).map_err(error)?
        };
        if let Some(m) = m {
            manifests.insert(f.node.id, m);
        }
        for e in &resolved[&f.node.id].edges {
            if e.kind == EdgeKind::Defines {
                if let (Some(Value::Int(start)), Some(Value::Int(end))) =
                    (e.props.get("start_byte"), e.props.get("end_byte"))
                {
                    locations.insert(e.to, (f.node.id, *start as u32, *end as u32));
                }
            }
        }
    }
    for f in files.iter().filter(|f| active.contains(&f.node.id)) {
        let mut desired = Vec::new();
        for e in resolved[&f.node.id]
            .edges
            .iter()
            .filter(|e| e.props.get("resolved_via") == Some(&Value::Text("import".into())))
        {
            let Some((origin, a, b)) = locations.get(&e.from) else {
                continue;
            };
            let Some((subject, _, _)) = manifests
                .get(origin)
                .and_then(|m| m.spans.iter().find(|(_, s, t)| (s, t) == (a, b)))
            else {
                continue;
            };
            let Some((target, a, b)) = locations.get(&e.to) else {
                continue;
            };
            let Some(target_manifest) = manifests.get(target) else {
                continue;
            };
            if !target_manifest
                .spans
                .iter()
                .any(|(_, s, t)| (s, t) == (a, b))
            {
                continue;
            }
            let mut props = e.props.clone();
            props.remove("derived_at"); // Wall time is not an identity input.
            let value = serde_json::to_vec(&serde_json::json!({
                "path": target_manifest.path, "start": a, "end": b,
                "source": target_manifest.source.map(|s| s.to_hex()),
                "kind": if e.kind == EdgeKind::Calls { "Calls" } else { "References" },
                "props": props, "origin": f.node.native_key,
            }))
            .map_err(error)?;
            if value.len() > 64 * 1024 {
                return Err(error(
                    "folder import annotation exceeds 64 KiB; use in-memory graph",
                ));
            }
            desired.push(Fact::new(
                "import_edge",
                *subject,
                value,
                ("moonkale-index", &super::normaliser()),
                vec![sophia_store::Input::Content(*subject)],
                0,
            ));
        }
        desired.sort_by_key(|f| f.id);
        desired.dedup_by_key(|f| f.id);
        if let Some(ingest) = produced.get_mut(&f.node.native_key) {
            ingest.facts.retain(|f| f.kind != "import_edge");
            ingest.facts.extend(desired);
            continue;
        }
        let Some(m) = manifests.get(&f.node.id) else {
            continue;
        };
        let facts = owned_facts(store, m)?;
        let mut old: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == "import_edge")
            .map(|f| f.id)
            .collect();
        old.sort();
        old.dedup();
        if old == desired.iter().map(|f| f.id).collect::<Vec<_>>() {
            continue;
        }
        let mut ingest = restore(store, f, m)?;
        ingest.facts = facts
            .into_iter()
            .filter(|f| f.kind != "import_edge")
            .chain(desired)
            .collect();
        changed.insert(f.node.native_key.clone());
        produced.insert(f.node.native_key.clone(), ingest);
    }
    Ok(())
}

fn owned_facts(
    store: &Store<Sqlite>,
    m: &sophia_store::FileManifest,
) -> Result<Vec<Fact>, SourceError> {
    let mut out = BTreeMap::new();
    let owned: BTreeSet<_> = m.facts.iter().collect();
    for subject in m.spans.iter().map(|(id, _, _)| id).collect::<BTreeSet<_>>() {
        for fact in store.read(|tx| tx.facts_of(subject)).map_err(error)? {
            if owned.contains(&fact.id) {
                out.insert(fact.id, fact);
            }
        }
    }
    Ok(out.into_values().collect())
}

fn restore(
    store: &Store<Sqlite>,
    f: &CodeFile,
    m: &sophia_store::FileManifest,
) -> Result<FileIngest, SourceError> {
    let ids: BTreeSet<_> = m
        .spans
        .iter()
        .map(|(id, _, _)| *id)
        .chain(m.names.iter().map(|(_, id)| *id))
        .chain(
            m.generics
                .iter()
                .flat_map(|(_, _, ids)| ids.iter().copied()),
        )
        .collect();
    let mut ingest = FileIngest {
        path: m.path.clone(),
        text: Some(f.text.clone()),
        normaliser: m.normaliser.clone(),
        ..Default::default()
    };
    let mut indices = HashMap::new();
    for id in ids {
        indices.insert(id, ingest.records.len());
        ingest.records.push(
            store
                .record(&id)
                .map_err(error)?
                .ok_or_else(|| error("missing import caller record"))?,
        );
    }
    ingest.spans = m
        .spans
        .iter()
        .map(|(id, a, b)| (indices[id], *a, *b))
        .collect();
    ingest.names = m
        .names
        .iter()
        .map(|(p, id)| (p.clone(), indices[id]))
        .collect();
    ingest.generics = m
        .generics
        .iter()
        .map(|(p, lang, ids)| sophia_store::FileGeneric {
            path: p.clone(),
            lang: lang.clone(),
            members: ids.iter().map(|id| indices[id]).collect(),
        })
        .collect();
    Ok(ingest)
}
