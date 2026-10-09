//! Revisioned, engine-neutral contract between an editor view and Workspace.
//!
//! A document revision belongs to the live editor session and is deliberately
//! separate from `Document::version`, which tracks the backing source/file.
//! Ranges in [`TextChange`] are UTF-8 byte offsets into the batch's base text;
//! caret/selection coordinates are absolute UTF-16 offsets to match LSP and
//! browser input APIs.

use moonkale_core::NodeId;
use std::{collections::VecDeque, ops::Range};

/// Monotonically increasing revision of one open editor document.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DocumentRevision(pub u64);

/// Stable identity and current text/revision of one editor document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditorSnapshot {
    /// The Workspace document this snapshot belongs to.
    pub node: NodeId,
    /// Editor-local revision, independent of the file/source version.
    pub revision: DocumentRevision,
    /// Current document text.
    pub text: String,
    /// Latest editor-owned selection, expressed as absolute UTF-16 offsets.
    pub selection: Option<Utf16Selection>,
}

/// One replacement against the batch's base snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextChange {
    /// Half-open UTF-8 byte range in the base text.
    pub range: Range<usize>,
    /// Text expected at `range`; checked before any part of a batch is applied.
    pub removed_text: String,
    /// Text to insert at `range`.
    pub inserted_text: String,
}

/// Active selection/caret, expressed as absolute UTF-16 document offsets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Utf16Selection {
    /// Offset where selection began; equals `head` for a caret.
    pub anchor: u32,
    /// Active end of the selection (the caret).
    pub head: u32,
}

/// A set of non-overlapping changes, all addressed against one old revision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditBatch {
    /// Workspace identity prevents a batch from being applied to another tab.
    pub node: NodeId,
    /// The snapshot revision against which every change range is measured.
    pub base_revision: DocumentRevision,
    /// Local text changes; all ranges use UTF-8 bytes in the base snapshot.
    pub changes: Vec<TextChange>,
    /// Selection after the changes, in absolute UTF-16 offsets in the new text.
    pub selection: Option<Utf16Selection>,
}

/// Position used by host reveal/focus requests; line and column are zero-based,
/// and the column counts UTF-16 code units (the LSP convention).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Utf16Position {
    /// Zero-based line.
    pub line: u32,
    /// Zero-based UTF-16 column.
    pub column: u32,
}

/// Commands sent by the Workspace/panel host into an editor session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditorCommand {
    /// Replace the view from canonical Workspace text and adopt this revision.
    ReplaceFromWorkspace(EditorSnapshot),
    /// Focus the editor input surface.
    Focus,
    /// Undo the latest local transaction.
    Undo,
    /// Redo the latest undone local transaction.
    Redo,
    /// Enable or disable soft wrapping.
    SetWrap(bool),
    /// Place the caret and reveal this LSP-coordinate position.
    Reveal(Utf16Position),
}

/// Events sent by the editor session back to its host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditorEvent {
    /// The view is ready for the supplied document snapshot.
    Ready(EditorSnapshot),
    /// A localized edit transaction and its resulting selection.
    Changed(EditBatch),
    /// Selection-only update; ignored when its revision is no longer current.
    Selection {
        /// Revision whose text the selection refers to.
        revision: DocumentRevision,
        /// Anchor/head offsets in UTF-16 units.
        selection: Utf16Selection,
    },
    /// Editor initialization or command failure.
    Failed(String),
}

/// A canonical text value, revision and local undo history for one editor
/// document. Workspace stores it by `NodeId`, independently of any panel
/// component's mount lifetime.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RevisionedDocument {
    snapshot: EditorSnapshot,
    undo: VecDeque<HistoryEntry>,
    redo: VecDeque<HistoryEntry>,
    utf16_len: u32,
}

// Keep localized inverse edits rather than one complete document per keystroke.
// Limit both transaction count and retained edit payload across undo and redo.
const HISTORY_LIMIT: usize = 100;
const HISTORY_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
struct HistoryEntry {
    forward: Vec<TextChange>,
    inverse: Vec<TextChange>,
    before: Option<Utf16Selection>,
    after: Option<Utf16Selection>,
}

impl HistoryEntry {
    fn bytes(&self) -> usize {
        self.forward
            .iter()
            .chain(&self.inverse)
            .map(|change| change.removed_text.len() + change.inserted_text.len())
            .sum()
    }
}

/// Why an edit batch was rejected. The embedded snapshot is canonical and can
/// be sent to the view with [`EditorCommand::ReplaceFromWorkspace`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BatchRejected {
    /// The event came from another document or an older editor revision.
    Stale(EditorSnapshot),
    /// The batch ranges are malformed, overlapping, or do not match old text.
    Invalid {
        /// Current canonical snapshot for recovery.
        snapshot: EditorSnapshot,
        /// Reason suitable for logs and diagnostics.
        reason: &'static str,
    },
    /// The revision counter cannot advance further.
    RevisionExhausted(EditorSnapshot),
}

impl RevisionedDocument {
    /// Start a document session at revision zero.
    pub fn new(node: NodeId, text: String) -> Self {
        let utf16_len = text.encode_utf16().count() as u32;
        Self {
            snapshot: EditorSnapshot {
                node,
                revision: DocumentRevision(0),
                text,
                selection: None,
            },
            undo: VecDeque::new(),
            redo: VecDeque::new(),
            utf16_len,
        }
    }

    /// Cached UTF-16 length of canonical text, updated with each transaction.
    pub fn utf16_len(&self) -> u32 {
        self.utf16_len
    }

    /// Read the current immutable snapshot.
    pub fn snapshot(&self) -> &EditorSnapshot {
        &self.snapshot
    }

    /// Apply one atomic edit batch. All changes are validated against the same
    /// base text, then applied from right to left so offsets remain stable.
    pub fn apply(&mut self, batch: &EditBatch) -> Result<EditorSnapshot, BatchRejected> {
        if batch.node != self.snapshot.node || batch.base_revision != self.snapshot.revision {
            return Err(BatchRejected::Stale(self.snapshot.clone()));
        }

        let mut ordered: Vec<_> = batch.changes.iter().collect();
        ordered.sort_by_key(|change| (change.range.start, change.range.end));
        for (index, change) in ordered.iter().enumerate() {
            if change.range.start > change.range.end
                || change.range.end > self.snapshot.text.len()
                || !self.snapshot.text.is_char_boundary(change.range.start)
                || !self.snapshot.text.is_char_boundary(change.range.end)
                || self.snapshot.text.get(change.range.clone())
                    != Some(change.removed_text.as_str())
            {
                return Err(BatchRejected::Invalid {
                    snapshot: self.snapshot.clone(),
                    reason: "change range or removed text does not match the base snapshot",
                });
            }
            if index > 0 {
                let previous = ordered[index - 1];
                let overlaps = previous.range.end > change.range.start
                    || (previous.range.is_empty()
                        && change.range.is_empty()
                        && previous.range.start == change.range.start);
                if overlaps {
                    return Err(BatchRejected::Invalid {
                        snapshot: self.snapshot.clone(),
                        reason: "edit batch contains overlapping changes",
                    });
                }
            }
        }

        let mut next_text = self.snapshot.text.clone();
        for change in ordered.iter().rev() {
            next_text.replace_range(change.range.clone(), &change.inserted_text);
        }

        let next_length = ordered
            .iter()
            .fold(self.utf16_len as i64, |length, change| {
                length + change.inserted_text.encode_utf16().count() as i64
                    - change.removed_text.encode_utf16().count() as i64
            }) as u32;
        if batch
            .selection
            .is_some_and(|selection| selection.anchor > next_length || selection.head > next_length)
        {
            return Err(BatchRejected::Invalid {
                snapshot: self.snapshot.clone(),
                reason: "selection offset exceeds the resulting text length",
            });
        }

        if next_text == self.snapshot.text {
            self.snapshot.selection = batch.selection;
            return Ok(self.snapshot.clone());
        }
        let Some(next_revision) = self.snapshot.revision.0.checked_add(1) else {
            return Err(BatchRejected::RevisionExhausted(self.snapshot.clone()));
        };
        let mut offset = 0isize;
        let inverse = ordered
            .iter()
            .map(|change| {
                let start = change.range.start.checked_add_signed(offset).unwrap();
                offset += change.inserted_text.len() as isize - change.removed_text.len() as isize;
                TextChange {
                    range: start..start + change.inserted_text.len(),
                    removed_text: change.inserted_text.clone(),
                    inserted_text: change.removed_text.clone(),
                }
            })
            .collect();
        self.undo.push_back(HistoryEntry {
            forward: ordered.into_iter().cloned().collect(),
            inverse,
            before: self.snapshot.selection,
            after: batch.selection,
        });
        self.redo.clear();
        self.trim_history();
        self.utf16_len = next_length;
        self.snapshot = EditorSnapshot {
            node: self.snapshot.node,
            revision: DocumentRevision(next_revision),
            text: next_text,
            selection: batch.selection,
        };
        Ok(self.snapshot.clone())
    }

    /// Record a selection-only update if it belongs to the current revision.
    /// UTF-16 offsets beyond the current text are rejected.
    pub fn set_selection(&mut self, revision: DocumentRevision, selection: Utf16Selection) -> bool {
        if revision != self.snapshot.revision {
            return false;
        }
        let length = self.utf16_len;
        if selection.anchor > length || selection.head > length {
            return false;
        }
        self.snapshot.selection = Some(selection);
        true
    }

    fn trim_history(&mut self) {
        while self.undo.len() + self.redo.len() > HISTORY_LIMIT
            || self
                .undo
                .iter()
                .chain(&self.redo)
                .map(HistoryEntry::bytes)
                .sum::<usize>()
                > HISTORY_BYTES
        {
            if self.undo.pop_front().is_none() {
                self.redo.pop_front();
            }
        }
    }

    fn restore(&mut self, redo: bool) -> Result<Option<EditorSnapshot>, BatchRejected> {
        let history = if redo { &mut self.redo } else { &mut self.undo };
        if history.is_empty() {
            return Ok(None);
        }
        let Some(revision) = self.snapshot.revision.0.checked_add(1) else {
            return Err(BatchRejected::RevisionExhausted(self.snapshot.clone()));
        };
        let mut entry = history.pop_back().unwrap();
        // The user may move or extend selection after the text transaction.
        // Capture that live selection when leaving a history state, just as
        // snapshot history did, so redo restores the range used by commands.
        if redo {
            entry.before = self.snapshot.selection;
        } else {
            entry.after = self.snapshot.selection;
        }
        let changes = if redo { &entry.forward } else { &entry.inverse };
        for change in changes.iter().rev() {
            self.snapshot
                .text
                .replace_range(change.range.clone(), &change.inserted_text);
            self.utf16_len =
                (self.utf16_len as i64 + change.inserted_text.encode_utf16().count() as i64
                    - change.removed_text.encode_utf16().count() as i64) as u32;
        }
        self.snapshot.revision = DocumentRevision(revision);
        self.snapshot.selection = if redo { entry.after } else { entry.before };
        if redo {
            self.undo.push_back(entry);
        } else {
            self.redo.push_back(entry);
        }
        Ok(Some(self.snapshot.clone()))
    }

    /// Undo one atomic local transaction as a new revision.
    pub fn undo(&mut self) -> Result<Option<EditorSnapshot>, BatchRejected> {
        self.restore(false)
    }

    /// Redo one atomic local transaction as a new revision.
    pub fn redo(&mut self) -> Result<Option<EditorSnapshot>, BatchRejected> {
        self.restore(true)
    }

    /// Adopt a Workspace-originated replacement. Unchanged text preserves the
    /// revision; changed text advances it and invalidates outstanding batches.
    pub fn replace_from_workspace(
        &mut self,
        text: String,
    ) -> Result<EditorSnapshot, BatchRejected> {
        if self.snapshot.text == text {
            return Ok(self.snapshot.clone());
        }
        let Some(next_revision) = self.snapshot.revision.0.checked_add(1) else {
            return Err(BatchRejected::RevisionExhausted(self.snapshot.clone()));
        };
        self.snapshot.revision = DocumentRevision(next_revision);
        self.utf16_len = text.encode_utf16().count() as u32;
        self.snapshot.text = text;
        self.snapshot.selection = None;
        self.undo.clear();
        self.redo.clear();
        Ok(self.snapshot.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document(text: &str) -> RevisionedDocument {
        RevisionedDocument::new(NodeId::fresh("editor-contract-test"), text.into())
    }

    fn batch(doc: &RevisionedDocument, changes: Vec<TextChange>) -> EditBatch {
        EditBatch {
            node: doc.snapshot().node,
            base_revision: doc.snapshot().revision,
            changes,
            selection: None,
        }
    }

    fn change(range: Range<usize>, removed: &str, inserted: &str) -> TextChange {
        TextChange {
            range,
            removed_text: removed.into(),
            inserted_text: inserted.into(),
        }
    }

    #[test]
    fn unsorted_unicode_changes_share_one_base_and_one_undo_transaction() {
        let mut doc = document("a😀中z");
        let selection = Utf16Selection { anchor: 4, head: 1 };
        assert!(doc.set_selection(DocumentRevision(0), selection));
        let original = doc.snapshot().clone();
        let mut edits = batch(&doc, vec![change(8..9, "z", "!"), change(1..5, "😀", "λ")]);
        edits.selection = Some(Utf16Selection { anchor: 3, head: 1 });
        let result = doc.apply(&edits).unwrap();
        assert_eq!(result.text, "aλ中!");
        assert_eq!(result.revision, DocumentRevision(1));
        let undone = doc.undo().unwrap().unwrap();
        assert_eq!(undone.text, original.text);
        assert_eq!(undone.selection, original.selection);
        assert_eq!(undone.revision, DocumentRevision(2));
        let redone = doc.redo().unwrap().unwrap();
        assert_eq!(redone.text, result.text);
        assert_eq!(redone.selection, result.selection);
        assert_eq!(redone.revision, DocumentRevision(3));
        assert!(matches!(doc.apply(&edits), Err(BatchRejected::Stale(_))));
    }

    #[test]
    fn invalid_batches_leave_text_revision_selection_and_history_untouched() {
        let mut doc = document("a😀bc");
        let invalid = [
            vec![change(0..1, "a", "A"), change(5..6, "wrong", "B")],
            vec![change(0..1, "a", "A"), change(2..5, "", "X")],
            vec![change(0..6, "a😀b", "X"), change(5..7, "bc", "Y")],
            vec![change(1..1, "", "X"), change(1..1, "", "Y")],
            vec![change(8..8, "", "X")],
            vec![change(Range { start: 6, end: 5 }, "", "X")],
        ];
        for changes in invalid {
            let before = doc.clone();
            let edits = batch(&doc, changes);
            assert!(matches!(
                doc.apply(&edits),
                Err(BatchRejected::Invalid { .. })
            ));
            assert_eq!(doc, before);
        }
        let mut edits = batch(&doc, vec![change(0..1, "a", "A")]);
        edits.selection = Some(Utf16Selection { anchor: 6, head: 0 });
        let before = doc.clone();
        assert!(matches!(
            doc.apply(&edits),
            Err(BatchRejected::Invalid { .. })
        ));
        assert_eq!(doc, before);
    }

    #[test]
    fn wrong_document_and_stale_revision_return_the_canonical_snapshot() {
        let mut doc = document("abc");
        let mut edits = batch(&doc, vec![change(0..1, "a", "A")]);
        edits.node = NodeId::fresh("different-document");
        assert_eq!(
            doc.apply(&edits),
            Err(BatchRejected::Stale(doc.snapshot().clone()))
        );
        edits.node = doc.snapshot().node;
        doc.replace_from_workspace("xyz".into()).unwrap();
        assert_eq!(
            doc.apply(&edits),
            Err(BatchRejected::Stale(doc.snapshot().clone()))
        );
        assert_eq!(doc.snapshot().text, "xyz");
    }

    #[test]
    fn selection_only_updates_use_utf16_and_do_not_create_undo_entries() {
        let mut doc = document("😀中");
        let mut edits = batch(&doc, vec![]);
        edits.selection = Some(Utf16Selection { anchor: 3, head: 2 });
        assert_eq!(doc.apply(&edits).unwrap().revision, DocumentRevision(0));
        assert!(doc.undo().unwrap().is_none());
        assert!(!doc.set_selection(DocumentRevision(1), Utf16Selection { anchor: 0, head: 0 }));
        assert!(!doc.set_selection(DocumentRevision(0), Utf16Selection { anchor: 4, head: 0 }));
        assert_eq!(doc.snapshot().selection, edits.selection);
    }

    #[test]
    fn external_replacement_clears_history_and_selection_but_identical_text_preserves_them() {
        let mut doc = document("abc");
        let edits = batch(&doc, vec![change(0..1, "a", "A")]);
        doc.apply(&edits).unwrap();
        doc.set_selection(DocumentRevision(1), Utf16Selection { anchor: 2, head: 1 });
        let before = doc.clone();
        doc.replace_from_workspace("Abc".into()).unwrap();
        assert_eq!(doc, before);
        doc.undo().unwrap();
        doc.replace_from_workspace("external".into()).unwrap();
        assert_eq!(doc.snapshot().revision, DocumentRevision(3));
        assert_eq!(doc.snapshot().selection, None);
        assert!(doc.undo().unwrap().is_none());
        assert!(doc.redo().unwrap().is_none());
        assert!(!doc.set_selection(DocumentRevision(1), Utf16Selection { anchor: 0, head: 0 }));
    }

    #[test]
    fn a_new_edit_after_undo_discards_the_redo_branch() {
        let mut doc = document("abc");
        let edits = batch(&doc, vec![change(0..1, "a", "A")]);
        doc.apply(&edits).unwrap();
        doc.undo().unwrap();
        let edits = batch(&doc, vec![change(1..2, "b", "B")]);
        doc.apply(&edits).unwrap();
        assert!(doc.redo().unwrap().is_none());
        assert_eq!(doc.snapshot().text, "aBc");
        assert_eq!(doc.undo().unwrap().unwrap().text, "abc");
    }

    #[test]
    fn large_documents_retain_local_edits_and_only_the_latest_hundred_transactions() {
        let mut doc = document(&"x".repeat(3 * 1024 * 1024));
        for _ in 0..150 {
            let edits = batch(&doc, vec![change(0..0, "", "😀")]);
            doc.apply(&edits).unwrap();
        }
        assert_eq!(doc.undo.len(), HISTORY_LIMIT);
        assert_eq!(doc.undo.iter().map(HistoryEntry::bytes).sum::<usize>(), 800);
        for _ in 0..HISTORY_LIMIT {
            doc.undo().unwrap().unwrap();
        }
        assert!(doc.undo().unwrap().is_none());
        assert!(doc.snapshot().text.starts_with(&"😀".repeat(50)));
        for _ in 0..HISTORY_LIMIT {
            doc.redo().unwrap().unwrap();
        }
        assert!(doc.redo().unwrap().is_none());
        assert!(doc.snapshot().text.starts_with(&"😀".repeat(150)));
    }

    #[test]
    fn history_restores_selection_moved_after_a_text_transaction() {
        let mut doc = document("abc");
        let edits = batch(&doc, vec![change(0..1, "a", "A")]);
        doc.apply(&edits).unwrap();
        let selection = Utf16Selection { anchor: 0, head: 3 };
        doc.set_selection(doc.snapshot().revision, selection);
        doc.undo().unwrap();
        assert_eq!(doc.redo().unwrap().unwrap().selection, Some(selection));
    }

    #[test]
    fn oversized_transaction_is_applied_without_retaining_unbounded_history() {
        let mut doc = document("old");
        let edits = batch(&doc, vec![change(0..3, "old", &"x".repeat(HISTORY_BYTES))]);
        doc.apply(&edits).unwrap();
        assert_eq!(doc.snapshot().text.len(), HISTORY_BYTES);
        assert!(doc.undo().unwrap().is_none());
    }

    #[test]
    fn same_start_insert_and_replace_round_trip_as_one_transaction() {
        let mut doc = document("a😀z");
        let edits = batch(
            &doc,
            vec![change(1..1, "", "prefix"), change(1..5, "😀", "中")],
        );
        doc.apply(&edits).unwrap();
        assert_eq!(doc.snapshot().text, "aprefix中z");
        assert_eq!(doc.undo().unwrap().unwrap().text, "a😀z");
        assert_eq!(doc.redo().unwrap().unwrap().text, "aprefix中z");
    }

    #[test]
    fn revision_exhaustion_is_atomic_for_edits_replacement_and_history() {
        let mut doc = document("abc");
        let edits = batch(&doc, vec![change(0..1, "a", "A")]);
        doc.apply(&edits).unwrap();
        doc.snapshot.revision = DocumentRevision(u64::MAX);
        let before = doc.clone();
        let edits = batch(&doc, vec![change(1..2, "b", "B")]);
        assert!(matches!(
            doc.apply(&edits),
            Err(BatchRejected::RevisionExhausted(_))
        ));
        assert_eq!(doc, before);
        assert!(matches!(
            doc.replace_from_workspace("external".into()),
            Err(BatchRejected::RevisionExhausted(_))
        ));
        assert_eq!(doc, before);
        assert!(matches!(
            doc.undo(),
            Err(BatchRejected::RevisionExhausted(_))
        ));
        assert_eq!(doc, before);
        doc.snapshot.revision = DocumentRevision(1);
        doc.undo().unwrap();
        doc.snapshot.revision = DocumentRevision(u64::MAX);
        let before = doc.clone();
        assert!(matches!(
            doc.redo(),
            Err(BatchRejected::RevisionExhausted(_))
        ));
        assert_eq!(doc, before);
    }
}
