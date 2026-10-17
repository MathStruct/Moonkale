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

#[test]
fn save_completion_acknowledges_only_the_submitted_snapshot() {
    use futures_util::FutureExt;
    use std::sync::{Arc, Mutex};
    struct HeldSave(Mutex<Option<futures_channel::oneshot::Receiver<()>>>);
    #[moonkale_core::async_trait]
    impl Source for HeldSave {
        fn id(&self) -> SourceId {
            node().source
        }
        fn descriptor(&self) -> SourceDescriptor {
            Files.descriptor()
        }
        async fn query(&self, _: Query) -> Result<QueryResult, SourceError> {
            Ok(QueryResult::default())
        }
        async fn fetch_text(&self, _: NodeId) -> Result<(String, Version), SourceError> {
            Ok((TEXT.into(), Version::default()))
        }
        async fn apply(&self, _: Transaction) -> Result<Applied, SourceError> {
            let receiver = self.0.lock().unwrap().take().unwrap();
            receiver.await.unwrap();
            Ok(Applied {
                results: vec![moonkale_core::OpResult::Ok {
                    node: node().id,
                    version: Version(42),
                }],
            })
        }
    }
    let (dom, mut ws) = setup();
    dom.in_scope(ScopeId::ROOT, || {
        let (release, receiver) = futures_channel::oneshot::channel();
        ws.add_source(Arc::new(HeldSave(Mutex::new(Some(receiver)))));
        rename(ws).apply(ws).unwrap();
        let mut doc = ws.document(node().id).unwrap();
        let submitted = doc.peek().text.clone();
        let mut saving = std::pin::pin!(ws.save(node().id));
        assert!(saving.as_mut().now_or_never().is_none());
        doc.write().text.push_str("// newer unsaved edit\r\n");
        release.send(()).unwrap();
        saving.as_mut().now_or_never().unwrap().unwrap();
        assert_eq!(doc.peek().saved, submitted);
        assert_eq!(
            ws.history.log.peek().text_at(node().id, None),
            Some(submitted)
        );
        assert_eq!(doc.peek().version, Version(42));
        assert!(doc.peek().text.ends_with("// newer unsaved edit\r\n"));
        assert!(doc.peek().dirty());
    });
}

fn second_group_doc(mut ws: Workspace) -> NodeId {
    let mut n = node();
    n.id = NodeId::fresh("group-second");
    n.native_key = "second.rs".into();
    let doc = Signal::new_in_scope(
        Document::new(n.clone(), TEXT.into(), n.version),
        ScopeId::ROOT,
    );
    ws.docs.open.with_mut(|docs| docs.push((n.id, doc)));
    n.id
}
fn group_proposal(
    ws: Workspace,
    second: NodeId,
) -> moonkale_ext_api::workspace::grouped_edits::MultiDocumentEditProposal {
    let files = [node().id, second]
        .into_iter()
        .map(|id| {
            let base = ws.document_edit_base(id).unwrap();
            let start = base.snapshot().text.find("old").unwrap();
            base.propose(vec![TextChange {
                range: start..start + 3,
                removed_text: "old".into(),
                inserted_text: "新名".into(),
            }])
            .unwrap()
        })
        .collect();
    moonkale_ext_api::workspace::grouped_edits::MultiDocumentEditProposal::new(files).unwrap()
}
#[test]
fn grouped_apply_and_guarded_history_are_atomic_and_preserve_newer_edits() {
    use futures_util::FutureExt;
    use moonkale_ext_api::workspace::grouped_edits as groups;
    let (dom, ws) = setup();
    dom.in_scope(ScopeId::ROOT, || {
        let second = second_group_doc(ws);
        let proposal = group_proposal(ws, second);
        let id = proposal.apply(ws).now_or_never().unwrap().unwrap();
        for node in [node().id, second] {
            assert_eq!(
                ws.document(node).unwrap().peek().text,
                TEXT.replacen("old", "新名", 1)
            );
        }
        let mut session = ws.editor_session(second).unwrap();
        let base = session.peek().snapshot().clone();
        session
            .write()
            .apply(&moonkale_ext_api::editor::EditBatch {
                node: second,
                base_revision: base.revision,
                changes: vec![TextChange {
                    range: 0..0,
                    removed_text: String::new(),
                    inserted_text: "// newer\r\n".into(),
                }],
                selection: None,
            })
            .unwrap();
        ws.document(second).unwrap().write().text = session.peek().snapshot().text.clone();
        assert!(groups::restore(ws, id, false)
            .now_or_never()
            .unwrap()
            .is_err());
        assert!(ws.document(node().id).unwrap().peek().text.contains("新名"));
        let restored = session.write().undo().unwrap().unwrap();
        ws.document(second).unwrap().write().text = restored.text;
        groups::restore(ws, id, false)
            .now_or_never()
            .unwrap()
            .unwrap();
        for node in [node().id, second] {
            assert_eq!(ws.document(node).unwrap().peek().text, TEXT);
        }
        groups::restore(ws, id, true)
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_eq!(groups::is_applied(ws, id), Some(true));
        // Closing one participant permanently retires the coordinated history.
        ws.close_node(second);
        assert!(groups::restore(ws, id, false)
            .now_or_never()
            .unwrap()
            .is_err());
        assert!(ws.document(node().id).unwrap().peek().text.contains("新名"));
    });
}
#[test]
fn group_validation_and_view_refusal_never_partially_commit() {
    use futures_util::FutureExt;
    use moonkale_ext_api::workspace::grouped_edits as groups;
    let (dom, ws) = setup();
    dom.in_scope(ScopeId::ROOT, || {
        let second = second_group_doc(ws);
        let proposal = group_proposal(ws, second);
        ws.document(second).unwrap().write().version = Version(9);
        assert!(proposal.apply(ws).now_or_never().unwrap().is_err());
        assert_eq!(
            ws.editor_session(node().id)
                .unwrap()
                .peek()
                .snapshot()
                .revision
                .0,
            0
        );
        assert_eq!(ws.document(node().id).unwrap().peek().text, TEXT);
        let proposal = group_proposal(ws, second);
        groups::register_view(
            ws,
            second,
            "test",
            Some(std::rc::Rc::new(|_| Box::pin(async { false }))),
        );
        assert!(proposal.apply(ws).now_or_never().unwrap().is_err());
        assert_eq!(ws.document(node().id).unwrap().peek().text, TEXT);
        assert_eq!(ws.document(second).unwrap().peek().text, TEXT);
    });
}
#[test]
fn group_partial_save_retains_failed_buffers_and_undo_is_buffer_only() {
    use futures_util::FutureExt;
    use moonkale_ext_api::workspace::grouped_edits as groups;
    struct Partial;
    #[moonkale_core::async_trait]
    impl Source for Partial {
        fn id(&self) -> SourceId {
            Files.id()
        }
        fn descriptor(&self) -> SourceDescriptor {
            Files.descriptor()
        }
        async fn query(&self, _: Query) -> Result<QueryResult, SourceError> {
            Ok(Default::default())
        }
        async fn fetch_text(&self, _: NodeId) -> Result<(String, Version), SourceError> {
            Ok((TEXT.into(), Version(0)))
        }
        async fn apply(&self, tx: Transaction) -> Result<Applied, SourceError> {
            let moonkale_core::Op::WriteText { node: target, .. } = tx.ops[0] else {
                panic!("write only")
            };
            if target == node().id {
                Ok(Applied {
                    results: vec![moonkale_core::OpResult::Ok {
                        node: target,
                        version: Version(1),
                    }],
                })
            } else {
                Err(SourceError::Conflict {
                    expected: Version(0),
                    actual: Version(2),
                })
            }
        }
    }
    let (dom, mut ws) = setup();
    dom.in_scope(ScopeId::ROOT, || {
        ws.add_source(std::sync::Arc::new(Partial));
        let second = second_group_doc(ws);
        let id = group_proposal(ws, second)
            .apply(ws)
            .now_or_never()
            .unwrap()
            .unwrap();
        let results = groups::save(ws, id).now_or_never().unwrap();
        assert_eq!(results[0].status, groups::FileSaveStatus::Saved);
        assert_eq!(results[1].status, groups::FileSaveStatus::Conflict);
        assert!(!ws.document(node().id).unwrap().peek().dirty());
        assert!(ws.document(second).unwrap().peek().dirty());
        let results = groups::save(ws, id).now_or_never().unwrap();
        assert_eq!(results[0].status, groups::FileSaveStatus::AlreadySaved);
        assert_eq!(results[1].status, groups::FileSaveStatus::Conflict);
        groups::restore(ws, id, false)
            .now_or_never()
            .unwrap()
            .unwrap();
        assert!(ws.document(node().id).unwrap().peek().dirty());
        assert!(!ws.document(second).unwrap().peek().dirty());
    });
}

#[test]
fn grouped_history_rejects_branching_redo_and_backend_changes() {
    use futures_util::FutureExt;
    use moonkale_ext_api::workspace::grouped_edits as groups;
    let (dom, ws) = setup();
    dom.in_scope(ScopeId::ROOT, || {
        let second = second_group_doc(ws);
        groups::register_view(ws, node().id, "native", None);
        let first = group_proposal(ws, second)
            .apply(ws)
            .now_or_never()
            .unwrap()
            .unwrap();
        groups::restore(ws, first, false)
            .now_or_never()
            .unwrap()
            .unwrap();
        // Applying a new group after undo creates a new history branch.
        let next = group_proposal(ws, second)
            .apply(ws)
            .now_or_never()
            .unwrap()
            .unwrap();
        assert!(groups::restore(ws, first, true)
            .now_or_never()
            .unwrap()
            .is_err());
        assert_eq!(groups::is_applied(ws, next), Some(true));
        groups::register_view(ws, node().id, "codemirror", None);
        assert_eq!(groups::is_applied(ws, next), None);
        assert!(groups::restore(ws, next, false)
            .now_or_never()
            .unwrap()
            .is_err());
        for id in [node().id, second] {
            assert!(ws.document(id).unwrap().peek().text.contains("新名"));
        }
    });
}

#[test]
fn grouped_limits_and_superseded_review_refuse_before_mutation() {
    use futures_util::FutureExt;
    use moonkale_ext_api::workspace::grouped_edits as groups;
    let (dom, ws) = setup();
    dom.in_scope(ScopeId::ROOT, || {
        let second = second_group_doc(ws);
        assert!(groups::MultiDocumentEditProposal::new(vec![]).is_err());
        assert!(groups::MultiDocumentEditProposal::new(vec![rename(ws); 65]).is_err());
        assert!(groups::MultiDocumentEditProposal::new(vec![rename(ws); 2]).is_err());
        assert!(group_proposal(ws, second)
            .apply_guarded(ws, || false)
            .now_or_never()
            .unwrap()
            .is_err());
        for id in [node().id, second] {
            assert_eq!(ws.document(id).unwrap().peek().text, TEXT);
        }
    });
}

#[test]
fn postcommit_view_failure_retires_history_without_losing_complete_buffers() {
    use futures_util::FutureExt;
    use moonkale_ext_api::workspace::grouped_edits as groups;
    let (dom, ws) = setup();
    dom.in_scope(ScopeId::ROOT, || {
        let second = second_group_doc(ws);
        groups::register_view(
            ws,
            second,
            "test",
            Some(std::rc::Rc::new(|r| {
                Box::pin(async move { r.phase != "commit" })
            })),
        );
        let id = group_proposal(ws, second)
            .apply(ws)
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_eq!(groups::is_applied(ws, id), None);
        assert!(groups::restore(ws, id, false)
            .now_or_never()
            .unwrap()
            .is_err());
        for id in [node().id, second] {
            assert!(ws.document(id).unwrap().peek().text.contains("新名"));
        }
    });
}

#[test]
fn dropping_preparation_releases_the_group_without_mutating_buffers() {
    use futures_util::FutureExt;
    use moonkale_ext_api::workspace::grouped_edits as groups;
    let (dom, ws) = setup();
    dom.in_scope(ScopeId::ROOT, || {
        let second = second_group_doc(ws);
        groups::register_view(
            ws,
            second,
            "held",
            Some(std::rc::Rc::new(|_| Box::pin(std::future::pending()))),
        );
        assert!(group_proposal(ws, second)
            .apply(ws)
            .now_or_never()
            .is_none());
        for id in [node().id, second] {
            assert_eq!(ws.document(id).unwrap().peek().text, TEXT);
        }
        groups::register_view(ws, second, "native", None);
        assert!(group_proposal(ws, second)
            .apply(ws)
            .now_or_never()
            .unwrap()
            .is_ok());
    });
}

#[test]
fn aggregate_text_budget_is_checked_before_apply() {
    use moonkale_ext_api::workspace::grouped_edits::MultiDocumentEditProposal;
    let (dom, mut ws) = setup();
    dom.in_scope(ScopeId::ROOT, || {
        let mut proposals = vec![];
        for i in 0..9 {
            let mut n = node();
            n.id = NodeId::fresh(&format!("large-group-{i}"));
            let text = "x".repeat(500 * 1024);
            let doc =
                Signal::new_in_scope(Document::new(n.clone(), text, n.version), ScopeId::ROOT);
            ws.docs.open.with_mut(|docs| docs.push((n.id, doc)));
            proposals.push(
                ws.document_edit_base(n.id)
                    .unwrap()
                    .propose(vec![TextChange {
                        range: 0..1,
                        removed_text: "x".into(),
                        inserted_text: "y".into(),
                    }])
                    .unwrap(),
            );
        }
        assert_eq!(
            MultiDocumentEditProposal::new(proposals)
                .err()
                .unwrap()
                .reason,
            ProposalConflict::LimitExceeded
        );
        for (_, doc) in ws.docs.open.peek().iter().skip(1) {
            assert!(doc.peek().text.starts_with('x'));
        }
    });
}

#[test]
fn evicted_group_markers_never_fall_back_to_partial_history() {
    use futures_util::FutureExt;
    use moonkale_ext_api::workspace::grouped_edits as groups;
    let (dom, ws) = setup();
    dom.in_scope(ScopeId::ROOT, || {
        let second = second_group_doc(ws);
        let mut ids = vec![];
        for i in 0..33 {
            let files = [node().id, second]
                .into_iter()
                .map(|id| {
                    ws.document_edit_base(id)
                        .unwrap()
                        .propose(vec![TextChange {
                            range: 0..0,
                            removed_text: String::new(),
                            inserted_text: format!("// {i}\r\n"),
                        }])
                        .unwrap()
                })
                .collect();
            ids.push(
                groups::MultiDocumentEditProposal::new(files)
                    .unwrap()
                    .apply(ws)
                    .now_or_never()
                    .unwrap()
                    .unwrap(),
            );
        }
        assert_eq!(groups::is_applied(ws, ids[0]), None);
        for id in ids.iter().skip(1).rev() {
            groups::restore(ws, *id, false)
                .now_or_never()
                .unwrap()
                .unwrap();
        }
        assert_eq!(groups::next_group(ws, node().id, false), Some(ids[0]));
        assert!(groups::restore(ws, ids[0], false)
            .now_or_never()
            .unwrap()
            .is_err());
        for node in [node().id, second] {
            assert_eq!(
                ws.document(node).unwrap().peek().text,
                format!("// 0\r\n{TEXT}")
            );
        }
    });
}
