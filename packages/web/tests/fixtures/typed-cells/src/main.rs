//! Hermetic source for production table-grid links and locale switching.
use dioxus::prelude::*;
use moonkale_core::*;
use moonkale_ext_api::{
    AttachFuture, FolderAccess, OpenFolderFuture, OpenOptions, Workspace, WorkspaceConfig,
};
use std::sync::Arc;
struct Fixture;
impl Fixture {
    fn node(&self, key: &str, kind: NodeKind) -> Node {
        Node {
            id: NodeId::derive(&self.id(), key),
            source: self.id(),
            label: key.into(),
            native_key: key.into(),
            kind,
            content: None,
            version: Version(0),
            props: Default::default(),
        }
    }
}
#[cfg_attr(not(target_arch = "wasm32"), moonkale_core::async_trait)]
#[cfg_attr(target_arch="wasm32", moonkale_core::async_trait(?Send))]
impl Source for Fixture {
    fn id(&self) -> SourceId {
        SourceId::new("typed-fixture")
    }
    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: self.id(),
            display_name: "Typed fixture".into(),
            root: self.node("dates", NodeKind::Table).id,
            truth: SourceTruth::Store,
            family: SourceFamily::Sql,
            capabilities: Capabilities {
                read: true,
                text_query: Some(TextDialect::Sql),
                ..Default::default()
            },
        }
    }
    async fn query(&self, q: Query) -> Result<QueryResult, SourceError> {
        let target = self.node("target.md", NodeKind::File);
        match q {
            Query::Node(id) if id == target.id => Ok(QueryResult::single(target)),
            Query::Text { .. } => Ok(QueryResult {
                nodes: vec![target.clone()],
                table: Some(Table {
                    columns: vec!["instant".into(), "reference".into(), "samples".into()],
                    rows: vec![vec![
                        Value::DateTime(DateTimeCell::parse("2026-10-23T10:30:00+02:00").unwrap()),
                        Value::Ref {
                            node: target.id,
                            label: "Open target".into(),
                        },
                        Value::Vector(NumericVector::new(vec![1., 4., 2., 5.]).unwrap()),
                    ]],
                    truncated: false,
                }),
                ..Default::default()
            }),
            _ => Ok(QueryResult::default()),
        }
    }
    async fn fetch_text(&self, _: NodeId) -> Result<(String, Version), SourceError> {
        Ok(("# Target".into(), Version(0)))
    }
    async fn apply(&self, _: Transaction) -> Result<Applied, SourceError> {
        Err(SourceError::Unsupported("read only".into()))
    }
}
fn open(_: String, _: OpenOptions) -> OpenFolderFuture {
    Box::pin(async { Err(SourceError::NotFound) })
}
fn attach(_: SourceDescriptor) -> AttachFuture {
    Box::pin(async { Ok(Arc::new(Fixture) as Arc<dyn Source>) })
}
fn main() {
    dioxus::launch(App)
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
    use_hook(|| ws.add_source(Arc::new(Fixture)));
    let active = (*ws.docs.active.read())
        .map(|id| id.to_string())
        .unwrap_or_default();
    rsx! {
        style { "body {{margin:0;color:CanvasText;background:Canvas;}} .mk-table {{height:600px;}}" }
        for lang in ["en","de","zh-CN"] {
            button {id:"lang-{lang}",onclick:move |_| ws.settings.resolved.with_mut(|s|s.language=lang.into()),"{lang}"}
        }
        output {id:"active", "data-node":"{active}", "{active}"}
        moonkale_editor_table::TablePanel {ws,node:Fixture.node("dates",NodeKind::Table)}
    }
}
