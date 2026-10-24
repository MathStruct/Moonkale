//! Mirrors Sophia `code::to_record` at c3c48e02: canonical parts with
//! references replaced by full digests, excluding intra-component refs.
//! Resolution is Moonkale's: the index extractor for Rust/Python and the
//! shared module resolver for Julia. No sophia-ingest dependency/second parser.
use super::{error, normaliser, CodeFile, FolderReport};
use moonkale_content_id::{
    self as cid, Binding, Budget, Definition, Identity, JuliaScopes, NamingContext, Part,
};
use moonkale_core::{EdgeKind, SourceError, SourceId, Value};
use sophia_hash::{Digest, Record, Status};
use sophia_store::{Fact, FileGeneric, FileIngest};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

pub(super) fn produce(
    root: &Path,
    files: &[CodeFile],
    changed: &BTreeSet<String>,
    julia_changed: bool,
    report: &mut FolderReport,
) -> Result<BTreeMap<String, FileIngest>, SourceError> {
    let mut out = BTreeMap::new();
    for f in files
        .iter()
        .filter(|f| f.language() != "julia" && changed.contains(&f.node.native_key))
    {
        let ingest = ordinary(root, f, report)?;
        report.parsed_files += 1;
        out.insert(f.node.native_key.clone(), ingest);
    }
    if julia_changed {
        let julia: Vec<_> = files.iter().filter(|f| f.language() == "julia").collect();
        report.parsed_files += julia.len();
        out.extend(julia_module(&julia, report)?);
    }
    Ok(out)
}

fn empty(f: &CodeFile) -> FileIngest {
    FileIngest {
        path: f.node.native_key.clone(),
        text: Some(f.text.clone()),
        normaliser: normaliser(),
        ..Default::default()
    }
}

fn record<K: Copy + Eq + std::hash::Hash>(
    lang: &str,
    d: &Definition,
    parts: &[Part<K>],
    ids: &HashMap<K, Identity>,
    key: K,
) -> Record {
    let id = &ids[&key];
    let mut bytes = Vec::new();
    let mut refs = Vec::new();
    for p in parts {
        match p {
            Part::Bytes(b) => bytes.extend_from_slice(b),
            Part::Reference(k) => {
                let target = Digest(ids[k].digest);
                bytes.extend_from_slice(target.as_bytes());
                if ids[k].component_digest != id.component_digest && !refs.contains(&target) {
                    refs.push(target);
                }
            }
        }
    }
    Record {
        id: Digest(id.digest),
        lang: lang.into(),
        normaliser: normaliser(),
        kind: d.kind.clone(),
        component: Digest(id.component_digest),
        index: id.component_index,
        component_size: id.component_size,
        bytes,
        display: d.name.as_bytes().to_vec(),
        status: Status::Complete,
        refs,
        nominal_refs: Vec::new(),
    }
}

fn add_bindings(
    ingest: &mut FileIngest,
    defs: &[Definition],
    package: &str,
    prefix: &str,
    report: &mut FolderReport,
) -> Result<(), SourceError> {
    for g in groups(defs, package, report)? {
        match g.binding {
            Binding::Named(path) => ingest
                .names
                .push((cid::entity_path(prefix, &path), g.members[0])),
            Binding::Method(path) => ingest.generics.push(FileGeneric {
                path: cid::entity_path(prefix, &path),
                lang: ingest.records[g.members[0]].lang.clone(),
                members: g.members,
            }),
        }
    }
    Ok(())
}

fn syntax_at<'a>(root: cid::Syntax<'a>, start: usize, end: usize) -> Option<cid::Syntax<'a>> {
    let mut n = root.descendant_for_byte_range(start, end)?;
    while (n.start_byte(), n.end_byte()) != (start, end) {
        n = n.parent()?;
    }
    Some(n)
}

fn ordinary(
    root_path: &Path,
    f: &CodeFile,
    report: &mut FolderReport,
) -> Result<FileIngest, SourceError> {
    let lang = f.language();
    let mut budget = Budget::new();
    let tree = cid::parse(lang, &f.text, &mut budget).map_err(error)?;
    let (package, path) = context(root_path, &f.node.native_key, lang);
    let defs = cid::definitions(
        lang,
        tree.root_node(),
        &f.text,
        NamingContext {
            package: package.as_deref(),
            path: &path,
            module: None,
        },
        &JuliaScopes::default(),
        &mut budget,
    )
    .map_err(error)?;
    // Reuse the index's conservative lexical reference extraction, including
    // shadows and nested definitions, on this same parsed tree.
    let index = SourceId::new("index:folder-store");
    let mut symbols = crate::extract::Symbols::new(&index, &f.node, &f.text);
    match lang {
        "rust" => crate::extract::symbols_rust::walk(
            f.text.as_bytes(),
            tree.root_node(),
            None,
            &mut symbols,
        ),
        "python" => crate::extract::symbols_python::walk(
            f.text.as_bytes(),
            tree.root_node(),
            None,
            &mut symbols,
        ),
        _ => unreachable!(),
    }
    let derived = symbols.finish(tree.root_node(), &f.text);
    let by_span: HashMap<_, _> = defs
        .iter()
        .enumerate()
        .map(|(i, d)| ((d.start, d.end), i))
        .collect();
    let mut keys = HashMap::new();
    for e in derived.edges.iter().filter(|e| e.kind == EdgeKind::Defines) {
        if let (Some(Value::Int(a)), Some(Value::Int(b))) =
            (e.props.get("start_byte"), e.props.get("end_byte"))
        {
            if let Some(i) = by_span.get(&(*a as usize, *b as usize)) {
                keys.insert(e.to, *i);
            }
        }
    }
    let mut refs = vec![HashMap::new(); defs.len()];
    for e in derived
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::References)
    {
        if let (Some(&i), Some(&target), Some(Value::Text(ranges))) =
            (keys.get(&e.from), keys.get(&e.to), e.props.get("ranges"))
        {
            for r in ranges.split(',') {
                if let Some((a, b)) = r
                    .split_once(':')
                    .and_then(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?)))
                {
                    refs[i].insert((a, b), target);
                }
            }
        }
    }
    let mut units = HashMap::new();
    let mut failed = BTreeSet::new();
    loop {
        let before = failed.len();
        for (i, d) in defs.iter().enumerate() {
            if failed.contains(&i) || units.contains_key(&i) {
                continue;
            }
            let n = syntax_at(tree.root_node(), d.start, d.end)
                .ok_or_else(|| error("definition span lost"))?;
            match cid::definition(lang, n, &f.text, &refs[i], &mut budget) {
                Ok(parts) => {
                    units.insert(i, parts);
                }
                Err(cid::ContentIdError::LimitExceeded) => {
                    return Err(error("folder-store normalisation budget exceeded"))
                }
                Err(_) => {
                    failed.insert(i);
                }
            }
        }
        if before == failed.len() {
            break;
        }
        for (i, r) in refs.iter_mut().enumerate() {
            let before = r.len();
            r.retain(|_, target| !failed.contains(target));
            if before != r.len() {
                units.remove(&i);
            }
        }
    }
    report.refused_definitions += failed.len();
    let ids = cid::identities(&units, &mut budget).map_err(error)?;
    let mut ingest = empty(f);
    let mut accepted = Vec::new();
    for (i, d) in defs.iter().enumerate().filter(|(i, _)| ids.contains_key(i)) {
        ingest
            .spans
            .push((ingest.records.len(), d.start as u32, d.end as u32));
        ingest.records.push(record(lang, d, &units[&i], &ids, i));
        accepted.push(d.clone());
    }
    add_bindings(
        &mut ingest,
        &accepted,
        package.as_deref().unwrap_or("unknown"),
        if lang == "rust" { "rs" } else { "py" },
        report,
    )?;
    Ok(ingest)
}

fn metadata(path: &Path) -> Option<toml::Value> {
    if path.symlink_metadata().ok()?.file_type().is_symlink()
        || path.metadata().ok()?.len() > 64 * 1024
    {
        return None;
    }
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(64 * 1024 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > 64 * 1024 {
        return None;
    }
    std::str::from_utf8(&bytes).ok()?.parse().ok()
}

/// Read only bounded naming metadata; it never becomes a source record.
fn context(root: &Path, path: &str, lang: &str) -> (Option<String>, String) {
    let absolute = root.join(path);
    let mut dir = absolute.parent().unwrap_or(root).to_path_buf();
    if lang == "rust" {
        loop {
            if let Some(name) = metadata(&dir.join("Cargo.toml")).and_then(|v| {
                v.get("package")?
                    .get("name")?
                    .as_str()
                    .map(|s| s.replace('-', "_"))
            }) {
                return (
                    Some(name),
                    absolute
                        .strip_prefix(&dir)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
            if dir == root || !dir.pop() {
                break;
            }
        }
    } else {
        let mut package_dir = None;
        loop {
            if dir.join("__init__.py").is_file() {
                package_dir = Some(dir.clone());
            }
            if let Some(v) = metadata(&dir.join("pyproject.toml")) {
                let name = v
                    .get("project")
                    .and_then(|p| p.get("name"))
                    .and_then(|s| s.as_str())
                    .map(|s| s.replace('-', "_"));
                if package_dir.is_none() && name.is_some() {
                    return (
                        name,
                        absolute
                            .strip_prefix(&dir)
                            .unwrap()
                            .to_string_lossy()
                            .replace('\\', "/"),
                    );
                }
            }
            if dir == root || !dir.pop() {
                break;
            }
        }
        if let Some(dir) = package_dir {
            return (
                dir.file_name().map(|s| s.to_string_lossy().into_owned()),
                absolute
                    .strip_prefix(&dir)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
    (None, path.to_string())
}

fn include_path(path: &str, relative: &str) -> Option<String> {
    if Path::new(relative).is_absolute() || relative.contains('\\') {
        return None;
    }
    let mut parts: Vec<_> = path.split('/').collect();
    parts.pop();
    for p in relative.split('/') {
        match p {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            p => parts.push(p),
        }
    }
    Some(parts.join("/"))
}

fn julia_module(
    files: &[&CodeFile],
    report: &mut FolderReport,
) -> Result<BTreeMap<String, FileIngest>, SourceError> {
    let mut trees = Vec::new();
    for f in files {
        trees.push(cid::parse("julia", &f.text, &mut Budget::new()).map_err(error)?);
    }
    let paths: HashMap<_, _> = files
        .iter()
        .enumerate()
        .map(|(i, f)| (f.node.native_key.as_str(), i))
        .collect();
    let includes: Vec<_> = files
        .iter()
        .zip(&trees)
        .map(|(f, t)| cid::julia::includes(&f.text, t))
        .collect();
    let mut incoming = BTreeSet::new();
    for (i, inc) in includes.iter().enumerate() {
        for inc in inc {
            if let Some(j) = inc
                .path
                .as_deref()
                .and_then(|p| include_path(&files[i].node.native_key, p))
                .and_then(|p| paths.get(p.as_str()))
            {
                incoming.insert(*j);
            }
        }
    }
    let mut bases: Vec<Option<Vec<String>>> = vec![None; files.len()];
    let mut dynamic = Vec::new();
    let roots: Vec<_> = (0..files.len())
        .filter(|i| !incoming.contains(i))
        .chain(0..files.len())
        .collect();
    for root in roots {
        if bases[root].is_some() {
            continue;
        }
        let mut pending = vec![(root, Vec::new())];
        while let Some((i, base)) = pending.pop() {
            if bases[i].is_some() {
                continue;
            }
            bases[i] = Some(base.clone());
            for inc in includes[i].iter().rev() {
                let mut module = base.clone();
                module.extend(inc.module.clone());
                match inc
                    .path
                    .as_deref()
                    .and_then(|p| include_path(&files[i].node.native_key, p))
                    .and_then(|p| paths.get(p.as_str()).copied())
                {
                    Some(j) if bases[j].is_none() => pending.push((j, module)),
                    Some(j) if bases[j].as_ref() == Some(&module) => {}
                    _ => dynamic.push(module),
                }
            }
        }
    }
    let module_files: Vec<_> = files
        .iter()
        .zip(&trees)
        .enumerate()
        .map(|(i, (f, t))| cid::julia::ModuleFile {
            text: &f.text,
            tree: t,
            module: bases[i].clone().unwrap_or_default(),
        })
        .collect();
    let resolved = cid::julia::module(&module_files, &dynamic).map_err(error)?;
    let mut scopes = JuliaScopes::default();
    for m in &module_files {
        cid::julia_scopes(m.tree.root_node(), m.text, &m.module, &mut scopes);
    }
    let mut named = HashMap::new();
    for (i, f) in files.iter().enumerate() {
        for d in cid::definitions(
            "julia",
            trees[i].root_node(),
            &f.text,
            NamingContext {
                package: Some("Main"),
                path: &f.node.native_key,
                module: Some(&module_files[i].module),
            },
            &scopes,
            &mut Budget::new(),
        )
        .map_err(error)?
        {
            named.insert((i as u32, d.start, d.end), d);
        }
    }
    let ids: HashMap<_, _> = resolved
        .definitions
        .iter()
        .filter_map(|d| d.identity.as_ref().ok().map(|id| (d.key, id.clone())))
        .collect();
    let mut out: Vec<_> = files.iter().map(|f| empty(f)).collect();
    let mut accepted = vec![Vec::new(); files.len()];
    for d in &resolved.definitions {
        let Some(meta) = named.get(&d.key) else {
            continue;
        };
        if !ids.contains_key(&d.key) {
            report.refused_definitions += 1;
            continue;
        }
        let i = d.key.0 as usize;
        let node = syntax_at(trees[i].root_node(), d.key.1, d.key.2)
            .ok_or_else(|| error("Julia definition span lost"))?;
        let refs = d.references.iter().copied().collect();
        let parts = cid::definition("julia", node, &files[i].text, &refs, &mut Budget::new())
            .map_err(error)?;
        let index = out[i].records.len();
        out[i].spans.push((index, d.key.1 as u32, d.key.2 as u32));
        out[i]
            .records
            .push(record("julia", meta, &parts, &ids, d.key));
        accepted[i].push(meta.clone());
    }
    let flat: Vec<_> = accepted.iter().flatten().cloned().collect();
    let locations: Vec<_> = accepted
        .iter()
        .enumerate()
        .flat_map(|(f, defs)| (0..defs.len()).map(move |i| (f, i)))
        .collect();
    for group in groups(&flat, "Main", report)? {
        match group.binding {
            Binding::Named(path) => {
                let (f, i) = locations[group.members[0]];
                out[f].names.push((cid::entity_path("jl", &path), i));
            }
            Binding::Method(path) => {
                let mut members: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
                for member in group.members {
                    let (f, i) = locations[member];
                    members.entry(f).or_default().push(i);
                }
                for (f, members) in members {
                    out[f].generics.push(FileGeneric {
                        path: cid::entity_path("jl", &path),
                        lang: "julia".into(),
                        members,
                    });
                }
            }
        }
    }
    // Generic calls store the absolute entity path, since the store allocates
    // nominal ids inside replace_files. They are annotations, never id inputs.
    for (file, span, name) in &resolved.calls_generic {
        let i = *file as usize;
        if let Some((r, d)) = accepted[i]
            .iter()
            .enumerate()
            .filter(|(_, d)| d.start <= span.0 && d.end >= span.1)
            .min_by_key(|(_, d)| d.end - d.start)
        {
            let mut path = d.module.clone();
            if path.is_empty() {
                path.push("Main".into());
            }
            path.push(name.clone());
            let target = cid::entity_path("jl", &path);
            let subject = out[i].records[r].id;
            out[i].facts.push(Fact::new(
                "uses_generic",
                subject,
                target.into_bytes(),
                ("moonkale-index", &normaliser()),
                vec![sophia_store::Input::Content(subject)],
                0,
            ));
        }
    }
    Ok(files
        .iter()
        .zip(out)
        .map(|(f, ingest)| (f.node.native_key.clone(), ingest))
        .collect())
}

/// Conditional branches may bind the same named item. Keep their records,
/// expose the ambiguity, and publish no arbitrarily chosen target.
fn groups(
    defs: &[Definition],
    package: &str,
    report: &mut FolderReport,
) -> Result<Vec<cid::BindingGroup>, SourceError> {
    let conflicts = match cid::binding_groups(defs, package) {
        Ok(groups) => return Ok(groups),
        Err(conflicts) => conflicts,
    };
    report.naming_diagnostics.extend(conflicts.iter().cloned());
    let retained: Vec<_> = defs
        .iter()
        .enumerate()
        .filter(|(_, d)| {
            cid::binding(d, package).is_none_or(|b| {
                let key = match b {
                    Binding::Named(k) | Binding::Method(k) => k,
                };
                !conflicts.contains(&cid::entity_path("name", &key))
            })
        })
        .collect();
    let clean: Vec<_> = retained.iter().map(|(_, d)| (*d).clone()).collect();
    let mut groups = cid::binding_groups(&clean, package)
        .map_err(|e| error(format!("unresolved binding ambiguity: {e:?}")))?;
    for group in &mut groups {
        for member in &mut group.members {
            *member = retained[*member].0;
        }
    }
    Ok(groups)
}
