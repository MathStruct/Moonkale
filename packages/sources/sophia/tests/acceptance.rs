//! Sophia SPEC-004 §9 from Moonkale's side: the sample store through the
//! `Source` trait only, as the workspace and the graph panel use it.
use moonkale_core::{
    Direction, EdgeKind, Node, NodeId, NodeKind, Op, OpResult, Query, Risk, Source, SourceError,
    SourceTruth, TextPatch, Transaction, Value, Version,
};
use moonkale_sources_sophia::SophiaSource;

async fn children(s: &SophiaSource, id: NodeId) -> Vec<Node> {
    s.query(Query::Children(id)).await.unwrap().nodes
}

fn find<'a>(nodes: &'a [Node], kind: &str, label: &str) -> &'a Node {
    nodes
        .iter()
        .find(|n| {
            n.label == label && matches!(&n.kind, NodeKind::Custom(k) if k == kind)
                || (kind == "Directory" && n.kind == NodeKind::Directory && n.label == label)
        })
        .unwrap_or_else(|| panic!("no {kind} {label} in {nodes:?}"))
}

/// The sample namespace's names, browsed from the root as the Explorer does.
async fn sample_names(s: &SophiaSource) -> Vec<Node> {
    let root = children(s, s.descriptor().root).await;
    let ns = find(&root, "Directory", "sample").id;
    children(s, ns).await
}

/// What a name's `Names` edge points at now.
async fn named(s: &SophiaSource, name: NodeId) -> NodeId {
    let g = s
        .query(Query::Neighbours {
            node: name,
            depth: 1,
            direction: Direction::Out,
        })
        .await
        .unwrap();
    g.edges
        .iter()
        .find(|e| e.from == name && e.kind == EdgeKind::Custom("Names".into()))
        .map(|e| e.to)
        .expect("a Names edge")
}

async fn write(s: &SophiaSource, node: NodeId, expected: Version, text: &str) -> OpResult {
    let old = s.fetch_text(node).await.unwrap().0;
    let tx = Transaction::write_text(node, expected, TextPatch::whole(text, old.chars().count()));
    s.apply(tx).await.unwrap().results.remove(0)
}

#[tokio::test]
async fn sample_store_draws_as_a_store_backed_graph() {
    let s = SophiaSource::sample().unwrap();
    let d = s.descriptor();
    assert_eq!(d.truth, SourceTruth::Store);
    assert!(d.capabilities.write && d.capabilities.watch);
    let root = children(&s, d.root).await;
    find(&root, "sophia:Settings", "rendering profile");
    let names = sample_names(&s).await;
    assert_eq!(names.len(), 18);
    let add = find(&names, "Name", "add");
    assert!(add.content.is_some(), "names open as text");
    assert_eq!(add.native_key, "name:sample.add");

    let g = s
        .query(Query::Neighbours {
            node: add.id,
            depth: 2,
            direction: Direction::Both,
        })
        .await
        .unwrap();
    let def = g
        .nodes
        .iter()
        .find(|n| n.kind.is_definition() && n.label == "add")
        .expect("the definition");
    assert!(def.native_key.starts_with('#'));
    assert!(g
        .edges
        .iter()
        .any(|e| e.kind == EdgeKind::Custom("Names".into())));
    assert!(g.edges.iter().any(|e| e.kind == EdgeKind::References));

    // Every kind is drawn for store sources (the graph panel's `All`).
    let all = s
        .query(Query::All {
            limit: 1000,
            kinds: None,
        })
        .await
        .unwrap();
    assert!(all.nodes.iter().any(|n| n.kind.is_definition()));
    assert!(all.nodes.iter().any(|n| n.id == d.root));
    assert!(all
        .edges
        .iter()
        .any(|e| e.kind == EdgeKind::Contains && e.to == add.id));
    assert!(all.edges.iter().any(|e| e.kind == EdgeKind::References));
    assert!(all
        .edges
        .iter()
        .any(|e| e.kind == EdgeKind::Custom("MethodOf".into())));
    let five = s
        .query(Query::All {
            limit: 5,
            kinds: None,
        })
        .await
        .unwrap();
    assert!(five.truncated && five.nodes.len() == 5);
}

#[tokio::test]
async fn editing_moves_the_name_and_parse_errors_leave_it() {
    let s = SophiaSource::sample().unwrap();
    let names = sample_names(&s).await;
    let double = find(&names, "Name", "double").id;
    let before = named(&s, double).await;

    let (text, v0) = s.fetch_text(double).await.unwrap();
    assert_eq!(text, "function double(x::Int64)::Int64\n    x + x\nend\n");
    let edited = "function double(x::Int64)::Int64\n    2 * x\nend\n";
    let OpResult::Ok { node, version } = write(&s, double, v0, edited).await else {
        panic!("the edit was refused")
    };
    assert_eq!(node, double, "the name keeps its id");
    assert_ne!(version, v0, "an edit changes the token");
    let after = named(&s, double).await;
    assert_ne!(before, after, "the name points at new content");
    assert_eq!(s.fetch_text(double).await.unwrap().0, edited);

    // A parse error: diagnostics with a position, and the name stays.
    let bad = "function double(x::Int64)::Int64\n    2 *\nend\n";
    let OpResult::Refused {
        error: SourceError::Invalid(msg),
        ..
    } = write(&s, double, version, bad).await
    else {
        panic!("a parse error was accepted")
    };
    assert!(msg.lines().all(|l| l.split(':').count() >= 3), "{msg}");
    assert_eq!(named(&s, double).await, after);

    // A stale version conflicts.
    assert!(matches!(
        write(&s, double, v0, edited).await,
        OpResult::Refused {
            error: SourceError::Conflict { .. },
            ..
        }
    ));

    // A definition itself is immutable.
    assert!(matches!(
        write(&s, after, Version(0), "").await,
        OpResult::Refused { .. }
    ));
}

#[tokio::test]
async fn a_rename_arrives_through_the_change_feed() {
    let s = SophiaSource::sample().unwrap();
    let names = sample_names(&s).await;
    let double = find(&names, "Name", "double").id;
    let start = s.changes_since(0).await.unwrap().unwrap().seq;
    assert!(start > 0, "the sample's own writes are in the log");

    let r = s
        .apply(Transaction::rename(double, "sample/twice"))
        .await
        .unwrap();
    let OpResult::Ok { node: twice, .. } = r.results[0] else {
        panic!("{r:?}")
    };
    assert_ne!(twice, double);
    let c = s.changes_since(start).await.unwrap().unwrap();
    assert!(c.seq > start);
    assert!(c.paths.contains(&"name:sample.twice".to_string()), "{c:?}");
    assert!(c.paths.contains(&"name:sample.double".to_string()), "{c:?}");

    // What the workspace does with each path: the new name resolves, the
    // old one is gone and falls back to the id derived from its path.
    let q = |p: &str| Query::Text {
        dialect: "path".into(),
        text: p.into(),
    };
    assert_eq!(
        s.query(q("name:sample.twice")).await.unwrap().nodes[0].id,
        twice
    );
    assert_eq!(
        s.query(q("name:sample.double")).await,
        Err(SourceError::NotFound)
    );
    assert_eq!(NodeId::derive(&s.id(), "name:sample.double"), double);
}

#[tokio::test]
async fn a_profile_switch_reprints_without_changing_ids() {
    let s = SophiaSource::sample().unwrap();
    let root = children(&s, s.descriptor().root).await;
    let settings = find(&root, "sophia:Settings", "rendering profile").id;
    let add = find(&sample_names(&s).await, "Name", "add").id;
    let before = s.fetch_text(add).await.unwrap().0;
    assert!(before.starts_with("function add"), "{before}");
    let start = s.changes_since(0).await.unwrap().unwrap().seq;

    let (profile, v) = s.fetch_text(settings).await.unwrap();
    assert!(profile.contains("based_on = \"julia\""), "{profile}");
    assert!(matches!(
        write(&s, settings, v, "based_on = rust\n").await,
        OpResult::Ok { .. }
    ));
    let after = s.fetch_text(add).await.unwrap().0;
    assert!(
        after.starts_with("fn add(a: Nat, b: Nat) -> Nat {"),
        "{after}"
    );
    assert_eq!(find(&sample_names(&s).await, "Name", "add").id, add);
    assert!(
        s.changes_since(start).await.unwrap().unwrap().reset,
        "open editors re-read everything"
    );

    // A bad profile text is refused with diagnostics.
    let (_, v) = s.fetch_text(settings).await.unwrap();
    assert!(matches!(
        write(&s, settings, v, "this is not a setting").await,
        OpResult::Refused {
            error: SourceError::Invalid(_),
            ..
        }
    ));
}

#[tokio::test]
async fn sql_answers_a_table_and_a_subgraph_and_refuses_writes() {
    let s = SophiaSource::sample().unwrap();
    let sql = |text: &str| Query::Text {
        dialect: "sql".into(),
        text: text.into(),
    };
    let r = s
        .query(sql(
            "SELECT id, kind FROM definitions WHERE kind = 'method'",
        ))
        .await
        .unwrap();
    assert_eq!(r.table.as_ref().unwrap().rows.len(), 7);
    assert_eq!(r.nodes.len(), 7);
    assert!(s.query(sql("DELETE FROM t_name")).await.is_err());
    assert!(matches!(
        s.query(Query::Text {
            dialect: "cypher".into(),
            text: "MATCH (n) RETURN n".into()
        })
        .await,
        Err(SourceError::Unsupported(_))
    ));
}

#[tokio::test]
async fn the_query_dialect_answers_a_table_and_a_subgraph() {
    let s = SophiaSource::sample().unwrap();
    // Sophia's example (SPEC-007 §8): who uses `sample.add`, transitively.
    let text = r#"query users
        uses_(a, b) = refs(a, b)
        uses_(a, c) = refs(a, b) && uses_(b, c)
        select a, p where name("sample.add", t) && uses_(a, t) && name(p, a)
    end"#;
    assert_eq!(s.classify("query", text), Risk::Read);
    let r = s
        .query(Query::Text {
            dialect: "query".into(),
            text: text.into(),
        })
        .await
        .unwrap();
    let t = r.table.unwrap();
    assert_eq!(t.columns, vec!["a", "p"]);
    assert!(
        t.rows
            .iter()
            .any(|row| row[1] == Value::Text("sample.add_test".into())),
        "{:?}",
        t.rows
    );
    assert!(!r.nodes.is_empty(), "id cells become nodes");
    assert!(s
        .query(Query::Text {
            dialect: "query".into(),
            text: "query broken select".into(),
        })
        .await
        .is_err());
}

#[tokio::test]
async fn the_views_open_as_tables_in_both_dialects() {
    let s = SophiaSource::sample().unwrap();
    let dialects: Vec<_> = s
        .descriptor()
        .capabilities
        .text_dialects()
        .map(|d| d.name())
        .collect();
    assert_eq!(dialects, ["sql", "query"]);
    let root = children(&s, s.descriptor().root).await;
    let views = find(&root, "Directory", "views").id;
    let tables = children(&s, views).await;
    assert_eq!(tables.len(), 9, "{tables:?}");
    let defs = tables.iter().find(|n| n.label == "definitions").unwrap();
    assert_eq!(defs.kind, NodeKind::Table);
    assert_eq!(defs.native_key, "table:definitions");
    // What the table panel runs first in each dialect.
    let r = s
        .query(Query::Text {
            dialect: "sql".into(),
            text: "SELECT * FROM \"definitions\" LIMIT 1000".into(),
        })
        .await
        .unwrap();
    assert!(!r.table.unwrap().rows.is_empty());
    let r = s
        .query(Query::Text {
            dialect: "query".into(),
            text: "select p, d where name(p, d)".into(),
        })
        .await
        .unwrap();
    assert!(r.table.unwrap().rows.len() >= 16);
    // Views resolve like other nodes, and refuse edits.
    let again = s.query(Query::Node(defs.id)).await.unwrap().nodes;
    assert_eq!(again[0].label, "definitions");
    assert!(s.fetch_text(defs.id).await.is_err());
    let tx = Transaction {
        ops: vec![Op::CreateText {
            parent: views,
            name: "x".into(),
            text: "f() = 1".into(),
        }],
    };
    assert!(matches!(
        s.apply(tx).await.unwrap().results[0],
        OpResult::Refused { .. }
    ));
}

#[tokio::test]
async fn text_creates_names_and_directories() {
    let s = SophiaSource::sample().unwrap();
    let root = children(&s, s.descriptor().root).await;
    let ns = find(&root, "Directory", "sample").id;
    let r = s
        .apply(Transaction {
            ops: vec![
                Op::CreateText {
                    parent: ns,
                    name: "triple".into(),
                    text: "function triple(x::Int64)::Int64\n    3 * x\nend\n".into(),
                },
                Op::CreateDir {
                    parent: ns,
                    name: "empty".into(),
                },
            ],
        })
        .await
        .unwrap();
    assert!(r.first_error().is_none(), "{r:?}");
    let names = children(&s, ns).await;
    let triple = find(&names, "Name", "triple").id;
    assert!(s.fetch_text(triple).await.unwrap().0.contains("3 * x"));
    find(&names, "Directory", "empty");
}

#[tokio::test]
async fn store_files_open_and_missing_ones_do_not() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("code.sophia");
    let iface = sophia_interface::Interface::open(
        sophia_store::sqlite::Sqlite::open(&path).unwrap(),
        "moonkale",
    )
    .unwrap();
    iface.load_sample().unwrap();
    drop(iface);
    let s = SophiaSource::open(&path).await.unwrap();
    assert_eq!(s.descriptor().display_name, "code");
    assert_eq!(sample_names(&s).await.len(), 18);
    assert!(matches!(
        SophiaSource::open(dir.path().join("none.sophia")).await,
        Err(SourceError::NotFound)
    ));
    let openers = moonkale_sources_sophia::openers();
    assert!((openers[0].matches)("work/code.sophia"));
    assert!(!(openers[0].matches)("work/code.sqlite"));
}

// Sophia's review demonstration 2: two clients render one definition in
// different profiles, edit concurrently, and neither silently overwrites the
// other, not even through a recursive group that covers both names.
#[tokio::test]
async fn two_clients_in_two_profiles_cannot_overwrite_each_other() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("shared.sophia");
    let iface = sophia_interface::Interface::open(
        sophia_store::sqlite::Sqlite::open(&path).unwrap(),
        "julia-reader",
    )
    .unwrap();
    iface.load_sample().unwrap();
    drop(iface);
    let a = SophiaSource::open_as(&path, "julia-reader").await.unwrap();
    let b = SophiaSource::open_as(&path, "rust-reader").await.unwrap();

    let settings = find(
        &children(&b, b.descriptor().root).await,
        "sophia:Settings",
        "rendering profile",
    )
    .id;
    let (_, v) = b.fetch_text(settings).await.unwrap();
    assert!(matches!(
        write(&b, settings, v, "based_on = rust\n").await,
        OpResult::Ok { .. }
    ));
    let add_a = find(&sample_names(&a).await, "Name", "add").id;
    let add_b = find(&sample_names(&b).await, "Name", "add").id;
    assert_eq!(add_a, add_b, "one name, one id");
    let (ta, _) = a.fetch_text(add_a).await.unwrap();
    let (tb, _) = b.fetch_text(add_b).await.unwrap();
    assert!(
        ta.starts_with("function add") && tb.starts_with("fn add"),
        "{ta}\n{tb}"
    );

    // `even`'s text covers `odd` too. B saves `odd` first; A's save of
    // `even`, read before that, must conflict instead of reverting `odd`.
    let even = find(&sample_names(&a).await, "Name", "even").id;
    let odd = find(&sample_names(&b).await, "Name", "odd").id;
    let (even_text, even_token) = a.fetch_text(even).await.unwrap();
    assert!(even_text.contains("function odd"), "{even_text}");
    let (odd_text, odd_token) = b.fetch_text(odd).await.unwrap();
    let odd_before = named(&b, odd).await;
    let odd_edit = odd_text.replacen("false", "1 == 0", 1);
    assert_ne!(odd_edit, odd_text, "{odd_text}");
    assert!(matches!(
        write(&b, odd, odd_token, &odd_edit).await,
        OpResult::Ok { .. }
    ));
    let odd_after = named(&b, odd).await;
    assert_ne!(odd_before, odd_after);

    let even_edit = even_text.replacen("true", "0 == 0", 1);
    assert_ne!(even_edit, even_text);
    assert!(matches!(
        write(&a, even, even_token, &even_edit).await,
        OpResult::Refused {
            error: SourceError::Conflict { .. },
            ..
        }
    ));
    assert_eq!(named(&a, odd).await, odd_after, "B's edit of odd stands");

    // Re-read, and A's edit goes through on top of B's.
    let (fresh, token) = a.fetch_text(even).await.unwrap();
    let fresh_edit = fresh.replacen("true", "0 == 0", 1);
    assert!(matches!(
        write(&a, even, token, &fresh_edit).await,
        OpResult::Ok { .. }
    ));
    assert!(b.fetch_text(odd).await.unwrap().0.contains("1 == 0"));
}
