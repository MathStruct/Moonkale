//! Structured text change deltas.
//!
//! `editor-core` historically exposed state changes as a coarse event
//! ([`crate::StateChangeType::DocumentModified`]) plus a best-effort affected region.
//! For a full-featured editor, incremental consumers (LSP sync, incremental parsing, indexing,
//! match highlighting, etc.) typically need **structured edits** without diffing old/new text.
//!
//! This module defines a small, UI-agnostic delta format expressed in **character offsets**
//! (Unicode scalar values).

/// A single text edit expressed in character offsets.
///
/// Semantics:
/// - `start` is a character offset in the document **at the time this edit is applied**.
/// - The deleted range is defined by the length (in `char`s) of `deleted_text`.
/// - Edits inside a [`TextDelta`] must be applied **in order** to transform the "before" document
///   into the "after" document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextDeltaEdit {
    /// Start character offset of the edit.
    pub start: usize,
    /// Exact deleted text (may be empty).
    pub deleted_text: String,
    /// Exact inserted text (may be empty).
    pub inserted_text: String,
}

impl TextDeltaEdit {
    /// Length of `deleted_text` in characters.
    pub fn deleted_len(&self) -> usize {
        self.deleted_text.chars().count()
    }

    /// Length of `inserted_text` in characters.
    pub fn inserted_len(&self) -> usize {
        self.inserted_text.chars().count()
    }

    /// Exclusive end character offset in the pre-edit document.
    pub fn end(&self) -> usize {
        self.start.saturating_add(self.deleted_len())
    }
}

/// A structured description of a document text change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextDelta {
    /// Character count before applying `edits`.
    pub before_char_count: usize,
    /// Character count after applying `edits`.
    pub after_char_count: usize,
    /// Ordered list of edits that transforms the "before" document into the "after" document.
    pub edits: Vec<TextDeltaEdit>,
    /// If known, the undo group id associated with this change.
    pub undo_group_id: Option<usize>,
}

impl TextDelta {
    /// Returns `true` if this delta contains no edits.
    pub fn is_empty(&self) -> bool {
        self.edits.is_empty()
    }

    /// Merge two consecutive deltas into a single equivalent delta.
    ///
    /// `first` transforms document `B0 -> B1` and `second` transforms `B1 -> B2` (i.e. `second`
    /// was produced against the document state that results from applying `first`). The returned
    /// delta transforms `B0 -> B2`.
    ///
    /// Because a [`TextDelta`]'s edits are defined to be applied **in order**, each using the
    /// offset valid *at the time that edit is applied*, the merge is simply the concatenation of
    /// the two edit lists: applying `first`'s edits reaches `B1`, then applying `second`'s edits
    /// (already expressed against `B1`) reaches `B2`. No coordinate remapping is required.
    ///
    /// This is used to coalesce buffer deltas that accumulate between consumptions, so a consumer
    /// that only reads the latest stored delta still observes every intervening edit.
    pub fn merge(first: &TextDelta, second: &TextDelta) -> TextDelta {
        debug_assert_eq!(
            first.after_char_count, second.before_char_count,
            "merged deltas must be consecutive: first.after must equal second.before"
        );

        // Concatenating empty lists still yields a valid delta; handle the fast paths cheaply.
        if first.is_empty() {
            return second.clone();
        }
        if second.is_empty() {
            return first.clone();
        }

        let mut edits = Vec::with_capacity(first.edits.len() + second.edits.len());
        edits.extend(first.edits.iter().cloned());
        edits.extend(second.edits.iter().cloned());

        TextDelta {
            before_char_count: first.before_char_count,
            after_char_count: second.after_char_count,
            edits,
            // The merged delta spans two changes; only keep a group id if both agree on one.
            undo_group_id: match (first.undo_group_id, second.undo_group_id) {
                (Some(a), Some(b)) if a == b => Some(a),
                _ => None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Apply a delta's edits, in order, to a string using char offsets (the delta contract).
    fn apply(text: &str, delta: &TextDelta) -> String {
        let mut chars: Vec<char> = text.chars().collect();
        assert_eq!(chars.len(), delta.before_char_count, "before_char_count");
        for edit in &delta.edits {
            let start = edit.start;
            let end = edit.end();
            assert!(end <= chars.len(), "edit out of range");
            let inserted: Vec<char> = edit.inserted_text.chars().collect();
            chars.splice(start..end, inserted);
        }
        assert_eq!(chars.len(), delta.after_char_count, "after_char_count");
        chars.into_iter().collect()
    }

    fn edit(start: usize, deleted: &str, inserted: &str) -> TextDeltaEdit {
        TextDeltaEdit {
            start,
            deleted_text: deleted.to_string(),
            inserted_text: inserted.to_string(),
        }
    }

    #[test]
    fn merge_of_two_inserts_replays_both() {
        // B0 "abc" --insert ">>" at 0--> B1 ">>abc" --insert "!" at 5--> B2 ">>abc!"
        let d1 = TextDelta {
            before_char_count: 3,
            after_char_count: 5,
            edits: vec![edit(0, "", ">>")],
            undo_group_id: Some(1),
        };
        let d2 = TextDelta {
            before_char_count: 5,
            after_char_count: 6,
            edits: vec![edit(5, "", "!")],
            undo_group_id: Some(2),
        };

        let merged = TextDelta::merge(&d1, &d2);
        assert_eq!(merged.before_char_count, 3);
        assert_eq!(merged.after_char_count, 6);
        // Applying the merged delta to B0 must equal applying d1 then d2.
        let via_merged = apply("abc", &merged);
        let via_steps = apply(&apply("abc", &d1), &d2);
        assert_eq!(via_merged, ">>abc!");
        assert_eq!(via_merged, via_steps);
        // Differing group ids collapse to None.
        assert_eq!(merged.undo_group_id, None);
    }

    #[test]
    fn merge_with_deletions_replays_both() {
        // B0 "hello world" --del [0,6)--> B1 "world" --replace [0,5) with "there"--> B2 "there"
        let d1 = TextDelta {
            before_char_count: 11,
            after_char_count: 5,
            edits: vec![edit(0, "hello ", "")],
            undo_group_id: None,
        };
        let d2 = TextDelta {
            before_char_count: 5,
            after_char_count: 5,
            edits: vec![edit(0, "world", "there")],
            undo_group_id: None,
        };
        let merged = TextDelta::merge(&d1, &d2);
        assert_eq!(apply("hello world", &merged), "there");
        assert_eq!(
            apply("hello world", &merged),
            apply(&apply("hello world", &d1), &d2)
        );
    }

    #[test]
    fn merge_preserves_shared_group_id() {
        let d1 = TextDelta {
            before_char_count: 0,
            after_char_count: 1,
            edits: vec![edit(0, "", "a")],
            undo_group_id: Some(7),
        };
        let d2 = TextDelta {
            before_char_count: 1,
            after_char_count: 2,
            edits: vec![edit(1, "", "b")],
            undo_group_id: Some(7),
        };
        assert_eq!(TextDelta::merge(&d1, &d2).undo_group_id, Some(7));
    }

    #[test]
    fn merge_with_empty_delta_is_identity() {
        let empty = TextDelta {
            before_char_count: 3,
            after_char_count: 3,
            edits: Vec::new(),
            undo_group_id: None,
        };
        let d = TextDelta {
            before_char_count: 3,
            after_char_count: 4,
            edits: vec![edit(3, "", "!")],
            undo_group_id: Some(1),
        };
        // empty ∘ d == d
        assert_eq!(TextDelta::merge(&empty, &d), d);
        // d ∘ empty' == d, where empty' matches d's after state
        let empty_after = TextDelta {
            before_char_count: 4,
            after_char_count: 4,
            edits: Vec::new(),
            undo_group_id: None,
        };
        assert_eq!(TextDelta::merge(&d, &empty_after), d);
    }

    #[test]
    fn merge_multi_edit_deltas_replays_in_order() {
        // Two multi-edit deltas; verify the merged edits reproduce sequential application.
        let d1 = TextDelta {
            before_char_count: 5,
            after_char_count: 7,
            // "abcde": insert "X" at 5, then (already-applied-order) insert "Y" at 0 uses
            // post-first-edit coordinates per the in-order contract.
            edits: vec![edit(5, "", "X"), edit(0, "", "Y")],
            undo_group_id: None,
        };
        let b1 = apply("abcde", &d1); // "YabcdeX"
        let d2 = TextDelta {
            before_char_count: 7,
            after_char_count: 6,
            edits: vec![edit(0, "Y", "")],
            undo_group_id: None,
        };
        let merged = TextDelta::merge(&d1, &d2);
        assert_eq!(apply("abcde", &merged), apply(&b1, &d2));
        assert_eq!(apply("abcde", &merged), "abcdeX");
    }
}
