//! Hermetic lazy million-node source exercising the production GraphPanel.
//! Only requested cuts materialise; full hierarchy correctness lives in core tests.
use dioxus::prelude::*;
use moonkale_core::*;
use moonkale_ext_api::{
    AttachFuture, FolderAccess, OpenFolderFuture, OpenOptions, Workspace, WorkspaceConfig,
};
use std::sync::Arc;
const FOLDERS: usize = 500;
const FILES: usize = 2000;
struct Synthetic;
impl Synthetic {
    fn node(&self, key: String, kind: NodeKind, members: usize) -> Node {
        Node {
            id: NodeId::derive(&self.id(), &key),
            source: self.id(),
            kind,
            label: key.clone(),
            native_key: key,
            content: None,
            version: Version(0),
            props: [("coarse.members".into(), Value::Int(members as i64))].into(),
        }
    }
}
#[cfg_attr(not(target_arch = "wasm32"), moonkale_core::async_trait)]
#[cfg_attr(target_arch="wasm32", moonkale_core::async_trait(?Send))]
impl Source for Synthetic {
    fn id(&self) -> SourceId {
        SourceId::new("coarse-fixture")
    }
    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: self.id(),
            display_name: "Million nodes".into(),
            root: NodeId::derive(&self.id(), "root"),
            truth: SourceTruth::Store,
            family: SourceFamily::Graph,
            capabilities: Capabilities {
                read: true,
                ..Default::default()
            },
        }
    }
    async fn query(&self, query: Query) -> Result<QueryResult, SourceError> {
        match query {
            Query::Coarse { level: 1, region } => {
                let nodes = if let Some(region) = region {
                    let folder = (0..FOLDERS)
                        .find(|i| NodeId::derive(&self.id(), &format!("folder-{i}")) == region)
                        .ok_or(SourceError::NotFound)?;
                    (0..FILES)
                        .map(|i| self.node(format!("folder-{folder}/file-{i}"), NodeKind::File, 0))
                        .collect::<Vec<_>>()
                } else {
                    (0..FOLDERS)
                        .map(|i| self.node(format!("folder-{i}"), NodeKind::Directory, FILES))
                        .collect()
                };
                let edges = (1..nodes.len())
                    .map(|i| Edge {
                        source: self.id(),
                        from: nodes[i - 1].id,
                        to: nodes[i].id,
                        kind: EdgeKind::References,
                        props: [("coarse.weight".into(), Value::Int(3))].into(),
                    })
                    .collect();
                Ok(QueryResult {
                    nodes,
                    edges,
                    table: Some(moonkale_core::source::Table {
                        columns: vec!["source.nodes".into(), "source.edges".into()],
                        rows: vec![vec![
                            Value::Int((1 + FOLDERS + FOLDERS * FILES) as i64),
                            Value::Int(0),
                        ]],
                        truncated: false,
                    }),
                    truncated: false,
                })
            }
            Query::All { limit, .. } => {
                let nodes = (0..limit.min(FOLDERS * FILES))
                    .map(|i| {
                        let mut n = self.node(
                            format!("folder-{}/file-{}", i / FILES, i % FILES),
                            NodeKind::File,
                            0,
                        );
                        n.props.clear();
                        n
                    })
                    .collect();
                Ok(QueryResult {
                    nodes,
                    truncated: limit < FOLDERS * FILES,
                    ..Default::default()
                })
            }
            _ => Err(SourceError::Unsupported("synthetic query".into())),
        }
    }
    async fn fetch_text(&self, _: NodeId) -> Result<(String, Version), SourceError> {
        Err(SourceError::Unsupported("fixture content".into()))
    }
    async fn apply(&self, _: Transaction) -> Result<Applied, SourceError> {
        Err(SourceError::Unsupported("fixture writes".into()))
    }
}
fn open(_: String, _: OpenOptions) -> OpenFolderFuture {
    Box::pin(async { Err(SourceError::NotFound) })
}
fn attach(_: SourceDescriptor) -> AttachFuture {
    Box::pin(async { Ok(Arc::new(Synthetic) as Arc<dyn Source>) })
}
fn main() {
    dioxus::launch(App);
}
#[component]
fn App() -> Element {
    let mut ws = use_hook(|| {
        Workspace::new(WorkspaceConfig {
            folders: FolderAccess {
                open,
                attach,
                pick: None,
                reopen_last: false,
                openers: &moonkale_core::source::opener::NO_OPENERS,
            },
            processes: Default::default(),
            persistence: Default::default(),
            network: Default::default(),
            runtimes: Default::default(),
            services: &[],
        })
    });
    use_context_provider(|| ws);
    rsx! {
        style { "html,body {{ margin:0; width:100%;height:100%; }} #app {{height:100%;}} .fixture {{height:900px;}}" }
        button {id:"coarse-open", onclick:move |_| { ws.add_source(Arc::new(Synthetic)); }, "Open generated million-node source"}
        div {class:"fixture", moonkale_editor_graph::GraphPanel {ws}}
    }
}
