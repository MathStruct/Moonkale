use moonkale_core::{NodeKind, Query, Source, SourceError, TextPatch, Transaction, Version};
use moonkale_project_fs::FolderSource;
use std::fs;

fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    fs::create_dir_all(p.join("src")).unwrap();
    fs::create_dir_all(p.join("target/debug")).unwrap();
    fs::write(p.join("src/main.rs"), "fn main() {}\n").unwrap();
    fs::write(p.join("README.md"), "# hi\n").unwrap();
    fs::write(p.join("logo.png"), [0x89, b'P', b'N', b'G']).unwrap();
    fs::write(p.join(".gitignore"), "target/\n").unwrap();
    fs::write(p.join("target/debug/junk"), "x").unwrap();
    dir
}

#[tokio::test]
async fn lists_root_children_dirs_first_and_honours_gitignore() {
    let dir = fixture();
    let src = FolderSource::open(dir.path()).unwrap();
    let res = src.query(Query::Children(src.root_id())).await.unwrap();
    let names: Vec<_> = res.nodes.iter().map(|n| n.label.as_str()).collect();
    assert_eq!(
        names,
        ["src", "README.md", "logo.png"],
        "target/ is ignored, .gitignore is hidden"
    );
    assert_eq!(res.nodes[0].kind, NodeKind::Directory);
    assert!(matches!(
        res.nodes[2].content,
        Some(moonkale_core::ContentRef::Blob { .. })
    ));
    assert_eq!(res.edges.len(), 3);
    assert!(res.edges.iter().all(|e| e.from == src.root_id()));
}

#[tokio::test]
async fn nested_listing_uses_stable_ids() {
    let dir = fixture();
    let src = FolderSource::open(dir.path()).unwrap();
    let root = src.query(Query::Children(src.root_id())).await.unwrap();
    let src_dir = root.nodes.iter().find(|n| n.label == "src").unwrap();
    let inner = src.query(Query::Children(src_dir.id)).await.unwrap();
    assert_eq!(inner.nodes.len(), 1);
    let main = &inner.nodes[0];
    assert_eq!(main.native_key, "src/main.rs");
    assert_eq!(
        main.content,
        Some(moonkale_core::ContentRef::Text {
            len: 13,
            lang: Some("rust".into())
        })
    );

    // Re-opening the same folder yields the same ids.
    let again = FolderSource::open(dir.path()).unwrap();
    assert_eq!(again.id(), src.id());
    assert_eq!(again.root_id(), src.root_id());
}

#[tokio::test]
async fn write_round_trip_is_atomic_and_versioned() {
    let dir = fixture();
    let src = FolderSource::open(dir.path()).unwrap();
    let root = src.query(Query::Children(src.root_id())).await.unwrap();
    let readme = root.nodes.iter().find(|n| n.label == "README.md").unwrap();

    let (text, v1) = src.fetch_text(readme.id).await.unwrap();
    assert_eq!(text, "# hi\n");

    let tx = Transaction::write_text(
        readme.id,
        v1,
        TextPatch::whole("# hello\n", text.chars().count()),
    );
    let applied = src.apply(tx).await.unwrap();
    let v2 = applied.version_of(readme.id).expect("write succeeded");
    assert_ne!(v1, v2);
    assert_eq!(
        fs::read_to_string(dir.path().join("README.md")).unwrap(),
        "# hello\n"
    );
    assert!(
        !dir.path().join("README.md.moonkale-tmp").exists(),
        "temp file cleaned up"
    );

    let (text2, v3) = src.fetch_text(readme.id).await.unwrap();
    assert_eq!(text2, "# hello\n");
    assert_eq!(v2, v3);
}

#[tokio::test]
async fn stale_version_is_a_conflict() {
    let dir = fixture();
    let src = FolderSource::open(dir.path()).unwrap();
    let root = src.query(Query::Children(src.root_id())).await.unwrap();
    let readme = root.nodes.iter().find(|n| n.label == "README.md").unwrap();
    let (text, v1) = src.fetch_text(readme.id).await.unwrap();

    // Someone else edits the file (different length so the version moves
    // even inside one mtime tick).
    fs::write(dir.path().join("README.md"), "# changed elsewhere\n").unwrap();

    let tx = Transaction::write_text(
        readme.id,
        v1,
        TextPatch::whole("# mine\n", text.chars().count()),
    );
    let applied = src.apply(tx).await.unwrap();
    assert!(matches!(
        applied.first_error(),
        Some(SourceError::Conflict { .. })
    ));
    assert_eq!(
        fs::read_to_string(dir.path().join("README.md")).unwrap(),
        "# changed elsewhere\n"
    );
}

#[tokio::test]
async fn unknown_id_is_not_found_and_stale_version_type_is_default() {
    let dir = fixture();
    let src = FolderSource::open(dir.path()).unwrap();
    let bogus = moonkale_core::NodeId::fresh("nope");
    assert!(matches!(
        src.fetch_text(bogus).await,
        Err(SourceError::NotFound)
    ));
    assert_eq!(Version::default(), Version(0));
}

#[tokio::test]
async fn create_text_makes_directories_and_refuses_overwrite() {
    let dir = fixture();
    let src = FolderSource::open(dir.path()).unwrap();
    let tx = Transaction::create_text(src.root_id(), ".moonkale/chats/one.md", "# chat\n");
    let applied = src.apply(tx).await.unwrap();
    let node = match &applied.results[0] {
        moonkale_core::OpResult::Ok { node, .. } => *node,
        other => panic!("{other:?}"),
    };
    assert_eq!(
        fs::read_to_string(dir.path().join(".moonkale/chats/one.md")).unwrap(),
        "# chat\n"
    );
    let (text, _) = src.fetch_text(node).await.unwrap();
    assert_eq!(text, "# chat\n");

    let again = src
        .apply(Transaction::create_text(
            src.root_id(),
            ".moonkale/chats/one.md",
            "x",
        ))
        .await
        .unwrap();
    assert!(matches!(
        again.results[0],
        moonkale_core::OpResult::Refused { .. }
    ));
    let escape = src
        .apply(Transaction::create_text(src.root_id(), "../evil.md", "x"))
        .await
        .unwrap();
    assert!(matches!(
        escape.results[0],
        moonkale_core::OpResult::Refused { .. }
    ));
}
