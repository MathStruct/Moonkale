//! Structural proposals use canonical editor history and fail without mutation.
use dioxus::prelude::*;
use moonkale_core::{Node, NodeId, NodeKind, SourceError, SourceId, Version};
use moonkale_ext_api::{
    document::Document,
    editor::{TextChange, Utf16Selection},
    AttachFuture, FolderAccess, OpenFolderFuture, OpenOptions, Workspace, WorkspaceConfig,
};

fn open(_: String, _: OpenOptions) -> OpenFolderFuture {
    Box::pin(async { Err(SourceError::NotFound) })
}
fn attach(_: moonkale_core::SourceDescriptor) -> AttachFuture {
    Box::pin(async { Err(SourceError::NotFound) })
}
thread_local! {
    static SETUP: std::cell::RefCell<Option<(Workspace, Signal<bool>)>> = const { std::cell::RefCell::new(None) };
}
fn node() -> Node {
    Node {
        id: NodeId::fresh("session-remount"),
        source: SourceId::new("test"),
        kind: NodeKind::File,
        label: "test.rs".into(),
        native_key: "test.rs".into(),
        props: Default::default(),
        content: None,
        version: Version::default(),
    }
}
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
            Document::new(n.clone(), TEXT.into(), n.version),
            ScopeId::ROOT,
        );
        ws.docs.open.with_mut(|docs| docs.push((n.id, doc)));
        ws.add_source(std::sync::Arc::new(Files));
        ws
    });
    let mounted = use_signal(|| true);
    use_hook(move || SETUP.with(|setup| *setup.borrow_mut() = Some((ws, mounted))));
    rsx! {}
}

use moonkale_core::{
    Applied, Query, QueryResult, Source, SourceDescriptor, SourceFamily, Transaction,
};
use moonkale_ext_api::workspace::proposals::{DocumentEditProposal, ProposalConflict};
use moonkale_index::lossless::RustSyntaxSnapshot;
const TEXT: &str = "// 😀\r\nfn old() {}\r\nfn second() {}\r\n";
struct Files;
#[moonkale_core::async_trait]
impl Source for Files {
    fn id(&self) -> SourceId {
        node().source
    }
    fn descriptor(&self) -> SourceDescriptor {
        let capabilities = moonkale_core::Capabilities {
            write: true,
            ..Default::default()
        };
        SourceDescriptor {
            truth: Default::default(),
            id: self.id(),
            root: node().id,
            display_name: "test".into(),
            family: SourceFamily::Folder,
            capabilities,
        }
    }
    async fn query(&self, _: Query) -> Result<QueryResult, SourceError> {
        Ok(QueryResult::default())
    }
    async fn fetch_text(&self, _: NodeId) -> Result<(String, Version), SourceError> {
        Ok((TEXT.into(), Version::default()))
    }
    async fn apply(&self, _: Transaction) -> Result<Applied, SourceError> {
        Err(SourceError::Conflict {
            expected: Version(0),
            actual: Version(1),
        })
    }
}
fn rename(ws: Workspace) -> DocumentEditProposal {
    let base = ws.document_edit_base(node().id).unwrap();
    let syntax = RustSyntaxSnapshot::parse(&base.snapshot().text).unwrap();
    let plan = syntax.plan_declaration_rename(0, "新名").unwrap();
    plan.apply_to(&base.snapshot().text).unwrap();
    base.propose(
        plan.edits()
            .iter()
            .map(|edit| TextChange {
                range: edit.range.clone(),
                removed_text: edit.removed_text.clone(),
                inserted_text: edit.inserted_text.clone(),
            })
            .collect(),
    )
    .unwrap()
}
fn setup() -> (VirtualDom, Workspace) {
    let mut dom = VirtualDom::new(App);
    dom.rebuild_in_place();
    let (ws, _) = SETUP.with(|setup| setup.borrow_mut().take()).unwrap();
    (dom, ws)
}
#[test]
fn proposal_preview_commit_and_history_preserve_unicode_crlf_and_live_selection() {
    let (dom, ws) = setup();
    dom.in_scope(ScopeId::ROOT, || {
        let proposal = rename(ws);
        let mut session = ws.editor_session(node().id).unwrap();
        let revision = session.peek().snapshot().revision;
        let end = session.peek().utf16_len();
        session.write().set_selection(
            revision,
            Utf16Selection {
                anchor: end,
                head: end,
            },
        );
        assert_eq!(ws.document(node().id).unwrap().peek().text, TEXT);
        let next = proposal.apply(ws).unwrap();
        assert_eq!(next.text, TEXT.replacen("old", "新名", 1));
        assert_eq!(next.text, proposal.preview());
        assert_eq!(next.selection.unwrap().head, end - 1);
        let doc = ws.document(node().id).unwrap();
        assert!(doc.peek().dirty());
        assert_eq!(doc.peek().saved, TEXT);
        assert_eq!(session.write().undo().unwrap().unwrap().text, TEXT);
        assert_eq!(session.write().redo().unwrap().unwrap().text, next.text);
        assert_eq!(proposal.apply(ws), Err(ProposalConflict::Changed));
    });
}
#[test]
fn stale_revision_and_reopened_identical_text_do_not_accept_old_proposals() {
    let (dom, mut ws) = setup();
    dom.in_scope(ScopeId::ROOT, || {
        let proposal = rename(ws);
        let mut session = ws.editor_session(node().id).unwrap();
        let batch = moonkale_ext_api::editor::EditBatch {
            node: node().id,
            base_revision: session.peek().snapshot().revision,
            changes: vec![TextChange {
                range: 0..0,
                removed_text: "".into(),
                inserted_text: " ".into(),
            }],
            selection: None,
        };
        session.write().apply(&batch).unwrap();
        session.write().undo().unwrap();
        assert_eq!(proposal.apply(ws), Err(ProposalConflict::Changed));
        assert_eq!(session.peek().snapshot().text, TEXT);
        ws.close_node(node().id);
        let n = node();
        ws.docs.open.write().push((
            n.id,
            Signal::new_in_scope(
                Document::new(n, TEXT.into(), Version::default()),
                ScopeId::ROOT,
            ),
        ));
        ws.editor_session(node().id).unwrap();
        assert_eq!(proposal.apply(ws), Err(ProposalConflict::Closed));
        assert_eq!(proposal.preview(), TEXT.replacen("old", "新名", 1));
    });
}
#[test]
fn canonical_edits_versions_and_permissions_are_checked_again_at_commit() {
    let (dom, mut ws) = setup();
    dom.in_scope(ScopeId::ROOT, || {
        let proposal = rename(ws);
        let mut doc = ws.document(node().id).unwrap();
        doc.write().text.push_str("// unsaved\r\n");
        assert_eq!(proposal.apply(ws), Err(ProposalConflict::Changed));
        assert!(matches!(
            ws.document_edit_base(node().id),
            Err(ProposalConflict::Changed)
        ));
        doc.write().text = TEXT.into();
        doc.write().version = Version(1);
        assert_eq!(proposal.apply(ws), Err(ProposalConflict::Changed));
        doc.write().version = Version::default();
        ws.sources.open.write()[0].descriptor.capabilities.write = false;
        assert_eq!(proposal.apply(ws), Err(ProposalConflict::UnsupportedSource));
        ws.sources.open.write()[0].descriptor.capabilities.write = true;
        ws.sources.open.write()[0].descriptor.truth = moonkale_core::SourceTruth::Store;
        assert_eq!(proposal.apply(ws), Err(ProposalConflict::UnsupportedSource));
        ws.sources.open.write()[0].descriptor.truth = Default::default();
        ws.add_source(std::sync::Arc::new(Files));
        assert_eq!(proposal.apply(ws), Err(ProposalConflict::Closed));
        assert_eq!(doc.peek().text, TEXT);
    });
}
#[test]
fn invalid_and_overlapping_edits_never_mutate_live_history() {
    let (dom, ws) = setup();
    dom.in_scope(ScopeId::ROOT, || {
        let base = ws.document_edit_base(node().id).unwrap();
        for edits in [
            vec![TextChange {
                range: 4..5,
                removed_text: "".into(),
                inserted_text: "x".into(),
            }],
            vec![
                TextChange {
                    range: 0..0,
                    removed_text: "".into(),
                    inserted_text: "x".into()
                };
                2
            ],
        ] {
            assert!(matches!(
                base.propose(edits),
                Err(ProposalConflict::Invalid(_))
            ));
        }
        assert_eq!(ws.document(node().id).unwrap().peek().text, TEXT);
        assert!(ws
            .editor_session(node().id)
            .unwrap()
            .write()
            .undo()
            .unwrap()
            .is_none());
    });
}

#[test]
fn move_is_one_history_entry_and_save_conflict_retains_dirty_preview() {
    use futures_util::FutureExt;
    let (dom, ws) = setup();
    dom.in_scope(ScopeId::ROOT, || {
        let base = ws.document_edit_base(node().id).unwrap();
        let syntax = RustSyntaxSnapshot::parse(&base.snapshot().text).unwrap();
        let plan = syntax.plan_move_function(1, Some(0)).unwrap();
        let expected = plan.apply_to(&base.snapshot().text).unwrap();
        let proposal = base
            .propose(
                plan.edits()
                    .iter()
                    .map(|edit| TextChange {
                        range: edit.range.clone(),
                        removed_text: edit.removed_text.clone(),
                        inserted_text: edit.inserted_text.clone(),
                    })
                    .collect(),
            )
            .unwrap();
        let applied = proposal.apply(ws).unwrap();
        assert_eq!(applied.text, expected);
        assert!(matches!(
            ws.save(node().id).now_or_never().unwrap(),
            Err(SourceError::Conflict { .. })
        ));
        let mut doc = ws.document(node().id).unwrap();
        assert!(doc.peek().dirty());
        assert_eq!(doc.peek().text, proposal.preview());
        assert_eq!(doc.peek().saved, TEXT);
        let mut session = ws.editor_session(node().id).unwrap();
        assert_eq!(session.write().undo().unwrap().unwrap().text, TEXT);
        assert!(session.write().undo().unwrap().is_none());
        // Resynchronize through the ordinary document path before recomputing.
        doc.write().text = TEXT.into();
        assert!(rename(ws).apply(ws).is_ok());
    });
}

#[test]
fn proposal_limits_and_noop_are_explicit() {
    let (dom, ws) = setup();
    dom.in_scope(ScopeId::ROOT, || {
        let base = ws.document_edit_base(node().id).unwrap();
        let too_many = vec![
            TextChange {
                range: 0..0,
                removed_text: String::new(),
                inserted_text: String::new()
            };
            4097
        ];
        assert!(matches!(
            base.propose(too_many),
            Err(ProposalConflict::LimitExceeded)
        ));
        assert!(matches!(
            base.propose(vec![TextChange {
                range: 0..0,
                removed_text: String::new(),
                inserted_text: "x".repeat(512 * 1024)
            }]),
            Err(ProposalConflict::LimitExceeded)
        ));
        let noop = base.propose(vec![]).unwrap();
        assert_eq!(noop.apply(ws).unwrap().revision, base.snapshot().revision);
        assert!(!ws.document(node().id).unwrap().peek().dirty());
        assert!(ws
            .editor_session(node().id)
            .unwrap()
            .write()
            .undo()
            .unwrap()
            .is_none());
    });
}
