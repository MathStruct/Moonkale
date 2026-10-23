use moonkale_content_id::{
    binding, binding_groups, definitions, entity_path, julia_scopes, parse, Binding, Budget,
    Definition, JuliaScopes, NamingContext,
};
fn extract(lang: &str, src: &str, path: &str, package: Option<&str>) -> Vec<Definition> {
    let mut budget = Budget::new();
    let tree = parse(lang, src, &mut budget).unwrap();
    let mut scopes = JuliaScopes::default();
    julia_scopes(tree.root_node(), src, &[], &mut scopes);
    definitions(
        lang,
        tree.root_node(),
        src,
        NamingContext {
            package,
            path,
            module: None,
        },
        &scopes,
        &mut budget,
    )
    .unwrap()
}
fn path(d: &Definition, package: &str, prefix: &str) -> Option<String> {
    binding(d, package).map(|b| {
        entity_path(
            prefix,
            match &b {
                Binding::Method(k) | Binding::Named(k) => k,
            },
        )
    })
}

// Sophia's validate_path / split_path, 773d9898, MIT, as an independent check.
fn valid_path(path: &str) -> bool {
    let (mut start, mut quoted) = (0, false);
    let mut segments = Vec::new();
    for (i, c) in path.char_indices() {
        match c {
            '`' => quoted = !quoted,
            '.' if !quoted => {
                segments.push(&path[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    segments.push(&path[start..]);
    !quoted
        && segments.into_iter().all(|s| {
            if let Some(op) = s.strip_prefix('`').and_then(|s| s.strip_suffix('`')) {
                return !op.is_empty()
                    && !op
                        .chars()
                        .any(|c| c.is_whitespace() || c.is_control() || c == '`');
            }
            let mut chars = s.chars();
            chars
                .next()
                .is_some_and(|f| !f.is_ascii_digit() && (f.is_alphanumeric() || f == '_'))
                && chars.all(|c| c.is_alphanumeric() || c == '_' || c == '!')
        })
}

#[test]
fn rust_module_tree_impls_and_traits() {
    let ds = extract("rust", "mod nested { struct Foo<T>(T); impl Foo<i32> { fn new() {} } impl Foo<u8> { fn new() {} } impl fmt::Display for Foo<i32> { fn fmt() {} } }", "src/m/mod.rs", Some("c"));
    let ps: Vec<_> = ds.iter().filter_map(|d| path(d, "c", "rs")).collect();
    assert!(ps.contains(&"rs.c.m.nested.Foo.new".to_string()), "{ps:?}");
    assert!(
        ps.contains(&"rs.c.m.nested.Foo.`fmt::Display`.fmt".to_string()),
        "{ps:?}"
    );
    let groups = binding_groups(&ds, "c").unwrap();
    assert!(groups
        .iter()
        .any(|g| g.members.len() == 2 && matches!(&g.binding, Binding::Method(_))));
    assert!(ps.iter().all(|p| valid_path(p)));
    for file in ["src/lib.rs", "src/main.rs"] {
        assert_eq!(
            path(&extract("rust", "fn f() {}", file, Some("c"))[0], "c", "rs").unwrap(),
            "rs.c.f"
        );
    }
    assert_eq!(
        path(
            &extract("rust", "fn f() {}", "src/m.rs", Some("c"))[0],
            "c",
            "rs"
        )
        .unwrap(),
        "rs.c.m.f"
    );
}
#[test]
fn python_package_and_unknown_root() {
    let ds = extract(
        "python",
        "class C:\n def f(self): pass\n",
        "sub/mod.py",
        Some("pkg"),
    );
    assert_eq!(path(&ds[1], "pkg", "py").unwrap(), "py.pkg.sub.mod.C.f");
    assert_eq!(
        path(
            &extract("python", "def f(): pass", "sub/__init__.py", Some("pkg"))[0],
            "pkg",
            "py"
        )
        .unwrap(),
        "py.pkg.sub.f"
    );
    let d = &extract("python", "def f(): pass", "loose/file.py", None)[0];
    assert!(d.path_derived);
    assert_eq!(path(d, "ignored", "py").unwrap(), "py.loose.file.f");
}
#[test]
fn caller_can_supply_module_tree_overrides() {
    let src = "mod inner { fn f() {} }";
    let tree = parse("rust", src, &mut Budget::new()).unwrap();
    let module = vec!["logical".into()];
    let ds = definitions(
        "rust",
        tree.root_node(),
        src,
        NamingContext {
            package: Some("c"),
            path: "odd/path.rs",
            module: Some(&module),
        },
        &JuliaScopes::default(),
        &mut Budget::new(),
    )
    .unwrap();
    assert_eq!(path(&ds[1], "c", "rs").unwrap(), "rs.c.logical.inner.f");
}
#[test]
fn unspellable_names_are_reported_without_sanitizing() {
    let mut d = extract("rust", "fn f() {}", "src/lib.rs", Some("c"))[0].clone();
    for bad in ["a b", "a`b", "", "a\nb", "\0"] {
        d.name = bad.into();
        assert!(binding(&d, "c").is_none());
    }
    for name in ["+", "==", "9x", "a.b", "λ!", "fmt::Display"] {
        d.name = name.into();
        assert!(valid_path(&path(&d, "c", "rs").unwrap()));
    }
    let ds = extract("julia", "Base.$f(x) = x\n", "a.jl", Some("Pkg"));
    assert!(ds
        .iter()
        .all(|d| d.diagnostic.is_some() && binding(d, "Pkg").is_none()));
    let ds = extract("rust", "fn f() {}", "a b.rs", Some("c"));
    assert!(ds[0].diagnostic.is_some());
}
#[test]
fn named_collisions_are_explicit() {
    let d = extract("rust", "struct T;", "lib.rs", Some("c"));
    assert!(binding_groups(&[d[0].clone(), d[0].clone()], "c").is_err());
}

// Port of Sophia tests/generics.rs: checks naming and membership without a store.
#[test]
fn generic_functions_are_module_qualified() {
    let sources = [
        include_str!("../golden/generics/Gen.jl"),
        include_str!("../golden/generics/sub.jl"),
    ];
    let trees: Vec<_> = sources
        .iter()
        .map(|s| parse("julia", s, &mut Budget::new()).unwrap())
        .collect();
    let bases = [vec![], vec!["Gen".to_string()]];
    let mut scopes = JuliaScopes::default();
    for i in 0..2 {
        julia_scopes(trees[i].root_node(), sources[i], &bases[i], &mut scopes);
    }
    let mut ds = Vec::new();
    for i in 0..2 {
        ds.extend(
            definitions(
                "julia",
                trees[i].root_node(),
                sources[i],
                NamingContext {
                    package: Some("Gen"),
                    path: "",
                    module: Some(&bases[i]),
                },
                &scopes,
                &mut Budget::new(),
            )
            .unwrap(),
        );
    }
    let groups = binding_groups(&ds, "Gen").unwrap();
    let methods = |p: &str| {
        groups
            .iter()
            .find(|g| match &g.binding {
                Binding::Method(k) => entity_path("jl", k) == p,
                _ => false,
            })
            .unwrap()
            .members
            .len()
    };
    assert_eq!(methods("jl.Gen.A.f"), 4);
    assert_eq!(methods("jl.Gen.B.f"), 1);
    assert_eq!(methods("jl.Gen.f"), 2);
    assert_eq!(methods("jl.Gen.Point.new"), 2);
    for p in [
        "jl.Base.show",
        "jl.Base.length",
        "jl.Base.`+`",
        "jl.Base.getindex",
    ] {
        assert_eq!(methods(p), 1);
    }
    let expected: Vec<_> = groups
        .iter()
        .map(|g| (format!("{:?}", g.binding), g.members.len()))
        .collect();
    ds.reverse();
    let reversed: Vec<_> = binding_groups(&ds, "Gen")
        .unwrap()
        .iter()
        .map(|g| (format!("{:?}", g.binding), g.members.len()))
        .collect();
    assert_eq!(expected, reversed);
}

#[test]
fn base_submodules_relative_paths_and_import_order() {
    let src = "module P\nBroadcast.broadcasted(x) = x\nBase.:(==)(x,y) = true\nend";
    let ds = extract("julia", src, "", Some("P"));
    assert_eq!(
        path(&ds[0], "P", "jl").unwrap(),
        "jl.Base.Broadcast.broadcasted"
    );
    assert_eq!(path(&ds[1], "P", "jl").unwrap(), "jl.Base.`==`");
    let sources = [
        "module P\nimport A: f\nend",
        "module P\nimport B: f\nf(x)=x\nend",
    ];
    let trees: Vec<_> = sources
        .iter()
        .map(|s| parse("julia", s, &mut Budget::new()).unwrap())
        .collect();
    for order in [[0, 1], [1, 0]] {
        let mut scopes = JuliaScopes::default();
        for i in order {
            julia_scopes(trees[i].root_node(), sources[i], &[], &mut scopes);
        }
        let ds = definitions(
            "julia",
            trees[1].root_node(),
            sources[1],
            NamingContext {
                package: Some("P"),
                path: "",
                module: None,
            },
            &scopes,
            &mut Budget::new(),
        )
        .unwrap();
        assert!(ds[0].diagnostic.is_some());
    }
}

#[test]
fn golden_bindings() {
    use std::path::Path;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("golden");
    let fixtures = [
        ("geometry.rs", "rust", vec![]),
        ("geometry.py", "python", vec![]),
        ("geometry.jl", "julia", vec![]),
        ("scopes.jl", "julia", vec![]),
        ("module/Geo.jl", "julia", vec![]),
        ("module/types.jl", "julia", vec!["Geo".to_string()]),
        ("module/ops.jl", "julia", vec!["Geo".to_string()]),
        ("module/Dyn.jl", "julia", vec![]),
        ("module/Callable.jl", "julia", vec![]),
        ("module/Macros.jl", "julia", vec![]),
        ("generics/Gen.jl", "julia", vec![]),
        ("generics/sub.jl", "julia", vec!["Gen".to_string()]),
    ];
    let texts: Vec<_> = fixtures
        .iter()
        .map(|(f, _, _)| std::fs::read_to_string(root.join(f)).unwrap())
        .collect();
    let trees: Vec<_> = fixtures
        .iter()
        .zip(&texts)
        .map(|((_, lang, _), src)| parse(lang, src, &mut Budget::new()).unwrap())
        .collect();
    let mut scopes = JuliaScopes::default();
    for (i, (_, lang, base)) in fixtures.iter().enumerate() {
        if *lang == "julia" {
            julia_scopes(trees[i].root_node(), &texts[i], base, &mut scopes);
        }
    }
    let mut rows = Vec::new();
    for (i, (file, lang, base)) in fixtures.iter().enumerate() {
        let context = NamingContext {
            package: Some("Fixture"),
            path: file,
            module: if *lang == "julia" { Some(base) } else { None },
        };
        for d in definitions(
            lang,
            trees[i].root_node(),
            &texts[i],
            context,
            &scopes,
            &mut Budget::new(),
        )
        .unwrap()
        {
            let prefix = match *lang {
                "rust" => "rs",
                "python" => "py",
                _ => "jl",
            };
            let p = path(&d, "Fixture", prefix);
            if let Some(p) = &p {
                assert!(valid_path(p), "{p}");
            }
            rows.push(format!(
                "{file}\t{}\t{}\t{:?}\t{:?}\t{:?}\t{:?}\n",
                d.start,
                d.end,
                d.name,
                binding(&d, "Fixture"),
                p,
                d.diagnostic
            ));
        }
    }
    let actual = String::from("# file\tstart\tend\tname\tbinding\tentity_path\tdiagnostic\n")
        + &rows.concat();
    let target = root.join("bindings.tsv");
    if std::env::var_os("MOONKALE_BLESS_BINDINGS").is_some() {
        std::fs::write(&target, &actual).unwrap();
    }
    assert_eq!(std::fs::read_to_string(target).unwrap(), actual);
}

#[test]
fn rust_raw_names_trait_arguments_and_path_trivia() {
    let ds = extract(
        "rust",
        "mod r#type { struct Foo<T>(T); impl fmt :: Display<u8> for Foo<i32> { fn r#match() {} } }",
        "src/lib.rs",
        Some("c"),
    );
    assert!(
        ds.iter()
            .any(|d| path(d, "c", "rs").as_deref() == Some("rs.c.type.Foo.`fmt::Display`.match")),
        "{ds:?}"
    );
    let ds = extract("rust", "fn f() {}", "src/lib.rs", None);
    assert!(ds[0].path_derived);
    assert_eq!(path(&ds[0], "ignored", "rs").unwrap(), "rs.src.lib.f");
}

#[test]
fn file_enumeration_order_does_not_change_names_or_membership() {
    for (lang, prefix, a, b) in [
        (
            "rust",
            "rs",
            "impl Foo<i32> {fn new() {}}",
            "impl Foo<u8> {fn new() {}}",
        ),
        (
            "python",
            "py",
            "class C:\n def f(self): pass",
            "class D:\n def f(self): pass",
        ),
    ] {
        let module = vec!["m".to_string()];
        let extract_file = |src: &str| {
            let tree = parse(lang, src, &mut Budget::new()).unwrap();
            definitions(
                lang,
                tree.root_node(),
                src,
                NamingContext {
                    package: Some("p"),
                    path: "",
                    module: Some(&module),
                },
                &JuliaScopes::default(),
                &mut Budget::new(),
            )
            .unwrap()
        };
        let da = extract_file(a);
        let db = extract_file(b);
        let summarize = |ds: &[Definition]| {
            binding_groups(ds, "p")
                .unwrap()
                .iter()
                .map(|g| {
                    (
                        match &g.binding {
                            Binding::Method(k) | Binding::Named(k) => entity_path(prefix, k),
                        },
                        g.members.len(),
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            summarize(&[da.clone(), db.clone()].concat()),
            summarize(&[db, da].concat())
        );
    }
}

#[test]
fn metadata_extraction_obeys_the_request_budget() {
    let src = "fn f() {}";
    let tree = parse("rust", src, &mut Budget::new()).unwrap();
    assert!(definitions(
        "rust",
        tree.root_node(),
        src,
        NamingContext {
            package: Some("c"),
            path: "src/lib.rs",
            module: None
        },
        &JuliaScopes::default(),
        &mut Budget::with_steps(0)
    )
    .is_err());
    let ds = extract("julia", "var\"not a name\"(x) = x", "", Some("P"));
    assert!(!ds.is_empty());
    assert!(ds
        .iter()
        .all(|d| binding(d, "P").is_none() && d.diagnostic.is_some()));
}
