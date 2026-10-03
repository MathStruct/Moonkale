//! An open text document. Owned by the [`crate::Workspace`], rendered by an
//! editor extension — the Rust-owned truth of ADR-0008.

use moonkale_core::{Node, TextPatch, Version};

/// An open document: the node, its text as edited, and the version it was read at. Owned by the workspace ([`crate::Workspace::document`]); editors are views on it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Document {
    /// The node it edits.
    pub node: Node,
    /// The text as last loaded or saved.
    pub saved: String,
    /// The text as currently edited.
    pub text: String,
    /// The source's version when the text was read or last saved (conflict detection).
    pub version: Version,
}

impl Document {
    /// A document whose text is as saved.
    pub fn new(node: Node, text: String, version: Version) -> Self {
        Self {
            node,
            saved: text.clone(),
            text,
            version,
        }
    }

    /// Whether the text differs from what was saved.
    pub fn dirty(&self) -> bool {
        self.text != self.saved
    }

    /// The patch that turns `saved` into `text`. Milestone 1 sends the whole
    /// document; a diff-based patch is a drop-in replacement here.
    pub fn patch(&self) -> TextPatch {
        TextPatch::whole(self.text.clone(), self.saved.chars().count())
    }

    /// Record a successful save.
    pub fn mark_saved(&mut self, version: Version) {
        self.saved = self.text.clone();
        self.version = version;
    }
}
