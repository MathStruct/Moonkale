//! Reference-aware rename proposals shared with the graph editor.
//! Complete same-folder plans are prepared twice; no LSP edit is applied here.
use crate::{lsp::LspManager, native_workspace_edit::edit_batch_bounded, uri};
use dioxus::prelude::{ReadableExt, Signal};
use futures_util::future::{select, Either};
use moonkale_core::Node;
use moonkale_ext_api::{
    workspace::proposals::{DocumentEditBase, DocumentEditProposal, ProposalConflict},
    Document, Workspace,
};
use moonkale_lsp::WorkspaceEdit;
use std::time::Duration;

/// Recoverable categories suitable for localized presentation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenameError {
    /// No supported folder/server connection.
    Unavailable,
    /// A document or server connection changed during planning.
    Stale,
    /// The complete plan requires another file; nothing is staged.
    MultiFile,
    /// The server refused rename or returned malformed/empty edits.
    Invalid,
    /// The bounded request did not finish.
    Timeout,
    /// The edit count or payload exceeded the proposal budget.
    Limit,
    /// The authoritative response introduced an unprepared target. Recompute.
    TargetsChanged,
}

struct OpenBase {
    id: moonkale_core::NodeId,
    document: Signal<Document>,
    text: String,
    session: Option<(
        Signal<moonkale_ext_api::editor::RevisionedDocument>,
        moonkale_ext_api::editor::DocumentRevision,
    )>,
}

/// Resolver inputs retained with a preview, rechecked again before Apply.
/// This supplements the proposal's origin guard; it is not a disk transaction.
pub struct RenameGuard {
    documents: Vec<OpenBase>,
    manager: LspManager,
    session: moonkale_lsp::LspSession,
    uri: String,
    version: i32,
}
impl RenameGuard {
    /// Refuse a preview if its open buffers or server connection/version drifted.
    pub fn validate(&self, ws: Workspace) -> Result<(), RenameError> {
        if self.documents.len() != ws.docs.open.read().len()
            || self.documents.iter().any(|entry| {
                ws.document(entry.id) != Some(entry.document)
                    || entry.document.read().text != entry.text
                    || ws
                        .editor_session(entry.id)
                        .map(|session| (session, session.read().snapshot().revision))
                        != entry.session
            })
            || !self
                .manager
                .peek_document_session(&self.uri)
                .is_some_and(|live| live.same_connection(&self.session))
            || self.manager.peek_document_version(&self.uri) != Some(self.version)
        {
            return Err(RenameError::Stale);
        }
        Ok(())
    }
}

fn single_file_changes(
    base: &moonkale_ext_api::editor::EditorSnapshot,
    root: &str,
    path: &str,
    version: i32,
    edit: &WorkspaceEdit,
) -> Result<Vec<moonkale_ext_api::editor::TextChange>, RenameError> {
    // Validate every target (including empty entries and version-only targets)
    // before looking at any edits. URI aliases cannot hide another file.
    let mut count = 0;
    let mut bytes = 0usize;
    for (target, edits) in &edit.changes {
        if uri::relative_file(root, target).as_deref() != Some(path) {
            return Err(RenameError::MultiFile);
        }
        count += edits.len();
        for change in edits {
            bytes = bytes.saturating_add(change.new_text.len());
        }
    }
    for (target, expected) in &edit.versions {
        if uri::relative_file(root, target).as_deref() != Some(path) {
            return Err(RenameError::MultiFile);
        }
        if *expected != version {
            return Err(RenameError::Stale);
        }
    }
    if count > 4096 || bytes > 1024 * 1024 {
        return Err(RenameError::Limit);
    }
    let edits = edit
        .changes
        .iter()
        .flat_map(|(_, edits)| edits.iter().cloned())
        .collect::<Vec<_>>();
    let batch = edit_batch_bounded(base, &edits, 1024 * 1024).map_err(|error| {
        if error == "edit payload limit exceeded" {
            RenameError::Limit
        } else {
            RenameError::Invalid
        }
    })?;
    if batch.changes.is_empty() {
        return Err(RenameError::Invalid);
    }
    Ok(batch.changes)
}

/// Ask the existing language server for a complete rename against an exact base.
/// `byte` must be the declaration-name token offset from a validated syntax plan.
/// Startup uses the manager's existing timeout; rename itself is capped at 5 s.
/// The resulting proposal remains unsaved and uses ordinary document history.
/// Retain the returned guard alongside the preview and validate it before Apply.
async fn request(
    ws: Workspace,
    node: &Node,
    base: &DocumentEditBase,
    byte: usize,
    name: &str,
) -> Result<(WorkspaceEdit, RenameGuard), RenameError> {
    base.validate(ws).map_err(|_| RenameError::Stale)?;
    if base.snapshot().node != node.id || name.is_empty() || name.len() > 256 {
        return Err(RenameError::Invalid);
    }
    let text = &base.snapshot().text;
    let prefix = text.get(..byte).ok_or(RenameError::Invalid)?;
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() as u32;
    let col = prefix
        .rsplit('\n')
        .next()
        .unwrap_or_default()
        .encode_utf16()
        .count() as u32;
    let root = node
        .source
        .as_str()
        .strip_prefix("folder:")
        .ok_or(RenameError::Unavailable)?;
    let manager = LspManager::for_workspace(ws);
    let session = manager
        .ensure(ws, "rust", root)
        .await
        .ok_or(RenameError::Unavailable)?;
    base.validate(ws).map_err(|_| RenameError::Stale)?;
    manager.retain_workspace_document(ws, node, session.clone(), root, "rust");
    manager.sync_workspace_documents(ws);
    let uri = uri::file_uri(root, &node.native_key);
    let version = manager
        .peek_document_version(&uri)
        .ok_or(RenameError::Unavailable)?;
    // Changes to other open buffers can alter name resolution as well. Include
    // handles/revisions so edit+undo and close+reopen do not pass by equal text.
    let documents = ws
        .docs
        .open
        .peek()
        .iter()
        .map(|(id, doc)| OpenBase {
            id: *id,
            document: *doc,
            text: doc.peek().text.clone(),
            session: ws
                .editor_session(*id)
                .map(|session| (session, session.peek().snapshot().revision)),
        })
        .collect();
    let guard = RenameGuard {
        documents,
        manager,
        session: session.clone(),
        uri: uri.clone(),
        version,
    };
    let edit = match select(
        Box::pin(session.rename(&uri, line, col, name)),
        Box::pin(futures_timer::Delay::new(Duration::from_secs(5))),
    )
    .await
    {
        Either::Left((result, _)) => result.map_err(|_| RenameError::Invalid)?,
        Either::Right(_) => return Err(RenameError::Timeout),
    };
    base.validate(ws).map_err(|_| RenameError::Stale)?;
    guard.validate(ws)?;
    Ok((edit, guard))
}

/// Legacy single-file proposal API; multi-file plans remain refused here.
pub async fn propose(
    ws: Workspace,
    node: &Node,
    base: &DocumentEditBase,
    byte: usize,
    name: &str,
) -> Result<(DocumentEditProposal, RenameGuard), RenameError> {
    let (edit, guard) = request(ws, node, base, byte, name).await?;
    let root = node
        .source
        .as_str()
        .strip_prefix("folder:")
        .ok_or(RenameError::Unavailable)?;
    let version = guard.version;
    let changes = single_file_changes(base.snapshot(), root, &node.native_key, version, &edit)?;
    if !changes
        .iter()
        .any(|change| change.range.start == byte && change.inserted_text == name)
    {
        return Err(RenameError::Invalid);
    }
    let proposal = base.propose(changes).map_err(|error| match error {
        ProposalConflict::LimitExceeded => RenameError::Limit,
        _ => RenameError::Invalid,
    })?;
    Ok((proposal, guard))
}

/// Discover/load complete targets, then request a snapshot-guarded authoritative rename.
/// Only writable files within the originating folder are accepted.
pub async fn propose_multi(
    ws: Workspace,
    node: &Node,
    base: &DocumentEditBase,
    byte: usize,
    name: &str,
) -> Result<
    (
        moonkale_ext_api::workspace::grouped_edits::MultiDocumentEditProposal,
        RenameGuard,
    ),
    RenameError,
> {
    propose_multi_guarded(ws, node, base, byte, name, || true).await
}

/// As propose_multi, cancelling file loading and the second request when review expires.
pub async fn propose_multi_guarded(
    ws: Workspace,
    node: &Node,
    base: &DocumentEditBase,
    byte: usize,
    name: &str,
    current: impl Fn() -> bool,
) -> Result<
    (
        moonkale_ext_api::workspace::grouped_edits::MultiDocumentEditProposal,
        RenameGuard,
    ),
    RenameError,
> {
    use moonkale_ext_api::workspace::grouped_edits::MultiDocumentEditProposal;
    let root = node
        .source
        .as_str()
        .strip_prefix("folder:")
        .ok_or(RenameError::Unavailable)?;
    let (discovery, first_guard) = request(ws, node, base, byte, name).await?;
    if !current() {
        return Err(RenameError::Stale);
    }
    let paths = targets(root, &discovery)?;
    let manager = first_guard.manager;
    let mut prepared = std::collections::BTreeMap::new();
    let mut bytes = 0usize;
    for path in paths {
        let target = crate::native_definition::resolve_target(ws, node, &path)
            .await
            .map_err(|_| RenameError::Unavailable)?;
        if !matches!(
            ws.load_node_guarded(target.clone(), || current() && base.validate(ws).is_ok())
                .await,
            Ok(true)
        ) {
            return Err(RenameError::Stale);
        }
        let captured = ws
            .document_edit_base(target.id)
            .map_err(|_| RenameError::Stale)?;
        bytes = bytes.saturating_add(captured.snapshot().text.len());
        if bytes > 8 * 1024 * 1024 {
            return Err(RenameError::Limit);
        }
        manager.retain_workspace_document(ws, &target, first_guard.session.clone(), root, "rust");
        prepared.insert(path, captured);
    }
    if !current() {
        return Err(RenameError::Stale);
    }
    let (edit, guard) = request(ws, node, base, byte, name).await?;
    if !current() {
        return Err(RenameError::Stale);
    }
    for path in targets(root, &edit)? {
        if !prepared.contains_key(&path) {
            return Err(RenameError::TargetsChanged);
        }
    }
    for (uri, version) in &edit.versions {
        if manager.peek_document_version(uri) != Some(*version) {
            return Err(RenameError::Stale);
        }
    }
    let mut files = vec![];
    let mut origin_changed = false;
    for (uri, edits) in &edit.changes {
        let path = uri::relative_file(root, uri).ok_or(RenameError::MultiFile)?;
        let captured = prepared.get(&path).ok_or(RenameError::TargetsChanged)?;
        captured.validate(ws).map_err(|_| RenameError::Stale)?;
        let batch = edit_batch_bounded(captured.snapshot(), edits, 1024 * 1024)
            .map_err(|_| RenameError::Invalid)?;
        if captured.snapshot().node == node.id {
            origin_changed = batch
                .changes
                .iter()
                .any(|c| c.range.start == byte && c.inserted_text == name);
        }
        if !batch.changes.is_empty() {
            files.push(
                captured
                    .propose(batch.changes)
                    .map_err(|_| RenameError::Invalid)?,
            );
        }
    }
    if !origin_changed {
        return Err(RenameError::Invalid);
    }
    guard.validate(ws)?;
    let proposal = MultiDocumentEditProposal::new(files).map_err(|_| RenameError::Limit)?;
    Ok((proposal, guard))
}

fn targets(
    root: &str,
    edit: &WorkspaceEdit,
) -> Result<std::collections::BTreeSet<String>, RenameError> {
    let mut paths = std::collections::BTreeSet::new();
    let mut count = 0usize;
    let mut bytes = 0usize;
    for (uri, edits) in &edit.changes {
        let path = uri::relative_file(root, uri).ok_or(RenameError::MultiFile)?;
        if !paths.insert(path) {
            return Err(RenameError::Invalid);
        }
        count += edits.len();
        for edit in edits {
            bytes = bytes.saturating_add(edit.new_text.len());
        }
    }
    for (uri, _) in &edit.versions {
        paths.insert(uri::relative_file(root, uri).ok_or(RenameError::MultiFile)?);
    }
    if paths.len() > 64 || count > 4096 || bytes > 4 * 1024 * 1024 {
        return Err(RenameError::Limit);
    }
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::*;
    use moonkale_core::{NodeId, SourceId};
    use moonkale_ext_api::editor::RevisionedDocument;
    use moonkale_lsp::TextEdit;

    fn snapshot() -> moonkale_ext_api::editor::EditorSnapshot {
        RevisionedDocument::new(
            NodeId::derive(&SourceId::new("test"), "main.rs"),
            "// 😀\r\nfn old() {}\r\nfn main() { old(); }\r\n".into(),
        )
        .snapshot()
        .clone()
    }
    fn edit(line: u32, col: u32, end_col: u32, name: &str) -> TextEdit {
        TextEdit {
            line,
            col,
            end_line: line,
            end_col,
            new_text: name.into(),
        }
    }
    #[test]
    fn complete_single_file_plan_preserves_bytes_and_groups_history() {
        let base = snapshot();
        let plan = WorkspaceEdit {
            changes: vec![(
                "file://localhost/project%20space/main.rs".into(),
                vec![edit(1, 3, 6, "新"), edit(2, 12, 15, "新")],
            )],
            versions: vec![("file:///project%20space/main.rs".into(), 7)],
        };
        let changes = single_file_changes(&base, "/project space", "main.rs", 7, &plan).unwrap();
        let mut document = RevisionedDocument::new(base.node, base.text.clone());
        document
            .apply_in_place(&moonkale_ext_api::editor::EditBatch {
                node: base.node,
                base_revision: base.revision,
                changes,
                selection: None,
            })
            .unwrap();
        assert_eq!(document.snapshot().text, base.text.replace("old", "新"));
        document.undo().unwrap();
        assert_eq!(document.snapshot().text, base.text);
        document.redo().unwrap();
        assert_eq!(document.snapshot().text, base.text.replace("old", "新"));
    }
    #[test]
    fn refuses_other_files_versions_alias_duplicates_ranges_and_budgets() {
        let base = snapshot();
        let uri = "file:///project/main.rs";
        let valid = WorkspaceEdit {
            changes: vec![(uri.into(), vec![edit(1, 3, 6, "new")])],
            versions: vec![],
        };
        let check =
            |plan: &WorkspaceEdit| single_file_changes(&base, "/project", "main.rs", 1, plan);
        for target in [
            "file:///project/other.rs",
            "file:///outside/main.rs",
            "file:///project/../main.rs",
            "file://remote/project/main.rs",
        ] {
            let mut plan = valid.clone();
            plan.changes.push((target.into(), vec![]));
            assert_eq!(check(&plan), Err(RenameError::MultiFile));
        }
        let mut plan = valid.clone();
        plan.versions.push(("file:///project/other.rs".into(), 1));
        assert_eq!(check(&plan), Err(RenameError::MultiFile));
        plan.versions = vec![(uri.into(), 2)];
        assert_eq!(check(&plan), Err(RenameError::Stale));
        plan = valid.clone();
        plan.changes.push((
            "file://localhost/project/%6dain.rs".into(),
            plan.changes[0].1.clone(),
        ));
        assert_eq!(check(&plan), Err(RenameError::Invalid));
        plan = valid.clone();
        plan.changes[0].1 = vec![edit(0, 4, 5, "x")]; // split supplementary character
        assert_eq!(check(&plan), Err(RenameError::Invalid));
        plan.changes[0].1 = vec![edit(1, 3, 6, "new"); 4097];
        assert_eq!(check(&plan), Err(RenameError::Limit));
        assert_eq!(check(&WorkspaceEdit::default()), Err(RenameError::Invalid));
        let large =
            moonkale_ext_api::editor::RevisionedDocument::new(base.node, "x".repeat(512 * 1024));
        let plan = WorkspaceEdit {
            changes: vec![(uri.into(), vec![edit(0, 0, 512 * 1024, ""); 3])],
            versions: vec![],
        };
        assert_eq!(
            single_file_changes(large.snapshot(), "/project", "main.rs", 1, &plan),
            Err(RenameError::Limit)
        );
    }
    #[test]
    fn complete_target_preflight_checks_folder_aliases_and_aggregate_limits() {
        let mut plan = WorkspaceEdit {
            changes: vec![
                ("file:///project/main.rs".into(), vec![edit(1, 3, 6, "new")]),
                (
                    "file:///project/sub/consumer.rs".into(),
                    vec![edit(0, 0, 3, "new")],
                ),
            ],
            versions: vec![],
        };
        assert_eq!(targets("/project", &plan).unwrap().len(), 2);
        plan.changes
            .push(("file://localhost/project/%6dain.rs".into(), vec![]));
        assert_eq!(targets("/project", &plan), Err(RenameError::Invalid));
        plan.changes.pop();
        plan.versions.push(("file:///outside/main.rs".into(), 1));
        assert_eq!(targets("/project", &plan), Err(RenameError::MultiFile));
        plan.versions.clear();
        plan.changes[0].1 = vec![edit(0, 0, 0, "x"); 4096];
        assert_eq!(targets("/project", &plan), Err(RenameError::Limit));
        plan.changes[0].1 = vec![edit(0, 0, 0, &"x".repeat(4 * 1024 * 1024))];
        assert_eq!(targets("/project", &plan), Err(RenameError::Limit));
        plan.changes = (0..65)
            .map(|i| (format!("file:///project/{i}.rs"), vec![]))
            .collect();
        assert_eq!(targets("/project", &plan), Err(RenameError::Limit));
    }
}
