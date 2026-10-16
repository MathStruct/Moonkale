#![cfg(feature = "content-ids")]

use moonkale_core::{NodeId, Query, Source};
use moonkale_index::{
    content::{ContentIdError, ContentIdentity},
    IndexSource,
};
use moonkale_project_fs::FolderSource;
use std::sync::Arc;

async fn identity(path: &str, text: &str, name: &str) -> (NodeId, ContentIdentity) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(path), text).unwrap();
    let folder = Arc::new(FolderSource::open(dir.path()).unwrap());
    let index = IndexSource::build(folder).await.unwrap();
    let nodes = index
        .query(Query::All {
            limit: 100,
            kinds: None,
        })
        .await
        .unwrap()
        .nodes;
    let node = nodes
        .iter()
        .find(|n| {
            n.kind.is_definition()
                && n.props.get("name") == Some(&moonkale_core::Value::Text(name.into()))
        })
        .unwrap_or_else(|| panic!("no {name}: {nodes:?}"));
    assert!(
        !node.props.contains_key("content_id"),
        "build must remain lazy"
    );
    let result = index
        .content_id(node.id)
        .await
        .unwrap_or_else(|e| panic!("{path}: {text}: {e}"));
    let published = index.query(Query::Node(node.id)).await.unwrap();
    assert_eq!(
        published.nodes[0].props["content_id"],
        moonkale_core::Value::Text(result.id.to_string())
    );
    assert_eq!(index.content_id(node.id).await.unwrap(), result);
    (node.id, result)
}

#[tokio::test]
async fn three_language_rename_move_trivia_and_body_fixtures() {
    let cases = [
        (
            "rs",
            "fn f(x: i32) -> i32 { let y = x + 1; y }",
            "fn renamed(a: i32) -> i32 { let b = a + 1; b }",
            "/// Docs\nfn f(x:i32)->i32{ /* comment */ let y=x+1; y }",
            "fn f(x: i32) -> i32 { let y = x + 2; y }",
        ),
        (
            "py",
            "def f(x):\n    y = x + 1\n    return y\n",
            "def renamed(x):\n    z = x + 1\n    return z\n",
            "# comment\ndef f(x):\n    \"\"\"Docs\"\"\"\n    y=x+1\n    return y\n",
            "def f(x):\n    y = x + 2\n    return y\n",
        ),
        (
            "jl",
            "function f(x)\n y = x + 1\n y\nend",
            "function renamed(a)\n b = a + 1\n b\nend",
            "\"Docs\"\nfunction f(x)\n # comment\n y=x+1\n y\nend",
            "function f(x)\n y = x + 2\n y\nend",
        ),
    ];
    for (extension, original, renamed, trivia, body) in cases {
        let (location, before) = identity(&format!("a.{extension}"), original, "f").await;
        let (_, rename) = identity(&format!("a.{extension}"), renamed, "renamed").await;
        assert_eq!(before.id, rename.id, "{extension}: rename");
        let (moved_location, moved) = identity(&format!("b.{extension}"), original, "f").await;
        assert_ne!(location, moved_location);
        assert_eq!(before.id, moved.id, "{extension}: move");
        let (_, commented) = identity(&format!("a.{extension}"), trivia, "f").await;
        assert_eq!(before.id, commented.id, "{extension}: trivia");
        let (_, edited) = identity(&format!("a.{extension}"), body, "f").await;
        assert_ne!(before.id, edited.id, "{extension}: body");
    }
}

#[tokio::test]
async fn dependencies_and_recursive_components_are_name_and_order_invariant() {
    let cases = [
        (
            "rs",
            "fn f() { g(); } fn g() { f(); }",
            "fn b() { a(); } fn a() { b(); }",
            "fn g() { f(); } fn f() { g(); }",
            "fn f() { g(); } fn g() { f(); 1; }",
        ),
        (
            "py",
            "def f():\n    return g()\ndef g():\n    return f()\n",
            "def b():\n    return a()\ndef a():\n    return b()\n",
            "def g():\n    return f()\ndef f():\n    return g()\n",
            "def f():\n    return g()\ndef g():\n    return f()+1\n",
        ),
        (
            "jl",
            "f() = g()\ng() = f()",
            "b() = a()\na() = b()",
            "g() = f()\nf() = g()",
            "f() = g()\ng() = f()+1",
        ),
    ];
    for (ext, original, renamed, reordered, edited) in cases {
        let path = format!("a.{ext}");
        let (_, f) = identity(&path, original, "f").await;
        let (_, g) = identity(&path, original, "g").await;
        assert_eq!(f.component_size, 2, "{ext}");
        assert_eq!(f.component_digest, g.component_digest);
        assert_eq!(
            f.id,
            identity(&path, renamed, "a").await.1.id,
            "{ext}: renamed recursion"
        );
        assert_eq!(
            f.id,
            identity(&path, reordered, "f").await.1.id,
            "{ext}: reordered recursion"
        );
        assert_ne!(
            f.id,
            identity(&path, edited, "f").await.1.id,
            "{ext}: dependency edit"
        );
    }
}

#[tokio::test]
async fn unresolved_names_are_stable_and_not_empty() {
    for (ext, src, renamed, different) in [
        (
            "rs",
            "fn f(x:i32) { external(x); }",
            "// hi\nfn f(a:i32) { external(a); }",
            "fn f(x:i32) { other(x); }",
        ),
        (
            "py",
            "def f(x):\n return external(x)\n",
            "# hi\ndef f(x):\n return external(x)\n",
            "def f(x):\n return other(x)\n",
        ),
        (
            "jl",
            "f(x) = external(x)",
            "# hi\nf(a) = external(a)",
            "f(x) = other(x)",
        ),
    ] {
        let path = format!("a.{ext}");
        let a = identity(&path, src, "f").await.1;
        assert_eq!(a.id, identity(&path, renamed, "f").await.1.id, "{ext}");
        assert_ne!(a.id, identity(&path, different, "f").await.1.id, "{ext}");
    }
}

#[tokio::test]
async fn observable_spellings_and_mutability_are_retained() {
    for (ext, a, b) in [
        ("py", "def f(x):\n return x\n", "def f(y):\n return y\n"),
        (
            "py",
            "@one\ndef f():\n return 1\n",
            "@two\ndef f():\n return 1\n",
        ),
        (
            "rs",
            "fn f() { let x = 1; x; }",
            "fn f() { let mut x = 1; x; }",
        ),
        ("jl", "f(x) = x.field", "f(x) = x.other"),
    ] {
        let path = format!("a.{ext}");
        assert_ne!(
            identity(&path, a, "f").await.1.id,
            identity(&path, b, "f").await.1.id,
            "{ext}"
        );
    }
    assert_eq!(
        identity("a.py", "def f(x, /):\n return x\n", "f")
            .await
            .1
            .id,
        identity("a.py", "def f(y, /):\n return y\n", "f")
            .await
            .1
            .id
    );
}

#[tokio::test]
async fn stale_source_and_unsupported_syntax_do_not_publish() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.py");
    std::fs::write(&path, "def f(x):\n return [i for i in x]\n").unwrap();
    let folder = Arc::new(FolderSource::open(dir.path()).unwrap());
    let index = IndexSource::build(folder).await.unwrap();
    let id = index
        .query(Query::All {
            limit: 100,
            kinds: None,
        })
        .await
        .unwrap()
        .nodes
        .into_iter()
        .find(|n| n.kind.is_definition())
        .unwrap()
        .id;
    assert_eq!(
        index.content_id(id).await,
        Err(ContentIdError::UnsupportedSyntax)
    );
    std::fs::write(path, "def f(x):\n return x+1\n").unwrap();
    assert_eq!(index.content_id(id).await, Err(ContentIdError::StaleInput));
    assert!(!index.query(Query::Node(id)).await.unwrap().nodes[0]
        .props
        .contains_key("content_id"));
}

#[tokio::test]
async fn acyclic_dependants_change_only_when_their_dependency_changes() {
    for (ext, a, b) in [
        (
            "rs",
            "fn f() { g(); } fn g() { 1; }",
            "fn f() { g(); } fn g() { 2; }",
        ),
        (
            "py",
            "def f():\n return g()\ndef g():\n return 1\n",
            "def f():\n return g()\ndef g():\n return 2\n",
        ),
        ("jl", "f() = g()\ng() = 1", "f() = g()\ng() = 2"),
    ] {
        let path = format!("a.{ext}");
        let first = identity(&path, a, "f").await.1;
        let second = identity(&path, b, "f").await.1;
        assert_ne!(first.id, second.id, "{ext}");
        assert_eq!(first.component_size, 1);
    }
}

#[tokio::test]
async fn lexical_shadowing_and_keyword_labels_are_distinguished() {
    let (a, b) = (
        "fn f(x:i32)->i32 { let x=x+1; x }",
        "fn f(a:i32)->i32 { let b=a+1; b }",
    );
    assert_eq!(
        identity("a.rs", a, "f").await.1.id,
        identity("a.rs", b, "f").await.1.id
    );
    assert_ne!(
        identity("a.py", "def f(x):\n return external(x=x)\n", "f")
            .await
            .1
            .id,
        identity("a.py", "def f(x):\n return external(y=x)\n", "f")
            .await
            .1
            .id
    );
    assert_ne!(
        identity("a.rs", "#[inline]\nfn f() {}", "f").await.1.id,
        identity("a.rs", "#[cold]\nfn f() {}", "f").await.1.id
    );
    assert_eq!(
        identity("a.rs", "#[doc = \"A\"]\nfn f() {}", "f")
            .await
            .1
            .id,
        identity("a.rs", "fn f() {}", "f").await.1.id
    );
}

#[tokio::test]
async fn persistent_cache_is_lazy_reusable_and_refresh_invalidates_dependants() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.rs");
    std::fs::write(&path, "fn f() { g(); } fn g() { 1; }").unwrap();
    let folder = Arc::new(FolderSource::open(dir.path()).unwrap());
    let store = Arc::new(moonkale_state::MemoryStore::new());
    let index = IndexSource::build(folder.clone())
        .await
        .unwrap()
        .with_content_id_store(store.clone());
    use moonkale_state::StateStore;
    assert!(store
        .scan(moonkale_state::tables::INDEX, b"moonkale-content-v1:")
        .unwrap()
        .is_empty());
    let all = index
        .query(Query::All {
            limit: 100,
            kinds: None,
        })
        .await
        .unwrap();
    let f = all.nodes.iter().find(|n| n.label == "fn f").unwrap().id;
    let file = all
        .nodes
        .iter()
        .find(|n| n.kind == moonkale_core::NodeKind::File)
        .unwrap()
        .id;
    let before = index.content_id(f).await.unwrap();
    assert!(!store
        .scan(moonkale_state::tables::INDEX, b"moonkale-content-v1:")
        .unwrap()
        .is_empty());
    let reopened = IndexSource::build(folder.clone())
        .await
        .unwrap()
        .with_content_id_store(store.clone());
    assert_eq!(before, reopened.content_id(f).await.unwrap());
    // A valid input-specific cache entry must not survive a changed dependency.
    std::fs::write(&path, "fn f() { g(); } fn g() { 2; }").unwrap();
    index.refresh(file).await.unwrap();
    assert!(!index.query(Query::Node(f)).await.unwrap().nodes[0]
        .props
        .contains_key("content_id"));
    assert_ne!(before.id, index.content_id(f).await.unwrap().id);
    std::fs::remove_file(path).unwrap();
    index.refresh(file).await.unwrap();
    assert_eq!(
        index.content_id(f).await,
        Err(ContentIdError::NotDefinition)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_refresh_discards_the_entire_in_flight_result() {
    use moonkale_state::{Batch, Entries, StateError, StateStore};
    struct Gate {
        store: moonkale_state::MemoryStore,
        entered: tokio::sync::Notify,
        once: std::sync::atomic::AtomicBool,
        released: (std::sync::Mutex<bool>, std::sync::Condvar),
    }
    impl StateStore for Gate {
        fn get(&self, t: &str, k: &[u8]) -> Result<Option<Vec<u8>>, StateError> {
            if self.once.swap(false, std::sync::atomic::Ordering::SeqCst) {
                self.entered.notify_one();
                let (lock, cv) = &self.released;
                let mut released = lock.lock().unwrap();
                while !*released {
                    released = cv.wait(released).unwrap();
                }
            }
            self.store.get(t, k)
        }
        fn scan(&self, t: &str, k: &[u8]) -> Result<Entries, StateError> {
            self.store.scan(t, k)
        }
        fn write(&self, b: Batch) -> Result<(), StateError> {
            self.store.write(b)
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.rs");
    std::fs::write(&path, "fn f() { g(); } fn g() { 1; }").unwrap();
    let folder = Arc::new(FolderSource::open(dir.path()).unwrap());
    let gate = Arc::new(Gate {
        store: Default::default(),
        entered: Default::default(),
        once: std::sync::atomic::AtomicBool::new(true),
        released: (Default::default(), Default::default()),
    });
    let index = Arc::new(
        IndexSource::build(folder)
            .await
            .unwrap()
            .with_content_id_store(gate.clone()),
    );
    let all = index
        .query(Query::All {
            limit: 100,
            kinds: None,
        })
        .await
        .unwrap();
    let f = all.nodes.iter().find(|n| n.label == "fn f").unwrap().id;
    let file = all
        .nodes
        .iter()
        .find(|n| n.kind == moonkale_core::NodeKind::File)
        .unwrap()
        .id;
    let running = index.clone();
    let task = tokio::spawn(async move { running.content_id(f).await });
    let entered =
        tokio::time::timeout(std::time::Duration::from_secs(5), gate.entered.notified()).await;
    let refresh = if entered.is_ok() {
        std::fs::write(path, "fn f() { g(); } fn g() { 2; }").unwrap();
        index.refresh(file).await
    } else {
        Ok(())
    };
    *gate.released.0.lock().unwrap() = true;
    gate.released.1.notify_all();
    entered.unwrap();
    refresh.unwrap();
    assert_eq!(task.await.unwrap(), Err(ContentIdError::StaleInput));
    let all = index
        .query(Query::All {
            limit: 100,
            kinds: None,
        })
        .await
        .unwrap();
    assert!(all
        .nodes
        .iter()
        .all(|n| !n.props.contains_key("content_id")));
}

#[tokio::test]
async fn oversized_deep_and_malformed_input_have_typed_failures() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.rs");
    std::fs::write(&path, "fn f() { 1; }").unwrap();
    let folder = Arc::new(FolderSource::open(dir.path()).unwrap());
    let index = IndexSource::build(folder).await.unwrap();
    let all = index
        .query(Query::All {
            limit: 100,
            kinds: None,
        })
        .await
        .unwrap();
    let f = all.nodes.iter().find(|n| n.label == "fn f").unwrap().id;
    let file = all
        .nodes
        .iter()
        .find(|n| n.kind == moonkale_core::NodeKind::File)
        .unwrap()
        .id;
    std::fs::write(
        &path,
        "x".repeat(moonkale_index::content::MAX_FILE_BYTES + 1),
    )
    .unwrap();
    assert_eq!(
        index.content_id(f).await,
        Err(ContentIdError::LimitExceeded)
    );
    std::fs::write(
        &path,
        format!("fn f() {{ {}1{}; }}", "(".repeat(140), ")".repeat(140)),
    )
    .unwrap();
    index.refresh(file).await.unwrap();
    assert_eq!(
        index.content_id(f).await,
        Err(ContentIdError::LimitExceeded)
    );
    std::fs::write(&path, "fn f() { let x = ; }").unwrap();
    index.refresh(file).await.unwrap();
    assert_eq!(
        index.content_id(f).await,
        Err(ContentIdError::MalformedSyntax)
    );
}

#[tokio::test]
async fn self_recursion_and_observable_lifetimes_are_explicit() {
    for (ext, a, b) in [
        (
            "rs",
            "fn f(x:i32) { f(x); }",
            "fn renamed(a:i32) { renamed(a); }",
        ),
        (
            "py",
            "def f(x):\n return f(x)\n",
            "def renamed(x):\n return renamed(x)\n",
        ),
        ("jl", "f(x) = f(x)", "renamed(a) = renamed(a)"),
    ] {
        let path = format!("a.{ext}");
        let first = identity(&path, a, "f").await.1;
        assert_eq!(first.component_size, 1);
        assert_eq!(first.id, identity(&path, b, "renamed").await.1.id);
    }
    assert_ne!(
        identity("a.rs", "fn f<'a>(x: &'a str) -> &'a str { x }", "f")
            .await
            .1
            .id,
        identity("a.rs", "fn f<'b>(x: &'b str) -> &'b str { x }", "f")
            .await
            .1
            .id
    );
}

#[tokio::test]
async fn python_docstrings_and_default_expressions_respect_their_context() {
    assert_eq!(
        identity("a.py", "def f():\n # comment\n \"Docs\"\n return 1\n", "f")
            .await
            .1
            .id,
        identity("a.py", "def f():\n return 1\n", "f").await.1.id
    );
    assert_ne!(
        identity("a.py", "def f():\n b\"data\"\n return 1\n", "f")
            .await
            .1
            .id,
        identity("a.py", "def f():\n return 1\n", "f").await.1.id
    );
    assert_eq!(
        identity("a.py", "def f(x=external, /):\n return x\n", "f")
            .await
            .1
            .id,
        identity("a.py", "def f(y=external, /):\n return y\n", "f")
            .await
            .1
            .id
    );
    assert_ne!(
        identity("a.py", "def f(x=x, /):\n return x\n", "f")
            .await
            .1
            .id,
        identity("a.py", "def f(y=y, /):\n return y\n", "f")
            .await
            .1
            .id
    );
}

#[tokio::test]
async fn lifetime_names_do_not_enter_the_value_binding_namespace() {
    assert_eq!(
        identity("a.rs", "fn f<'a>(a: &'a str) -> &'a str { a }", "f")
            .await
            .1
            .id,
        identity("a.rs", "fn f<'a>(value: &'a str) -> &'a str { value }", "f")
            .await
            .1
            .id
    );
}

#[tokio::test]
async fn unimplemented_binding_forms_fail_closed() {
    for (path, text) in [
        ("a.jl", "f(x) = map(y -> y + x, x)"),
        ("a.jl", "f(x) = [y for y in x]"),
        ("a.py", "def f(x):\n import external as x\n return x\n"),
        (
            "a.rs",
            "fn f(x: Option<i32>) { let Some(x) = x else { return; }; x; }",
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(path), text).unwrap();
        let index = IndexSource::build(Arc::new(FolderSource::open(dir.path()).unwrap()))
            .await
            .unwrap();
        let id = index
            .query(Query::All {
                limit: 100,
                kinds: None,
            })
            .await
            .unwrap()
            .nodes
            .into_iter()
            .find(|n| n.kind.is_definition())
            .unwrap()
            .id;
        assert_eq!(
            index.content_id(id).await,
            Err(ContentIdError::UnsupportedSyntax),
            "{path}: {text}"
        );
    }
}

#[tokio::test]
async fn python_variadic_binding_names_are_not_keyword_labels() {
    assert_eq!(
        identity(
            "a.py",
            "def f(*args, **kwargs):\n return args, kwargs\n",
            "f"
        )
        .await
        .1
        .id,
        identity(
            "a.py",
            "def f(*values, **options):\n return values, options\n",
            "f"
        )
        .await
        .1
        .id
    );
}
