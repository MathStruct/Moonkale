#![cfg(all(feature = "sophia", not(target_arch = "wasm32")))]
use moonkale_core::{NodeId, Query, Source, SourceTruth, TextPatch, Transaction, Value, Version};
use moonkale_index::{
    folder_store::{CodeFile, FolderStore, STORE_PATH},
    IndexSource,
};
use moonkale_project_fs::FolderSource;
use moonkale_sources_sophia::SophiaSource;
use sophia_store::{sqlite::Sqlite, LogOp, Store, Target};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    sync::Arc,
};

fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    fs::create_dir_all(p.join("src")).unwrap();
    fs::create_dir_all(p.join("pkg")).unwrap();
    fs::write(
        p.join("Cargo.toml"),
        "[package]\nname = \"demo-crate\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    fs::write(
        p.join("src/lib.rs"),
        "pub fn add(x: i32) -> i32 { x + 1 }\n",
    )
    .unwrap();
    fs::write(p.join("pkg/__init__.py"), "").unwrap();
    fs::write(p.join("pkg/a.py"), "def double(x):\n    return x * 2\n").unwrap();
    fs::write(
        p.join("P.jl"),
        "module P\ninclude(\"a.jl\")\ninclude(\"b.jl\")\nend\n",
    )
    .unwrap();
    fs::write(
        p.join("a.jl"),
        "const answer = 1\nf(x::Int) = x\nf(x::Float64) = x\ncaller(x) = f(x)\n",
    )
    .unwrap();
    fs::write(p.join("b.jl"), "const other = 2\n").unwrap();
    fs::write(p.join(".gitignore"), "ignored.py\n").unwrap();
    fs::write(p.join("ignored.py"), "SECRET = 'do not store'\n").unwrap();
    fs::write(p.join(".env"), "SECRET=do-not-store\n").unwrap();
    fs::write(p.join("config.toml"), "secret = 'do not store'\n").unwrap();
    fs::write(p.join("large.py"), " ".repeat(512 * 1024 + 1)).unwrap();
    dir
}
async fn open(dir: &tempfile::TempDir) -> (Arc<FolderSource>, IndexSource) {
    let folder = Arc::new(FolderSource::open(dir.path()).unwrap());
    let index = IndexSource::build(folder.clone()).await.unwrap();
    (folder, index)
}
fn store(dir: &tempfile::TempDir) -> Store<Sqlite> {
    Store::new(Sqlite::open(dir.path().join(STORE_PATH)).unwrap())
}
fn manifests(s: &Store<Sqlite>) -> BTreeMap<String, sophia_store::FileManifest> {
    s.files()
        .unwrap()
        .into_iter()
        .map(|p| {
            let m = s.file_manifest(&p).unwrap().unwrap();
            (p, m)
        })
        .collect()
}
fn content_names(s: &Store<Sqlite>) -> BTreeMap<String, sophia_hash::Digest> {
    s.read(|tx| {
        Ok(tx
            .names_under("", 1000)?
            .into_iter()
            .filter_map(|n| match n.target {
                Target::Content(id) => Some((n.path, id)),
                _ => None,
            })
            .collect())
    })
    .unwrap()
}

#[tokio::test]
async fn reopen_is_a_measured_noop_and_only_eligible_code_enters_store() {
    let dir = fixture();
    let (_, first) = open(&dir).await;
    let r = first.folder_store_report().unwrap();
    assert_eq!(r.written.files, 6);
    assert_eq!(r.parsed_files, 6);
    let s = store(&dir);
    let before = manifests(&s);
    let seq = s.last_seq().unwrap();
    let names = s.read(|tx| tx.names_under("", 1000)).unwrap();
    assert!(names.iter().any(|n| n.path == "rs.demo_crate.add"));
    assert!(names.iter().any(|n| n.path == "py.pkg.a.double"));
    assert!(names.iter().any(|n| n.path == "jl.P.answer"));
    assert!(!before["a.jl"].facts.is_empty(), "generic call fact");
    assert!(before.keys().all(|p| !p.contains("ignored")
        && !p.starts_with('.')
        && !p.ends_with("toml")
        && p != "large.py"));
    let (_, second) = open(&dir).await;
    let r = second.folder_store_report().unwrap();
    assert_eq!(
        (r.parsed_files, r.written.files, r.skipped_files),
        (0, 0, 6)
    );
    assert_eq!(manifests(&s), before);
    assert_eq!(s.last_seq().unwrap(), seq);
    assert_eq!(
        fs::read_to_string(dir.path().join(".moonkale/.gitignore")).unwrap(),
        "*\n"
    );
}

#[tokio::test]
async fn one_file_change_rewrites_only_that_manifest_and_definition_id_matches_index() {
    let dir = fixture();
    let (folder, index) = open(&dir).await;
    let s = store(&dir);
    let before = manifests(&s);
    fs::write(
        dir.path().join("src/lib.rs"),
        "pub fn add(x: i32) -> i32 { x + 2 }\n",
    )
    .unwrap();
    index
        .refresh_batch(&[NodeId::derive(&folder.id(), "src/lib.rs")])
        .await
        .unwrap();
    assert_eq!(index.folder_store_report().unwrap().written.files, 1);
    let after = manifests(&s);
    let diff: Vec<_> = before.keys().filter(|p| before[*p] != after[*p]).collect();
    assert_eq!(diff, vec!["src/lib.rs"]);
    let all = index
        .query(Query::All {
            limit: 1000,
            kinds: None,
        })
        .await
        .unwrap();
    let def = all
        .nodes
        .iter()
        .find(|n| n.label.contains("add") && n.kind.is_definition())
        .unwrap();
    let id = index.content_id(def.id).await.unwrap();
    let target = s.resolve("rs.demo_crate.add").unwrap().unwrap().target;
    let Target::Nominal(generic) = target else {
        panic!("function group")
    };
    let members = s.read(|tx| tx.relation(&generic)).unwrap();
    assert_eq!(members[0].member.as_bytes(), &id.digest);
}

#[tokio::test]
async fn debounce_move_is_one_batch_and_feed_reports_move_without_delete_create() {
    let dir = fixture();
    let (folder, index) = open(&dir).await;
    let s = store(&dir);
    let seq = s.last_seq().unwrap();
    fs::write(
        dir.path().join("a.jl"),
        "f(x::Int) = x\nf(x::Float64) = x\ncaller(x) = f(x)\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("b.jl"),
        "const other = 2\nconst answer = 3\n",
    )
    .unwrap();
    index
        .refresh_batch(&[
            NodeId::derive(&folder.id(), "a.jl"),
            NodeId::derive(&folder.id(), "b.jl"),
        ])
        .await
        .unwrap();
    assert_eq!(index.folder_store_report().unwrap().written.files, 2);
    let feed = s.read(|tx| tx.log_since(seq, 1000)).unwrap();
    let ops: Vec<_> = feed
        .iter()
        .filter(|e| e.subject == "jl.P.answer")
        .map(|e| e.op)
        .collect();
    assert_eq!(ops, vec![LogOp::MoveName]);
    let source = SophiaSource::open(dir.path().join(STORE_PATH))
        .await
        .unwrap();
    let all = source
        .query(Query::All {
            limit: 1000,
            kinds: None,
        })
        .await
        .unwrap();
    let name = all
        .nodes
        .iter()
        .find(|n| n.native_key == "name:jl.P.answer")
        .unwrap();
    assert_eq!(
        source.fetch_text(name.id).await.unwrap().0,
        "const answer = 3"
    );
}

async fn inputs(folder: &FolderSource) -> Vec<CodeFile> {
    let (entries, _) =
        moonkale_index::walk::walk(folder, folder.descriptor().root, &Default::default())
            .await
            .unwrap();
    let mut files = Vec::new();
    for (_, node) in entries {
        if moonkale_index::walk::wants_text(&node, &Default::default())
            && matches!(node.language_hint(), Some("rust" | "julia" | "python"))
        {
            let (text, _) = folder.fetch_text(node.id).await.unwrap();
            files.push(CodeFile { node, text });
        }
    }
    files
}
#[tokio::test]
async fn name_claimed_adds_owner_and_retries_once() {
    let dir = fixture();
    let (folder, _) = open(&dir).await;
    fs::write(dir.path().join("a.jl"), "").unwrap();
    fs::write(dir.path().join("b.jl"), "const answer = 8\n").unwrap();
    let files = inputs(&folder).await;
    let writer = FolderStore::open(dir.path()).unwrap();
    let r = writer
        .sync_selected(&files, Some(&BTreeSet::from(["b.jl".to_string()])))
        .unwrap();
    assert_eq!(r.retries, 1);
    assert_eq!(r.written.files, 2);
    let s = store(&dir);
    assert!(s.file_manifest("a.jl").unwrap().unwrap().names.is_empty());
    assert!(s.resolve("jl.P.answer").unwrap().is_some());
}

#[tokio::test]
async fn deleting_cache_and_normaliser_mismatch_rebuild_content_names_and_ids() {
    let dir = fixture();
    let (_, first) = open(&dir).await;
    let names = content_names(&store(&dir));
    drop(first);
    fs::remove_file(dir.path().join(STORE_PATH)).unwrap();
    let (_, second) = open(&dir).await;
    assert_eq!(content_names(&store(&dir)), names);
    store(&dir)
        .set_meta("folder/normaliser", b"moonkale-wp66@0")
        .unwrap();
    let (_, third) = open(&dir).await;
    assert!(third.folder_store_report().unwrap().rebuilt);
    assert_eq!(content_names(&store(&dir)), names);
    assert_eq!(third.folder_store_report().unwrap().written.files, 6);
    drop(second);
}

#[tokio::test]
async fn derived_source_refuses_writes_and_falls_back_to_current_file_with_stale_marker() {
    let dir = fixture();
    let (folder, index) = open(&dir).await;
    let source = SophiaSource::open(dir.path().join(STORE_PATH))
        .await
        .unwrap();
    assert_eq!(source.descriptor().truth, SourceTruth::Files);
    assert!(!source.descriptor().capabilities.write);
    let all = source
        .query(Query::All {
            limit: 1000,
            kinds: None,
        })
        .await
        .unwrap();
    let name = all
        .nodes
        .iter()
        .find(|n| n.native_key == "name:jl.P.answer")
        .unwrap();
    assert_eq!(
        source.fetch_text(name.id).await.unwrap().0,
        "const answer = 1"
    );
    let fresh = "# changed before watcher\nconst answer = 99\n";
    fs::write(dir.path().join("a.jl"), fresh).unwrap();
    assert_eq!(source.fetch_text(name.id).await.unwrap().0, fresh);
    assert_eq!(source.stale_files(), vec!["a.jl"]);
    assert_eq!(
        source.query(Query::Node(name.id)).await.unwrap().nodes[0].props["stale"],
        Value::Bool(true)
    );
    assert!(source
        .apply(Transaction::write_text(
            name.id,
            Version(0),
            TextPatch::whole("bad", 1)
        ))
        .await
        .is_err());
    index
        .refresh(NodeId::derive(&folder.id(), "a.jl"))
        .await
        .unwrap();
    assert_eq!(
        source.fetch_text(name.id).await.unwrap().0,
        "const answer = 99"
    );
    assert!(source.stale_files().is_empty());
    // A live reader must follow recreation, rather than hold the deleted DB.
    fs::remove_file(dir.path().join(STORE_PATH)).unwrap();
    index
        .refresh_batch(&[folder.descriptor().root])
        .await
        .unwrap();
    let all = source
        .query(Query::All {
            limit: 1000,
            kinds: None,
        })
        .await
        .unwrap();
    assert!(all.nodes.iter().any(|n| n.native_key == "name:jl.P.answer"));
}

#[tokio::test]
async fn julia_dependency_change_rewrites_unchanged_caller_and_retracts_generic_facts() {
    let dir = fixture();
    let (folder, index) = open(&dir).await;
    // Move the caller into the other included file, keeping the module.
    fs::write(
        dir.path().join("a.jl"),
        "f(x::Int) = x\nf(x::Float64) = x\n",
    )
    .unwrap();
    fs::write(dir.path().join("b.jl"), "caller(x) = f(x)\n").unwrap();
    index
        .refresh_batch(&[folder.descriptor().root])
        .await
        .unwrap();
    let s = store(&dir);
    let before = s.file_manifest("b.jl").unwrap().unwrap();
    assert_eq!(before.facts.len(), 1);
    let subject = before.spans[0].0;
    let source = SophiaSource::open(dir.path().join(STORE_PATH))
        .await
        .unwrap();
    let all = source
        .query(Query::All {
            limit: 1000,
            kinds: None,
        })
        .await
        .unwrap();
    assert!(all
        .edges
        .iter()
        .any(|e| e.kind == moonkale_core::EdgeKind::Calls));
    // With one method, f becomes an ordinary content reference. The caller
    // bytes are unchanged, but its identity and file-owned fact must change.
    fs::write(dir.path().join("a.jl"), "f(x::Int) = x\n").unwrap();
    index
        .refresh(NodeId::derive(&folder.id(), "a.jl"))
        .await
        .unwrap();
    let after = s.file_manifest("b.jl").unwrap().unwrap();
    assert_eq!(before.source, after.source);
    assert_ne!(before.spans, after.spans);
    assert!(after.facts.is_empty());
    assert!(s.read(|tx| tx.facts_of(&subject)).unwrap().is_empty());
}

#[tokio::test]
async fn duplicate_claim_rolls_back_and_is_reported_without_an_unbounded_retry() {
    let dir = fixture();
    let (folder, index) = open(&dir).await;
    fs::write(
        dir.path().join("src/lib.rs"),
        "pub const COLLISION: i32 = 1;\n",
    )
    .unwrap();
    index
        .refresh(NodeId::derive(&folder.id(), "src/lib.rs"))
        .await
        .unwrap();
    let s = store(&dir);
    let before = manifests(&s);
    let seq = s.last_seq().unwrap();
    fs::write(
        dir.path().join("src/main.rs"),
        "pub const COLLISION: i32 = 2;\n",
    )
    .unwrap();
    assert!(index
        .refresh(NodeId::derive(&folder.id(), "src/main.rs"))
        .await
        .is_err());
    assert!(index.folder_store_report().is_err());
    assert_eq!(s.last_seq().unwrap(), seq);
    assert_eq!(manifests(&s), before);
}

#[cfg(unix)]
#[test]
fn cache_paths_cannot_follow_symlinks() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    symlink(outside.path(), dir.path().join(".moonkale")).unwrap();
    assert!(FolderStore::open(dir.path()).is_err());
    fs::remove_file(dir.path().join(".moonkale")).unwrap();
    fs::create_dir(dir.path().join(".moonkale")).unwrap();
    symlink(
        outside.path().join("untouched"),
        dir.path().join(STORE_PATH),
    )
    .unwrap();
    assert!(FolderStore::open(dir.path()).is_err());
    assert!(!outside.path().join("untouched").exists());
}

#[cfg(unix)]
#[tokio::test]
async fn code_symlink_to_hidden_secret_is_not_persisted() {
    let dir = fixture();
    std::os::unix::fs::symlink(".env", dir.path().join("alias.py")).unwrap();
    let (_, index) = open(&dir).await;
    assert!(index.folder_store_report().is_ok());
    assert!(!store(&dir).files().unwrap().iter().any(|p| p == "alias.py"));
}
