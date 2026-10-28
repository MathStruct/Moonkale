use moonkale_core::{EdgeKind, NodeId, Query, Source, Value};
use moonkale_index::IndexSource;
use moonkale_project_fs::FolderSource;
use std::{fs, sync::Arc};

fn fixture(lang: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/cross-file")
        .join(lang);
    fn copy(from: &std::path::Path, to: &std::path::Path) {
        fs::create_dir_all(to).unwrap();
        for e in fs::read_dir(from).unwrap() {
            let e = e.unwrap();
            let target = to.join(e.file_name());
            if e.file_type().unwrap().is_dir() {
                copy(&e.path(), &target);
            } else {
                fs::copy(e.path(), target).unwrap();
            }
        }
    }
    copy(&source, dir.path());
    dir
}
async fn imported(index: &IndexSource) -> Vec<moonkale_core::Edge> {
    let g = index
        .query(Query::All {
            limit: 10000,
            kinds: None,
        })
        .await
        .unwrap();
    g.edges
        .into_iter()
        .filter(|e| {
            e.kind == EdgeKind::Calls
                && e.props.get("resolved_via") == Some(&Value::Text("import".into()))
        })
        .collect()
}

#[tokio::test]
async fn explicit_imports_resolve_and_target_changes_invalidate_callers() {
    for (lang, path, ambiguous, restored) in [
        (
            "rust",
            "src/helpers.rs",
            "pub fn plus(x:i32)->i32{x+2}\npub fn plus(x:i32)->i32{x+3}\n",
            "pub fn plus(x:i32)->i32{x+4}\n",
        ),
        (
            "python",
            "helpers.py",
            "def plus(x):\n    return x+2\ndef plus(x):\n    return x+3\n",
            "def plus(x):\n    return x+4\n",
        ),
        (
            "julia",
            "helpers.jl",
            "module Helpers\nplus(x::Int)=x+2\nplus(x::Float64)=x+3\nend\n",
            "module Helpers\nplus(x)=x+4\nend\n",
        ),
    ] {
        let dir = fixture(lang);
        let folder = Arc::new(FolderSource::open(dir.path()).unwrap());
        let index = IndexSource::build(folder.clone()).await.unwrap();
        let edges = imported(&index).await;
        assert!(!edges.is_empty(), "{lang}: no imported calls");
        assert!(edges.iter().all(|e| e.props.contains_key("import")));
        let file = NodeId::derive(&folder.id(), path);
        fs::write(dir.path().join(path), ambiguous).unwrap();
        index.refresh(file).await.unwrap();
        assert!(
            imported(&index).await.is_empty(),
            "{lang}: ambiguous target"
        );
        fs::write(dir.path().join(path), restored).unwrap();
        index.refresh(file).await.unwrap();
        assert!(
            !imported(&index).await.is_empty(),
            "{lang}: restored target"
        );
        fs::remove_file(dir.path().join(path)).unwrap();
        index.refresh(file).await.unwrap();
        assert!(imported(&index).await.is_empty(), "{lang}: stale target");
        fs::write(dir.path().join(path), restored).unwrap();
        index.refresh(file).await.unwrap();
        assert!(
            !imported(&index).await.is_empty(),
            "{lang}: reappeared target"
        );
    }
}

#[tokio::test]
async fn glob_external_and_shadowed_imports_remain_unresolved() {
    for (lang, caller, text) in [
        (
            "rust",
            "src/lib.rs",
            "mod helpers; use crate::helpers::*; fn caller(){plus(1);}",
        ),
        (
            "python",
            "main.py",
            "from helpers import *\ndef caller(x):\n    return plus(x)\n",
        ),
        ("julia", "main.jl", "using Helpers\ncaller(x)=plus(x)\n"),
        (
            "rust",
            "src/lib.rs",
            "mod helpers; use outside::plus; fn caller(){plus(1);}",
        ),
        (
            "python",
            "main.py",
            "from outside import plus\ndef caller(x):\n    return plus(x)\n",
        ),
        (
            "julia",
            "main.jl",
            "using Outside: plus\ncaller(x)=plus(x)\n",
        ),
        (
            "rust",
            "src/lib.rs",
            "mod helpers; use crate::helpers::plus; fn caller(plus:fn(i32)){plus(1);}",
        ),
        (
            "python",
            "main.py",
            "from helpers import plus\ndef caller(plus):\n    return plus(1)\n",
        ),
        (
            "julia",
            "main.jl",
            "using Helpers: plus\ncaller(plus)=plus(1)\n",
        ),
    ] {
        let dir = fixture(lang);
        fs::write(dir.path().join(caller), text).unwrap();
        let folder = Arc::new(FolderSource::open(dir.path()).unwrap());
        let index = IndexSource::build(folder).await.unwrap();
        assert!(imported(&index).await.is_empty(), "{lang}: {text}");
    }
}

#[tokio::test]
async fn references_and_import_variants_and_unsupported_environments() {
    for (lang, caller, text, resolved) in [
        ("rust", "src/lib.rs", "mod helpers; use helpers::plus; fn caller(){let f = plus;}", true),
        ("python", "main.py", "import helpers\ndef caller():\n    return helpers.plus\n", true),
        ("julia", "main.jl", "import Helpers.plus\ncaller(x)=plus(x)\n", true),
        ("julia", "main.jl", "caller(x)=Helpers.plus(x)\n", true),
        ("julia", "main.jl", "module Caller\nusing Helpers: plus\ncaller(x)=plus(x)\nend\n", true),
        ("python", "main.py", "from helpers import plus\nfrom elsewhere import plus\ndef caller(x):\n    return plus(x)\n", false),
        ("python", "main.py", "from helpers import plus\nfrom elsewhere import other as plus\ndef caller(x):\n    return plus(x)\n", false),
        ("python", "main.py", "from helpers import plus\ndef outer(plus):\n    def caller():\n        return plus(1)\n    return caller()\n", false),
        ("rust", "src/lib.rs", "mod helpers; use crate::helpers::plus; mod child { fn caller(){plus(1);} }", false),
        ("julia", "main.jl", "using Helpers: plus\nmodule Child\ncaller(x)=plus(x)\nend\n", false),
        ("rust", "src/lib.rs", "#[path=\"wrong.rs\"] // attribute still applies\nmod helpers; use crate::helpers::plus; fn caller(){plus(1);}", false),
        ("rust", "src/lib.rs", "use helpers::plus; fn caller(){plus(1);}", false), // no mod declaration
    ] {
        let dir=fixture(lang); fs::write(dir.path().join(caller), text).unwrap();
        let folder=Arc::new(FolderSource::open(dir.path()).unwrap()); let index=IndexSource::build(folder).await.unwrap();
        let g=index.query(Query::All{limit:10000,kinds:None}).await.unwrap();
        let edges:Vec<_>=g.edges.iter().filter(|e| e.kind==EdgeKind::References && e.props.contains_key("resolved_via")).collect();
        assert_eq!(!edges.is_empty(),resolved,"{lang}: {text}");
    }
}

#[cfg(feature = "sophia")]
#[tokio::test]
async fn cache_import_edges_follow_target_changes_and_match_fresh_store() {
    use moonkale_sources_sophia::SophiaSource;
    for (lang, target, replacement) in [
        ("rust", "src/helpers.rs", "pub fn plus(x:i32)->i32{x+77}\n"),
        ("python", "helpers.py", "def plus(x):\n    return x+77\n"),
        ("julia", "helpers.jl", "module Helpers\nplus(x)=x+77\nend\n"),
    ] {
        let dir = fixture(lang);
        let folder = Arc::new(FolderSource::open(dir.path()).unwrap());
        let index = IndexSource::build(folder.clone()).await.unwrap();
        let path = index
            .folder_store_path()
            .unwrap_or_else(|| panic!("{lang}: {:?}", index.folder_store_report()));
        let cache = SophiaSource::open(&path).await.unwrap();
        let before = imported_cache(&cache).await;
        assert!(!before.is_empty(), "{lang}: no cache calls");
        let file = NodeId::derive(&folder.id(), target);
        fs::write(dir.path().join(target), replacement).unwrap();
        index.refresh(file).await.unwrap();
        let after = imported_cache(&cache).await;
        assert!(
            !after.is_empty(),
            "{lang}: cache lost calls after edit: {:?}",
            index.folder_store_report()
        );
        assert!(
            after
                .iter()
                .all(|e| !before.iter().any(|old| old.to == e.to)),
            "{lang}: old target id"
        );
        assert!(
            index.folder_store_report().unwrap().written.files >= 2,
            "caller manifest must update"
        );
        let writer = moonkale_index::folder_store::FolderStore::open(dir.path()).unwrap();
        let all = index
            .query(Query::All {
                limit: 10000,
                kinds: None,
            })
            .await
            .unwrap();
        let mut files = Vec::new();
        for node in all.nodes.into_iter().filter(|n| {
            n.kind == moonkale_core::NodeKind::File
                && matches!(n.language_hint(), Some("rust" | "python" | "julia"))
        }) {
            let text = folder.fetch_text(node.id).await.unwrap().0;
            files.push(moonkale_index::folder_store::CodeFile { node, text });
        }
        assert!(
            writer.check(&files).unwrap().is_empty(),
            "{lang}: fresh/cache mismatch"
        );
        // A fresh index/store comparison also covers annotation ownership.
        fs::remove_file(dir.path().join(target)).unwrap();
        index.refresh(file).await.unwrap();
        assert!(
            imported_cache(&cache).await.is_empty(),
            "{lang}: stale cache calls"
        );
        let reopened = IndexSource::build(folder.clone()).await.unwrap();
        assert_eq!(
            reopened.folder_store_report().unwrap().written.files,
            0,
            "no-op reopen"
        );
    }
}
#[cfg(feature = "sophia")]
async fn imported_cache(cache: &moonkale_sources_sophia::SophiaSource) -> Vec<moonkale_core::Edge> {
    cache
        .query(Query::All {
            limit: 10000,
            kinds: None,
        })
        .await
        .unwrap()
        .edges
        .into_iter()
        .filter(|e| e.kind == EdgeKind::Calls && e.props.contains_key("resolved_via"))
        .collect()
}

#[tokio::test]
async fn ambiguous_module_owners_do_not_resolve_a_unique_leaf() {
    for (lang, path, text) in [
        (
            "python",
            "helpers/__init__.py",
            "def other(x):\n    return x\n",
        ),
        ("julia", "duplicate.jl", "module Helpers\nother(x)=x\nend\n"),
    ] {
        let dir = fixture(lang);
        let path = dir.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
        let index = IndexSource::build(Arc::new(FolderSource::open(dir.path()).unwrap()))
            .await
            .unwrap();
        assert!(
            imported(&index).await.is_empty(),
            "{lang}: ambiguous module owner"
        );
    }
}

#[tokio::test]
async fn merged_import_edges_keep_every_occurrence() {
    let dir = fixture("python");
    let index = IndexSource::build(Arc::new(FolderSource::open(dir.path()).unwrap()))
        .await
        .unwrap();
    let calls = imported(&index).await;
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].props["count"], Value::Int(2));
    let Value::Text(ranges) = &calls[0].props["ranges"] else {
        panic!("missing ranges")
    };
    assert_eq!(ranges.split(',').count(), 2);
    let Value::Text(imports) = &calls[0].props["imports"] else {
        panic!("missing imports")
    };
    assert_eq!(
        serde_json::from_str::<Vec<String>>(imports).unwrap().len(),
        2
    );
}

#[cfg(feature = "sophia")]
#[tokio::test]
async fn selected_batch_never_combines_an_unselected_callers_new_text_with_old_records() {
    use moonkale_index::folder_store::{CodeFile, FolderStore};
    use sophia_store::{sqlite::Sqlite, Store};
    use std::collections::BTreeSet;
    let dir = fixture("python");
    let folder = Arc::new(FolderSource::open(dir.path()).unwrap());
    let index = IndexSource::build(folder.clone()).await.unwrap();
    let writer = FolderStore::open(dir.path()).unwrap();
    let store = Store::new(Sqlite::open(index.folder_store_path().unwrap()).unwrap());
    let before = store.file_manifest("main.py").unwrap().unwrap();
    fs::write(
        dir.path().join("helpers.py"),
        "def plus(x):\n    return x+99\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("main.py"),
        "from helpers import plus\ndef caller(x):\n    return plus(x)+8888\n",
    )
    .unwrap();
    let (entries, _) = moonkale_index::walk::walk(
        folder.as_ref(),
        folder.descriptor().root,
        &Default::default(),
    )
    .await
    .unwrap();
    let mut files = Vec::new();
    for (_, node) in entries
        .into_iter()
        .filter(|(_, n)| n.kind == moonkale_core::NodeKind::File)
    {
        let text = folder.fetch_text(node.id).await.unwrap().0;
        files.push(CodeFile { node, text });
    }
    writer
        .sync_selected(&files, Some(&BTreeSet::from(["helpers.py".into()])))
        .unwrap();
    assert_eq!(
        store.file_manifest("main.py").unwrap().unwrap(),
        before,
        "unselected caller remains owned by its old manifest"
    );
    writer
        .sync_selected(&files, Some(&BTreeSet::from(["main.py".into()])))
        .unwrap();
    assert_ne!(
        store.file_manifest("main.py").unwrap().unwrap().source,
        before.source
    );
    assert!(writer.check(&files).unwrap().is_empty());
}

#[cfg(feature = "sophia")]
#[tokio::test]
async fn cache_content_deduplication_keeps_one_edge_with_all_origins() {
    let dir = fixture("python");
    let caller = "from helpers import plus\ndef caller(x):\n    return plus(x)\n";
    fs::write(dir.path().join("main.py"), caller).unwrap();
    fs::write(dir.path().join("other.py"), caller).unwrap();
    let index = IndexSource::build(Arc::new(FolderSource::open(dir.path()).unwrap()))
        .await
        .unwrap();
    let cache = moonkale_sources_sophia::SophiaSource::open(index.folder_store_path().unwrap())
        .await
        .unwrap();
    let calls = imported_cache(&cache).await;
    assert_eq!(calls.len(), 1, "content-addressed edge identity");
    assert_eq!(calls[0].props["count"], Value::Int(2));
    let Value::Text(origins) = &calls[0].props["origins"] else {
        panic!("no origins")
    };
    assert_eq!(
        serde_json::from_str::<Vec<String>>(origins).unwrap(),
        vec!["main.py", "other.py"]
    );
}

#[tokio::test]
async fn rust_nested_modules_follow_both_conventional_file_layouts() {
    for module_path in ["src/helpers.rs", "src/helpers/mod.rs"] {
        let dir = fixture("rust");
        fs::remove_file(dir.path().join("src/helpers.rs")).unwrap();
        fs::create_dir_all(dir.path().join("src/helpers")).unwrap();
        fs::write(dir.path().join(module_path), "pub mod nested;\n").unwrap();
        fs::write(
            dir.path().join("src/helpers/nested.rs"),
            "pub fn plus(x:i32)->i32{x+1}\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("src/lib.rs"),
            "mod helpers; use helpers::nested::plus; fn caller(x:i32)->i32{plus(x)}\n",
        )
        .unwrap();
        let index = IndexSource::build(Arc::new(FolderSource::open(dir.path()).unwrap()))
            .await
            .unwrap();
        assert_eq!(imported(&index).await.len(), 1, "{module_path}");
    }
}
