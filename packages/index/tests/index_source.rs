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
