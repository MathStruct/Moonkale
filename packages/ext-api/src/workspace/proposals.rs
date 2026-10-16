//! Guarded, single-document proposals. Planning never grants write authority.
//!
//! Keep a proposal after a conflict to show its base, preview and live text.
//! Recompute by capturing a new base and rerunning the planner; never silently
//! rebase old byte ranges. Dropping the value discards it without changing text.
use super::*;
use crate::editor::{BatchRejected, EditBatch, EditorSnapshot, RevisionedDocument, TextChange};
use moonkale_core::{SourceTruth, Version};

const MAX_BYTES: usize = 512 * 1024;
const MAX_CHANGES: usize = 4096;

/// A captured open document, including session identity and backing version.
/// Capture on the UI thread; pass only `snapshot()` to background planners.
#[derive(Clone)]
pub struct DocumentEditBase {
    document: Signal<Document>,
    session: Signal<RevisionedDocument>,
    source: Arc<dyn Source>,
    snapshot: EditorSnapshot,
    version: Version,
}

/// An immutable proposed edit. Applying it never saves the backing source.
#[derive(Clone)]
pub struct DocumentEditProposal {
    base: DocumentEditBase,
    changes: Vec<TextChange>,
    preview: String,
}

/// Recoverable reasons a proposal cannot be constructed or applied.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProposalConflict {
    /// The document or source is no longer open, or its session was replaced.
    Closed,
    /// Only writable Files sources are supported by this adapter.
    UnsupportedSource,
    /// Text, editor revision or backing version changed; capture and replan.
    Changed,
    /// The bounded proposal budget was exceeded.
    LimitExceeded,
    /// The editor rejected a malformed edit or exhausted its revision counter.
    Invalid(BatchRejected),
}

impl DocumentEditBase {
    /// Recheck an asynchronous plan's captured identity without mutating text/history.
    pub fn validate(&self, ws: Workspace) -> Result<(), ProposalConflict> {
        self.live(ws).map(|_| ())
    }

    fn live(
        &self,
        ws: Workspace,
    ) -> Result<(Signal<Document>, Signal<RevisionedDocument>), ProposalConflict> {
        let id = self.snapshot.node;
        let doc = ws
            .docs
            .open
            .peek()
            .iter()
            .find(|(node, _)| *node == id)
            .map(|(_, doc)| *doc)
            .ok_or(ProposalConflict::Closed)?;
        let session = ws
            .docs
            .editor_sessions
            .peek()
            .get(&id)
            .copied()
            .ok_or(ProposalConflict::Closed)?;
        // Compare generational handles before dereferencing a possibly disposed session.
        if doc != self.document || session != self.session {
            return Err(ProposalConflict::Closed);
        }
        let source = ws
            .sources
            .open
            .peek()
            .iter()
            .find(|source| source.descriptor.id == doc.peek().node.source)
            .cloned()
            .ok_or(ProposalConflict::Closed)?;
        if !Arc::ptr_eq(&source.source, &self.source) {
            return Err(ProposalConflict::Closed);
        }
        writable_files(&source)?;
        let live = session.peek();
        let live = live.snapshot();
        if live.revision != self.snapshot.revision
            || live.text != self.snapshot.text
            || doc.peek().text != self.snapshot.text
            || doc.peek().version != self.version
        {
            return Err(ProposalConflict::Changed);
        }
        Ok((doc, session))
    }

    /// Exact unsaved text and revision against which a planner must run.
    pub fn snapshot(&self) -> &EditorSnapshot {
        &self.snapshot
    }

    /// Validate byte edits and build a preview without mutating Workspace.
    /// Callers must verify their syntax plan against `snapshot().text` first.
    pub fn propose(
        &self,
        changes: Vec<TextChange>,
    ) -> Result<DocumentEditProposal, ProposalConflict> {
        if changes.len() > MAX_CHANGES
            || changes
                .iter()
                .try_fold(0usize, |size, change| {
                    size.checked_add(change.inserted_text.len())?
                        .checked_add(change.removed_text.len())
                })
                .is_none_or(|size| size > 2 * MAX_BYTES)
        {
            return Err(ProposalConflict::LimitExceeded);
        }
        // Validation uses a disposable session, never live history.
        let mut trial = RevisionedDocument::new(self.snapshot.node, self.snapshot.text.clone());
        let batch = EditBatch {
            node: self.snapshot.node,
            base_revision: trial.snapshot().revision,
            changes: changes.clone(),
            selection: None,
        };
        trial
            .apply_in_place(&batch)
            .map_err(ProposalConflict::Invalid)?;
        let preview = trial.snapshot().text.clone();
        if preview.len() > MAX_BYTES {
            return Err(ProposalConflict::LimitExceeded);
        }
        Ok(DocumentEditProposal {
            base: self.clone(),
            changes,
            preview,
        })
    }
}

impl DocumentEditProposal {
    /// Captured text and revision for a conflict comparison.
    pub fn base(&self) -> &EditorSnapshot {
        self.base.snapshot()
    }

    /// Validated replacements for rendering an exact range-based review.
    pub fn changes(&self) -> &[TextChange] {
        &self.changes
    }

    /// Proposed text; safe to display as data, never execute as markup/code.
    pub fn preview(&self) -> &str {
        &self.preview
    }

    /// Apply one undoable batch to the still-current open document.
    /// On failure neither buffer nor history changes and this value is retained.
    /// A later ordinary `Workspace::save` performs backing-version validation.
    pub fn apply(&self, ws: Workspace) -> Result<EditorSnapshot, ProposalConflict> {
        let id = self.base.snapshot.node;
        let (mut doc, mut session) = self.base.live(ws)?;
        let live = session.peek().snapshot().clone();
        let selection = rebase_selection(&live, &self.changes);
        let batch = EditBatch {
            node: id,
            base_revision: live.revision,
            changes: self.changes.clone(),
            selection,
        };
        // No awaits or callbacks between validation and the complete commit.
        let next = session
            .with_mut(|state| state.apply(&batch))
            .map_err(ProposalConflict::Invalid)?;
        doc.write().text = next.text.clone();
        Ok(next)
    }
}

impl Workspace {
    /// Capture an existing open document for bounded, single-file structural edits.
    /// A stale editor mirror is a conflict, not permission to overwrite unsaved text.
    pub fn document_edit_base(&self, node: NodeId) -> Result<DocumentEditBase, ProposalConflict> {
        let document = self
            .docs
            .open
            .peek()
            .iter()
            .find(|(id, _)| *id == node)
            .map(|(_, doc)| *doc)
            .ok_or(ProposalConflict::Closed)?;
        let source = self
            .sources
            .open
            .peek()
            .iter()
            .find(|source| source.descriptor.id == document.peek().node.source)
            .cloned()
            .ok_or(ProposalConflict::Closed)?;
        writable_files(&source)?;
        if document.peek().text.len() > MAX_BYTES {
            return Err(ProposalConflict::LimitExceeded);
        }
        let session = self.editor_session(node).ok_or(ProposalConflict::Closed)?;
        let snapshot = session.peek().snapshot().clone();
        if snapshot.text != document.peek().text {
            return Err(ProposalConflict::Changed);
        }
        let version = document.peek().version;
        Ok(DocumentEditBase {
            document,
            session,
            source: source.source,
            snapshot,
            version,
        })
    }
}

fn writable_files(source: &SourceHandle) -> Result<(), ProposalConflict> {
    if source.descriptor.truth != SourceTruth::Files || !source.descriptor.capabilities.write {
        Err(ProposalConflict::UnsupportedSource)
    } else {
        Ok(())
    }
}

fn rebase_selection(
    base: &EditorSnapshot,
    changes: &[TextChange],
) -> Option<crate::editor::Utf16Selection> {
    let mut changes: Vec<_> = changes.iter().collect();
    changes.sort_by_key(|change| (change.range.start, change.range.end));
    let mut previous_byte = 0;
    let mut previous_utf16 = 0i64;
    let coordinates: Vec<_> = changes
        .iter()
        .map(|change| {
            let start = previous_utf16
                + base.text[previous_byte..change.range.start]
                    .encode_utf16()
                    .count() as i64;
            let end = start + change.removed_text.encode_utf16().count() as i64;
            previous_byte = change.range.end;
            previous_utf16 = end;
            (
                start,
                end,
                change.inserted_text.encode_utf16().count() as i64,
            )
        })
        .collect();
    let map = |point: u32| {
        let mut shift = 0i64;
        for &(start, end, inserted) in &coordinates {
            if i64::from(point) < start {
                break;
            }
            if i64::from(point) <= end {
                return (start + shift + inserted) as u32;
            }
            shift += inserted - (end - start);
        }
        (i64::from(point) + shift) as u32
    };
    base.selection
        .map(|selection| crate::editor::Utf16Selection {
            anchor: map(selection.anchor),
            head: map(selection.head),
        })
}
