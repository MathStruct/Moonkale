//! First-class decorations (virtual text) data model.
//!
//! Decorations represent UI-facing annotations anchored to document character offsets, without
//! modifying the document text. Common examples:
//!
//! - LSP inlay hints (inline type hints)
//! - code lens (line-level virtual text)
//! - document links
//! - match highlights / bracket highlights (when not represented as pure style intervals)
//!
//! Decorations are derived editor state. They typically originate from integrations via
//! [`ProcessingEdit`](crate::processing::ProcessingEdit) and are rendered by the host.

use crate::intervals::StyleId;

/// A source/layer identifier for decorations.
///
/// This mirrors `StyleLayerId`, but for non-text, non-style derived state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DecorationLayerId(pub u32);

impl DecorationLayerId {
    /// Decorations produced from LSP `textDocument/inlayHint`.
    pub const INLAY_HINTS: Self = Self(1);
    /// Decorations produced from LSP `textDocument/codeLens`.
    pub const CODE_LENS: Self = Self(2);
    /// Decorations representing document links (e.g. URIs, file paths).
    pub const DOCUMENT_LINKS: Self = Self(3);
    /// Decorations representing match highlights (search matches, bracket matches, etc.).
    pub const MATCH_HIGHLIGHTS: Self = Self(4);

    /// Create a new layer id.
    pub fn new(id: u32) -> Self {
        Self(id)
    }
}

/// A half-open character-offset range (`start..end`) in the document.
///
/// For point-anchored decorations, use `start == end`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecorationRange {
    /// Range start offset (inclusive), in Unicode scalar values (`char`) from the start of the document.
    pub start: usize,
    /// Range end offset (exclusive), in Unicode scalar values (`char`) from the start of the document.
    pub end: usize,
}

impl DecorationRange {
    /// Create a new decoration range.
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

/// Where to render a decoration relative to its anchor range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecorationPlacement {
    /// Render before the anchor (in logical order).
    Before,
    /// Render after the anchor (in logical order).
    After,
    /// Render above the anchor line (e.g. code lens).
    AboveLine,
}

/// A coarse decoration kind tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DecorationKind {
    /// Inline inlay hint (usually from LSP).
    InlayHint,
    /// Code lens (usually above a line).
    CodeLens,
    /// Document link (clickable range).
    DocumentLink,
    /// Highlight decoration (e.g. match/bracket highlights).
    Highlight,
    /// A custom, integration-defined kind.
    Custom(u32),
}

/// A single decoration item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoration {
    /// Anchor range in character offsets.
    pub range: DecorationRange,
    /// Relative placement (before/after/above).
    pub placement: DecorationPlacement,
    /// A coarse decoration kind.
    pub kind: DecorationKind,
    /// Optional virtual text to render.
    pub text: Option<String>,
    /// Optional style ids to apply when rendering this decoration.
    pub styles: Vec<StyleId>,
    /// Optional tooltip payload (plain text; markup is host-defined).
    pub tooltip: Option<String>,
    /// Optional integration-specific payload (JSON text).
    pub data_json: Option<String>,
}
