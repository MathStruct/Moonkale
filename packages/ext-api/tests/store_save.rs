//! Saving to a store that reprints what it stores (Sophia, WP-68): the
//! document takes the store's spelling back, so the next patch applies to
//! the text the store has.
use dioxus::prelude::*;
use futures_util::FutureExt;
use moonkale_core::{
    Applied, Node, NodeId, NodeKind, OpResult, Query, QueryResult, Source, SourceDescriptor,
    SourceError, SourceFamily, SourceId, SourceTruth, Transaction, Version,
};
use moonkale_ext_api::{
    document::Document, AttachFuture, FolderAccess, OpenFolderFuture, OpenOptions, Workspace,
    WorkspaceConfig,
};
use std::sync::Mutex;

fn open(_: String, _: OpenOptions) -> OpenFolderFuture {
    Box::pin(async { Err(SourceError::NotFound) })
}
fn attach(_: SourceDescriptor) -> AttachFuture {
    Box::pin(async { Err(SourceError::NotFound) })
}
thread_local! {
    static WS: std::cell::RefCell<Option<Workspace>> = const { std::cell::RefCell::new(None) };
}
fn node() -> Node {
    Node {
        id: NodeId::fresh("store-save"),
        source: SourceId::new("store"),
        kind: NodeKind::Custom("Name".into()),
        label: "double".into(),
        native_key: "name:double".into(),
        props: Default::default(),
        content: None,
        version: Version(1),
    }
}

/// Stores text with every line's leading whitespace cut to four spaces.
struct Reprinting(Mutex<(String, u64)>);

fn reprint(text: &str) -> String {
    text.lines()
        .map(|l| match l.trim_start() {
            t if t.len() < l.len() => format!("    {t}\n"),
            t => format!("{t}\n"),
        })
        .collect()
}

#[moonkale_core::async_trait]
impl Source for Reprinting {
    fn id(&self) -> SourceId {
        node().source
    }
    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            truth: SourceTruth::Store,
            id: self.id(),
            root: node().id,
            display_name: "store".into(),
            family: SourceFamily::Graph,
            capabilities: moonkale_core::Capabilities {
                read: true,
                write: true,
                ..Default::default()
            },
        }
    }
    async fn query(&self, _: Query) -> Result<QueryResult, SourceError> {
        Ok(QueryResult::default())
    }
    async fn fetch_text(&self, _: NodeId) -> Result<(String, Version), SourceError> {
        let s = self.0.lock().unwrap();
        Ok((s.0.clone(), Version(s.1)))
    }
    async fn apply(&self, tx: Transaction) -> Result<Applied, SourceError> {
        let mut s = self.0.lock().unwrap();
        let mut results = Vec::new();
        for op in tx.ops {
            let moonkale_core::Op::WriteText {
                node,
                expected,
                patch,
            } = op
            else {
                unreachable!()
            };
            results.push(if expected.0 != s.1 {
                OpResult::Refused {
                    node,
                    error: SourceError::Conflict {
                        expected,
                        actual: Version(s.1),
                    },
                }
            } else {
                match patch.apply(&s.0) {
                    Ok(text) => {
                        *s = (reprint(&text), s.1 + 1);
                        OpResult::Ok {
                            node,
                            version: Version(s.1),
                        }
                    }
                    Err(error) => OpResult::Refused { node, error },
                }
            });
        }
        Ok(Applied { results })
    }
}

const STORED: &str = "function double(x)\n    x + x\nend\n";

#[component]
fn App() -> Element {
    let ws = use_hook(|| {
        let mut ws = Workspace::new(WorkspaceConfig {
            folders: FolderAccess {
                open,
                pick: None,
                attach,
                reopen_last: false,
                openers: &moonkale_core::source::opener::NO_OPENERS,
            },
            processes: Default::default(),
            persistence: Default::default(),
            network: Default::default(),
            runtimes: Default::default(),
            services: &[],
        });
        let n = node();
        let doc = Signal::new_in_scope(
            Document::new(n.clone(), STORED.into(), n.version),
            ScopeId::ROOT,
        );
        ws.docs.open.with_mut(|docs| docs.push((n.id, doc)));
        ws.add_source(std::sync::Arc::new(Reprinting(Mutex::new((
            STORED.into(),
            1,
        )))));
        ws
    });
    use_hook(move || WS.with(|w| *w.borrow_mut() = Some(ws)));
    rsx! {}
}

#[test]
fn a_save_takes_the_stores_spelling_back() {
    let mut dom = VirtualDom::new(App);
    dom.rebuild_in_place();
    let ws = WS.with(|w| w.borrow().unwrap());
    dom.in_scope(ScopeId::ROOT, || {
        let mut doc = ws.document(node().id).unwrap();
        doc.write().text = "function double(x)\n        2 * x\nend\n".into();
        ws.save(node().id).now_or_never().unwrap().unwrap();
        let canonical = "function double(x)\n    2 * x\nend\n";
        assert_eq!(
            doc.peek().text,
            canonical,
            "the buffer shows what was stored"
        );
        assert_eq!(doc.peek().saved, canonical);
        assert!(!doc.peek().dirty());
        assert_eq!(doc.peek().version, Version(2));

        // The next save patches the stored text, not the old spelling.
        doc.write().text = "function double(x)\n    3 * x\nend\n".into();
        ws.save(node().id).now_or_never().unwrap().unwrap();
        assert_eq!(doc.peek().text, "function double(x)\n    3 * x\nend\n");
        assert_eq!(doc.peek().version, Version(3));
    });
}
