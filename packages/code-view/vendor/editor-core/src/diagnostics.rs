//! First-class diagnostics data model.
//!
//! This module stores structured diagnostics (errors/warnings/hints) as derived editor state.
//! Renderers can use this for:
//! - problems panels / gutter markers
//! - hover tooltips / inline messages
//! - mapping diagnostics back to style layers (underlines)

/// A half-open character-offset range (`start..end`) in the document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiagnosticRange {
    /// Range start offset (inclusive), in Unicode scalar values (`char`) from the start of the document.
    pub start: usize,
    /// Range end offset (exclusive), in Unicode scalar values (`char`) from the start of the document.
    pub end: usize,
}

impl DiagnosticRange {
    /// Create a new diagnostic range.
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

/// Diagnostic severity levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    /// Error diagnostics.
    Error,
    /// Warning diagnostics.
    Warning,
    /// Informational diagnostics.
    Information,
    /// Hint diagnostics.
    Hint,
}

/// A single diagnostic item for the current document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// Diagnostic range in character offsets.
    pub range: DiagnosticRange,
    /// Optional diagnostic severity.
    pub severity: Option<DiagnosticSeverity>,
    /// Optional diagnostic code (stringified).
    pub code: Option<String>,
    /// Optional diagnostic source (e.g. `"rust-analyzer"`).
    pub source: Option<String>,
    /// Diagnostic message.
    pub message: String,
    /// Optional related information payload, encoded as JSON text (if provided by an integration).
    pub related_information_json: Option<String>,
    /// Optional extra data payload, encoded as JSON text (if provided by an integration).
    pub data_json: Option<String>,
}
