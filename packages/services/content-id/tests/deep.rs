//! SR-145: nesting consumes heap memory, including on wasm's fixed stack.
use moonkale_content_id::*;
use std::collections::{HashMap, HashSet};

const DEPTH: usize = 100_000;

fn small_stack(test: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(test)
        .unwrap()
        .join()
        .unwrap();
}

fn exercise(lang: &str, source: String) {
    let mut budget = Budget::new();
    let tree = parse(lang, &source, &mut budget).unwrap();
    assert!(!tree.root_node().has_error(), "{lang}: generated syntax");
    let root = tree.root_node();
    let mut scopes = JuliaScopes::default();
    julia_scopes(root, &source, &[], &mut scopes);
    let defs = definitions(
        lang,
        root,
        &source,
        NamingContext {
            package: Some("deep"),
            path: "deep",
            module: None,
        },
        &scopes,
        &mut budget,
    )
    .unwrap();
    assert_eq!(defs.len(), 1);
    let d = &defs[0];
    assert!(binding(d, "deep").is_some());
    assert_eq!(binding_groups(&defs, "deep").unwrap().len(), 1);
    let mut node = root.named_child(0).unwrap();
    while !is_definition(lang, node) {
        node = node.named_child(0).unwrap();
    }
    assert!(modules_around(node, &source).is_empty());
    assert_eq!(locate(&[], 0, &["Deep".into()], &scopes), vec!["Deep"]);
    let parts = definition::<usize>(lang, node, &source, &HashMap::new(), &mut budget).unwrap();
    let (again, hits) = definition_and_uses::<usize>(
        lang,
        node,
        &source,
        &HashMap::new(),
        &HashSet::new(),
        &mut budget,
    )
    .unwrap();
    assert!(hits.is_empty());
    // Comparing serialized parts also checks the two public normalization paths.
    let units = HashMap::from([(0, parts)]);
    assert_eq!(
        identities(&units, &mut budget).unwrap(),
        identities(&HashMap::from([(0, again)]), &mut budget).unwrap()
    );
    if lang == "julia" {
        assert!(julia_name(node).is_some());
        assert!(julia::includes(&source, &tree).is_empty());
        let module = julia::module(
            &[julia::ModuleFile {
                text: &source,
                tree: &tree,
                module: vec![],
            }],
            &[],
        )
        .unwrap();
        assert_eq!(module.definitions.len(), 1);
        assert!(module.definitions[0].identity.is_ok());
    }
}

#[test]
fn hundred_thousand_parentheses_on_small_stack() {
    small_stack(|| {
        for lang in ["rust", "julia", "python"] {
            let expression = format!("{}1{}", "(".repeat(DEPTH), ")".repeat(DEPTH));
            exercise(
                lang,
                match lang {
                    "rust" => format!("fn deep() {{ {expression} }}"),
                    "julia" => format!("deep() = {expression}"),
                    _ => format!("def deep():\n    return {expression}\n"),
                },
            );
        }
    });
}

#[test]
fn hundred_thousand_binary_operators_on_small_stack() {
    small_stack(|| {
        for lang in ["rust", "julia", "python"] {
            let expression = format!("{}1", "1+".repeat(DEPTH));
            exercise(
                lang,
                match lang {
                    "rust" => format!("fn deep() {{ {expression} }}"),
                    "julia" => format!("deep() = {expression}"),
                    _ => format!("def deep():\n    return {expression}\n"),
                },
            );
        }
    });
}

#[test]
fn hundred_thousand_blocks_on_small_stack() {
    small_stack(|| {
        exercise(
            "rust",
            format!(
                "fn deep() {{ {}1{} }}",
                "{".repeat(DEPTH),
                "}".repeat(DEPTH)
            ),
        );
        exercise(
            "julia",
            format!(
                "deep() = {}1{}",
                "begin\n".repeat(DEPTH),
                "\nend".repeat(DEPTH)
            ),
        );
        // Python requires increasing indentation for nested suites: depth 100k
        // needs >10 GB of source. Its source-byte budget rejects it before parse.
        let bytes_required = DEPTH * (DEPTH + 1) / 2;
        assert!(bytes_required > MAX_FILE_BYTES);
        assert!(matches!(
            parse(
                "python",
                &" ".repeat(MAX_FILE_BYTES + 1),
                &mut Budget::new()
            ),
            Err(ContentIdError::LimitExceeded)
        ));
    });
}

#[test]
fn exhausted_work_and_byte_budgets_refuse_without_partial_results() {
    small_stack(|| {
        for lang in ["rust", "julia", "python"] {
            assert!(matches!(
                parse(lang, "", &mut Budget::with_steps(0)),
                Err(ContentIdError::LimitExceeded)
            ));
            let src = match lang {
                "rust" => "fn f() {}",
                "julia" => "f() = 1",
                _ => "def f():\n    return 1\n",
            };
            let tree = parse(lang, src, &mut Budget::new()).unwrap();
            let node = tree.root_node().named_child(0).unwrap();
            assert!(matches!(
                definition::<usize>(lang, node, src, &HashMap::new(), &mut Budget::with_steps(0)),
                Err(ContentIdError::LimitExceeded)
            ));
            assert!(matches!(
                definitions(
                    lang,
                    tree.root_node(),
                    src,
                    NamingContext {
                        package: None,
                        path: "f",
                        module: None
                    },
                    &JuliaScopes::default(),
                    &mut Budget::with_steps(0)
                ),
                Err(ContentIdError::LimitExceeded)
            ));
        }
        let units = HashMap::from([(0, vec![Part::Bytes(vec![0; MAX_INPUT_BYTES + 1])])]);
        assert_eq!(
            identities(&units, &mut Budget::new()),
            Err(ContentIdError::LimitExceeded)
        );
        assert_eq!(
            identities(
                &HashMap::from([(0, vec![Part::Reference(1)])]),
                &mut Budget::new()
            ),
            Err(ContentIdError::MalformedSyntax)
        );
    });
}

#[test]
fn hundred_thousand_binding_patterns_and_julia_scopes_on_small_stack() {
    small_stack(|| {
        let pattern = format!("{}x{}", "(".repeat(DEPTH), ",)".repeat(DEPTH));
        exercise("rust", format!("fn deep() {{ let {pattern} = 1; }}"));
        exercise("julia", format!("deep({pattern}) = 1"));
        exercise(
            "julia",
            format!(
                "deep() where {}T{} = 1",
                "{".repeat(DEPTH),
                "}".repeat(DEPTH)
            ),
        );
        exercise(
            "julia",
            format!(
                "deep() = {}1{}",
                "let\n".repeat(DEPTH),
                "\nend".repeat(DEPTH)
            ),
        );
    });
}

#[test]
fn hundred_thousand_graph_dependencies_on_small_stack() {
    small_stack(|| {
        let mut units = HashMap::new();
        for n in 0..DEPTH {
            units.insert(
                n,
                if n + 1 < DEPTH {
                    vec![Part::Reference(n + 1)]
                } else {
                    vec![Part::Bytes(vec![1])]
                },
            );
        }
        assert_eq!(identities(&units, &mut Budget::new()).unwrap().len(), DEPTH);
        assert_eq!(
            identities(&units, &mut Budget::with_steps(10)),
            Err(ContentIdError::LimitExceeded)
        );
    });
}
