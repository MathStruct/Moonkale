//! Anchored offsets that stay stable under text edits.
//!
//! Many editor features need to remember locations in the document (bookmarks, marks, jump list,
//! snippets, …) and have those locations update when the document is edited.
//!
//! This module provides a small, UI-agnostic anchored offset type ([`TextAnchor`]) plus rules for
//! shifting it through [`crate::TextDelta`] edits.

use crate::TextDelta;

/// Bias controls how an anchor behaves when text is inserted **exactly at** the anchor offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AnchorBias {
    /// Keep the anchor at the original offset (anchor stays **before** inserted text).
    Left,
    /// Move the anchor to the end of inserted text (anchor stays **after** inserted text).
    #[default]
    Right,
}

/// A character-offset anchor that shifts through edits.
///
/// Offsets are expressed as Unicode scalar indices (Rust `char` offsets), consistent with the
/// rest of `editor-core`’s public API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextAnchor {
    /// Character offset in the document.
    pub offset: usize,
    /// Tie-break behavior for insertions at the same offset.
    pub bias: AnchorBias,
}

impl TextAnchor {
    /// Create a new anchor.
    pub fn new(offset: usize, bias: AnchorBias) -> Self {
        Self { offset, bias }
    }

    /// Shift this anchor through a text delta.
    pub fn apply_delta(&mut self, delta: &TextDelta) {
        let mut offset = self.offset;
        for edit in &delta.edits {
            let start = edit.start;
            let end = edit.end();
            let deleted_len = edit.deleted_len();
            let inserted_len = edit.inserted_len();

            if offset < start {
                continue;
            }

            if deleted_len == 0 && offset == start {
                // Pure insertion at the anchor point.
                if self.bias == AnchorBias::Right {
                    offset = start.saturating_add(inserted_len);
                }
                continue;
            }

            if offset < end {
                // Inside a replaced range: clamp to start or end of inserted text based on bias.
                offset = match self.bias {
                    AnchorBias::Left => start,
                    AnchorBias::Right => start.saturating_add(inserted_len),
                };
                continue;
            }

            // After the replaced range: shift by net length delta.
            if inserted_len >= deleted_len {
                offset = offset.saturating_add(inserted_len - deleted_len);
            } else {
                offset = offset.saturating_sub(deleted_len - inserted_len);
            }
        }

        self.offset = offset.min(delta.after_char_count);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TextDeltaEdit;

    fn delta_insert(at: usize, text: &str) -> TextDelta {
        TextDelta {
            before_char_count: 0,
            after_char_count: text.chars().count(),
            edits: vec![TextDeltaEdit {
                start: at,
                deleted_text: String::new(),
                inserted_text: text.to_string(),
            }],
            undo_group_id: None,
        }
    }

    #[test]
    fn insertion_bias_left_stays_before_inserted() {
        let mut a = TextAnchor::new(0, AnchorBias::Left);
        a.apply_delta(&delta_insert(0, "abc"));
        assert_eq!(a.offset, 0);
    }

    #[test]
    fn insertion_bias_right_moves_after_inserted() {
        let mut a = TextAnchor::new(0, AnchorBias::Right);
        a.apply_delta(&delta_insert(0, "abc"));
        assert_eq!(a.offset, 3);
    }
}
