//! Bounded multi-document proposals, coordinated history and explicit partial saves.
//! Applying and restoring never writes sources. Saving is deliberately per file.
use super::proposals::{rebase_selection, DocumentEditProposal, ProposalConflict};
use super::*;
use crate::editor::{EditBatch, RevisionedDocument};
use futures_util::future::{select, Either};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};

/// Editor-side coordination request, containing data only.
#[derive(Clone, Serialize, Deserialize)]
pub struct ViewRequest {
    /// Prepare, commit or cancel a held view.
    pub phase: String,
    /// Unique preparation identity, including across repeated undo/redo.
    pub token: String,
    /// Apply, undo or redo.
    pub direction: String,
    /// Opaque group identity (decimal string for JavaScript precision).
    pub group: String,
    /// Expected current text, then desired text at commit.
    pub text: String,
}
/// An editor acknowledgement, false when its input/history has drifted.
pub type ViewFuture = Pin<Box<dyn Future<Output = bool>>>;
/// A persistent document-view adapter, independent of a panel's mount lifetime.
pub type ViewAdapter = Rc<dyn Fn(ViewRequest) -> ViewFuture>;

#[derive(Clone, Default)]
struct Store {
    next: u64,
    busy: bool,
    groups: VecDeque<Rc<Group>>,
    views: HashMap<NodeId, (Signal<Document>, ViewAdapter)>,
    kinds: HashMap<NodeId, (Signal<Document>, String)>,
}
#[derive(Clone, Copy)]
struct Shared(Signal<Store>);
fn store(ws: Workspace) -> Signal<Store> {
    ws.shared_state(|| Shared(Signal::new_in_scope(Store::default(), ScopeId::ROOT)))
        .0
}

/// Register native or CodeMirror history ownership. Changing implementations retires groups.
pub fn register_view(ws: Workspace, node: NodeId, kind: &str, view: Option<ViewAdapter>) {
    let Some(doc) = ws.document(node) else {
        return;
    };
    let mut shared = store(ws);
    shared.with_mut(|state| {
        if state
            .kinds
            .get(&node)
            .is_some_and(|old| old.0 != doc || old.1 != kind)
        {
            state
                .groups
                .retain(|group| !group.files.iter().any(|file| file.base().node == node));
        }
        state.kinds.insert(node, (doc, kind.into()));
        if let Some(view) = view {
            state.views.insert(node, (doc, view));
        } else {
            state.views.remove(&node);
        }
    });
}

/// Release view adapters and retire groups when a document closes.
pub fn forget_document(ws: Workspace, node: NodeId) {
    let existing = ws
        .shared
        .peek()
        .get(&std::any::TypeId::of::<Shared>())
        .and_then(|value| value.downcast_ref::<Shared>())
        .copied();
    let Some(Shared(mut shared)) = existing else {
        return;
    };
    shared.with_mut(|state| {
        state.views.remove(&node);
        state.kinds.remove(&node);
        state
            .groups
            .retain(|group| !group.files.iter().any(|file| file.base().node == node));
    });
}

/// A failed group operation identifies the file that prevented the complete commit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupConflict {
    /// Blocking file, when the failure is document-specific.
    pub node: Option<NodeId>,
    /// Existing typed proposal failure; Changed also represents a history/view conflict.
    pub reason: ProposalConflict,
}
fn conflict(node: NodeId, reason: ProposalConflict) -> GroupConflict {
    GroupConflict {
        node: Some(node),
        reason,
    }
}

/// Immutable multi-file preview. One source, at most 64 files / 4096 edits.
#[derive(Clone)]
pub struct MultiDocumentEditProposal {
    files: Rc<Vec<DocumentEditProposal>>,
}
struct Group {
    id: u64,
    files: Rc<Vec<DocumentEditProposal>>,
    applied: bool,
}
impl MultiDocumentEditProposal {
    /// Compose already validated file proposals, refusing duplicates and aggregate overflow.
    pub fn new(mut files: Vec<DocumentEditProposal>) -> Result<Self, GroupConflict> {
        let fail = || GroupConflict {
            node: None,
            reason: ProposalConflict::LimitExceeded,
        };
        if files.is_empty() || files.len() > 64 {
            return Err(fail());
        }
        let source = files[0].base.source.clone();
        if files
            .iter()
            .any(|file| !Arc::ptr_eq(&source, &file.base.source))
        {
            return Err(GroupConflict {
                node: None,
                reason: ProposalConflict::UnsupportedSource,
            });
        }
        files.retain(|file| file.preview() != file.base().text);
        if files.is_empty() {
            return Err(fail());
        }
        let mut ids = std::collections::HashSet::new();
        let mut text = 0usize;
        let mut payload = 0usize;
        let mut edits = 0;
        for file in &files {
            if !ids.insert(file.base().node) {
                return Err(conflict(file.base().node, ProposalConflict::Changed));
            }
            text = text
                .saturating_add(file.base().text.len())
                .saturating_add(file.preview().len());
            edits += file.changes().len();
            for change in file.changes() {
                payload = payload
                    .saturating_add(change.removed_text.len())
                    .saturating_add(change.inserted_text.len());
            }
        }
        if text > 8 * 1024 * 1024 || payload > 4 * 1024 * 1024 || edits > 4096 {
            return Err(fail());
        }
        Ok(Self {
            files: Rc::new(files),
        })
    }
    /// Per-file original/proposed text and exact ranges, in deterministic plan order.
    pub fn files(&self) -> &[DocumentEditProposal] {
        &self.files
    }
    /// Non-mutating freshness/authority check for every preview.
    pub fn validate(&self, ws: Workspace) -> Result<(), GroupConflict> {
        for file in self.files.iter() {
            file.base
                .validate(ws)
                .map_err(|e| conflict(file.base().node, e))?;
        }
        Ok(())
    }
    /// Apply all buffers together after view preparation. No source is saved.
    pub async fn apply(&self, ws: Workspace) -> Result<u64, GroupConflict> {
        self.apply_guarded(ws, || true).await
    }
    /// As apply, but cancel if the originating review was superseded during preparation.
    pub async fn apply_guarded(
        &self,
        ws: Workspace,
        current: impl Fn() -> bool,
    ) -> Result<u64, GroupConflict> {
        self.validate(ws)?;
        let mut state = store(ws);
        let id = state.with_mut(|state| {
            state.next = state.next.saturating_add(1);
            state.next
        });
        run(
            ws,
            Rc::new(Group {
                id,
                files: self.files.clone(),
                applied: true,
            }),
            "apply",
            current,
        )
        .await?;
        Ok(id)
    }
}

struct Lease {
    ws: Workspace,
    requests: Vec<(NodeId, ViewAdapter, ViewRequest)>,
}
impl Drop for Lease {
    fn drop(&mut self) {
        let mut state = store(self.ws);
        state.with_mut(|state| state.busy = false);
        for (_, adapter, mut request) in self.requests.drain(..) {
            request.phase = "cancel".into();
            dioxus::core::spawn_forever(async move {
                let _ = select(
                    (adapter)(request),
                    Box::pin(futures_timer::Delay::new(std::time::Duration::from_secs(5))),
                )
                .await;
            });
        }
    }
}

async fn run(
    ws: Workspace,
    group: Rc<Group>,
    direction: &str,
    current: impl Fn() -> bool,
) -> Result<(), GroupConflict> {
    let mut shared = store(ws);
    let fail = || GroupConflict {
        node: None,
        reason: ProposalConflict::Changed,
    };
    if shared.peek().busy {
        return Err(fail());
    }
    shared.with_mut(|state| state.busy = true);
    let token = shared.with_mut(|state| {
        state.next = state.next.saturating_add(1);
        state.next.to_string()
    });
    let owners = shared.peek().kinds.clone();
    let mut lease = Lease {
        ws,
        requests: vec![],
    };
    for file in group.files.iter() {
        let node = file.base().node;
        let (doc, _) = file.base.identity(ws).map_err(|e| conflict(node, e))?;
        let adapter = shared
            .peek()
            .views
            .get(&node)
            .filter(|(owner, _)| *owner == doc)
            .map(|(_, adapter)| adapter.clone());
        if let Some(adapter) = adapter {
            lease.requests.push((
                node,
                adapter,
                ViewRequest {
                    phase: "prepare".into(),
                    token: token.clone(),
                    direction: direction.into(),
                    group: group.id.to_string(),
                    text: if direction == "redo" || direction == "apply" {
                        file.base().text.clone()
                    } else {
                        file.preview().into()
                    },
                },
            ));
        }
    }
    let prepared = futures_util::future::join_all(
        lease
            .requests
            .iter()
            .map(|(_, adapter, request)| adapter(request.clone())),
    );
    match select(
        Box::pin(prepared),
        Box::pin(futures_timer::Delay::new(std::time::Duration::from_secs(5))),
    )
    .await
    {
        Either::Left((ready, _)) => {
            if let Some(index) = ready.iter().position(|ready| !ready) {
                return Err(conflict(lease.requests[index].0, ProposalConflict::Changed));
            }
        }
        _ => return Err(fail()),
    }
    if !current()
        || group
            .files
            .iter()
            .any(|file| owners.get(&file.base().node) != shared.peek().kinds.get(&file.base().node))
        || (direction != "apply" && !shared.peek().groups.iter().any(|old| old.id == group.id))
    {
        return Err(fail());
    }
    let mut staged: Vec<(
        Signal<Document>,
        Signal<RevisionedDocument>,
        RevisionedDocument,
        String,
    )> = vec![];
    for file in group.files.iter() {
        let node = file.base().node;
        let (doc, session) = if direction == "apply" {
            file.base.live(ws)
        } else {
            file.base.identity(ws)
        }
        .map_err(|e| conflict(node, e))?;
        let adapter = shared
            .peek()
            .views
            .get(&node)
            .is_some_and(|(owner, _)| *owner == doc);
        let mut next = session.peek().clone();
        let expected = if direction == "redo" || direction == "apply" {
            &file.base().text
        } else {
            file.preview()
        };
        if doc.peek().text != expected || next.snapshot().text != expected {
            return Err(conflict(node, ProposalConflict::Changed));
        }
        if direction == "apply" {
            let live = next.snapshot();
            let batch = EditBatch {
                node,
                base_revision: live.revision,
                changes: file.changes().to_vec(),
                selection: rebase_selection(live, file.changes()),
            };
            next.apply_group(&batch, group.id)
                .map_err(|e| conflict(node, ProposalConflict::Invalid(e)))?;
        } else if adapter {
            next.replace_from_workspace(if direction == "undo" {
                file.base().text.clone()
            } else {
                file.preview().into()
            })
            .map_err(|e| conflict(node, ProposalConflict::Invalid(e)))?;
        } else {
            let redo = direction == "redo";
            if next.history_group(redo) != Some(group.id) {
                return Err(conflict(node, ProposalConflict::Changed));
            }
            (if redo { next.redo() } else { next.undo() })
                .map_err(|e| conflict(node, ProposalConflict::Invalid(e)))?;
        }
        let text = next.snapshot().text.clone();
        staged.push((doc, session, next, text));
    }
    // All fallible work is complete. No awaits/callbacks between these assignments.
    for (mut doc, mut session, next, text) in staged {
        session.set(next);
        doc.with_mut(|doc| doc.text = text);
    }
    shared.with_mut(|state| {
        state.groups.retain(|old| old.id != group.id);
        state.groups.push_back(Rc::new(Group {
            id: group.id,
            files: group.files.clone(),
            applied: direction != "undo",
        }));
        while state.groups.len() > 32
            || state
                .groups
                .iter()
                .map(|group| {
                    group
                        .files
                        .iter()
                        .map(|file| {
                            file.base().text.len()
                                + file.preview().len()
                                + file
                                    .changes()
                                    .iter()
                                    .map(|c| c.removed_text.len() + c.inserted_text.len())
                                    .sum::<usize>()
                        })
                        .sum::<usize>()
                })
                .sum::<usize>()
                > 32 * 1024 * 1024
        {
            state.groups.pop_front();
        }
    });
    for (node, _, request) in &mut lease.requests {
        request.phase = "commit".into();
        let file = group
            .files
            .iter()
            .find(|file| file.base().node == *node)
            .expect("prepared file");
        request.text = if direction == "undo" {
            file.base().text.clone()
        } else {
            file.preview().into()
        };
    }
    // Views are held, and their prepared history transitions were checked.
    let committed = futures_util::future::join_all(
        lease
            .requests
            .iter()
            .map(|(_, adapter, request)| adapter(request.clone())),
    );
    let view_failed = !matches!(select(
        Box::pin(committed),
        Box::pin(futures_timer::Delay::new(std::time::Duration::from_secs(5))),
    ).await, Either::Left((ready, _)) if ready.iter().all(|ready| *ready));
    // Buffers have committed. A lost view cannot undo a subset: retire the group,
    // and the lease's cancellation resynchronizes held views to canonical text.
    if view_failed {
        shared.with_mut(|state| state.groups.retain(|old| old.id != group.id));
    }
    Ok(())
}

/// Undo/redo a complete retained group. Newer edits block rather than being overwritten.
pub async fn restore(ws: Workspace, id: u64, redo: bool) -> Result<(), GroupConflict> {
    let group = store(ws)
        .peek()
        .groups
        .iter()
        .find(|group| group.id == id)
        .cloned()
        .ok_or(GroupConflict {
            node: None,
            reason: ProposalConflict::Closed,
        })?;
    if group.applied == redo {
        return Err(GroupConflict {
            node: None,
            reason: ProposalConflict::Changed,
        });
    }
    run(ws, group, if redo { "redo" } else { "undo" }, || true).await
}
/// Current retained group state, subscribing the caller to history changes.
pub fn is_applied(ws: Workspace, id: u64) -> Option<bool> {
    store(ws)
        .read()
        .groups
        .iter()
        .find(|group| group.id == id)
        .map(|group| group.applied)
}
/// Group marker reached by a native editor's next history command.
pub fn next_group(ws: Workspace, node: NodeId, redo: bool) -> Option<u64> {
    ws.editor_session(node)?.peek().history_group(redo)
}

/// Outcome of saving one reviewed file; success never implies a disk-wide transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileSaveStatus {
    /// Written successfully.
    Saved,
    /// Already contains the reviewed text on the backing source.
    AlreadySaved,
    /// Newer text/history prevents saving this review.
    Changed,
    /// Document/source is no longer open.
    Closed,
    /// Source is no longer writable Files content.
    ReadOnly,
    /// Expected backing version was refused.
    Conflict,
    /// Source reported another failure.
    Failed,
}
/// One explicit per-file save result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileSaveResult {
    /// Document identity.
    pub node: NodeId,
    /// Save outcome.
    pub status: FileSaveStatus,
}
/// Save only reviewed applied text, collecting every failure without rolling back successful writes.
pub async fn save(ws: Workspace, id: u64) -> Vec<FileSaveResult> {
    let group = store(ws)
        .peek()
        .groups
        .iter()
        .find(|group| group.id == id && group.applied)
        .cloned();
    let Some(group) = group else {
        return vec![];
    };
    let mut results = vec![];
    for file in group.files.iter() {
        let node = file.base().node;
        let status = match file.base.identity(ws) {
            Err(ProposalConflict::UnsupportedSource) => FileSaveStatus::ReadOnly,
            Err(_) => FileSaveStatus::Closed,
            Ok((doc, _)) if doc.peek().text != file.preview() => FileSaveStatus::Changed,
            Ok((doc, _)) if doc.peek().saved == file.preview() => FileSaveStatus::AlreadySaved,
            Ok(_) => match ws.save(node).await {
                Ok(()) => FileSaveStatus::Saved,
                Err(moonkale_core::SourceError::Conflict { .. }) => FileSaveStatus::Conflict,
                Err(_) => FileSaveStatus::Failed,
            },
        };
        results.push(FileSaveResult { node, status });
    }
    results
}
