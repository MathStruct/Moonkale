#![cfg(not(target_arch = "wasm32"))]
use moonkale_core::{
    graph::code, Direction, EdgeKind, Node, NodeId, NodeKind, Openers, Query, QueryResult, Source,
    SourceTruth, Value,
};
use moonkale_sources_unison::UnisonSource;
use std::path::Path;

mod support;
use support::{transcript, ucm, INITIAL};
async fn graph(path: &Path) -> (UnisonSource, QueryResult) {
    let s = UnisonSource::open(path).await.unwrap();
    let r = s
        .query(Query::All {
            limit: 4096,
            kinds: None,
        })
        .await
        .unwrap();
    assert!(!r.truncated);
    (s, r)
}
fn named<'a>(r: &'a QueryResult, name: &str) -> &'a Node {
    let n = r
        .nodes
        .iter()
        .find(|n| n.kind == NodeKind::Custom(code::NAME.into()) && n.label == name)
        .unwrap_or_else(|| panic!("missing {name}"));
    let target = r
        .edges
        .iter()
        .find(|e| e.from == n.id && e.kind == EdgeKind::Custom(code::NAMES.into()))
        .unwrap()
        .to;
    r.nodes.iter().find(|n| n.id == target).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn schema_identity_alias_update_recursion_and_opener() {
    let Some(bin) = ucm() else { return };
    let tmp = tempfile::tempdir().unwrap();
    std::env::set_var("XDG_DATA_HOME", tmp.path().join("data"));
    let path = tmp.path().join("fixture.unison");
    transcript(&bin, tmp.path(), &path, INITIAL);
    let (source, before) = graph(&path).await;
    let d = source.descriptor();
    assert_eq!(d.truth, SourceTruth::Store);
    assert!(
        d.capabilities.read
            && !d.capabilities.write
            && !d.capabilities.watch
            && d.capabilities.text_query.is_none()
    );
    assert_eq!(source.changes_since(0).await.unwrap(), None);
    assert_eq!(
        named(&before, "Ask").props["def_kind"],
        Value::Text("interface".into())
    );
    assert_eq!(
        named(&before, "ns.check").props["def_kind"],
        Value::Text("test".into())
    );
    let a = named(&before, "ns.a").id;
    let b = named(&before, "ns.b").id;
    let before_text = source.fetch_text(b).await.unwrap().0;
    let c = named(&before, "ns.c").id;
    assert!(before
        .edges
        .iter()
        .any(|e| e.from == b && e.to == a && e.kind == EdgeKind::References));
    assert!(before
        .edges
        .iter()
        .any(|e| e.from == c && e.to == b && e.kind == EdgeKind::References));
    let even = named(&before, "ns.even");
    let odd = named(&before, "ns.odd");
    assert_ne!(even.id, odd.id);
    assert_eq!(
        even.props["content_component"],
        odd.props["content_component"]
    );
    assert_ne!(
        even.props["content_component_index"],
        odd.props["content_component_index"]
    );
    assert_eq!(
        named(&before, "Choice").props["def_kind"],
        Value::Text("type".into())
    );
    assert_eq!(
        named(&before, "Choice.Left").props["def_kind"],
        Value::Text("variant".into())
    );
    let pick = named(&before, "ns.pick").id;
    assert!(before.edges.iter().any(|e| e.from == pick
        && e.to == named(&before, "Choice").id
        && e.kind == EdgeKind::Custom(code::TYPE_OF.into())));
    assert!(source.fetch_text(b).await.unwrap().0.contains("ns.b"));
    assert!(source
        .fetch_text(named(&before, "Choice.Left").id)
        .await
        .unwrap()
        .0
        .contains("Choice"));
    let children = source.query(Query::Children(d.root)).await.unwrap();
    assert!(!children.nodes.is_empty());
    let neighbours = source
        .query(Query::Neighbours {
            node: b,
            depth: 1,
            direction: Direction::Out,
        })
        .await
        .unwrap();
    assert!(neighbours.nodes.iter().any(|n| n.id == a));
    assert!(
        source
            .query(Query::All {
                limit: 1,
                kinds: None
            })
            .await
            .unwrap()
            .truncated
    );
    assert!(source
        .query(Query::Node(NodeId::fresh("absent")))
        .await
        .is_err());
    transcript(
        &bin,
        tmp.path(),
        &path,
        r#"
```ucm
fixture/main> move.term ns.a ns.renamed
fixture/main> alias.term ns.renamed ns.also
```
"#,
    );
    let (_, renamed) = graph(&path).await;
    assert_eq!(named(&renamed, "ns.renamed").id, a);
    assert_eq!(named(&renamed, "ns.also").id, a);
    assert!(!renamed
        .nodes
        .iter()
        .any(|n| n.kind == NodeKind::Custom(code::NAME.into()) && n.label == "ns.a"));
    assert_eq!(named(&renamed, "ns.b").id, b);
    transcript(
        &bin,
        tmp.path(),
        &path,
        r#"
```ucm
fixture/main> delete.term ns.also
```
```unison
ns.renamed x = false
```
```ucm
fixture/main> update
```
"#,
    );
    let (_, updated) = graph(&path).await;
    assert_ne!(named(&updated, "ns.renamed").id, a);
    assert_ne!(named(&updated, "ns.b").id, b);
    assert_ne!(named(&updated, "ns.c").id, c);
    // An already opened source is an immutable snapshot, including its printed text.
    assert_eq!(source.fetch_text(b).await.unwrap().0, before_text);
    let opened = Openers::new(moonkale_sources_unison::openers())
        .open(path, true)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(opened.descriptor().truth, SourceTruth::Store);
    let all = opened
        .query(Query::All {
            limit: 4096,
            kinds: None,
        })
        .await
        .unwrap();
    assert!(all.nodes.iter().any(|n| n.kind == NodeKind::Directory));
    assert!(all.nodes.iter().any(|n| n.kind.is_definition()));
    assert!(all.edges.iter().all(
        |e| all.nodes.iter().any(|n| n.id == e.from) && all.nodes.iter().any(|n| n.id == e.to)
    ));
    assert!(all
        .nodes
        .iter()
        .all(|n| matches!(n.kind, NodeKind::Directory)
            || n.kind.is_definition()
            || n.kind == NodeKind::Custom(code::NAME.into())));
}
