//! Command data model types.

use crate::intervals::StyleId;
use crate::layout::{WrapIndent, WrapMode};
use crate::search::SearchOptions;
use crate::snapshot::HeadlessGrid;
use editor_core_lang::{CommentConfig, IndentationConfig};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

const COMMAND_HISTORY_TEXT_PREVIEW_BYTES: usize = 256;

/// Position coordinates (line and column numbers)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Position {
    /// Zero-based logical line index.
    pub line: usize,
    /// Zero-based column in characters within the logical line.
    pub column: usize,
}

impl Position {
    /// Create a new logical position.
    pub fn new(line: usize, column: usize) -> Self {
        Self { line, column }
    }
}

impl Ord for Position {
    fn cmp(&self, other: &Self) -> Ordering {
        self.line
            .cmp(&other.line)
            .then_with(|| self.column.cmp(&other.column))
    }
}

impl PartialOrd for Position {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Selection range
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Selection {
    /// Selection start position
    pub start: Position,
    /// Selection end position
    pub end: Position,
    /// Selection direction
    pub direction: SelectionDirection,
}

/// Selection direction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum SelectionDirection {
    /// Forward selection (from start to end)
    Forward,
    /// Backward selection (from end to start)
    Backward,
}

/// Selection expansion unit for [`CursorCommand::ExpandSelectionBy`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpandSelectionUnit {
    /// Expand by Unicode scalar values (Rust `char` indices).
    Character,
    /// Expand by "word" units (configured word boundary rules).
    Word,
    /// Expand by logical lines.
    Line,
}

/// Selection expansion direction for [`CursorCommand::ExpandSelectionBy`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpandSelectionDirection {
    /// Expand towards the beginning of the document.
    Backward,
    /// Expand towards the end of the document.
    Forward,
}

/// Controls how a Tab key press is handled by the editor when using [`EditCommand::InsertTab`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabKeyBehavior {
    /// Insert a literal tab character (`'\t'`).
    Tab,
    /// Insert spaces up to the next tab stop (based on the current `tab_width` setting).
    Spaces,
}

/// A single auto-pair entry (opening + closing delimiter).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct AutoPair {
    /// Opening delimiter.
    pub open: char,
    /// Closing delimiter.
    pub close: char,
}

impl AutoPair {
    /// Create a new auto-pair entry.
    pub const fn new(open: char, close: char) -> Self {
        Self { open, close }
    }
}

/// Auto-pairs configuration used by [`EditCommand::TypeChar`], and optionally by delete-like
/// commands (pair deletion).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct AutoPairsConfig {
    /// Master enable switch for auto-pairs behaviors.
    pub enabled: bool,
    /// Configured delimiter pairs (order matters when overlapping; first match wins).
    pub pairs: Vec<AutoPair>,
    /// When typing an opening delimiter over a non-empty selection, wrap the selection.
    pub wrap_selection: bool,
    /// When typing a closing delimiter and the next character matches, skip over it instead of inserting.
    pub skip_over_closing: bool,
    /// When backspacing/deleting adjacent matching delimiters, delete both.
    pub delete_pair: bool,
}

impl Default for AutoPairsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            pairs: vec![
                AutoPair::new('(', ')'),
                AutoPair::new('[', ']'),
                AutoPair::new('{', '}'),
                AutoPair::new('"', '"'),
                AutoPair::new('\'', '\''),
                AutoPair::new('`', '`'),
            ],
            wrap_selection: true,
            skip_over_closing: true,
            delete_pair: true,
        }
    }
}

impl AutoPairsConfig {
    pub(super) fn close_for_open(&self, open: char) -> Option<char> {
        self.pairs.iter().find(|p| p.open == open).map(|p| p.close)
    }

    pub(super) fn open_for_close(&self, close: char) -> Option<char> {
        self.pairs.iter().find(|p| p.close == close).map(|p| p.open)
    }

    pub(super) fn is_matching_pair(&self, open: char, close: char) -> bool {
        self.pairs
            .iter()
            .any(|p| p.open == open && p.close == close)
    }
}

/// A simple document text edit (character offsets, half-open).
///
/// This is commonly used for applying a batch of "simultaneous" edits (e.g. rename, refactor, or
/// workspace-wide search/replace), where the edit list is expressed in **pre-edit** coordinates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEditSpec {
    /// Inclusive start character offset.
    pub start: usize,
    /// Exclusive end character offset.
    pub end: usize,
    /// Replacement text.
    pub text: String,
}

impl TextEditSpec {
    fn history_summary(&self) -> Self {
        Self {
            start: self.start,
            end: self.end,
            text: summarize_history_text(&self.text),
        }
    }
}

fn summarize_history_text(text: &str) -> String {
    if text.len() <= COMMAND_HISTORY_TEXT_PREVIEW_BYTES {
        return text.to_string();
    }

    let mut preview_end = 0;
    for (byte_idx, ch) in text.char_indices() {
        let next = byte_idx + ch.len_utf8();
        if next > COMMAND_HISTORY_TEXT_PREVIEW_BYTES {
            break;
        }
        preview_end = next;
    }

    format!(
        "{}...[history truncated: {} bytes]",
        &text[..preview_end],
        text.len()
    )
}

/// Text editing commands
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditCommand {
    /// Insert text at the specified position
    Insert {
        /// Character offset to insert at.
        offset: usize,
        /// Text to insert.
        text: String,
    },
    /// Delete text in specified range
    Delete {
        /// Character offset of the deletion start.
        start: usize,
        /// Length of the deletion in characters.
        length: usize,
    },
    /// Replace text in specified range
    Replace {
        /// Character offset of the replacement start.
        start: usize,
        /// Length of the replaced range in characters.
        length: usize,
        /// Replacement text.
        text: String,
    },
    /// Replace text in specified range, requesting undo coalescing when the edit is eligible.
    ///
    /// This is primarily useful for UI layers that need to keep IME composition updates and the
    /// final commit in one explicitly delimited undo group.
    ///
    /// Notes:
    /// - Normal typing coalescing is intentionally limited to pure adjacent insertions without
    ///   newlines.
    /// - This explicit composition path may coalesce replacements only when each update replaces
    ///   the exact range inserted by the previous update and the selection state is continuous.
    /// - The caller is expected to explicitly delimit boundaries via [`EditCommand::EndUndoGroup`]
    ///   so eligible IME insertions do not merge with normal typing groups.
    ReplaceCoalescingUndo {
        /// Character offset of the replacement start.
        start: usize,
        /// Length of the replaced range in characters.
        length: usize,
        /// Replacement text.
        text: String,
    },
    /// Like [`EditCommand::ReplaceCoalescingUndo`], but also sets the primary selection/caret.
    ///
    /// This is primarily useful for IME composition updates where the host provides a
    /// selection range inside the marked (preedit) string while keeping composition updates in
    /// one explicitly delimited undo group.
    ///
    /// Notes:
    /// - `selection_start/selection_end` are **post-edit** character offsets (Unicode scalar indices)
    ///   in the resulting document.
    /// - If `selection_start == selection_end`, the selection is cleared and the caret is moved
    ///   to `selection_end`.
    ReplaceCoalescingUndoWithSelection {
        /// Character offset of the replacement start.
        start: usize,
        /// Length of the replaced range in characters.
        length: usize,
        /// Replacement text.
        text: String,
        /// Selection start (post-edit) in character offsets.
        selection_start: usize,
        /// Selection end (post-edit) in character offsets.
        selection_end: usize,
    },
    /// VSCode-like typing/paste: apply to all carets/selections (primary + secondary)
    InsertText {
        /// Text to insert/replace at each selection/caret.
        text: String,
    },
    /// Type a single character using auto-pairs rules (if enabled).
    ///
    /// This is intended for UI "typing" paths (not paste). It supports:
    /// - auto-close pairs (`()`, `{}`, `[]`, quotes)
    /// - skip over existing closing delimiters
    /// - wrap selection with pairs (optional)
    TypeChar {
        /// The typed character.
        ch: char,
    },
    /// Insert a tab at each caret (or replace each selection), using the current tab settings.
    ///
    /// - If `TabKeyBehavior::Tab`, inserts `'\t'`.
    /// - If `TabKeyBehavior::Spaces`, inserts spaces up to the next tab stop.
    InsertTab,
    /// Insert a newline at each caret (or replace each selection).
    ///
    /// If `auto_indent` is true, the inserted newline is followed by the leading whitespace
    /// prefix of the current logical line.
    InsertNewline {
        /// Whether to auto-indent the new line.
        auto_indent: bool,
    },
    /// Indent the selected lines (or the current line for an empty selection).
    Indent,
    /// Outdent the selected lines (or the current line for an empty selection).
    Outdent,
    /// Duplicate the selected line(s) (or the current line for an empty selection).
    ///
    /// This is a line-based operation and will act on all carets/selections (primary + secondary),
    /// including rectangular selections.
    DuplicateLines,
    /// Delete the selected line(s) (or the current line for an empty selection).
    ///
    /// This is a line-based operation and will act on all carets/selections (primary + secondary),
    /// including rectangular selections.
    DeleteLines,
    /// Move the selected line(s) up by one line.
    ///
    /// This is a line-based operation and will act on all carets/selections (primary + secondary),
    /// including rectangular selections.
    MoveLinesUp,
    /// Move the selected line(s) down by one line.
    ///
    /// This is a line-based operation and will act on all carets/selections (primary + secondary),
    /// including rectangular selections.
    MoveLinesDown,
    /// Join the current line with the next line (for each caret/selection).
    ///
    /// If multiple carets/selections exist, joins are applied from bottom to top to keep offsets stable.
    JoinLines,
    /// Split the current line at each caret (or replace each selection) by inserting a newline.
    ///
    /// This is a convenience alias for [`EditCommand::InsertNewline`] with `auto_indent: false`.
    SplitLine,
    /// Toggle comments for the selected line(s) or selection ranges, using a language-provided
    /// comment configuration.
    ToggleComment {
        /// Comment tokens/config for the current language (data-driven).
        config: CommentConfig,
    },
    /// Apply a batch of text edits as a single undoable step.
    ///
    /// - Edits are interpreted in **pre-edit** character offsets.
    /// - Edits must be non-overlapping; they are applied in descending offset order internally.
    ApplyTextEdits {
        /// The edit list (character offsets, half-open).
        edits: Vec<TextEditSpec>,
    },
    /// Apply a snippet-shaped insert as a single undoable step.
    ///
    /// This is primarily intended for LSP completion items with `insertTextFormat == 2`.
    ///
    /// - `start`/`end` are interpreted in **pre-edit** character offsets (half-open).
    /// - `additional_edits` are applied in the same undo step (also in pre-edit coordinates).
    /// - The snippet is expanded (placeholders removed / defaults inserted), and the first
    ///   placeholder (lowest index) is selected for navigation via
    ///   [`CursorCommand::SnippetNextPlaceholder`] / [`CursorCommand::SnippetPrevPlaceholder`].
    ApplySnippet {
        /// Inclusive start character offset.
        start: usize,
        /// Exclusive end character offset.
        end: usize,
        /// Snippet text in TextMate / VS Code snippet syntax.
        snippet: String,
        /// Additional text edits (LSP `additionalTextEdits`), in pre-edit coordinates.
        additional_edits: Vec<TextEditSpec>,
    },
    /// Smart backspace: if the caret is in leading whitespace, delete back to the previous tab stop.
    ///
    /// Otherwise, behaves like [`EditCommand::Backspace`].
    DeleteToPrevTabStop,
    /// Delete the previous Unicode grapheme cluster (UAX #29) for each caret/selection.
    DeleteGraphemeBack,
    /// Delete the next Unicode grapheme cluster (UAX #29) for each caret/selection.
    DeleteGraphemeForward,
    /// Delete back to the previous Unicode word boundary (UAX #29) for each caret/selection.
    DeleteWordBack,
    /// Delete forward to the next Unicode word boundary (UAX #29) for each caret/selection.
    DeleteWordForward,
    /// Backspace-like deletion: delete selection(s) if any, otherwise delete 1 char before each caret.
    Backspace,
    /// Delete key-like deletion: delete selection(s) if any, otherwise delete 1 char after each caret.
    DeleteForward,
    /// Undo last edit operation (supports grouping)
    Undo,
    /// Redo last undone operation (supports grouping)
    Redo,
    /// Explicitly end the current undo group (for idle or external boundaries)
    EndUndoGroup,
    /// Replace the current occurrence of `query` (based on selection/caret) with `replacement`.
    ///
    /// - Honors `options` (case sensitivity / whole-word / regex).
    /// - Treated as a single undoable edit.
    ReplaceCurrent {
        /// Search query.
        query: String,
        /// Replacement text.
        replacement: String,
        /// Search options (case sensitivity, whole-word, regex).
        options: SearchOptions,
    },
    /// Replace all occurrences of `query` with `replacement`.
    ///
    /// - Honors `options` (case sensitivity / whole-word / regex).
    /// - Treated as a single undoable edit.
    ReplaceAll {
        /// Search query.
        query: String,
        /// Replacement text.
        replacement: String,
        /// Search options (case sensitivity, whole-word, regex).
        options: SearchOptions,
    },
}

/// Cursor & selection commands
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CursorCommand {
    /// Move cursor to the specified position
    MoveTo {
        /// Target logical line index.
        line: usize,
        /// Target column in characters (will be clamped to line length).
        column: usize,
    },
    /// Move cursor relatively
    MoveBy {
        /// Delta in logical lines.
        delta_line: isize,
        /// Delta in columns (characters).
        delta_column: isize,
    },
    /// Move cursor by visual rows (soft wrap + folding aware).
    ///
    /// This uses a "preferred x" in **cells** (sticky column) similar to many editors:
    /// horizontal moves update preferred x, while vertical visual moves try to preserve it.
    MoveVisualBy {
        /// Delta in global visual rows (after wrapping/folding).
        delta_rows: isize,
    },
    /// Move cursor to a visual position (global visual row + x in cells).
    MoveToVisual {
        /// Target global visual row (after wrapping/folding).
        row: usize,
        /// Target x offset in cells within that visual row.
        x_cells: usize,
    },
    /// Move cursor to the start of the current logical line.
    MoveToLineStart,
    /// Move cursor to the end of the current logical line.
    MoveToLineEnd,
    /// Move cursor to the start of the current visual line segment (wrap-aware).
    MoveToVisualLineStart,
    /// Move cursor to the end of the current visual line segment (wrap-aware).
    MoveToVisualLineEnd,
    /// Move cursor left by one Unicode grapheme cluster (UAX #29).
    MoveGraphemeLeft,
    /// Move cursor right by one Unicode grapheme cluster (UAX #29).
    MoveGraphemeRight,
    /// Move cursor left to the previous Unicode word boundary (UAX #29).
    MoveWordLeft,
    /// Move cursor right to the next Unicode word boundary (UAX #29).
    MoveWordRight,
    /// Move each caret to its matching bracket (if the caret is on or adjacent to a bracket).
    ///
    /// Matching is performed for the configured bracket pairs (typically `()`, `[]`, `{}`).
    MoveToMatchingBracket,
    /// If a snippet session is active, jump to the **next** snippet tabstop (placeholder).
    ///
    /// This is typically bound to the Tab key while a completion snippet is active.
    SnippetNextPlaceholder,
    /// If a snippet session is active, jump to the **previous** snippet tabstop (placeholder).
    ///
    /// This is typically bound to Shift-Tab while a completion snippet is active.
    SnippetPrevPlaceholder,
    /// Set selection range
    SetSelection {
        /// Selection start position.
        start: Position,
        /// Selection end position.
        end: Position,
    },
    /// Extend selection range
    ExtendSelection {
        /// New active end position.
        to: Position,
    },
    /// Clear selection
    ClearSelection,
    /// Set multiple selections/multi-cursor (including primary)
    SetSelections {
        /// All selections (including primary).
        selections: Vec<Selection>,
        /// Index of the primary selection in `selections`.
        primary_index: usize,
    },
    /// Clear secondary selections/cursors, keeping only primary
    ClearSecondarySelections,
    /// Set rectangular selection (box/column selection), which expands into one Selection per line
    SetRectSelection {
        /// Anchor position (fixed corner).
        anchor: Position,
        /// Active position (moving corner).
        active: Position,
    },
    /// Select the entire current line (or the set of lines covered by the selection), for all carets.
    SelectLine,
    /// Select the word under each caret (or keep existing selections if already non-empty).
    SelectWord,
    /// Expand selection in a basic, editor-friendly way.
    ///
    /// - If the selection is empty, expands to the word under the caret.
    /// - If the selection is non-empty, expands to full line(s).
    ExpandSelection,
    /// Expand selection by a configurable unit and direction.
    ///
    /// Notes:
    /// - This is an **expand-only** operation: it never shrinks the current selection.
    /// - The expansion direction is absolute (backward/forward in document order). If you call
    ///   it with different directions over time, the selection can expand on both ends.
    ExpandSelectionBy {
        /// Expansion unit.
        unit: ExpandSelectionUnit,
        /// Number of units to expand by. `0` is a no-op.
        count: usize,
        /// Expansion direction in document order.
        direction: ExpandSelectionDirection,
    },
    /// Add a new caret above each existing caret/selection (at the same column, clamped to line length).
    AddCursorAbove,
    /// Add a new caret below each existing caret/selection (at the same column, clamped to line length).
    AddCursorBelow,
    /// Multi-cursor match op: add the next occurrence of the current selection/word as a new selection.
    AddNextOccurrence {
        /// Search options (case sensitivity, whole-word, regex).
        options: SearchOptions,
    },
    /// Multi-cursor match op: select all occurrences of the current selection/word.
    AddAllOccurrences {
        /// Search options (case sensitivity, whole-word, regex).
        options: SearchOptions,
    },
    /// Find the next occurrence of `query` and select it (primary selection only).
    FindNext {
        /// Search query.
        query: String,
        /// Search options (case sensitivity, whole-word, regex).
        options: SearchOptions,
    },
    /// Find the previous occurrence of `query` and select it (primary selection only).
    FindPrev {
        /// Search query.
        query: String,
        /// Search options (case sensitivity, whole-word, regex).
        options: SearchOptions,
    },
}

/// View commands
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewCommand {
    /// Set viewport width
    SetViewportWidth {
        /// Width in character cells.
        width: usize,
    },
    /// Set soft wrap mode.
    SetWrapMode {
        /// Wrap mode.
        mode: WrapMode,
    },
    /// Set wrapped-line indentation policy.
    SetWrapIndent {
        /// Wrap indent policy.
        indent: WrapIndent,
    },
    /// Set tab width (in character cells) used for measuring `'\t'` and tab stops.
    SetTabWidth {
        /// Tab width in character cells (must be greater than 0).
        width: usize,
    },
    /// Configure how [`EditCommand::InsertTab`] inserts text.
    SetTabKeyBehavior {
        /// Tab key behavior.
        behavior: TabKeyBehavior,
    },
    /// Configure language-aware auto-indentation behavior used by [`EditCommand::InsertNewline`]
    /// when `auto_indent=true`.
    ///
    /// Notes:
    /// - This is view-local (each view can have different indentation rules).
    SetIndentationConfig {
        /// Indentation configuration.
        config: IndentationConfig,
    },
    /// Configure auto-pairs behavior used by [`EditCommand::TypeChar`].
    SetAutoPairsConfig {
        /// Auto-pairs config.
        config: AutoPairsConfig,
    },
    /// Enable/disable auto-pairs behavior (convenience wrapper over [`ViewCommand::SetAutoPairsConfig`]).
    SetAutoPairsEnabled {
        /// Whether auto-pairs are enabled.
        enabled: bool,
    },
    /// Override the ASCII word-boundary character set used by editor-friendly "word" operations.
    ///
    /// This is similar in spirit to VSCode's `wordSeparators`.
    ///
    /// Notes:
    /// - Only ASCII characters are configurable here; non-ASCII characters are always treated as boundaries.
    /// - ASCII whitespace is always treated as a boundary.
    SetWordBoundaryAsciiBoundaryChars {
        /// ASCII word-boundary characters (as a string of separators).
        boundary_chars: String,
    },
    /// Reset word-boundary configuration to the default (ASCII identifier-like words).
    ResetWordBoundaryDefaults,
    /// Scroll to specified line
    ScrollTo {
        /// Logical line index to scroll to.
        line: usize,
    },
    /// Get current viewport content
    GetViewport {
        /// Starting visual row.
        start_row: usize,
        /// Number of visual rows requested.
        count: usize,
    },
}

/// Style and folding commands
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StyleCommand {
    /// Add style interval
    AddStyle {
        /// Interval start offset in characters.
        start: usize,
        /// Interval end offset in characters (exclusive).
        end: usize,
        /// Style identifier.
        style_id: StyleId,
    },
    /// Remove style interval
    RemoveStyle {
        /// Interval start offset in characters.
        start: usize,
        /// Interval end offset in characters (exclusive).
        end: usize,
        /// Style identifier.
        style_id: StyleId,
    },
    /// Fold code block
    Fold {
        /// Start logical line (inclusive).
        start_line: usize,
        /// End logical line (inclusive).
        end_line: usize,
    },
    /// Unfold code block
    Unfold {
        /// Start logical line (inclusive) of the fold region to unfold.
        start_line: usize,
    },
    /// Unfold all folds
    UnfoldAll,
    /// Recompute bracket-match highlights for the current cursor/selections.
    ///
    /// This updates the derived style layer [`StyleLayerId::BRACKET_MATCHES`].
    UpdateBracketMatchHighlights,
    /// Clear bracket-match highlights (removes [`StyleLayerId::BRACKET_MATCHES`]).
    ClearBracketMatchHighlights,
}

/// Unified command enum
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Text editing commands
    Edit(EditCommand),
    /// Cursor command
    Cursor(CursorCommand),
    /// View commands
    View(ViewCommand),
    /// Style command
    Style(StyleCommand),
}

impl EditCommand {
    fn is_mutating(&self) -> bool {
        match self {
            EditCommand::Insert { .. }
            | EditCommand::Delete { .. }
            | EditCommand::Replace { .. }
            | EditCommand::ReplaceCoalescingUndo { .. }
            | EditCommand::ReplaceCoalescingUndoWithSelection { .. }
            | EditCommand::InsertText { .. }
            | EditCommand::TypeChar { .. }
            | EditCommand::InsertTab
            | EditCommand::InsertNewline { .. }
            | EditCommand::Indent
            | EditCommand::Outdent
            | EditCommand::DuplicateLines
            | EditCommand::DeleteLines
            | EditCommand::MoveLinesUp
            | EditCommand::MoveLinesDown
            | EditCommand::JoinLines
            | EditCommand::SplitLine
            | EditCommand::ToggleComment { .. }
            | EditCommand::ApplyTextEdits { .. }
            | EditCommand::ApplySnippet { .. }
            | EditCommand::DeleteToPrevTabStop
            | EditCommand::DeleteGraphemeBack
            | EditCommand::DeleteGraphemeForward
            | EditCommand::DeleteWordBack
            | EditCommand::DeleteWordForward
            | EditCommand::Backspace
            | EditCommand::DeleteForward
            | EditCommand::Undo
            | EditCommand::Redo
            | EditCommand::EndUndoGroup
            | EditCommand::ReplaceCurrent { .. }
            | EditCommand::ReplaceAll { .. } => true,
        }
    }

    fn history_summary(&self) -> Self {
        match self {
            EditCommand::Insert { offset, text } => EditCommand::Insert {
                offset: *offset,
                text: summarize_history_text(text),
            },
            EditCommand::Delete { start, length } => EditCommand::Delete {
                start: *start,
                length: *length,
            },
            EditCommand::Replace {
                start,
                length,
                text,
            } => EditCommand::Replace {
                start: *start,
                length: *length,
                text: summarize_history_text(text),
            },
            EditCommand::ReplaceCoalescingUndo {
                start,
                length,
                text,
            } => EditCommand::ReplaceCoalescingUndo {
                start: *start,
                length: *length,
                text: summarize_history_text(text),
            },
            EditCommand::ReplaceCoalescingUndoWithSelection {
                start,
                length,
                text,
                selection_start,
                selection_end,
            } => EditCommand::ReplaceCoalescingUndoWithSelection {
                start: *start,
                length: *length,
                text: summarize_history_text(text),
                selection_start: *selection_start,
                selection_end: *selection_end,
            },
            EditCommand::InsertText { text } => EditCommand::InsertText {
                text: summarize_history_text(text),
            },
            EditCommand::TypeChar { ch } => EditCommand::TypeChar { ch: *ch },
            EditCommand::InsertTab => EditCommand::InsertTab,
            EditCommand::InsertNewline { auto_indent } => EditCommand::InsertNewline {
                auto_indent: *auto_indent,
            },
            EditCommand::Indent => EditCommand::Indent,
            EditCommand::Outdent => EditCommand::Outdent,
            EditCommand::DuplicateLines => EditCommand::DuplicateLines,
            EditCommand::DeleteLines => EditCommand::DeleteLines,
            EditCommand::MoveLinesUp => EditCommand::MoveLinesUp,
            EditCommand::MoveLinesDown => EditCommand::MoveLinesDown,
            EditCommand::JoinLines => EditCommand::JoinLines,
            EditCommand::SplitLine => EditCommand::SplitLine,
            EditCommand::ToggleComment { config } => EditCommand::ToggleComment {
                config: config.clone(),
            },
            EditCommand::ApplyTextEdits { edits } => EditCommand::ApplyTextEdits {
                edits: edits.iter().map(TextEditSpec::history_summary).collect(),
            },
            EditCommand::ApplySnippet {
                start,
                end,
                snippet,
                additional_edits,
            } => EditCommand::ApplySnippet {
                start: *start,
                end: *end,
                snippet: summarize_history_text(snippet),
                additional_edits: additional_edits
                    .iter()
                    .map(TextEditSpec::history_summary)
                    .collect(),
            },
            EditCommand::DeleteToPrevTabStop => EditCommand::DeleteToPrevTabStop,
            EditCommand::DeleteGraphemeBack => EditCommand::DeleteGraphemeBack,
            EditCommand::DeleteGraphemeForward => EditCommand::DeleteGraphemeForward,
            EditCommand::DeleteWordBack => EditCommand::DeleteWordBack,
            EditCommand::DeleteWordForward => EditCommand::DeleteWordForward,
            EditCommand::Backspace => EditCommand::Backspace,
            EditCommand::DeleteForward => EditCommand::DeleteForward,
            EditCommand::Undo => EditCommand::Undo,
            EditCommand::Redo => EditCommand::Redo,
            EditCommand::EndUndoGroup => EditCommand::EndUndoGroup,
            EditCommand::ReplaceCurrent {
                query,
                replacement,
                options,
            } => EditCommand::ReplaceCurrent {
                query: summarize_history_text(query),
                replacement: summarize_history_text(replacement),
                options: *options,
            },
            EditCommand::ReplaceAll {
                query,
                replacement,
                options,
            } => EditCommand::ReplaceAll {
                query: summarize_history_text(query),
                replacement: summarize_history_text(replacement),
                options: *options,
            },
        }
    }
}

impl CursorCommand {
    fn is_mutating(&self) -> bool {
        match self {
            CursorCommand::MoveTo { .. }
            | CursorCommand::MoveBy { .. }
            | CursorCommand::MoveVisualBy { .. }
            | CursorCommand::MoveToVisual { .. }
            | CursorCommand::MoveToLineStart
            | CursorCommand::MoveToLineEnd
            | CursorCommand::MoveToVisualLineStart
            | CursorCommand::MoveToVisualLineEnd
            | CursorCommand::MoveGraphemeLeft
            | CursorCommand::MoveGraphemeRight
            | CursorCommand::MoveWordLeft
            | CursorCommand::MoveWordRight
            | CursorCommand::MoveToMatchingBracket
            | CursorCommand::SnippetNextPlaceholder
            | CursorCommand::SnippetPrevPlaceholder
            | CursorCommand::SetSelection { .. }
            | CursorCommand::ExtendSelection { .. }
            | CursorCommand::ClearSelection
            | CursorCommand::SetSelections { .. }
            | CursorCommand::ClearSecondarySelections
            | CursorCommand::SetRectSelection { .. }
            | CursorCommand::SelectLine
            | CursorCommand::SelectWord
            | CursorCommand::ExpandSelection
            | CursorCommand::ExpandSelectionBy { .. }
            | CursorCommand::AddCursorAbove
            | CursorCommand::AddCursorBelow
            | CursorCommand::AddNextOccurrence { .. }
            | CursorCommand::AddAllOccurrences { .. }
            | CursorCommand::FindNext { .. }
            | CursorCommand::FindPrev { .. } => false,
        }
    }

    fn history_summary(&self) -> Self {
        match self {
            CursorCommand::FindNext { query, options } => CursorCommand::FindNext {
                query: summarize_history_text(query),
                options: *options,
            },
            CursorCommand::FindPrev { query, options } => CursorCommand::FindPrev {
                query: summarize_history_text(query),
                options: *options,
            },
            _ => self.clone(),
        }
    }
}

impl ViewCommand {
    fn is_mutating(&self) -> bool {
        match self {
            ViewCommand::SetViewportWidth { .. }
            | ViewCommand::SetWrapMode { .. }
            | ViewCommand::SetWrapIndent { .. }
            | ViewCommand::SetTabWidth { .. }
            | ViewCommand::SetTabKeyBehavior { .. }
            | ViewCommand::SetIndentationConfig { .. }
            | ViewCommand::SetAutoPairsConfig { .. }
            | ViewCommand::SetAutoPairsEnabled { .. }
            | ViewCommand::SetWordBoundaryAsciiBoundaryChars { .. }
            | ViewCommand::ResetWordBoundaryDefaults => true,
            ViewCommand::ScrollTo { .. } | ViewCommand::GetViewport { .. } => false,
        }
    }

    fn history_summary(&self) -> Self {
        match self {
            ViewCommand::SetWordBoundaryAsciiBoundaryChars { boundary_chars } => {
                ViewCommand::SetWordBoundaryAsciiBoundaryChars {
                    boundary_chars: summarize_history_text(boundary_chars),
                }
            }
            _ => self.clone(),
        }
    }
}

impl StyleCommand {
    fn is_mutating(&self) -> bool {
        match self {
            StyleCommand::AddStyle { .. }
            | StyleCommand::RemoveStyle { .. }
            | StyleCommand::Fold { .. }
            | StyleCommand::Unfold { .. }
            | StyleCommand::UnfoldAll
            | StyleCommand::UpdateBracketMatchHighlights
            | StyleCommand::ClearBracketMatchHighlights => true,
        }
    }
}

impl Command {
    /// Returns whether this command mutates document content, history, configuration, or styles.
    pub fn is_mutating(&self) -> bool {
        match self {
            Command::Edit(command) => command.is_mutating(),
            Command::Cursor(command) => command.is_mutating(),
            Command::View(command) => command.is_mutating(),
            Command::Style(command) => command.is_mutating(),
        }
    }

    pub(super) fn history_summary(&self) -> Self {
        match self {
            Command::Edit(command) => Command::Edit(command.history_summary()),
            Command::Cursor(command) => Command::Cursor(command.history_summary()),
            Command::View(command) => Command::View(command.history_summary()),
            Command::Style(command) => Command::Style(command.clone()),
        }
    }
}

/// Command execution result
#[derive(Debug, Clone)]
pub enum CommandResult {
    /// Success, no return value
    Success,
    /// Success, returns text
    Text(String),
    /// Success, returns position
    Position(Position),
    /// Success, returns offset
    Offset(usize),
    /// Success, returns viewport content
    Viewport(HeadlessGrid),
    /// Find/search result: a match in char offsets (half-open).
    SearchMatch {
        /// Inclusive start character offset.
        start: usize,
        /// Exclusive end character offset.
        end: usize,
    },
    /// Find/search result: no match found.
    SearchNotFound,
    /// Replace result: how many occurrences were replaced.
    ReplaceResult {
        /// Number of occurrences replaced.
        replaced: usize,
    },
}

/// Command error type
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandError {
    /// Invalid offset
    InvalidOffset(usize),
    /// Invalid position
    InvalidPosition {
        /// Logical line index.
        line: usize,
        /// Column in characters.
        column: usize,
    },
    /// Invalid range
    InvalidRange {
        /// Inclusive start character offset.
        start: usize,
        /// Exclusive end character offset.
        end: usize,
    },
    /// Empty text
    EmptyText,
    /// Other error
    Other(String),
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommandError::InvalidOffset(offset) => {
                write!(f, "Invalid offset: {}", offset)
            }
            CommandError::InvalidPosition { line, column } => {
                write!(f, "Invalid position: line {}, column {}", line, column)
            }
            CommandError::InvalidRange { start, end } => {
                write!(f, "Invalid range: {}..{}", start, end)
            }
            CommandError::EmptyText => {
                write!(f, "Text cannot be empty")
            }
            CommandError::Other(msg) => {
                write!(f, "{}", msg)
            }
        }
    }
}

impl std::error::Error for CommandError {}
