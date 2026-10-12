//! Byte-range edits between the visible Rust editor and Workspace's canonical text.
//!
//! The textarea component reports its new value. This adapter reduces it to a
//! single minimal edit and applies that edit to the existing document buffer.
//! Ranges are UTF-8 byte offsets at Rust string boundaries; LSP and browser
//! positions are converted separately at their boundaries.

#[cfg(test)]
use std::ops::Range;

/// Convert a textarea/CodeMirror UTF-16 document offset into a zero-based
/// line and UTF-16 column. An offset inside a surrogate pair snaps to the
/// start of that scalar value; browser caret positions normally lie on scalar
/// boundaries, but hostile or synthetic inputs should remain harmless.
pub(crate) fn utf16_offset_to_line_col(text: &str, offset: usize) -> Option<(u32, u32)> {
    let mut units = 0usize;
    let mut line = 0u32;
    let mut col = 0u32;

    for ch in text.chars() {
        if units >= offset {
            return Some((line, col));
        }
        let width = ch.len_utf16();
        if offset < units + width {
            return Some((line, col));
        }
        units += width;
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += width as u32;
        }
    }

    (units == offset).then_some((line, col))
}

/// One replacement against the old text. `removed` is retained to reject a
/// stale edit instead of corrupting a document that changed in the meantime.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg(test)]
pub(crate) struct TextEdit {
    pub range: Range<usize>,
    pub removed: String,
    pub inserted: String,
}

#[cfg(test)]
impl TextEdit {
    /// Find the smallest character-boundary replacement from `old` to `new`.
    pub(crate) fn between(old: &str, new: &str) -> Option<Self> {
        if old == new {
            return None;
        }

        let old_bytes = old.as_bytes();
        let new_bytes = new.as_bytes();
        let mut start = 0;
        let shared = old.len().min(new.len());
        while start < shared && old_bytes[start] == new_bytes[start] {
            start += 1;
        }
        while start > 0 && (!old.is_char_boundary(start) || !new.is_char_boundary(start)) {
            start -= 1;
        }

        let mut old_end = old.len();
        let mut new_end = new.len();
        while old_end > start && new_end > start && old_bytes[old_end - 1] == new_bytes[new_end - 1]
        {
            old_end -= 1;
            new_end -= 1;
        }
        while old_end < old.len() && !old.is_char_boundary(old_end) {
            old_end += 1;
        }
        while new_end < new.len() && !new.is_char_boundary(new_end) {
            new_end += 1;
        }

        Some(Self {
            range: start..old_end,
            removed: old[start..old_end].to_owned(),
            inserted: new[start..new_end].to_owned(),
        })
    }

    /// Apply only if the expected old range still matches; failure is atomic.
    #[cfg(test)]
    pub(crate) fn apply(&self, text: &mut String) -> bool {
        if self.range.start > self.range.end
            || self.range.end > text.len()
            || !text.is_char_boundary(self.range.start)
            || !text.is_char_boundary(self.range.end)
            || text.get(self.range.clone()) != Some(self.removed.as_str())
        {
            return false;
        }
        text.replace_range(self.range.clone(), &self.inserted);
        true
    }

    #[cfg(test)]
    fn inverse(&self) -> Self {
        Self {
            range: self.range.start..self.range.start + self.inserted.len(),
            removed: self.inserted.clone(),
            inserted: self.removed.clone(),
        }
    }
}

/// A local history over edits, independent of source/file versions.
#[cfg(test)]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct EditHistory {
    edits: Vec<TextEdit>,
    cursor: usize,
}

#[cfg(test)]
impl EditHistory {
    pub(crate) fn record(&mut self, edit: TextEdit) {
        self.edits.truncate(self.cursor);
        self.edits.push(edit);
        self.cursor = self.edits.len();
    }

    pub(crate) fn undo(&mut self, text: &mut String) -> bool {
        let Some(edit) = self.edits.get(self.cursor.wrapping_sub(1)) else {
            return false;
        };
        if self.cursor == 0 || !edit.inverse().apply(text) {
            return false;
        }
        self.cursor -= 1;
        true
    }

    pub(crate) fn redo(&mut self, text: &mut String) -> bool {
        let Some(edit) = self.edits.get(self.cursor) else {
            return false;
        };
        if !edit.apply(text) {
            return false;
        }
        self.cursor += 1;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn textarea_offsets_convert_to_utf16_line_columns() {
        let text = "a😀total\nβx";
        assert_eq!(utf16_offset_to_line_col(text, 0), Some((0, 0)));
        assert_eq!(utf16_offset_to_line_col(text, 1), Some((0, 1)));
        assert_eq!(utf16_offset_to_line_col(text, 3), Some((0, 3)));
        assert_eq!(utf16_offset_to_line_col(text, 5), Some((0, 5)));
        assert_eq!(utf16_offset_to_line_col(text, 9), Some((1, 0)));
        assert_eq!(utf16_offset_to_line_col(text, 11), Some((1, 2)));
        assert_eq!(utf16_offset_to_line_col(text, 12), None);
    }

    #[test]
    fn offsets_inside_surrogate_pairs_snap_to_scalar_start() {
        assert_eq!(utf16_offset_to_line_col("😀x", 1), Some((0, 0)));
    }

    #[test]
    fn computes_insert_replace_delete_and_noop() {
        let insert = TextEdit::between("abcd", "abXcd").unwrap();
        assert_eq!(insert.range, 2..2);
        assert_eq!(insert.removed, "");
        assert_eq!(insert.inserted, "X");

        let replace = TextEdit::between("abcd", "abXYd").unwrap();
        assert_eq!(replace.range, 2..3);
        assert_eq!(replace.removed, "c");
        assert_eq!(replace.inserted, "XY");

        let delete = TextEdit::between("abcd", "abd").unwrap();
        assert_eq!(delete.range, 2..3);
        assert_eq!(delete.removed, "c");
        assert_eq!(delete.inserted, "");
        assert!(TextEdit::between("same", "same").is_none());
    }

    #[test]
    fn changes_keep_utf8_character_boundaries() {
        let edit = TextEdit::between("a😀b", "a😀é").unwrap();
        assert_eq!(edit.range, 5..6);
        assert_eq!(edit.removed, "b");
        assert_eq!(edit.inserted, "é");

        let edit = TextEdit::between("a😀b", "aX😀b").unwrap();
        assert_eq!(edit.range, 1..1);
        assert_eq!(edit.inserted, "X");

        let edit = TextEdit::between("cafe\u{301}!", "café!").unwrap();
        let mut text = "cafe\u{301}!".to_owned();
        assert!(edit.apply(&mut text));
        assert_eq!(text, "café!");
    }

    #[test]
    fn rejects_stale_and_invalid_ranges_without_partial_mutation() {
        let mut text = "changed".to_owned();
        let edit = TextEdit::between("abc", "axc").unwrap();
        assert!(!edit.apply(&mut text));
        assert_eq!(text, "changed");

        let invalid = TextEdit {
            range: 1..2,
            removed: "x".into(),
            inserted: "y".into(),
        };
        assert!(!invalid.apply(&mut text));
        assert_eq!(text, "changed");
    }

    #[test]
    fn history_undoes_redoes_and_discards_a_redo_branch() {
        let mut text = "abc".to_owned();
        let mut history = EditHistory::default();
        let first = TextEdit::between(&text, "aXbc").unwrap();
        assert!(first.apply(&mut text));
        history.record(first);
        let second = TextEdit::between(&text, "aXYbc").unwrap();
        assert!(second.apply(&mut text));
        history.record(second);
        assert_eq!(text, "aXYbc");

        assert!(history.undo(&mut text));
        assert!(history.undo(&mut text));
        assert_eq!(text, "abc");
        assert!(!history.undo(&mut text));
        assert!(history.redo(&mut text));
        assert_eq!(text, "aXbc");

        let branch = TextEdit::between(&text, "aZbc").unwrap();
        assert!(branch.apply(&mut text));
        history.record(branch);
        assert_eq!(text, "aZbc");
        assert!(!history.redo(&mut text));
    }

    #[test]
    fn history_round_trips_unicode() {
        let mut text = "start 😀 end".to_owned();
        let mut history = EditHistory::default();
        let edit = TextEdit::between(&text, "start 🦀 end").unwrap();
        assert!(edit.apply(&mut text));
        history.record(edit);
        assert_eq!(text, "start 🦀 end");
        assert!(history.undo(&mut text));
        assert_eq!(text, "start 😀 end");
        assert!(history.redo(&mut text));
        assert_eq!(text, "start 🦀 end");
    }

    #[test]
    fn history_undoes_and_redoes_unicode_prefix_insertion() {
        let mut text = "fn main() {}".to_owned();
        let original = text.clone();
        let mut history = EditHistory::default();
        let edit = TextEdit::between(&text, "λ😀fn main() {}").unwrap();
        assert!(edit.apply(&mut text));
        history.record(edit);
        assert!(history.undo(&mut text));
        assert_eq!(text, original);
        assert!(history.redo(&mut text));
        assert_eq!(text, "λ😀fn main() {}");
    }
}
