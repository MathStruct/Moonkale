//! Session-scoped, bounded review inbox. Submission never changes text.
use crate::L;
use dioxus::prelude::*;
use moonkale_core::{NodeId, Query, SourceId, SourceTruth, Version};
use moonkale_ext_api::{
    editor::TextChange,
    t,
    workspace::{
        grouped_edits::{self, MultiDocumentEditProposal},
        proposals::DocumentEditProposal,
    },
    Workspace,
};
use moonkale_llm::ToolCall;
use std::{collections::BTreeSet, rc::Rc};

const MAX_FILES: usize = 64;
const MAX_BYTES: usize = 4 * 1024 * 1024;
const MAX_PENDING: usize = 16;
#[derive(Clone, Copy, PartialEq, Eq)]
enum FilePhase {
    Pending,
    Buffered,
    Saved,
}
#[derive(Clone)]
struct File {
    source: SourceId,
    source_name: String,
    handle: std::sync::Arc<dyn moonkale_core::Source>,
    node: NodeId,
    path: String,
    version: Version,
    original: String,
    next: String,
    plan: Option<DocumentEditProposal>,
    selected: bool,
    phase: FilePhase,
}
#[derive(Clone)]
struct Conflict {
    id: String,
    partial: bool,
    file: usize,
    current: String,
    version: Version,
}
#[derive(Clone)]
struct Rows {
    source: SourceId,
    source_name: String,
    handle: std::sync::Arc<dyn moonkale_core::Source>,
    batch: moonkale_core::RowBatch,
    bases: Vec<Option<moonkale_core::RowSnapshot>>,
    selected: bool,
}
#[derive(Clone)]
struct RowConflict {
    source: SourceId,
    op: usize,
    current: Option<moonkale_core::RowSnapshot>,
}
#[derive(Clone)]
struct Proposal {
    id: String,
    outer: String,
    actor: String,
    created: u64,
    files: Vec<File>,
    rows: Vec<Rows>,
    conflict: Option<Conflict>,
    row_conflict: Option<RowConflict>,
    report: String,
    busy: bool,
}
#[derive(Clone, Copy)]
pub(crate) struct Inbox(Signal<Vec<Rc<Proposal>>>);
#[derive(Clone, Copy)]
struct ReviewOpened(Signal<bool>);
impl Inbox {
    pub fn opened(ws: Workspace) -> bool {
        *ws.shared_state(|| ReviewOpened(Signal::new_in_scope(false, ScopeId::ROOT)))
            .0
            .read()
    }
    pub fn open(mut ws: Workspace) {
        ws.shared_state(|| ReviewOpened(Signal::new_in_scope(false, ScopeId::ROOT)))
            .0
            .set(true);
        ws.show_panel("proposals");
    }
    fn get(ws: Workspace) -> Self {
        ws.shared_state(|| Self(Signal::new_in_scope(Vec::new(), ScopeId::ROOT)))
    }
    pub fn count(ws: Workspace) -> usize {
        let outer = ws.saved_sessions.active.read().clone();
        let pending = crate::panel::Chats::get(ws)
            .sessions
            .read()
            .iter()
            .filter(|s| s.pending.read().is_some())
            .count();
        pending
            + Self::get(ws)
                .0
                .read()
                .iter()
                .filter(|p| p.outer == outer && (!p.files.is_empty() || !p.rows.is_empty()))
                .count()
    }
    fn update(ws: Workspace, id: &str, edit: impl FnOnce(&mut Proposal)) {
        let mut inbox = Self::get(ws).0;
        inbox.with_mut(|items| {
            if let Some(p) = items.iter_mut().find(|p| p.id == id) {
                edit(Rc::make_mut(p));
            }
        });
    }
    fn begin(ws: Workspace, id: &str) -> bool {
        Self::get(ws).0.with_mut(|items| {
            let Some(item) = items.iter_mut().find(|p| p.id == id) else {
                return false;
            };
            let p = Rc::make_mut(item);
            if p.busy {
                return false;
            }
            p.busy = true;
            true
        })
    }
    fn select(ws: Workspace, id: &str, node: NodeId, selected: bool) {
        Self::update(ws, id, |p| {
            if !p.busy {
                if let Some(file) = p
                    .files
                    .iter_mut()
                    .find(|f| f.node == node && f.phase != FilePhase::Saved)
                {
                    file.selected = selected;
                }
            }
        });
    }
    fn reject(ws: Workspace, id: &str) {
        Self::update(ws, id, |p| {
            if !p.busy && (p.files.iter().any(|f| f.selected) || p.rows.iter().any(|r| r.selected))
            {
                p.rows.retain(|r| !r.selected);
                p.files
                    .retain(|f| !f.selected && f.phase != FilePhase::Saved);
                p.conflict = None;
                p.row_conflict = None;
            }
        });
    }
    fn remove(ws: Workspace, id: &str) {
        Self::get(ws)
            .0
            .with_mut(|items| items.retain(|p| p.id != id || p.busy));
    }
}
fn active(ws: Workspace, outer: &str) -> bool {
    *ws.saved_sessions.active.peek() == outer
}
fn handle(ws: Workspace, id: &SourceId) -> Result<moonkale_ext_api::SourceHandle, String> {
    let h = ws
        .sources
        .open
        .peek()
        .iter()
        .find(|s| &s.descriptor.id == id)
        .cloned()
        .ok_or("source closed")?;
    if !h.descriptor.capabilities.write {
        return Err("This review adapter requires a writable source".into());
    }
    Ok(h)
}
/// Require the exact read token, including for legacy single-file replacements.
fn expected(input: &serde_json::Value) -> Result<Version, String> {
    input
        .get("expected")
        .and_then(|v| v.as_str())
        .and_then(|v| v.parse().ok())
        .map(Version)
        .ok_or_else(|| "expected must be the version string returned by graph.fetch".into())
}

pub(crate) async fn submit(
    mut ws: Workspace,
    outer: String,
    actor: String,
    call: ToolCall,
) -> Result<String, String> {
    if !active(ws, &outer) {
        return Err("session changed".into());
    }
    let inputs = if call.name == "editor.replace" {
        vec![call.input.clone()]
    } else {
        call.input
            .get("files")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default()
    };
    let row_inputs = call
        .input
        .get("rows")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    if (inputs.is_empty() && row_inputs.is_empty())
        || inputs.len() + row_inputs.len() > MAX_FILES
        || call.input.to_string().len() > MAX_BYTES
    {
        return Err("proposal exceeds the file/text budget".into());
    }
    if Inbox::get(ws).0.peek().len() >= MAX_PENDING {
        return Err("approval inbox is full".into());
    }
    let mut files = Vec::new();
    let mut seen = BTreeSet::new();
    let mut bytes = 0usize;
    for input in inputs {
        let source = SourceId::new(
            input
                .get("source")
                .and_then(|v| v.as_str())
                .ok_or("source is required")?,
        );
        let h = handle(ws, &source)?;
        let node: NodeId = input
            .get("node")
            .and_then(|v| v.as_str())
            .ok_or("node is required")?
            .parse()
            .map_err(|_| "invalid node")?;
        if !seen.insert(node) {
            return Err("duplicate file in proposal".into());
        }
        let version = expected(&input)?;
        let (original, actual) = h.source.fetch_text(node).await.map_err(|e| e.to_string())?;
        if !active(ws, &outer) {
            return Err("session changed".into());
        }
        if actual != version {
            return Err("stale read: fetch the file again before proposing".into());
        }
        if original.len() > 512 * 1024 {
            return Err("file exceeds proposal text budget".into());
        }
        let n = h
            .source
            .query(Query::Node(node))
            .await
            .map_err(|e| e.to_string())?
            .nodes
            .into_iter()
            .next()
            .ok_or("file missing")?;
        if !active(ws, &outer) {
            return Err("session changed".into());
        }
        ws.links.agent_documents.with_mut(|ids| {
            ids.insert(node);
        });
        ws.load_node_guarded(n.clone(), || active(ws, &outer))
            .await
            .map_err(|e| e.to_string())?;
        let doc = ws.document(node).ok_or("file closed")?;
        if doc.peek().text != original || doc.peek().version != version {
            return Err("The open buffer differs from the version the agent read".into());
        }
        let next = if call.name == "editor.replace" {
            let old = input
                .get("old")
                .and_then(|v| v.as_str())
                .ok_or("old is required")?;
            let new = input
                .get("new")
                .and_then(|v| v.as_str())
                .ok_or("new is required")?;
            if old.is_empty() || original.matches(old).count() != 1 {
                return Err("old must match exactly once".into());
            }
            original.replacen(old, new, 1)
        } else {
            input
                .get("text")
                .and_then(|v| v.as_str())
                .ok_or("text is required")?
                .to_string()
        };
        bytes = bytes
            .saturating_add(original.len())
            .saturating_add(next.len());
        if bytes > MAX_BYTES {
            return Err("proposal exceeds text budget".into());
        }
        // A watcher can refresh a clean headless document without a mounted
        // editor to acknowledge it. Reconcile that mirror before capturing a
        // new proposal; never reconcile a dirty buffer or an old proposal.
        if !doc.peek().dirty() {
            let mut session = ws.editor_session(node).ok_or("file closed")?;
            session
                .write()
                .replace_from_workspace(original.clone())
                .map_err(|e| format!("{e:?}"))?;
        }
        let plan = if h.descriptor.truth == SourceTruth::Files {
            let base = ws.document_edit_base(node).map_err(|e| format!("{e:?}"))?;
            Some(
                base.propose(vec![TextChange {
                    range: 0..original.len(),
                    removed_text: original.clone(),
                    inserted_text: next.clone(),
                }])
                .map_err(|e| format!("{e:?}"))?,
            )
        } else {
            if !h.descriptor.capabilities.atomic_text {
                return Err("store source does not support atomic text proposals".into());
            }
            None
        };
        files.push(File {
            source,
            source_name: h.descriptor.display_name.clone(),
            handle: h.source.clone(),
            node,
            path: n.native_key,
            version,
            original,
            next,
            plan,
            selected: true,
            phase: FilePhase::Pending,
        });
    }
    let mut rows = Vec::new();
    let mut row_sources = BTreeSet::new();
    for input in row_inputs {
        let source = SourceId::new(
            input
                .get("source")
                .and_then(|v| v.as_str())
                .ok_or("row source is required")?,
        );
        if !row_sources.insert(source.clone()) {
            return Err("combine operations from one database into one row batch".into());
        }
        let h = handle(ws, &source)?;
        if !h.descriptor.capabilities.atomic_rows {
            return Err("source does not support atomic row writes".into());
        }
        let batch: moonkale_core::RowBatch =
            serde_json::from_value(input.get("batch").cloned().ok_or("row batch is required")?)
                .map_err(|e| e.to_string())?;
        batch.validate().map_err(|e| e.to_string())?;
        h.source
            .preview_rows(batch.clone())
            .await
            .map_err(|e| e.to_string())?;
        bytes = bytes.saturating_add(serde_json::to_vec(&batch).map_err(|e| e.to_string())?.len());
        let mut bases = Vec::new();
        for op in &batch.ops {
            let page = h
                .source
                .read_rows(op.table.clone(), 1000)
                .await
                .map_err(|e| e.to_string())?;
            let base = page.rows.into_iter().find(|r| r.key == op.key);
            if op.expected != base.as_ref().map(|r| r.version) {
                return Err("row is stale or outside the review page; read it again".into());
            }
            bytes =
                bytes.saturating_add(serde_json::to_vec(&base).map_err(|e| e.to_string())?.len());
            if bytes > MAX_BYTES {
                return Err("proposal exceeds row/text budget".into());
            }
            bases.push(base);
        }
        rows.push(Rows {
            source,
            source_name: h.descriptor.display_name.clone(),
            handle: h.source,
            batch,
            bases,
            selected: true,
        });
    }
    // Validate every captured buffer after asynchronous loading.
    let plans: Vec<_> = files.iter().filter_map(|f| f.plan.clone()).collect();
    if !plans.is_empty() {
        MultiDocumentEditProposal::new(plans)
            .map_err(|e| format!("{e:?}"))?
            .validate(ws)
            .map_err(|e| format!("{e:?}"))?;
    }
    for source in files
        .iter()
        .map(|f| f.source.clone())
        .collect::<BTreeSet<_>>()
    {
        let h = handle(ws, &source)?;
        h.source
            .preview_text(transaction(
                &files
                    .iter()
                    .filter(|f| f.source == source)
                    .collect::<Vec<_>>(),
            ))
            .await
            .map_err(|e| e.to_string())?;
    }
    if !active(ws, &outer) {
        return Err("session changed".into());
    }
    if Inbox::get(ws).0.peek().len() >= MAX_PENDING {
        return Err("approval inbox is full".into());
    }
    let id = uuid::Uuid::new_v4().to_string();
    let created = moonkale_ext_api::workspace::now_ms();
    Inbox::get(ws).0.with_mut(|items| {
        items.push(Rc::new(Proposal {
            id: id.clone(),
            outer,
            actor,
            created,
            files,
            rows,
            conflict: None,
            row_conflict: None,
            report: String::new(),
            busy: false,
        }))
    });
    ws.set_status(t!(ws, L, "proposal-waiting"));
    Ok(format!("Proposal {id} queued for review. No text changed."))
}

async fn apply(mut ws: Workspace, p: Rc<Proposal>) {
    let Some(p) = Inbox::get(ws)
        .0
        .peek()
        .iter()
        .find(|live| live.id == p.id)
        .cloned()
    else {
        return;
    };
    if p.busy
        || p.conflict.is_some()
        || p.row_conflict.is_some()
        || !(p.files.iter().any(|f| f.selected) || p.rows.iter().any(|r| r.selected))
        || !active(ws, &p.outer)
    {
        return;
    }
    if !Inbox::begin(ws, &p.id) {
        return;
    }
    Inbox::update(ws, &p.id, |p| p.report.clear());
    let result = apply_inner(ws, &p).await;
    Inbox::update(ws, &p.id, |p| {
        p.busy = false;
        if let Err(e) = result {
            p.report = e;
        }
    });
    if active(ws, &p.outer) {
        ws.set_status(t!(ws, L, "proposal-reviewed"));
    }
}
fn row_diff(
    op: &moonkale_core::RowOp,
    base: Option<&moonkale_core::RowSnapshot>,
) -> Vec<(String, String, String)> {
    use moonkale_core::RowChange;
    let before = base.map(|r| r.values.clone()).unwrap_or_default();
    let after = match &op.change {
        RowChange::Insert(values) => values.clone(),
        RowChange::Delete => Default::default(),
        RowChange::Update(values) => {
            let mut after = before.clone();
            after.extend(values.clone());
            after
        }
    };
    before
        .keys()
        .chain(after.keys())
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter_map(|key| {
            (before.get(&key) != after.get(&key)).then(|| {
                let old = before
                    .get(&key)
                    .map(ToString::to_string)
                    .unwrap_or_else(|| "—".into());
                let new = after
                    .get(&key)
                    .map(ToString::to_string)
                    .unwrap_or_else(|| "—".into());
                (key, old, new)
            })
        })
        .collect()
}

async fn row_conflict(ws: Workspace, p: &Proposal, rows: &Rows) -> Result<(), String> {
    let h = handle(ws, &rows.source)?;
    if !std::sync::Arc::ptr_eq(&h.source, &rows.handle) {
        return Err("row source was replaced".into());
    }
    for (index, op) in rows.batch.ops.iter().enumerate() {
        let page = h
            .source
            .read_rows(op.table.clone(), 1000)
            .await
            .map_err(|e| e.to_string())?;
        if !active(ws, &p.outer) {
            return Err("session changed".into());
        }
        let current = page.rows.into_iter().find(|r| r.key == op.key);
        if current.as_ref().map(|r| r.version) != op.expected {
            Inbox::update(ws, &p.id, |p| {
                p.row_conflict = Some(RowConflict {
                    source: rows.source.clone(),
                    op: index,
                    current,
                })
            });
            return Ok(());
        }
    }
    Inbox::update(ws, &p.id, |p| p.row_conflict = None);
    Ok(())
}

async fn apply_inner(mut ws: Workspace, p: &Proposal) -> Result<(), String> {
    let selected: Vec<_> = p.files.iter().filter(|f| f.selected).collect();
    if selected.is_empty() && !p.rows.iter().any(|r| r.selected) {
        return Err("Select at least one file".into());
    }
    // Preview every database before mutating any file. Apply checks again;
    // sources commit in order and never claim a cross-database transaction.
    for rows in p.rows.iter().filter(|r| r.selected) {
        let h = handle(ws, &rows.source)?;
        if !std::sync::Arc::ptr_eq(&h.source, &rows.handle) {
            return Err("row source was replaced; reopen review".into());
        }
        if let Err(e) = h.source.preview_rows(rows.batch.clone()).await {
            if matches!(e, moonkale_core::SourceError::Conflict { .. }) {
                row_conflict(ws, p, rows).await?;
            }
            return Err(e.to_string());
        }
        if !active(ws, &p.outer) {
            return Err("session changed".into());
        }
    }
    // Refuse the whole selected set before changing any buffer if disk or editor drifted.
    for (index, file) in p.files.iter().enumerate().filter(|(_, f)| f.selected) {
        let h = handle(ws, &file.source)?;
        if !std::sync::Arc::ptr_eq(&h.source, &file.handle) {
            return Err("source was replaced; reopen review".into());
        }
        let (text, version) = h
            .source
            .fetch_text(file.node)
            .await
            .map_err(|e| e.to_string())?;
        if !active(ws, &p.outer) {
            return Err("session changed".into());
        }
        let live = ws.document(file.node).map(|d| d.peek().text.clone());
        if version != file.version
            || text != file.original
            || file
                .plan
                .as_ref()
                .is_some_and(|plan| plan.validate(ws).is_err())
            || (file.plan.is_none() && live.as_deref() != Some(file.original.as_str()))
        {
            let current = if version != file.version {
                text
            } else {
                live.unwrap_or(text)
            };
            Inbox::update(ws, &p.id, |p| {
                p.conflict = Some(Conflict {
                    id: uuid::Uuid::new_v4().to_string(),
                    partial: p.files.iter().any(|f| f.phase != FilePhase::Pending),
                    file: index,
                    current,
                    version,
                })
            });
            return Err(if p.files.iter().any(|f| f.phase != FilePhase::Pending) {
                t!(ws, L, "proposal-stale-partial")
            } else {
                t!(ws, L, "proposal-stale")
            });
        }
    }
    // Recheck database-specific constraints for the entire selected subset.
    let sources: BTreeSet<_> = selected.iter().map(|f| f.source.clone()).collect();
    for source in &sources {
        let checked = handle(ws, source)?
            .source
            .preview_text(transaction(
                &selected
                    .iter()
                    .copied()
                    .filter(|f| &f.source == source)
                    .collect::<Vec<_>>(),
            ))
            .await;
        if let Err(error) = checked {
            return Err(
                if matches!(error, moonkale_core::SourceError::Conflict { .. }) {
                    late_conflict(ws, p, Some(source), None, false).await
                } else {
                    error.to_string()
                },
            );
        }
        if !active(ws, &p.outer) {
            return Err("session changed".into());
        }
    }
    let plans: Vec<_> = selected.iter().filter_map(|f| f.plan.clone()).collect();
    let mut report = Vec::new();
    if !plans.is_empty() {
        let count = plans.len();
        let group = MultiDocumentEditProposal::new(plans).map_err(|e| format!("{e:?}"))?;
        let group_id = match group.apply_guarded(ws, || active(ws, &p.outer)).await {
            Ok(id) => id,
            Err(error)
                if error.reason
                    == moonkale_ext_api::workspace::proposals::ProposalConflict::Changed =>
            {
                return Err(late_conflict(ws, p, None, error.node, false).await)
            }
            Err(error) => return Err(format!("{error:?}")),
        };
        Inbox::update(ws, &p.id, |p| {
            for file in p
                .files
                .iter_mut()
                .filter(|f| f.selected && f.plan.is_some())
            {
                file.phase = FilePhase::Buffered;
            }
        });
        for file in selected.iter().filter(|f| f.plan.is_some()) {
            ws.history.pending_actor.with_mut(|actors| {
                actors.insert(file.node, format!("agent:{}", p.actor));
            });
        }
        let saved = grouped_edits::save(ws, group_id).await;
        if saved.len() != count {
            return Err(t!(ws, L, "proposal-save-unavailable"));
        }
        report.extend(saved.iter().map(|r| {
            let path = p
                .files
                .iter()
                .find(|f| f.node == r.node)
                .map(|f| f.path.as_str())
                .unwrap_or("?");
            use grouped_edits::FileSaveStatus::*;
            let status = match r.status {
                Saved => t!(ws, L, "proposal-saved"),
                AlreadySaved => t!(ws, L, "proposal-already-saved"),
                Changed => t!(ws, L, "proposal-changed"),
                Closed => t!(ws, L, "proposal-closed"),
                ReadOnly => t!(ws, L, "proposal-readonly"),
                Conflict => t!(ws, L, "proposal-save-conflict"),
                Failed => t!(ws, L, "proposal-save-failed"),
            };
            format!("{path}: {status}")
        }));
        Inbox::update(ws, &p.id, |p| {
            p.report = report.join("\n");
            for file in &mut p.files {
                if saved.iter().any(|r| {
                    r.node == file.node
                        && matches!(
                            r.status,
                            grouped_edits::FileSaveStatus::Saved
                                | grouped_edits::FileSaveStatus::AlreadySaved
                        )
                }) {
                    file.phase = FilePhase::Saved;
                    file.selected = false;
                }
            }
        });
        if let Some(failed) = saved
            .iter()
            .find(|r| r.status == grouped_edits::FileSaveStatus::Conflict)
        {
            let source = p
                .files
                .iter()
                .find(|f| f.node == failed.node)
                .map(|f| &f.source);
            return Err(late_conflict(ws, p, source, Some(failed.node), true).await);
        }
    }
    Inbox::update(ws, &p.id, |p| p.report = report.join("\n"));
    for source in &sources {
        let files: Vec<_> = selected
            .iter()
            .copied()
            .filter(|f| &f.source == source && f.plan.is_none())
            .collect();
        if files.is_empty() {
            continue;
        }
        if !active(ws, &p.outer) {
            return Err("session changed after earlier source results; review the report".into());
        }
        let h = handle(ws, source)?;
        let applied = match h.source.apply(transaction(&files)).await {
            Ok(applied) => applied,
            Err(moonkale_core::SourceError::Conflict { .. }) => {
                return Err(late_conflict(ws, p, Some(source), None, !report.is_empty()).await)
            }
            Err(error) => return Err(error.to_string()),
        };
        if matches!(
            applied.first_error(),
            Some(moonkale_core::SourceError::Conflict { .. })
        ) {
            return Err(late_conflict(ws, p, Some(source), None, !report.is_empty()).await);
        }
        for file in files {
            if let Some(version) = applied.version_of(file.node) {
                let mut doc = ws.document(file.node).ok_or("file closed after save")?;
                if doc.peek().text == file.original {
                    doc.with_mut(|d| {
                        d.text = file.next.clone();
                        d.saved = file.next.clone();
                        d.version = version;
                    });
                }
                ws.record_event(
                    format!("agent:{}", p.actor),
                    moonkale_core::EventKind::Content {
                        node: file.node,
                        patch: moonkale_core::TextPatch::whole(
                            file.next.clone(),
                            file.original.chars().count(),
                        ),
                        chars_after: file.next.chars().count(),
                        base: Some(file.original.clone()),
                    },
                    Some(file.path.clone()),
                );
                report.push(format!(
                    "{}: {}",
                    file.path,
                    t!(ws, L, "proposal-saved-atomic")
                ));
            } else {
                return Err(format!("{}: {:?}", file.path, applied));
            }
        }
        // Store-backed sources need the same derived-index refresh as an
        // ordinary Workspace::save; their generated files never change.
        Inbox::update(ws, &p.id, |p| {
            for file in p
                .files
                .iter_mut()
                .filter(|f| &f.source == source && f.selected && f.plan.is_none())
            {
                file.phase = FilePhase::Saved;
                file.selected = false;
            }
        });
        ws.refresh_source(source).await;
    }
    for rows in p.rows.iter().filter(|r| r.selected) {
        if !active(ws, &p.outer) {
            return Err("session changed after earlier source results; review the report".into());
        }
        let h = handle(ws, &rows.source)?;
        if !std::sync::Arc::ptr_eq(&h.source, &rows.handle) {
            return Err("row source was replaced".into());
        }
        match h.source.apply_rows(rows.batch.clone()).await {
            Ok(_) => {
                report.push(format!(
                    "{}: {}",
                    rows.source_name,
                    t!(ws, L, "proposal-saved-atomic")
                ));
                Inbox::update(ws, &p.id, |p| {
                    p.rows.retain(|r| r.source != rows.source);
                    p.report = report.join("\n");
                });
            }
            Err(e) => {
                if matches!(e, moonkale_core::SourceError::Conflict { .. }) {
                    row_conflict(ws, p, rows).await?;
                }
                return Err(format!(
                    "{}\n{}: {}",
                    report.join("\n"),
                    rows.source_name,
                    e
                ));
            }
        }
    }
    let report = report.join("\n");
    Inbox::update(ws, &p.id, |p| {
        p.files
            .retain(|f| !f.selected && f.phase != FilePhase::Saved);
        p.conflict = None;
        p.report = report;
    });
    Ok(())
}

#[component]
pub(crate) fn Panel(ws: Workspace) -> Element {
    let inbox = Inbox::get(ws).0;
    let outer = ws.saved_sessions.active.read().clone();
    let proposals: Vec<_> = inbox
        .read()
        .iter()
        .filter(|p| p.outer == outer)
        .cloned()
        .collect();
    let pending: Vec<_> = crate::panel::Chats::get(ws)
        .sessions
        .read()
        .iter()
        .filter_map(|s| s.pending.read().clone())
        .collect();
    let empty = proposals.is_empty() && pending.is_empty();
    rsx! { div { class: "mk-proposals",
        for p in pending { PendingCard { ws, p } }
        if empty { p { class: "mk-muted", {t!(ws,L,"proposal-empty")} } }
        for p in proposals { {
            let id = p.id.clone();
            rsx! { section { key: "{id}", class: "mk-proposal", "data-proposal": "{id}", aria_busy: p.busy,
                p { {t!(ws,L,"proposal-author", actor=p.actor.clone(), at=format_time(p.created))} }
                if !p.report.is_empty() { pre { class: "mk-proposal-report", role: "status", "{p.report}" } }
                for file in &p.files { {
                    let id = id.clone();
                    let node=file.node;
                    rsx! { details { key: "{file.node}", open: true, class: "mk-proposal-file", "data-path": "{file.path}",
                        summary { label { title: "{file.source}",
                            input { r#type: "checkbox", checked: file.selected, disabled: p.busy || file.phase==FilePhase::Saved,
                                onchange: move |e: FormEvent| Inbox::select(ws,&id,node,e.checked()) }
                            span { class: "mk-muted", "{file.source_name} / " }
                            "{file.path}"
                            if file.phase==FilePhase::Saved { span { class: "mk-agent-ok", " · " {t!(ws,L,"proposal-saved")} } }
                        } }
                        div { class: "mk-agent-diff",
                            pre { class: "mk-agent-diff-old", "{file.original}" }
                            pre { class: "mk-agent-diff-new", "{file.next}" }
                        }
                    } }
                } }
                for (index, rows) in p.rows.iter().enumerate() { {
                    let id = id.clone();
                    rsx! { details { key: "rows:{rows.source}", open: true, class: "mk-proposal-rows", "data-source": "{rows.source}",
                        summary { label {
                            input { r#type: "checkbox", checked: rows.selected, disabled: p.busy,
                                onchange: move |e: FormEvent| Inbox::update(ws, &id, |p| { if !p.busy { p.rows[index].selected=e.checked(); } }) }
                            "{rows.source_name}"
                        } }
                        for (op, base) in rows.batch.ops.iter().zip(&rows.bases) {
                            div { class: "mk-proposal-table", "data-table": "{op.table}",
                                p { "{op.table}" }
                                table { class: "mk-proposal-cell-diff",
                                    thead { tr { th { {t!(ws,L,"proposal-row-column")} } th { {t!(ws,L,"proposal-row-before")} } th { {t!(ws,L,"proposal-row-after")} } } }
                                    tbody { for (column,old,new) in row_diff(op,base.as_ref()) { tr { key: "{column}", td { "{column}" } td { pre { "{old}" } } td { pre { "{new}" } } } } }
                                }
                            }
                        }
                    } }
                } }
                if let Some(conflict) = &p.row_conflict { {
                    let source = conflict.source.clone();
                    let index = conflict.op;
                    let group = p.rows.iter().find(|r| r.source == source);
                    let id = id.clone();
                    let keep = id.clone();
                    let discard = id.clone();
                    let reopen = id.clone();
                    let mine = group.map(|r| row_diff(&r.batch.ops[index],r.bases[index].as_ref()).into_iter().map(|(col,_,new)|format!("{col}: {new}")).collect::<Vec<_>>().join("\n")).unwrap_or_default();
                    let base = group.and_then(|r| r.bases[index].as_ref()).map(|r| r.values.iter().map(|(col,value)|format!("{col}: {value}")).collect::<Vec<_>>().join("\n")).unwrap_or_default();
                    let current = conflict.current.clone();
                    let current_text = current.as_ref().map(|r| r.values.iter().map(|(col,value)|format!("{col}: {value}")).collect::<Vec<_>>().join("\n")).unwrap_or_default();
                    let keep_source=source.clone();
                    let discard_source=source.clone();
                    rsx! { div { class: "mk-proposal-row-conflict", role: "alert",
                        p { {t!(ws,L,"proposal-stale")} }
                        p { {t!(ws,L,"proposal-current-unknown")} }
                        div { class: "mk-three-way-sides", pre { "{base}" } pre { "{mine}" } pre { "{current_text}" } }
                        button { class: "mk-proposal-row-mine", disabled:p.busy,
                            onclick: move |_| Inbox::update(ws,&keep,|p| {
                                if p.busy { return; }
                                if let Some(rows)=p.rows.iter_mut().find(|r|r.source==keep_source) {
                                    use moonkale_core::RowChange;
                                    let op=&mut rows.batch.ops[index];
                                    match (&op.change,&current) {
                                        (RowChange::Insert(values),Some(_)) => op.change=RowChange::Update(values.clone()),
                                        (RowChange::Update(values),None) => {
                                            let mut all=rows.bases[index].as_ref().map(|r| { let mut values=r.values.clone(); values.extend(r.stored.clone()); values }).unwrap_or_default();
                                            all.extend(op.key.clone());
                                            all.extend(values.clone()); op.change=RowChange::Insert(all);
                                        }
                                        (RowChange::Delete,None) => { rows.batch.ops.remove(index); rows.bases.remove(index); p.rows.retain(|r|!r.batch.ops.is_empty()); p.row_conflict=None; return; }
                                        _ => {}
                                    }
                                    op.expected=current.as_ref().map(|r|r.version);
                                    rows.bases[index]=current.clone();
                                }
                                p.row_conflict=None;
                            }), {t!(ws,L,"proposal-row-mine")} }
                        button { class: "mk-proposal-row-current", disabled:p.busy,
                            onclick: move |_| Inbox::update(ws,&discard,|p| { if !p.busy {
                                if let Some(rows)=p.rows.iter_mut().find(|r|r.source==discard_source) { rows.batch.ops.remove(index); rows.bases.remove(index); }
                                p.rows.retain(|r|!r.batch.ops.is_empty()); p.row_conflict=None;
                            } }), {t!(ws,L,"proposal-row-current")} }
                        button { class: "mk-proposal-row-reopen", disabled:p.busy,
                            onclick: move |_| { let id=reopen.clone(); let source=source.clone(); moonkale_ext_api::crash::spawn(async move {
                                let p=Inbox::get(ws).0.peek().iter().find(|p|p.id==id).cloned();
                                if let Some(p)=p { if let Some(rows)=p.rows.iter().find(|r|r.source==source) { let _=row_conflict(ws,&p,rows).await; } }
                            }); }, {t!(ws,L,"proposal-reopen")} }
                    } }
                } }
                if let Some(conflict) = &p.conflict { {
                    let file = &p.files[conflict.file];
                    let id = id.clone();
                    let conflict = conflict.clone();
                    let current = conflict.current.clone();
                    let proposed = file.next.clone();
                    let base = file.original.clone();
                    let refresh_id=id.clone();
                    let resolve_id=id.clone();
                    let stale=if conflict.partial {t!(ws,L,"proposal-stale-partial")}else{t!(ws,L,"proposal-stale")};
                    rsx! { div { class: "mk-proposal-conflict", role: "alert",
                        p { "{stale}" }
                        p { {t!(ws,L,"proposal-current-unknown")} }
                        button { class: "mk-btn mk-proposal-reopen", disabled:p.busy,
                            onclick: { let id=refresh_id.clone(); move |_| {let id=id.clone(); moonkale_ext_api::crash::spawn_forever(async move {
                                if let Err(e)=refresh_conflict(ws,&id).await {Inbox::update(ws,&id,|p|p.report=e);}
                            });} }, {t!(ws,L,"proposal-reopen")} }
                        moonkale_ext_api::conflict::ThreeWayEditor { key: "{id}:{conflict.id}", ws, base, ours: current.clone(), theirs: proposed, original: current, hunks: String::new(), whole_file: true, busy: p.busy,
                            onresolve: move |result: String| {
                                let id=resolve_id.clone(); let conflict=conflict.clone();
                                if !Inbox::begin(ws,&id){return;}
                                moonkale_ext_api::crash::spawn_forever(async move {
                                    if let Err(e)=reopen(ws,&id,conflict,result).await {
                                        Inbox::update(ws,&id,|p| p.report=e);
                                    }
                                    Inbox::update(ws,&id,|p|p.busy=false);
                                });
                            }
                        }
                    } }
                } }
                if !p.files.is_empty() || !p.rows.is_empty() {
                    button { class: "mk-btn mk-btn-on mk-proposal-approve", disabled: p.busy || p.conflict.is_some() || p.row_conflict.is_some(),
                        onclick: { let p=p.clone(); move |_| { let p=p.clone(); moonkale_ext_api::crash::spawn_forever(apply(ws,p)); } }, {t!(ws,L,"proposal-approve")} }
                    button { class: "mk-btn mk-proposal-reject", disabled: p.busy,
                        onclick: { let id=id.clone(); move |_| Inbox::reject(ws,&id) }, {t!(ws,L,"proposal-reject")} }
                }
                button { class: "mk-btn", disabled: p.busy, onclick: move |_| Inbox::remove(ws,&id), {t!(ws,L,"proposal-dismiss")} }
            } }
        } }
    } }
}

async fn reopen(
    mut ws: Workspace,
    id: &str,
    conflict: Conflict,
    result: String,
) -> Result<(), String> {
    let p = Inbox::get(ws)
        .0
        .peek()
        .iter()
        .find(|p| p.id == id)
        .cloned()
        .ok_or("proposal closed")?;
    if !active(ws, &p.outer) {
        return Err("session changed".into());
    }
    if p.conflict
        .as_ref()
        .is_none_or(|live| live.id != conflict.id)
    {
        return Err("conflict review was reopened".into());
    }
    let file = &p.files[conflict.file];
    let h = handle(ws, &file.source)?;
    if !std::sync::Arc::ptr_eq(&h.source, &file.handle) {
        return Err("source was replaced; reopen proposal".into());
    }
    let (text, version) = h
        .source
        .fetch_text(file.node)
        .await
        .map_err(|e| e.to_string())?;
    if !active(ws, &p.outer) || version != conflict.version {
        return Err("file changed again; reopen review".into());
    }
    // Loading an external version must not silently discard a person's dirty buffer.
    let doc = ws.document(file.node).ok_or("file closed")?;
    let own_buffer = file.phase == FilePhase::Buffered && doc.peek().text == file.next;
    if doc.peek().dirty() && doc.peek().text != conflict.current && !own_buffer {
        return Err("buffer changed again".into());
    }
    let changed_version = doc.peek().version != version;
    if changed_version {
        if doc.peek().dirty() && !own_buffer {
            return Err(
                "Save or discard the local buffer before reopening the external version".into(),
            );
        }
        let mut doc = doc;
        let node = doc.peek().node.clone();
        doc.set(moonkale_ext_api::document::Document::new(
            node,
            text.clone(),
            version,
        ));
    }
    if own_buffer && changed_version {
        ws.history.pending_actor.with_mut(|actors| {
            actors.remove(&file.node);
        });
    }
    let original = ws
        .document(file.node)
        .ok_or("file closed")?
        .peek()
        .text
        .clone();
    // Explicit conflict reopening reconciles a headless editor lease too;
    // ordinary structural proposals must never reconcile a stale mirror silently.
    let mut session = ws.editor_session(file.node).ok_or("file closed")?;
    session
        .write()
        .replace_from_workspace(original.clone())
        .map_err(|e| format!("{e:?}"))?;
    let plan = if h.descriptor.truth == SourceTruth::Files {
        Some(
            ws.document_edit_base(file.node)
                .map_err(|e| format!("{e:?}"))?
                .propose(vec![TextChange {
                    range: 0..original.len(),
                    removed_text: original,
                    inserted_text: result.clone(),
                }])
                .map_err(|e| format!("{e:?}"))?,
        )
    } else {
        None
    };
    Inbox::update(ws, id, |p| {
        let f = &mut p.files[conflict.file];
        f.plan = plan;
        f.phase = FilePhase::Pending;
        f.next = result;
        f.original = text;
        f.version = version;
        p.conflict = None;
        p.report.clear();
    });
    Ok(())
}

fn transaction(files: &[&File]) -> moonkale_core::Transaction {
    moonkale_core::Transaction {
        ops: files
            .iter()
            .map(|f| moonkale_core::Op::WriteText {
                node: f.node,
                expected: f.version,
                patch: moonkale_core::TextPatch::whole(f.next.clone(), f.original.chars().count()),
            })
            .collect(),
    }
}

#[component]
fn PendingCard(ws: Workspace, p: crate::host::PendingApproval) -> Element {
    rsx! {
    div { class: "mk-agent-approval",
                            div { class: "mk-agent-approval-title",
                                {t!(ws, L, "agent-wants")}
                                code { "{p.call.name}" }
                                " ({p.class:?})"
                            }
                            if p.call.name == "file.create" {
                                code { "{p.call.str(\"source\").unwrap_or_default()} / {p.call.str(\"path\").unwrap_or_default()}" }
                                div { class: "mk-agent-diff",
                                    pre { class: "mk-agent-diff-old", "" }
                                    pre { class: "mk-agent-diff-new", "{p.call.str(\"text\").unwrap_or_default()}" }
                                }
                            } else if p.call.name == "terminal.run" {
                                pre { class: "mk-agent-diff-cmd", "$ {p.call.str(\"command\").unwrap_or_default()}" }
                            } else {
                                code { class: "mk-agent-tool-input", "{p.call.input}" }
                            }
                            div { class: "mk-agent-approval-actions",
                                button { class: "mk-btn mk-btn-on", onclick: moonkale_ext_api::crash::event({ let p = p.clone(); move |_| { if let Some(tx) = p.reply.borrow_mut().take() { let _ = tx.send(true); } } }), {t!(ws, L, "agent-allow")} }
                                button { class: "mk-btn", onclick: moonkale_ext_api::crash::event({ let p = p.clone(); move |_| { if let Some(tx) = p.reply.borrow_mut().take() { let _ = tx.send(false); } } }), {t!(ws, L, "agent-deny")} }
                            }
                        }
        }
}

fn format_time(at: u64) -> String {
    i64::try_from(at)
        .ok()
        .and_then(chrono::DateTime::from_timestamp_millis)
        .map(|date| date.format("%Y-%m-%d %H:%M:%S UTC").to_string())
        .unwrap_or_default()
}

async fn refresh_conflict(ws: Workspace, id: &str) -> Result<(), String> {
    let p = Inbox::get(ws)
        .0
        .peek()
        .iter()
        .find(|p| p.id == id)
        .cloned()
        .ok_or("proposal closed")?;
    if !active(ws, &p.outer) || p.busy {
        return Err("review is unavailable".into());
    }
    let conflict = p.conflict.as_ref().ok_or("no conflict to reopen")?;
    let file = &p.files[conflict.file];
    let h = handle(ws, &file.source)?;
    if !std::sync::Arc::ptr_eq(&h.source, &file.handle) {
        return Err("source was replaced; reopen proposal".into());
    }
    let (text, version) = h
        .source
        .fetch_text(file.node)
        .await
        .map_err(|e| e.to_string())?;
    if !active(ws, &p.outer) {
        return Err("session changed".into());
    }
    let current = ws
        .document(file.node)
        .filter(|d| d.peek().dirty() && version == file.version)
        .map(|d| d.peek().text.clone())
        .unwrap_or(text);
    Inbox::update(ws, id, |p| {
        if p.conflict
            .as_ref()
            .is_some_and(|live| live.id == conflict.id)
        {
            p.conflict = Some(Conflict {
                id: uuid::Uuid::new_v4().to_string(),
                partial: conflict.partial,
                file: conflict.file,
                current,
                version,
            });
            p.report.clear();
        }
    });
    Ok(())
}

async fn late_conflict(
    ws: Workspace,
    p: &Proposal,
    source: Option<&SourceId>,
    node: Option<NodeId>,
    partial: bool,
) -> String {
    let partial = partial || p.files.iter().any(|f| f.phase != FilePhase::Pending);
    let mut first = None;
    let mut changed = None;
    for (index, file) in p.files.iter().enumerate().filter(|(_, f)| {
        f.selected && source.is_none_or(|id| id == &f.source) && node.is_none_or(|id| id == f.node)
    }) {
        if !active(ws, &p.outer) {
            return "session changed".into();
        }
        let h = match handle(ws, &file.source) {
            Ok(h) => h,
            Err(e) => return e,
        };
        if !std::sync::Arc::ptr_eq(&h.source, &file.handle) {
            return "source replaced".into();
        }
        let (text, version) = match h.source.fetch_text(file.node).await {
            Ok(pair) => pair,
            Err(e) => return e.to_string(),
        };
        let live = ws.document(file.node).map(|d| d.peek().text.clone());
        let drift = version != file.version
            || text != file.original
            || live.as_deref() != Some(file.original.as_str())
            || file
                .plan
                .as_ref()
                .is_some_and(|plan| plan.validate(ws).is_err());
        let current = if version != file.version {
            text
        } else {
            live.unwrap_or(text)
        };
        let candidate = Conflict {
            id: uuid::Uuid::new_v4().to_string(),
            partial,
            file: index,
            current,
            version,
        };
        if drift {
            changed = Some(candidate);
            break;
        }
        if first.is_none() {
            first = Some(candidate);
        }
    }
    if !active(ws, &p.outer) {
        return "session changed".into();
    }
    if let Some(conflict) = changed.or(first) {
        Inbox::update(ws, &p.id, |p| p.conflict = Some(conflict));
    }
    if partial {
        t!(ws, L, "proposal-stale-partial")
    } else {
        t!(ws, L, "proposal-stale")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::FutureExt;
    use moonkale_core::{
        Applied, Node, NodeKind, Op, OpResult, Source, SourceDescriptor, SourceError, SourceFamily,
        Transaction,
    };
    use moonkale_ext_api::{
        AttachFuture, FolderAccess, OpenFolderFuture, OpenOptions, WorkspaceConfig,
    };
    use std::{
        collections::BTreeMap,
        sync::{Arc, Mutex},
    };
    #[derive(Clone)]
    struct Memory(Arc<Mutex<BTreeMap<NodeId, (String, Version)>>>);
    fn sid() -> SourceId {
        SourceId::new("proposal-tests")
    }
    fn node(path: &str) -> Node {
        Node {
            id: NodeId::derive(&sid(), path),
            source: sid(),
            kind: NodeKind::File,
            label: path.into(),
            native_key: path.into(),
            props: Default::default(),
            content: None,
            version: Version(0),
        }
    }
    #[moonkale_core::async_trait]
    impl Source for Memory {
        fn id(&self) -> SourceId {
            sid()
        }
        fn descriptor(&self) -> SourceDescriptor {
            SourceDescriptor {
                id: sid(),
                root: NodeId::derive(&sid(), ""),
                truth: SourceTruth::Files,
                display_name: "tests".into(),
                family: SourceFamily::Folder,
                capabilities: moonkale_core::Capabilities {
                    write: true,
                    read: true,
                    ..Default::default()
                },
            }
        }
        async fn query(&self, q: Query) -> Result<moonkale_core::QueryResult, SourceError> {
            Ok(moonkale_core::QueryResult {
                nodes: ["a", "b"]
                    .into_iter()
                    .map(node)
                    .filter(|n| matches!(q,Query::Node(id) if id==n.id))
                    .collect(),
                ..Default::default()
            })
        }
        async fn fetch_text(&self, id: NodeId) -> Result<(String, Version), SourceError> {
            self.0
                .lock()
                .unwrap()
                .get(&id)
                .cloned()
                .ok_or(SourceError::NotFound)
        }
        async fn apply(&self, tx: Transaction) -> Result<Applied, SourceError> {
            let mut values = self.0.lock().unwrap();
            let mut results = Vec::new();
            for op in tx.ops {
                let Op::WriteText {
                    node,
                    expected,
                    patch,
                } = op
                else {
                    return Err(SourceError::Unsupported("test".into()));
                };
                let (text, actual) = values.get(&node).unwrap().clone();
                if actual != expected {
                    return Err(SourceError::Conflict { expected, actual });
                }
                let version = Version(actual.0 + 1);
                values.insert(node, (patch.apply(&text)?, version));
                results.push(OpResult::Ok {
                    node,
                    version,
                    warnings: Vec::new(),
                });
            }
            Ok(Applied { results })
        }
    }
    fn open(_: String, _: OpenOptions) -> OpenFolderFuture {
        Box::pin(async { Err(SourceError::NotFound) })
    }
    fn attach(_: SourceDescriptor) -> AttachFuture {
        Box::pin(async { Err(SourceError::NotFound) })
    }
    thread_local! {static SETUP: std::cell::RefCell<Option<(Workspace,Memory)>>=const{std::cell::RefCell::new(None)};}
    #[component]
    fn App() -> Element {
        use_hook(|| {
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
            let source = Memory(Arc::new(Mutex::new(
                [("a", "A\n"), ("b", "B\n")]
                    .into_iter()
                    .map(|(path, text)| (node(path).id, (text.into(), Version(0))))
                    .collect(),
            )));
            ws.add_source(Arc::new(source.clone()));
            SETUP.with(|s| *s.borrow_mut() = Some((ws, source)));
        });
        rsx! {}
    }
    fn setup() -> (VirtualDom, Workspace, Memory) {
        let mut dom = VirtualDom::new(App);
        dom.rebuild_in_place();
        let (ws, source) = SETUP.with(|s| s.borrow_mut().take().unwrap());
        (dom, ws, source)
    }
    fn call() -> ToolCall {
        ToolCall {
            id: "test".into(),
            name: "editor.propose".into(),
            input: serde_json::json!({"files":[{"source":sid().as_str(),"node":node("a").id.to_string(),"expected":"0","text":"AA\n"},{"source":sid().as_str(),"node":node("b").id.to_string(),"expected":"0","text":"BB\n"}]}),
        }
    }
    fn queued(ws: Workspace) -> Rc<Proposal> {
        submit(
            ws,
            ws.saved_sessions.active.peek().clone(),
            "test-agent".into(),
            call(),
        )
        .now_or_never()
        .unwrap()
        .unwrap();
        Inbox::get(ws).0.peek().last().unwrap().clone()
    }
    #[test]
    fn submission_is_nonmutating_and_partial_approval_preserves_unselected_file() {
        let (dom, ws, source) = setup();
        dom.in_scope(ScopeId::ROOT, || {
            let p = queued(ws);
            assert_eq!(ws.document(node("a").id).unwrap().peek().text, "A\n");
            assert_eq!(source.0.lock().unwrap()[&node("a").id].0, "A\n");
            Inbox::select(ws, &p.id, node("b").id, false);
            let p = Inbox::get(ws).0.peek()[0].clone();
            apply_inner(ws, &p).now_or_never().unwrap().unwrap();
            assert_eq!(source.0.lock().unwrap()[&node("a").id].0, "AA\n");
            assert_eq!(source.0.lock().unwrap()[&node("b").id].0, "B\n");
        });
    }
    #[test]
    fn stale_second_source_or_buffer_refuses_the_whole_selected_set() {
        for disk in [true, false] {
            let (dom, ws, source) = setup();
            dom.in_scope(ScopeId::ROOT, || {
                let p = queued(ws);
                if disk {
                    source
                        .0
                        .lock()
                        .unwrap()
                        .insert(node("b").id, ("other\n".into(), Version(1)));
                } else {
                    ws.document(node("b").id).unwrap().write().text = "unsaved person\n".into();
                }
                assert!(apply_inner(ws, &p).now_or_never().unwrap().is_err());
                assert_eq!(ws.document(node("a").id).unwrap().peek().text, "A\n");
                assert_eq!(source.0.lock().unwrap()[&node("a").id].0, "A\n");
                assert!(Inbox::get(ws).0.peek()[0].conflict.is_some());
            });
        }
    }
    #[test]
    fn fresh_read_after_clean_headless_refresh_and_reopened_conflict_require_approval() {
        let (dom, ws, source) = setup();
        dom.in_scope(ScopeId::ROOT, || {
            let old = queued(ws);
            source
                .0
                .lock()
                .unwrap()
                .insert(node("b").id, ("external\n".into(), Version(1)));
            let mut doc = ws.document(node("b").id).unwrap();
            doc.with_mut(|d| {
                d.text = "external\n".into();
                d.saved = d.text.clone();
                d.version = Version(1);
            });
            let mut fresh = call();
            fresh.input["files"][1]["expected"] = serde_json::json!("1");
            submit(
                ws,
                ws.saved_sessions.active.peek().clone(),
                "test".into(),
                fresh,
            )
            .now_or_never()
            .unwrap()
            .unwrap();
            assert!(apply_inner(ws, &old).now_or_never().unwrap().is_err());
            let conflict = Inbox::get(ws).0.peek()[0].conflict.clone().unwrap();
            reopen(ws, &old.id, conflict, "resolved\n".into())
                .now_or_never()
                .unwrap()
                .unwrap();
            assert_eq!(source.0.lock().unwrap()[&node("b").id].0, "external\n");
            let reviewed = Inbox::get(ws).0.peek()[0].clone();
            apply_inner(ws, &reviewed).now_or_never().unwrap().unwrap();
            assert_eq!(source.0.lock().unwrap()[&node("a").id].0, "AA\n");
            assert_eq!(source.0.lock().unwrap()[&node("b").id].0, "resolved\n");
        });
    }
    // Model an external commit that lands after preview, at the source's
    // final optimistic-write barrier. None of the proposed writes committed.
    struct CommitRace(Memory);
    #[moonkale_core::async_trait]
    impl Source for CommitRace {
        fn id(&self) -> SourceId {
            self.0.id()
        }
        fn descriptor(&self) -> SourceDescriptor {
            let mut d = self.0.descriptor();
            d.truth = SourceTruth::Store;
            d.capabilities.atomic_text = true;
            d
        }
        async fn query(&self, q: Query) -> Result<moonkale_core::QueryResult, SourceError> {
            self.0.query(q).await
        }
        async fn fetch_text(&self, id: NodeId) -> Result<(String, Version), SourceError> {
            self.0.fetch_text(id).await
        }
        async fn apply(&self, _: Transaction) -> Result<Applied, SourceError> {
            self.0
                 .0
                .lock()
                .unwrap()
                .insert(node("b").id, ("external commit\n".into(), Version(1)));
            Err(SourceError::Conflict {
                expected: Version(0),
                actual: Version(1),
            })
        }
    }
    #[test]
    fn conflict_at_atomic_commit_opens_review_without_touching_proposed_buffers() {
        let (dom, mut ws, source) = setup();
        dom.in_scope(ScopeId::ROOT, || {
            ws.add_source(Arc::new(CommitRace(source.clone())));
            let p = queued(ws);
            assert!(apply_inner(ws, &p).now_or_never().unwrap().is_err());
            assert_eq!(source.0.lock().unwrap()[&node("a").id].0, "A\n");
            assert_eq!(ws.document(node("a").id).unwrap().peek().text, "A\n");
            assert_eq!(ws.document(node("b").id).unwrap().peek().text, "B\n");
            let conflict = Inbox::get(ws).0.peek()[0].conflict.clone().unwrap();
            assert_eq!(conflict.file, 1);
            assert_eq!(conflict.current, "external commit\n");
            assert!(!conflict.partial);
        });
    }
    #[test]
    fn edit_during_view_preparation_opens_the_same_conflict_review() {
        let (dom, ws, source) = setup();
        dom.in_scope(ScopeId::ROOT, || {
            let p = queued(ws);
            let id = p.id.clone();
            grouped_edits::register_view(
                ws,
                node("b").id,
                "racing",
                Some(Rc::new(move |request| {
                    let id = id.clone();
                    Box::pin(async move {
                        if request.phase == "prepare" {
                            Inbox::select(ws, &id, node("a").id, false);
                            Inbox::reject(ws, &id);
                            Inbox::remove(ws, &id);
                            ws.document(node("b").id).unwrap().write().text =
                                "person typed\n".into();
                        }
                        true
                    })
                })),
            );
            apply(ws, p).now_or_never().unwrap();
            assert_eq!(Inbox::get(ws).0.peek()[0].files.len(), 2);
            assert!(Inbox::get(ws).0.peek()[0].files[0].selected);
            assert_eq!(ws.document(node("a").id).unwrap().peek().text, "A\n");
            assert_eq!(source.0.lock().unwrap()[&node("a").id].0, "A\n");
            assert_eq!(source.0.lock().unwrap()[&node("b").id].0, "B\n");
            assert_eq!(
                Inbox::get(ws).0.peek()[0]
                    .conflict
                    .as_ref()
                    .unwrap()
                    .current,
                "person typed\n"
            );
        });
    }
    #[test]
    fn lost_commit_acknowledgement_preserves_unsaved_buffers_and_review() {
        let (dom, ws, source) = setup();
        dom.in_scope(ScopeId::ROOT, || {
            let p = queued(ws);
            grouped_edits::register_view(
                ws,
                node("b").id,
                "lost-ack",
                Some(Rc::new(|request| {
                    Box::pin(async move { request.phase != "commit" })
                })),
            );
            let error = apply_inner(ws, &p).now_or_never().unwrap().unwrap_err();
            assert!(error.contains("buffers were updated"), "{error}");
            assert_eq!(source.0.lock().unwrap()[&node("a").id].0, "A\n");
            assert_eq!(source.0.lock().unwrap()[&node("b").id].0, "B\n");
            for (path, text) in [("a", "AA\n"), ("b", "BB\n")] {
                let doc = ws.document(node(path).id).unwrap();
                assert_eq!(doc.peek().text, text);
                assert!(doc.peek().dirty());
            }
            assert_eq!(Inbox::get(ws).0.peek()[0].files.len(), 2);
        });
    }

    struct FileSaveRace {
        memory: Memory,
        once: std::sync::atomic::AtomicBool,
    }
    #[moonkale_core::async_trait]
    impl Source for FileSaveRace {
        fn id(&self) -> SourceId {
            self.memory.id()
        }
        fn descriptor(&self) -> SourceDescriptor {
            self.memory.descriptor()
        }
        async fn query(&self, q: Query) -> Result<moonkale_core::QueryResult, SourceError> {
            self.memory.query(q).await
        }
        async fn fetch_text(&self, id: NodeId) -> Result<(String, Version), SourceError> {
            self.memory.fetch_text(id).await
        }
        async fn apply(&self, tx: Transaction) -> Result<Applied, SourceError> {
            if matches!(tx.ops.first(),Some(Op::WriteText{node:id,..}) if *id==node("b").id)
                && self.once.swap(false, std::sync::atomic::Ordering::SeqCst)
            {
                self.memory
                    .0
                    .lock()
                    .unwrap()
                    .insert(node("b").id, ("external save\n".into(), Version(1)));
                return Err(SourceError::Conflict {
                    expected: Version(0),
                    actual: Version(1),
                });
            }
            self.memory.apply(tx).await
        }
    }
    #[test]
    fn ordered_save_conflict_reviews_failed_file_and_keeps_successful_file_finished() {
        let (dom, mut ws, source) = setup();
        dom.in_scope(ScopeId::ROOT, || {
            ws.add_source(Arc::new(FileSaveRace {
                memory: source.clone(),
                once: std::sync::atomic::AtomicBool::new(true),
            }));
            let p = queued(ws);
            assert!(apply_inner(ws, &p).now_or_never().unwrap().is_err());
            assert_eq!(source.0.lock().unwrap()[&node("a").id].0, "AA\n");
            assert_eq!(source.0.lock().unwrap()[&node("b").id].0, "external save\n");
            let partial = Inbox::get(ws).0.peek()[0].clone();
            assert!(partial.files[0].phase == FilePhase::Saved);
            assert!(!partial.files[0].selected);
            let conflict = partial.conflict.clone().unwrap();
            assert_eq!(conflict.file, 1);
            assert!(conflict.partial);
            assert_eq!(conflict.current, "external save\n");
            assert!(partial.report.contains("Saved"));
            reopen(ws, &p.id, conflict, "BB\n".into())
                .now_or_never()
                .unwrap()
                .unwrap();
            let reviewed = Inbox::get(ws).0.peek()[0].clone();
            apply_inner(ws, &reviewed).now_or_never().unwrap().unwrap();
            assert_eq!(source.0.lock().unwrap()[&node("a").id].0, "AA\n");
            assert_eq!(source.0.lock().unwrap()[&node("b").id].0, "BB\n");
            assert!(Inbox::get(ws).0.peek()[0].files.is_empty());
        });
    }

    #[derive(Clone)]
    struct RowMemory {
        row: Arc<Mutex<moonkale_core::RowSnapshot>>,
        race: Arc<std::sync::atomic::AtomicBool>,
    }
    impl RowMemory {
        fn new() -> Self {
            Self {
                row: Arc::new(Mutex::new(moonkale_core::RowSnapshot {
                    key: BTreeMap::from([("id".into(), moonkale_core::Value::Int(1))]),
                    values: BTreeMap::from([
                        ("id".into(), moonkale_core::Value::Int(1)),
                        ("name".into(), moonkale_core::Value::Text("original".into())),
                    ]),
                    stored: Default::default(),
                    version: Version(1),
                })),
                race: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            }
        }
        fn batch(&self) -> moonkale_core::RowBatch {
            moonkale_core::RowBatch {
                ops: vec![moonkale_core::RowOp {
                    table: "items".into(),
                    key: self.row.lock().unwrap().key.clone(),
                    expected: Some(Version(1)),
                    change: moonkale_core::RowChange::Update(BTreeMap::from([(
                        "name".into(),
                        moonkale_core::Value::Text("proposed".into()),
                    )])),
                }],
            }
        }
    }
    #[moonkale_core::async_trait]
    impl Source for RowMemory {
        fn id(&self) -> SourceId {
            SourceId::new("row-proposal-tests")
        }
        fn descriptor(&self) -> SourceDescriptor {
            SourceDescriptor {
                id: self.id(),
                root: NodeId::derive(&self.id(), ""),
                display_name: "SQL fixture".into(),
                family: SourceFamily::Sql,
                truth: SourceTruth::Files,
                capabilities: moonkale_core::Capabilities {
                    read: true,
                    write: true,
                    atomic_rows: true,
                    ..Default::default()
                },
            }
        }
        async fn query(&self, _: Query) -> Result<moonkale_core::QueryResult, SourceError> {
            Err(SourceError::Unsupported("fixture".into()))
        }
        async fn fetch_text(&self, _: NodeId) -> Result<(String, Version), SourceError> {
            Err(SourceError::Unsupported("fixture".into()))
        }
        async fn apply(&self, _: Transaction) -> Result<Applied, SourceError> {
            Err(SourceError::Unsupported("fixture".into()))
        }
        async fn read_rows(
            &self,
            _: String,
            _: usize,
        ) -> Result<moonkale_core::RowPage, SourceError> {
            Ok(moonkale_core::RowPage {
                columns: vec!["id".into(), "name".into()],
                rows: vec![self.row.lock().unwrap().clone()],
                ..Default::default()
            })
        }
        async fn preview_rows(&self, batch: moonkale_core::RowBatch) -> Result<(), SourceError> {
            batch.validate()?;
            let actual = self.row.lock().unwrap().version;
            if let Some(op) = batch.ops.iter().find(|op| op.expected != Some(actual)) {
                return Err(SourceError::Conflict {
                    expected: op.expected.unwrap_or_default(),
                    actual,
                });
            }
            Ok(())
        }
        async fn apply_rows(
            &self,
            batch: moonkale_core::RowBatch,
        ) -> Result<Vec<Option<moonkale_core::RowSnapshot>>, SourceError> {
            if self.race.swap(false, std::sync::atomic::Ordering::SeqCst) {
                self.row.lock().unwrap().version = Version(2);
            }
            self.preview_rows(batch.clone()).await?;
            let mut row = self.row.lock().unwrap();
            let mut result = Vec::new();
            for op in batch.ops {
                if let moonkale_core::RowChange::Update(values) = op.change {
                    row.values.extend(values);
                    row.version.0 += 1;
                    result.push(Some(row.clone()));
                }
            }
            Ok(result)
        }
    }
    fn mixed(ws: Workspace, rows: &RowMemory) -> Rc<Proposal> {
        let mut call = call();
        call.input["rows"] = serde_json::json!([{"source":rows.id(),"batch":rows.batch()}]);
        submit(
            ws,
            ws.saved_sessions.active.peek().clone(),
            "agent".into(),
            call,
        )
        .now_or_never()
        .unwrap()
        .unwrap();
        Inbox::get(ws).0.peek().last().unwrap().clone()
    }
    #[test]
    fn two_files_and_sql_are_nonmutating_until_one_review() {
        let (dom, mut ws, files) = setup();
        dom.in_scope(ScopeId::ROOT, || {
            let rows = RowMemory::new();
            ws.add_source(Arc::new(rows.clone()));
            let p = mixed(ws, &rows);
            assert_eq!(rows.row.lock().unwrap().version, Version(1));
            assert_eq!(files.0.lock().unwrap()[&node("a").id].0, "A\n");
            assert_eq!(
                row_diff(&p.rows[0].batch.ops[0], p.rows[0].bases[0].as_ref()),
                vec![("name".into(), "original".into(), "proposed".into())]
            );
            apply_inner(ws, &p).now_or_never().unwrap().unwrap();
            assert_eq!(
                rows.row.lock().unwrap().values["name"],
                moonkale_core::Value::Text("proposed".into())
            );
            assert_eq!(files.0.lock().unwrap()[&node("a").id].0, "AA\n");
            assert_eq!(files.0.lock().unwrap()[&node("b").id].0, "BB\n");
            assert!(Inbox::get(ws).0.peek()[0].rows.is_empty());
        });
    }
    #[test]
    fn stale_sql_preflight_changes_no_file_and_opens_row_comparison() {
        let (dom, mut ws, files) = setup();
        dom.in_scope(ScopeId::ROOT, || {
            let rows = RowMemory::new();
            ws.add_source(Arc::new(rows.clone()));
            let p = mixed(ws, &rows);
            rows.row.lock().unwrap().version = Version(2);
            assert!(apply_inner(ws, &p).now_or_never().unwrap().is_err());
            assert_eq!(files.0.lock().unwrap()[&node("a").id].0, "A\n");
            assert_eq!(ws.document(node("b").id).unwrap().peek().text, "B\n");
            assert!(Inbox::get(ws).0.peek()[0].row_conflict.is_some());
        });
    }
    #[test]
    fn late_sql_conflict_retains_ordered_file_successes_for_second_review() {
        let (dom, mut ws, files) = setup();
        dom.in_scope(ScopeId::ROOT, || {
            let rows = RowMemory::new();
            ws.add_source(Arc::new(rows.clone()));
            let p = mixed(ws, &rows);
            rows.race.store(true, std::sync::atomic::Ordering::SeqCst);
            assert!(apply_inner(ws, &p).now_or_never().unwrap().is_err());
            assert_eq!(files.0.lock().unwrap()[&node("a").id].0, "AA\n");
            assert_eq!(
                rows.row.lock().unwrap().values["name"],
                moonkale_core::Value::Text("original".into())
            );
            let partial = Inbox::get(ws).0.peek()[0].clone();
            assert!(partial.row_conflict.is_some());
            assert!(partial
                .files
                .iter()
                .all(|f| f.phase == FilePhase::Saved && !f.selected));
            Inbox::update(ws, &p.id, |p| {
                p.rows[0].batch.ops[0].expected = Some(Version(2));
                p.rows[0].bases[0] = Some(rows.row.lock().unwrap().clone());
                p.row_conflict = None;
            });
            let retry = Inbox::get(ws).0.peek()[0].clone();
            apply_inner(ws, &retry).now_or_never().unwrap().unwrap();
            assert_eq!(
                rows.row.lock().unwrap().values["name"],
                moonkale_core::Value::Text("proposed".into())
            );
        });
    }

    #[test]
    fn missing_version_and_switched_session_never_queue() {
        let (dom, ws, _) = setup();
        dom.in_scope(ScopeId::ROOT, || {
            let mut bad = call();
            bad.input["files"][0]
                .as_object_mut()
                .unwrap()
                .remove("expected");
            assert!(submit(
                ws,
                ws.saved_sessions.active.peek().clone(),
                "test".into(),
                bad
            )
            .now_or_never()
            .unwrap()
            .is_err());
            assert!(submit(ws, "another-session".into(), "test".into(), call())
                .now_or_never()
                .unwrap()
                .is_err());
            assert!(Inbox::get(ws).0.peek().is_empty());
        });
    }
}
