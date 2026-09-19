use moonkale_core::{Direction, EdgeKind, NodeKind, Query, Source, TextPatch, Transaction};
use moonkale_index::IndexSource;
use moonkale_project_fs::FolderSource;
use std::fs;
use std::sync::Arc;

fn vault() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    fs::create_dir_all(p.join("notes")).unwrap();
    fs::create_dir_all(p.join("src")).unwrap();
    fs::write(
        p.join("Home.md"),
        "# Home\nSee [[Alpha]] and [[notes/Beta]] and [[Missing]].\n",
    )
    .unwrap();
    fs::write(
        p.join("Alpha.md"),
        "Back to [[Home]]. Code: [lib](src/lib.md)\n",
    )
    .unwrap();
    fs::write(
        p.join("notes/Beta.md"),
        "Beta links [[Alpha|the first]] and [[Alpha#Section]].\n",
    )
    .unwrap();
    fs::write(
        p.join("src/lib.rs"),
        "pub struct Thing;\nimpl Thing { pub fn go(&self) {} }\npub fn free() {}\n",
    )
    .unwrap();
    fs::write(p.join("logo.png"), [0x89, b'P']).unwrap();
    dir
}

async fn open(dir: &tempfile::TempDir) -> (Arc<FolderSource>, IndexSource) {
    let folder = Arc::new(FolderSource::open(dir.path()).unwrap());
    let index = IndexSource::build(folder.clone()).await.unwrap();
    (folder, index)
}

#[tokio::test]
async fn links_resolve_by_stem_path_and_phantom() {
    let dir = vault();
    let (_, index) = open(&dir).await;
    let s = index.stats();
    assert_eq!((s.files, s.links, s.symbols), (5, 6, 4), "{s:?}");

    let all = index
        .query(Query::All {
            limit: 100,
            kinds: None,
        })
        .await
        .unwrap();
    let phantom = all
        .nodes
        .iter()
        .find(|n| n.kind == NodeKind::Page && n.label == "Missing")
        .expect("phantom page");
    let home = all
        .nodes
        .iter()
        .find(|n| n.native_key == "Home.md")
        .unwrap();
    assert!(all
        .edges
        .iter()
        .any(|e| e.from == home.id && e.to == phantom.id && e.kind == EdgeKind::Links));

    // Beta → Alpha resolved twice (alias + heading) to one edge.
    let beta = all
        .nodes
        .iter()
        .find(|n| n.native_key == "notes/Beta.md")
        .unwrap();
    let alpha = all
        .nodes
        .iter()
        .find(|n| n.native_key == "Alpha.md")
        .unwrap();
    assert_eq!(
        all.edges
            .iter()
            .filter(|e| e.from == beta.id && e.to == alpha.id)
            .count(),
        1
    );
}

#[tokio::test]
async fn backlinks_are_incoming_neighbours() {
    let dir = vault();
    let (_, index) = open(&dir).await;
    let all = index
        .query(Query::All {
            limit: 100,
            kinds: Some(vec![NodeKind::File]),
        })
        .await
        .unwrap();
    let alpha = all
        .nodes
        .iter()
        .find(|n| n.native_key == "Alpha.md")
        .unwrap();
    let res = index
        .query(Query::Neighbours {
            node: alpha.id,
            depth: 1,
            direction: Direction::In,
        })
        .await
        .unwrap();
    // Backlinks = sources of incoming `Links` edges (the parent directory's `Contains` edge is incoming too).
    let mut from: Vec<_> = res
        .edges
        .iter()
        .filter(|e| e.to == alpha.id && e.kind == EdgeKind::Links)
        .filter_map(|e| res.nodes.iter().find(|n| n.id == e.from))
        .map(|n| n.native_key.as_str())
        .collect();
    from.sort();
    assert_eq!(from, ["Home.md", "notes/Beta.md"]);
}

#[tokio::test]
async fn rust_symbols_are_children_of_the_file() {
    let dir = vault();
    let (_, index) = open(&dir).await;
    let all = index
        .query(Query::All {
            limit: 100,
            kinds: Some(vec![NodeKind::File]),
        })
        .await
        .unwrap();
    let lib = all
        .nodes
        .iter()
        .find(|n| n.native_key == "src/lib.rs")
        .unwrap();
    let kids = index.query(Query::Children(lib.id)).await.unwrap();
    let mut labels: Vec<_> = kids.nodes.iter().map(|n| n.label.as_str()).collect();
    labels.sort();
    assert_eq!(labels, ["fn free", "fn go", "impl Thing", "struct Thing"]);
}

#[tokio::test]
async fn refresh_reindexes_one_file() {
    let dir = vault();
    let (folder, index) = open(&dir).await;
    let all = index
        .query(Query::All {
            limit: 100,
            kinds: Some(vec![NodeKind::File]),
        })
        .await
        .unwrap();
    let home = all
        .nodes
        .iter()
        .find(|n| n.native_key == "Home.md")
        .unwrap();
    let (text, v) = folder.fetch_text(home.id).await.unwrap();
    folder
        .apply(Transaction::write_text(
            home.id,
            v,
            TextPatch::whole("Only [[Alpha]] now.\n", text.chars().count()),
        ))
        .await
        .unwrap();
    index.refresh(home.id).await.unwrap();
    let out = index
        .query(Query::Neighbours {
            node: home.id,
            depth: 1,
            direction: Direction::Out,
        })
        .await
        .unwrap();
    let targets: Vec<_> = out
        .nodes
        .iter()
        .filter(|n| n.id != home.id)
        .map(|n| n.label.as_str())
        .collect();
    assert_eq!(targets, ["Alpha.md"]);
    assert_eq!(index.stats().links, 4);
}

#[tokio::test]
async fn search_is_a_text_dialect_with_paths_lines_and_file_nodes() {
    let dir = vault();
    let folder = Arc::new(FolderSource::open(dir.path()).unwrap());
    let mock: Arc<dyn moonkale_llm::Provider> = Arc::new(moonkale_llm::MockProvider::scripted());
    let index = IndexSource::build_with(folder.clone(), Some(mock))
        .await
        .unwrap();
    let (chunks, embedded) = index.search_stats();
    assert!(
        chunks >= 4 && embedded == 0,
        "{chunks} chunks, {embedded} embedded"
    );
    assert_eq!(index.embed_pending().await.unwrap(), chunks);
    assert_eq!(index.search_stats().1, chunks);

    let res = index
        .query(Query::Text {
            dialect: "search".into(),
            text: "pub fn free".into(),
        })
        .await
        .unwrap();
    let t = res.table.unwrap();
    assert_eq!(t.columns[0], "path");
    assert_eq!(t.rows[0][0].to_string(), "src/lib.rs");
    assert_eq!(t.rows[0][1].to_string(), "3");
    assert!(res
        .nodes
        .iter()
        .any(|n| n.native_key == "src/lib.rs" && n.kind == NodeKind::File));

    // Refresh keeps the search index in step with the file.
    let lib = res
        .nodes
        .iter()
        .find(|n| n.native_key == "src/lib.rs")
        .unwrap();
    let (text, v) = folder.fetch_text(lib.id).await.unwrap();
    folder
        .apply(Transaction::write_text(
            lib.id,
            v,
            TextPatch::whole("pub fn zebra_walk() {}\n", text.chars().count()),
        ))
        .await
        .unwrap();
    index.refresh(lib.id).await.unwrap();
    let res = index
        .query(Query::Text {
            dialect: "search".into(),
            text: "zebra walk".into(),
        })
        .await
        .unwrap();
    assert_eq!(res.table.unwrap().rows[0][0].to_string(), "src/lib.rs");
    assert!(index
        .query(Query::Text {
            dialect: "search".into(),
            text: "free".into()
        })
        .await
        .unwrap()
        .table
        .unwrap()
        .rows
        .is_empty());
    let err = index
        .query(Query::Text {
            dialect: "sql".into(),
            text: "x".into(),
        })
        .await;
    assert!(err.is_err());
}

#[tokio::test]
async fn refresh_follows_rename_delete_and_new_directories() {
    let dir = vault();
    let (folder, index) = open(&dir).await;
    let all = index
        .query(Query::All {
            limit: 100,
            kinds: Some(vec![NodeKind::File]),
        })
        .await
        .unwrap();
    let alpha = all
        .nodes
        .iter()
        .find(|n| n.native_key == "Alpha.md")
        .unwrap()
        .id;

    // Rename: the old node (and its symbols/links) go, the new one appears
    // under its parent, and wiki-links still resolve by stem.
    let applied = folder
        .apply(Transaction::rename(alpha, "notes/Alpha.md"))
        .await
        .unwrap();
    let new_id = match applied.results[0] {
        moonkale_core::OpResult::Ok { node, .. } => node,
        ref other => panic!("{other:?}"),
    };
    index.refresh(alpha).await.unwrap();
    index.refresh(new_id).await.unwrap();
    assert_eq!(
        files(&index).await,
        [
            "Home.md",
            "logo.png",
            "notes/Alpha.md",
            "notes/Beta.md",
            "src/lib.rs"
        ]
    );
    let notes_dir = all_dirs(&index)
        .await
        .into_iter()
        .find(|n| n.native_key == "notes")
        .unwrap();
    let kids = index.query(Query::Children(notes_dir.id)).await.unwrap();
    assert!(
        kids.nodes.iter().any(|n| n.id == new_id),
        "Contains edge to the moved file"
    );
    let home = index
        .query(Query::All {
            limit: 100,
            kinds: Some(vec![NodeKind::File]),
        })
        .await
        .unwrap()
        .nodes
        .into_iter()
        .find(|n| n.native_key == "Home.md")
        .unwrap();
    let out = index
        .query(Query::Neighbours {
            node: home.id,
            depth: 1,
            direction: Direction::Out,
        })
        .await
        .unwrap();
    assert!(
        out.nodes.iter().any(|n| n.id == new_id),
        "[[Alpha]] resolves to the moved file"
    );

    // A new directory with a file inside: one refresh on the directory walks it.
    let applied = folder
        .apply(Transaction::create_dir(folder.root_id(), "docs"))
        .await
        .unwrap();
    let docs = match applied.results[0] {
        moonkale_core::OpResult::Ok { node, .. } => node,
        ref other => panic!("{other:?}"),
    };
    folder
        .apply(Transaction::create_text(
            docs,
            "guide.md",
            "See [[Home]].\n",
        ))
        .await
        .unwrap();
    index.refresh(docs).await.unwrap();
    assert!(files(&index).await.contains(&"docs/guide.md".to_string()));

    // Delete a directory: its files and their derived symbols disappear.
    let src_dir = all_dirs(&index)
        .await
        .into_iter()
        .find(|n| n.native_key == "src")
        .unwrap();
    folder.apply(Transaction::delete(src_dir.id)).await.unwrap();
    index.refresh(src_dir.id).await.unwrap();
    assert!(!files(&index).await.iter().any(|k| k.starts_with("src/")));
    let symbols = index
        .query(Query::All {
            limit: 100,
            kinds: Some(vec![NodeKind::Symbol]),
        })
        .await
        .unwrap();
    assert!(
        symbols.nodes.is_empty(),
        "symbols of src/lib.rs are gone: {:?}",
        symbols.nodes
    );
}

async fn all_dirs(index: &IndexSource) -> Vec<moonkale_core::Node> {
    index
        .query(Query::All {
            limit: 100,
            kinds: Some(vec![NodeKind::Directory]),
        })
        .await
        .unwrap()
        .nodes
}

async fn files(index: &IndexSource) -> Vec<String> {
    let all = index
        .query(Query::All {
            limit: 100,
            kinds: Some(vec![NodeKind::File]),
        })
        .await
        .unwrap();
    let mut keys: Vec<String> = all.nodes.iter().map(|n| n.native_key.clone()).collect();
    keys.sort();
    keys
}
