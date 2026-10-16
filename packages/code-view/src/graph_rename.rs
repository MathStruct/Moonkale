//! Reference-aware, single-file rename proposals shared with the graph editor.
//! No LSP edit is applied here; multi-file plans are refused in full.
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
pub async fn propose(
    ws: Workspace,
    node: &Node,
    base: &DocumentEditBase,
    byte: usize,
    name: &str,
) -> Result<(DocumentEditProposal, RenameGuard), RenameError> {
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
}
