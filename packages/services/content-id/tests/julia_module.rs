//! Module-level Julia resolution (v5) on `golden/module/`, the fixture
//! shared with Sophia (SPEC-008). `golden/module.tsv` is the cross-check:
//! `MOONKALE_BLESS=1` rewrites it.
use moonkale_content_id::julia::{includes, module, JuliaModule, ModuleFile};
use moonkale_content_id::{parse, Budget, Tree};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/golden/module");

struct Read {
    names: Vec<String>,
    texts: Vec<String>,
    trees: Vec<Tree>,
    modules: Vec<Vec<String>>,
    dynamic: Vec<Vec<String>>,
}

/// The caller's side: follow literal includes depth first, from a root.
fn read(root: &str) -> Read {
    let mut r = Read {
        names: vec![],
        texts: vec![],
        trees: vec![],
        modules: vec![],
        dynamic: vec![],
    };
    fn visit(path: PathBuf, base: Vec<String>, r: &mut Read) {
        if r.names
            .contains(&path.file_name().unwrap().to_string_lossy().to_string())
        {
            return;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            return;
        };
        let tree = parse("julia", &text, &mut Budget::new()).unwrap();
        let found = includes(&text, &tree);
        r.names
            .push(path.file_name().unwrap().to_string_lossy().to_string());
        r.texts.push(text);
        r.trees.push(tree);
        r.modules.push(base.clone());
        for include in found {
            let mut m = base.clone();
            m.extend(include.module);
            match include.path {
                Some(p) => visit(path.parent().unwrap().join(p), m, r),
                None => r.dynamic.push(m),
            }
        }
    }
    visit(Path::new(DIR).join(root), vec![], &mut r);
    r
}

fn resolve(r: &Read) -> JuliaModule {
    let files: Vec<_> = (0..r.texts.len())
        .map(|i| ModuleFile {
            text: &r.texts[i],
            tree: &r.trees[i],
            module: r.modules[i].clone(),
        })
        .collect();
    module(&files, &r.dynamic).unwrap()
}

/// (from, to, to's file) for every reference the encodings use.
fn edges(r: &Read, m: &JuliaModule) -> BTreeSet<(String, String, String)> {
    let name = |k| {
        m.definitions
            .iter()
            .find(|d| d.key == k)
            .and_then(|d| d.name.clone())
            .unwrap_or_default()
    };
    m.definitions
        .iter()
        .flat_map(|d| {
            d.references.iter().map(|(_, t)| {
                (
                    d.name.clone().unwrap_or_default(),
                    name(*t),
                    r.names[t.0 as usize].clone(),
                )
            })
        })
        .collect()
}

fn has(e: &BTreeSet<(String, String, String)>, from: &str, to: &str) -> bool {
    e.iter().any(|(f, t, _)| f == from && t == to)
}

#[test]
fn includes_are_followed_in_order() {
    let r = read("Geo.jl");
    assert_eq!(r.names, ["Geo.jl", "types.jl", "ops.jl"]);
    assert_eq!(r.modules[1], ["Geo"]);
    assert!(r.dynamic.is_empty());
}

#[test]
fn references_resolve_across_files_conservatively() {
    let r = read("Geo.jl");
    let m = resolve(&r);
    let e = edges(&r, &m);
    let deps = |n: &str| -> BTreeSet<String> {
        let d = m
            .definitions
            .iter()
            .find(|d| d.name.as_deref() == Some(n))
            .unwrap();
        d.depends_on
            .iter()
            .map(|&f| r.names[f as usize].clone())
            .collect()
    };
    assert!(e.contains(&("dist".into(), "Point".into(), "types.jl".into())));
    assert!(has(&e, "dist", "norm2"));
    assert!(has(&e, "origin", "Point"));
    assert!(!has(&e, "len", "norm"), "imported");
    assert!(has(&e, "len", "Point"));
    assert!(!has(&e, "area", "helper"), "Inner's names are not Geo's");
    assert!(has(&e, "scale", "helper"));
    assert!(
        !has(&e, "use_twice", "twice"),
        "two methods: the generic function"
    );
    assert!(m.calls_generic.iter().any(|(_, _, n)| n == "twice"));
    assert!(!e.iter().any(|(f, _, _)| f == "show2"));
    assert_eq!(
        deps("dist"),
        BTreeSet::from(["ops.jl".into(), "types.jl".into()])
    );
    assert!(!has(&e, "Wrap", "parent"), "a field name");
    assert!(has(&e, "Wrap", "Point"), "a field type");
    assert!(has(&e, "scaled", "SCALE"));
    assert!(!has(&e, "show3", "show"));
    assert!(m
        .definitions
        .iter()
        .all(|d| !matches!(d.name.as_deref(), Some("Geo" | "Inner"))));
}

#[test]
fn a_dynamic_include_keeps_functions_open() {
    let r = read("Dyn.jl");
    assert_eq!(r.dynamic.len(), 1);
    let e = edges(&r, &resolve(&r));
    assert!(!has(&e, "f", "origin"));
    assert!(has(&e, "g", "Point"));
}

#[test]
fn golden_module_ids() {
    let mut lines = vec!["# root\tfile\tdefinition\tstart:end\tdigest|error\tdepends_on\treferences (span>file:start:end)".to_string()];
    for root in ["Geo.jl", "Dyn.jl", "../geometry.jl", "../scopes.jl"] {
        let r = read(root);
        let m = resolve(&r);
        for d in &m.definitions {
            let id = match &d.identity {
                Ok(i) => i.digest.iter().map(|b| format!("{b:02x}")).collect(),
                Err(e) => format!("error:{e:?}"),
            };
            let deps: Vec<_> = d
                .depends_on
                .iter()
                .map(|&f| r.names[f as usize].clone())
                .collect();
            let refs: Vec<_> = d
                .references
                .iter()
                .map(|((a, b), t)| format!("{a}:{b}>{}:{}:{}", r.names[t.0 as usize], t.1, t.2))
                .collect();
            lines.push(format!(
                "{root}\t{}\t{}\t{}:{}\t{id}\t{}\t{}",
                r.names[d.key.0 as usize],
                d.name.clone().unwrap_or_default(),
                d.key.1,
                d.key.2,
                deps.join(","),
                refs.join(",")
            ));
        }
        for (f, (a, b), n) in &m.calls_generic {
            lines.push(format!(
                "{root}\t{}\tcalls_generic\t{a}:{b}\t{n}\t\t",
                r.names[*f as usize]
            ));
        }
    }
    let text = lines.join("\n") + "\n";
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("golden/module.tsv");
    if std::env::var_os("MOONKALE_BLESS").is_some() {
        std::fs::write(&path, &text).unwrap();
    }
    assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
}

/// A lone file is a module of its own: the crate's ids for it are the ones
/// the index publishes (`golden/ids.tsv`).
#[test]
fn single_file_modules_match_the_index() {
    let ids =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/golden/ids.tsv")).unwrap();
    for root in ["../geometry.jl", "../scopes.jl"] {
        let r = read(root);
        for d in resolve(&r).definitions {
            let digest: String = d
                .identity
                .unwrap()
                .digest
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            let row = format!("{}\t{}\t{digest}\t", r.names[0], d.name.unwrap());
            assert!(ids.contains(&row), "{row}");
        }
    }
}
