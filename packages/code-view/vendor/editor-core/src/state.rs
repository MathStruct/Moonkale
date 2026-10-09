//! Editor State Interface
//!
//! Provides a complete state query interface for the editor, used for frontend rendering and state synchronization.
//!
//! # Overview
//!
//! The state interface layer exposes the editor's internal state to the frontend in a structured, immutable manner.
//! It supports:
//!
//! - **State Queries**: Retrieve document, cursor, viewport, and other state information
//! - **Version Tracking**: Track state changes through version numbers
//! - **Change Notifications**: Subscribe to state change events
//! - **Viewport Management**: Obtain rendering data for visible regions
//!
//! # Example
//!
//! ```rust
//! use editor_core::{Command, EditCommand, EditorStateManager};
//!
//! let mut manager = EditorStateManager::new("Hello, World!", 80);
//!
//! // Query document state
//! let doc_state = manager.get_document_state();
//! println!("Line count: {}", doc_state.line_count);
//!
//! // Subscribe to state changes
//! manager.subscribe(|change| {
//!     println!("State changed: {:?}", change.change_type);
//! });
//!
//! // Modify document through the command path so text, layout, and derived state stay in sync.
//! manager.execute(Command::Edit(EditCommand::Insert {
//!     offset: 0,
//!     text: "New: ".to_string(),
//! })).unwrap();
//! ```

use crate::delta::TextDelta;
use crate::intervals::{FoldRegion, Interval, StyleId, StyleLayerId};
use crate::processing::{DocumentProcessor, ProcessingEdit};
use crate::snapshot::{ComposedGrid, HeadlessGrid};
use crate::{
    AnchorBias, Command, CommandError, CommandExecutor, CommandResult, CursorCommand, Decoration,
    DecorationLayerId, Diagnostic, EditCommand, EditorCore, LineEnding, Position, Selection,
    SelectionDirection, StyleCommand, TextAnchor, UndoHistoryRestoreError, UndoHistorySnapshot,
    ViewCommand,
};
use std::collections::BTreeMap;
use std::ops::Range;
use std::sync::Arc;

/// Document state
#[derive(Debug, Clone)]
pub struct DocumentState {
    /// Total documentLine count
    pub line_count: usize,
    /// Total document character count
    pub char_count: usize,
    /// Total document byte count
    pub byte_count: usize,
    /// Whether document has been modified
    pub is_modified: bool,
    /// Document version number (incremented after each modification)
    pub version: u64,
}

/// Cursor state
#[derive(Debug, Clone)]
pub struct CursorState {
    /// Primary cursor position (logical coordinates)
    pub position: Position,
    /// Primary cursor position (char offsets)
    pub offset: usize,
    /// Multi-cursor list (active positions of secondary carets, excluding primary)
    pub multi_cursors: Vec<Position>,
    /// Primary selection range (only primary; returns None for empty selection)
    pub selection: Option<Selection>,
    /// All selection set (including primary; each Selection may be empty)
    pub selections: Vec<Selection>,
    /// Index of primary in `selections`
    pub primary_selection_index: usize,
}

/// Viewport state
#[derive(Debug, Clone)]
pub struct ViewportState {
    /// Viewport width (in character cells)
    pub width: usize,
    /// Viewport height (line count, determined by the frontend)
    pub height: Option<usize>,
    /// Current scroll position (visual line number)
    pub scroll_top: usize,
    /// Sub-row smooth-scroll offset (0..=65535, normalized).
    pub sub_row_offset: u16,
    /// Overscan rows used to compute `prefetch_lines`.
    pub overscan_rows: usize,
    /// Visible visual line range
    pub visible_lines: Range<usize>,
    /// Recommended prefetch range (visible range expanded by `overscan_rows`).
    pub prefetch_lines: Range<usize>,
    /// Total visual lines under current wrap/folding state.
    pub total_visual_lines: usize,
}

/// Smooth-scrolling state for single-view manager.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SmoothScrollState {
    /// Top visual row anchor.
    pub top_visual_row: usize,
    /// Sub-row offset within top row (0..=65535, normalized).
    pub sub_row_offset: u16,
    /// Overscan rows for prefetching.
    pub overscan_rows: usize,
}

/// Undo/redo stack state
#[derive(Debug, Clone)]
pub struct UndoRedoState {
    /// Can undo
    pub can_undo: bool,
    /// Can redo
    pub can_redo: bool,
    /// Undo stack depth
    pub undo_depth: usize,
    /// Redo stack depth
    pub redo_depth: usize,
    /// Number of redo branches available at the current history node.
    pub redo_branch_count: usize,
    /// Currently selected redo branch index, if any.
    pub selected_redo_branch_index: Option<usize>,
    /// Current change group ID
    pub current_change_group: Option<usize>,
}

/// Folding state
#[derive(Debug, Clone)]
pub struct FoldingState {
    /// All folding regions
    pub regions: Vec<FoldRegion>,
    /// Count of collapsed lines
    pub collapsed_line_count: usize,
    /// Count of visible logical lines
    pub visible_logical_lines: usize,
    /// Total visual line count (considering folding)
    pub total_visual_lines: usize,
}

/// Diagnostics state
#[derive(Debug, Clone)]
pub struct DiagnosticsState {
    /// Total number of diagnostics.
    pub diagnostics_count: usize,
}

/// Decorations state
#[derive(Debug, Clone)]
pub struct DecorationsState {
    /// Total number of decoration layers.
    pub layer_count: usize,
    /// Total number of decorations (across all layers).
    pub decoration_count: usize,
}

/// Style state
#[derive(Debug, Clone)]
pub struct StyleState {
    /// Total number of style intervals
    pub style_count: usize,
}

/// State change type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateChangeType {
    /// Document content modified
    DocumentModified,
    /// Cursor moved
    CursorMoved,
    /// Selection changed
    SelectionChanged,
    /// Navigation / anchor state changed (bookmarks, marks, jump list, ...).
    NavigationChanged,
    /// Viewport changed
    ViewportChanged,
    /// Folding state changed
    FoldingChanged,
    /// Style changed
    StyleChanged,
    /// Decorations changed
    DecorationsChanged,
    /// Diagnostics changed
    DiagnosticsChanged,
    /// Document symbols / outline changed
    SymbolsChanged,
}

/// State change record
#[derive(Debug, Clone)]
pub struct StateChange {
    /// Change type
    pub change_type: StateChangeType,
    /// Old version number
    pub old_version: u64,
    /// New version number
    pub new_version: u64,
    /// Affected region (character offset range)
    pub affected_region: Option<Range<usize>>,
    /// Structured text delta for document changes (if available).
    pub text_delta: Option<Arc<TextDelta>>,
}

impl StateChange {
    /// Create a new state change record without an affected region.
    pub fn new(change_type: StateChangeType, old_version: u64, new_version: u64) -> Self {
        Self {
            change_type,
            old_version,
            new_version,
            affected_region: None,
            text_delta: None,
        }
    }

    /// Attach the affected character range to this change record.
    pub fn with_region(mut self, region: Range<usize>) -> Self {
        self.affected_region = Some(region);
        self
    }

    /// Attach a structured text delta to this change record.
    pub fn with_text_delta(mut self, delta: Arc<TextDelta>) -> Self {
        self.text_delta = Some(delta);
        self
    }
}

/// Complete editor state snapshot
#[derive(Debug, Clone)]
pub struct EditorState {
    /// Document state
    pub document: DocumentState,
    /// Cursor state
    pub cursor: CursorState,
    /// Viewport state
    pub viewport: ViewportState,
    /// Undo/redo state
    pub undo_redo: UndoRedoState,
    /// Folding state
    pub folding: FoldingState,
    /// Diagnostics state
    pub diagnostics: DiagnosticsState,
    /// Decorations state
    pub decorations: DecorationsState,
    /// Style state
    pub style: StyleState,
}

/// State change callback function type
pub type StateChangeCallback = Box<dyn FnMut(&StateChange) + Send>;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct BookmarkSet {
    anchors: Vec<TextAnchor>,
}

impl BookmarkSet {
    fn toggle_line_start(&mut self, line_start_offset: usize) -> bool {
        let anchor = TextAnchor::new(line_start_offset, AnchorBias::Left);
        match self
            .anchors
            .binary_search_by_key(&anchor.offset, |a| a.offset)
        {
            Ok(idx) => {
                self.anchors.remove(idx);
                false
            }
            Err(idx) => {
                self.anchors.insert(idx, anchor);
                true
            }
        }
    }

    fn clear(&mut self) {
        self.anchors.clear();
    }

    fn apply_delta(&mut self, delta: &TextDelta) {
        for a in &mut self.anchors {
            a.apply_delta(delta);
        }
        self.anchors.sort_by_key(|a| a.offset);
        self.anchors.dedup_by_key(|a| a.offset);
    }

    fn line_numbers(&self, line_index: &crate::LineIndex) -> Vec<usize> {
        let mut lines: Vec<usize> = self
            .anchors
            .iter()
            .map(|a| line_index.char_offset_to_position(a.offset).0)
            .collect();
        lines.sort_unstable();
        lines.dedup();
        lines
    }

    fn next_after_line_start(&self, current_line_start: usize) -> Option<TextAnchor> {
        self.anchors
            .iter()
            .copied()
            .find(|a| a.offset > current_line_start)
            .or_else(|| self.anchors.first().copied())
    }

    fn prev_before_line_start(&self, current_line_start: usize) -> Option<TextAnchor> {
        self.anchors
            .iter()
            .copied()
            .rfind(|a| a.offset < current_line_start)
            .or_else(|| self.anchors.last().copied())
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct MarkSet {
    marks: BTreeMap<String, TextAnchor>,
}

impl MarkSet {
    fn set(&mut self, name: String, offset: usize) {
        self.marks
            .insert(name, TextAnchor::new(offset, AnchorBias::Right));
    }

    fn get(&self, name: &str) -> Option<TextAnchor> {
        self.marks.get(name).copied()
    }

    fn remove(&mut self, name: &str) -> bool {
        self.marks.remove(name).is_some()
    }

    fn clear(&mut self) {
        self.marks.clear();
    }

    fn names(&self) -> Vec<String> {
        self.marks.keys().cloned().collect()
    }

    fn apply_delta(&mut self, delta: &TextDelta) {
        for anchor in self.marks.values_mut() {
            anchor.apply_delta(delta);
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct JumpList {
    back: Vec<TextAnchor>,
    forward: Vec<TextAnchor>,
    max_len: usize,
}

impl JumpList {
    fn new(max_len: usize) -> Self {
        Self {
            back: Vec::new(),
            forward: Vec::new(),
            max_len: max_len.max(1),
        }
    }

    fn record(&mut self, offset: usize) {
        let anchor = TextAnchor::new(offset, AnchorBias::Right);
        if self.back.last().is_some_and(|last| *last == anchor) {
            return;
        }

        self.back.push(anchor);
        self.forward.clear();

        if self.back.len() > self.max_len {
            let overflow = self.back.len() - self.max_len;
            self.back.drain(0..overflow);
        }
    }

    fn back(&mut self, current_offset: usize) -> Option<TextAnchor> {
        let current = TextAnchor::new(current_offset, AnchorBias::Right);
        let target = self.back.pop()?;
        if !self.forward.last().is_some_and(|last| *last == current) {
            self.forward.push(current);
        }
        Some(target)
    }

    fn forward(&mut self, current_offset: usize) -> Option<TextAnchor> {
        let current = TextAnchor::new(current_offset, AnchorBias::Right);
        let target = self.forward.pop()?;
        if !self.back.last().is_some_and(|last| *last == current) {
            self.back.push(current);
        }
        Some(target)
    }

    fn clear(&mut self) {
        self.back.clear();
        self.forward.clear();
    }

    fn apply_delta(&mut self, delta: &TextDelta) {
        for a in self.back.iter_mut().chain(self.forward.iter_mut()) {
            a.apply_delta(delta);
        }
    }
}

/// Classifies a view command for change detection: a bare `SetViewportWidth` can be a no-op
/// (width unchanged), whereas other view-config commands mutate state unconditionally.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ViewChangeKind {
    ViewportWidthOnly,
    Other,
}

/// Editor state manager
///
/// `EditorStateManager` wraps the command executor ([`CommandExecutor`]) and its internal [`EditorCore`]
/// and provides the following features:
///
/// - **State Queries**: Retrieve various state snapshots (document, cursor, viewport, etc.)
/// - **Version Tracking**: Automatically increment version number after each modification, supporting incremental updates
/// - **Change Notifications**: Notify subscribers of state changes via callback mechanism
/// - **Viewport Management**: Manage scroll position and visible regions
/// - **Modification Tracking**: Track whether the document has been modified (for save prompts)
///
/// # Architecture Notes
///
/// The state manager adopts a "unidirectional data flow" pattern:
///
/// 1. Frontends execute edits, cursor movement, viewport changes, and other user actions via
///    [`execute()`](EditorStateManager::execute) or [`CommandExecutor`].
/// 2. Derived state such as diagnostics, styles, decorations, symbols, and folds is replaced via
///    explicit manager or [`Workspace`](crate::Workspace) methods so notifications stay in sync.
/// 3. [`editor_mut()`](EditorStateManager::editor_mut) is reserved for advanced access to
///    `EditorCore` public methods; `EditorCore` fields remain private and cannot be mutated
///    directly.
/// 4. Manager increments version number and triggers all subscribed callbacks.
/// 5. Frontends retrieve the latest state via various `get_*_state()` methods.
///
/// # Example
///
/// ```rust
/// use editor_core::{Command, EditCommand, EditorStateManager};
///
/// let mut manager = EditorStateManager::new("Initial text", 80);
///
/// // Subscribe to state changes
/// manager.subscribe(|change| {
///     println!("Version {} -> {}: {:?}",
///         change.old_version, change.new_version, change.change_type);
/// });
///
/// // Modify document (automatically maintains consistency + automatically triggers state notifications)
/// manager.execute(Command::Edit(EditCommand::Insert {
///     offset: 0,
///     text: "New: ".to_string(),
/// })).unwrap();
///
/// // Query state
/// let doc_state = manager.get_document_state();
/// assert!(doc_state.is_modified);
/// assert_eq!(doc_state.version, 1);
/// ```
pub struct EditorStateManager {
    /// Command executor (wraps EditorCore and maintains consistency)
    executor: CommandExecutor,
    /// State version number
    state_version: u64,
    /// Whether document has been modified
    is_modified: bool,
    /// State change callback list
    callbacks: Vec<StateChangeCallback>,
    /// Current scroll position
    scroll_top: usize,
    /// Sub-row smooth-scroll offset.
    scroll_sub_row_offset: u16,
    /// Overscan rows for prefetching.
    overscan_rows: usize,
    /// Viewport height (optional)
    viewport_height: Option<usize>,
    /// Structured text delta produced by the last document edit.
    last_text_delta: Option<Arc<TextDelta>>,
    bookmarks: BookmarkSet,
    marks: MarkSet,
    jump_list: JumpList,
}

impl EditorStateManager {
    /// Create a new state manager
    pub fn new(text: &str, viewport_width: usize) -> Self {
        Self {
            executor: CommandExecutor::new(text, viewport_width),
            state_version: 0,
            is_modified: false,
            callbacks: Vec::new(),
            scroll_top: 0,
            scroll_sub_row_offset: 0,
            overscan_rows: 0,
            viewport_height: None,
            last_text_delta: None,
            bookmarks: BookmarkSet::default(),
            marks: MarkSet::default(),
            jump_list: JumpList::new(200),
        }
    }

    /// Create an empty state manager
    pub fn empty(viewport_width: usize) -> Self {
        Self::new("", viewport_width)
    }

    /// Get a reference to the Editor Core
    pub fn editor(&self) -> &EditorCore {
        self.executor.editor()
    }

    /// Get a mutable reference to the field-private Editor Core.
    ///
    /// Prefer [`execute`](Self::execute) and the explicit derived-state methods on this manager for
    /// observable mutations. This accessor is intended for advanced callers that need to invoke
    /// public [`EditorCore`] methods directly; it does not expose private storage, layout, folding,
    /// style, or cursor fields.
    pub fn editor_mut(&mut self) -> &mut EditorCore {
        self.executor.editor_mut()
    }

    /// Get the preferred line ending for saving this document.
    pub fn line_ending(&self) -> LineEnding {
        self.executor.line_ending()
    }

    /// Override the preferred line ending for saving this document.
    pub fn set_line_ending(&mut self, line_ending: LineEnding) {
        self.executor.set_line_ending(line_ending);
    }

    /// Return `true` if this editor currently has an active snippet session.
    pub fn has_active_snippet_session(&self) -> bool {
        self.executor.has_active_snippet_session()
    }

    /// Get the current document text converted to the preferred line ending for saving.
    pub fn get_text_for_saving(&self) -> String {
        let text = self.editor().get_text();
        self.line_ending().apply_to_text(&text)
    }

    /// Execute a command and automatically trigger state change notifications.
    ///
    /// - This method calls the underlying [`CommandExecutor`] to ensure consistency of components
    ///   such as the text buffer, line index, and layout engine.
    /// - For commands that cause state changes, [`mark_modified`](Self::mark_modified) is automatically called.
    /// - For pure query commands (such as `GetViewport`), the version number is not incremented.
    pub fn execute(&mut self, command: Command) -> Result<CommandResult, CommandError> {
        let change_type = Self::change_type_for_command(&command);
        let is_delete_like = matches!(
            &command,
            Command::Edit(EditCommand::Backspace | EditCommand::DeleteForward)
        );
        // A bare `SetViewportWidth` may be a no-op (width unchanged); every other view command
        // that classifies as `ViewportChanged` mutates config unconditionally.
        let command_kind = if matches!(
            &command,
            Command::View(ViewCommand::SetViewportWidth { .. })
        ) {
            ViewChangeKind::ViewportWidthOnly
        } else {
            ViewChangeKind::Other
        };

        // Detect changes for potential no-ops: when command execution succeeds but state doesn't change, version should not increment.
        let cursor_before = self.executor.editor().cursor_position();
        let selection_before = self.executor.editor().selection().cloned();
        let secondary_before = self.executor.editor().secondary_selections().to_vec();
        let viewport_width_before = self.executor.editor().viewport_width();
        let char_count_before = self.executor.editor().char_count();

        let result = self.executor.execute(command)?;
        let char_count_after = self.executor.editor().char_count();
        let delta_present = self.executor.last_text_delta().is_some();

        if let Some(change_type) = change_type {
            let changed = match change_type {
                StateChangeType::CursorMoved => {
                    self.executor.editor().cursor_position() != cursor_before
                        || self.executor.editor().secondary_selections()
                            != secondary_before.as_slice()
                }
                StateChangeType::SelectionChanged => {
                    self.executor.editor().cursor_position() != cursor_before
                        || self.executor.editor().selection().cloned() != selection_before
                        || self.executor.editor().secondary_selections()
                            != secondary_before.as_slice()
                }
                StateChangeType::ViewportChanged => {
                    // View-config setters (tab-key behavior, indentation, auto-pairs, word
                    // boundaries, wrap mode/indent, tab width) mutate view-local state
                    // unconditionally; they are not observable through `viewport_width` alone.
                    // Only a bare `SetViewportWidth` is a potential no-op we can cheaply detect.
                    if matches!(command_kind, ViewChangeKind::ViewportWidthOnly) {
                        self.executor.editor().viewport_width() != viewport_width_before
                    } else {
                        true
                    }
                }
                StateChangeType::DocumentModified => {
                    // EditCommand::Backspace / DeleteForward can be valid no-ops at boundaries.
                    // Detect via char count change (they only delete text).
                    if is_delete_like {
                        char_count_after != char_count_before
                    } else {
                        delta_present
                    }
                }
                StateChangeType::NavigationChanged => true,
                // Style/folding/diagnostics commands are currently treated as "success means change".
                StateChangeType::FoldingChanged
                | StateChangeType::StyleChanged
                | StateChangeType::DecorationsChanged
                | StateChangeType::DiagnosticsChanged
                | StateChangeType::SymbolsChanged => true,
            };

            if changed {
                if matches!(change_type, StateChangeType::DocumentModified) {
                    let is_modified = !self.executor.is_clean();
                    let delta = self.executor.take_last_text_delta().map(Arc::new);
                    if let Some(ref delta) = delta {
                        self.bookmarks.apply_delta(delta);
                        self.marks.apply_delta(delta);
                        self.jump_list.apply_delta(delta);
                    }
                    self.last_text_delta = delta.clone();
                    self.mark_modified_internal(change_type, Some(is_modified), delta);
                } else {
                    self.mark_modified_internal(change_type, None, None);
                }
            }
        }

        Ok(result)
    }

    fn change_type_for_command(command: &Command) -> Option<StateChangeType> {
        match command {
            Command::Edit(EditCommand::Delete { length: 0, .. }) => None,
            Command::Edit(EditCommand::Replace {
                length: 0, text, ..
            }) if text.is_empty() => None,
            Command::Edit(EditCommand::EndUndoGroup) => None,
            Command::Edit(_) => Some(StateChangeType::DocumentModified),
            Command::Cursor(
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
                | CursorCommand::MoveToMatchingBracket,
            ) => Some(StateChangeType::CursorMoved),
            Command::Cursor(
                CursorCommand::SetSelection { .. }
                | CursorCommand::ExtendSelection { .. }
                | CursorCommand::ClearSelection
                | CursorCommand::SetSelections { .. }
                | CursorCommand::ClearSecondarySelections
                | CursorCommand::SetRectSelection { .. }
                | CursorCommand::SelectLine
                | CursorCommand::SelectWord
                | CursorCommand::ExpandSelection
                | CursorCommand::ExpandSelectionBy { .. }
                | CursorCommand::SnippetNextPlaceholder
                | CursorCommand::SnippetPrevPlaceholder
                | CursorCommand::AddCursorAbove
                | CursorCommand::AddCursorBelow
                | CursorCommand::AddNextOccurrence { .. }
                | CursorCommand::AddAllOccurrences { .. }
                | CursorCommand::FindNext { .. }
                | CursorCommand::FindPrev { .. },
            ) => Some(StateChangeType::SelectionChanged),
            // `ScrollTo` is transient viewport positioning and `GetViewport` is a pure query;
            // neither is a persisted state mutation. Every other view command changes view-local
            // config (wrap, tab, indentation, auto-pairs, word boundaries) and must notify
            // subscribers, matching `ViewCommand::is_mutating` and the `Workspace` execute path.
            Command::View(ViewCommand::ScrollTo { .. } | ViewCommand::GetViewport { .. }) => None,
            Command::View(_) => Some(StateChangeType::ViewportChanged),
            Command::Style(
                StyleCommand::AddStyle { .. }
                | StyleCommand::RemoveStyle { .. }
                | StyleCommand::UpdateBracketMatchHighlights
                | StyleCommand::ClearBracketMatchHighlights,
            ) => Some(StateChangeType::StyleChanged),
            Command::Style(
                StyleCommand::Fold { .. } | StyleCommand::Unfold { .. } | StyleCommand::UnfoldAll,
            ) => Some(StateChangeType::FoldingChanged),
        }
    }

    /// Get current version number
    pub fn version(&self) -> u64 {
        self.state_version
    }

    /// Set viewport height
    pub fn set_viewport_height(&mut self, height: usize) {
        self.viewport_height = Some(height);
    }

    /// Set scroll position
    pub fn set_scroll_top(&mut self, scroll_top: usize) {
        let old_scroll = self.scroll_top;
        self.scroll_top = scroll_top;

        if old_scroll != scroll_top {
            self.notify_change(StateChangeType::ViewportChanged);
        }
    }

    /// Set sub-row smooth-scroll offset (normalized 0..=65535).
    pub fn set_scroll_sub_row_offset(&mut self, sub_row_offset: u16) {
        let old = self.scroll_sub_row_offset;
        self.scroll_sub_row_offset = sub_row_offset;
        if old != sub_row_offset {
            self.notify_change(StateChangeType::ViewportChanged);
        }
    }

    /// Set overscan rows for viewport prefetch range.
    pub fn set_overscan_rows(&mut self, overscan_rows: usize) {
        let old = self.overscan_rows;
        self.overscan_rows = overscan_rows;
        if old != overscan_rows {
            self.notify_change(StateChangeType::ViewportChanged);
        }
    }

    /// Set full smooth-scroll state.
    pub fn set_smooth_scroll_state(&mut self, state: SmoothScrollState) {
        let mut changed = false;
        if self.scroll_top != state.top_visual_row {
            self.scroll_top = state.top_visual_row;
            changed = true;
        }
        if self.scroll_sub_row_offset != state.sub_row_offset {
            self.scroll_sub_row_offset = state.sub_row_offset;
            changed = true;
        }
        if self.overscan_rows != state.overscan_rows {
            self.overscan_rows = state.overscan_rows;
            changed = true;
        }
        if changed {
            self.notify_change(StateChangeType::ViewportChanged);
        }
    }

    /// Get smooth-scroll state.
    pub fn get_smooth_scroll_state(&self) -> SmoothScrollState {
        SmoothScrollState {
            top_visual_row: self.scroll_top,
            sub_row_offset: self.scroll_sub_row_offset,
            overscan_rows: self.overscan_rows,
        }
    }

    /// Get complete editor state snapshot
    pub fn get_full_state(&self) -> EditorState {
        EditorState {
            document: self.get_document_state(),
            cursor: self.get_cursor_state(),
            viewport: self.get_viewport_state(),
            undo_redo: self.get_undo_redo_state(),
            folding: self.get_folding_state(),
            diagnostics: self.get_diagnostics_state(),
            decorations: self.get_decorations_state(),
            style: self.get_style_state(),
        }
    }

    /// Get document state
    pub fn get_document_state(&self) -> DocumentState {
        let editor = self.executor.editor();
        DocumentState {
            line_count: editor.line_count(),
            char_count: editor.char_count(),
            byte_count: editor.get_text().len(),
            is_modified: self.is_modified,
            version: self.state_version,
        }
    }

    /// Get cursor state
    pub fn get_cursor_state(&self) -> CursorState {
        let editor = self.executor.editor();
        let mut selections: Vec<Selection> =
            Vec::with_capacity(1 + editor.secondary_selections().len());

        let primary = editor.selection().cloned().unwrap_or(Selection {
            start: editor.cursor_position(),
            end: editor.cursor_position(),
            direction: SelectionDirection::Forward,
        });
        selections.push(primary);
        selections.extend(editor.secondary_selections().iter().cloned());

        let (selections, primary_selection_index) =
            crate::selection_set::normalize_selections(selections, 0);
        let primary = selections
            .get(primary_selection_index)
            .cloned()
            .unwrap_or(Selection {
                start: editor.cursor_position(),
                end: editor.cursor_position(),
                direction: SelectionDirection::Forward,
            });

        let position = primary.end;
        let offset = editor
            .line_index()
            .position_to_char_offset(position.line, position.column);

        let selection = if primary.start == primary.end {
            None
        } else {
            Some(primary)
        };

        let multi_cursors: Vec<Position> = selections
            .iter()
            .enumerate()
            .filter_map(|(idx, sel)| {
                if idx == primary_selection_index {
                    None
                } else {
                    Some(sel.end)
                }
            })
            .collect();

        CursorState {
            position,
            offset,
            multi_cursors,
            selection,
            selections,
            primary_selection_index,
        }
    }

    /// Get viewport state
    pub fn get_viewport_state(&self) -> ViewportState {
        let editor = self.executor.editor();
        let total_visual_lines = editor.visual_line_count();
        let clamped_top = self.scroll_top.min(total_visual_lines);
        let visible_end = if let Some(height) = self.viewport_height {
            clamped_top.saturating_add(height)
        } else {
            total_visual_lines
        };
        let visible_lines = clamped_top..visible_end.min(total_visual_lines);
        let prefetch_start = visible_lines.start.saturating_sub(self.overscan_rows);
        let prefetch_end = visible_lines
            .end
            .saturating_add(self.overscan_rows)
            .min(total_visual_lines);

        ViewportState {
            width: editor.viewport_width(),
            height: self.viewport_height,
            scroll_top: clamped_top,
            sub_row_offset: self.scroll_sub_row_offset,
            overscan_rows: self.overscan_rows,
            visible_lines,
            prefetch_lines: prefetch_start..prefetch_end,
            total_visual_lines,
        }
    }

    /// Get undo/redo state
    pub fn get_undo_redo_state(&self) -> UndoRedoState {
        UndoRedoState {
            can_undo: self.executor.can_undo(),
            can_redo: self.executor.can_redo(),
            undo_depth: self.executor.undo_depth(),
            redo_depth: self.executor.redo_depth(),
            redo_branch_count: self.executor.redo_branch_count(),
            selected_redo_branch_index: self.executor.selected_redo_branch_index(),
            current_change_group: self.executor.current_change_group(),
        }
    }

    /// Get folding state
    pub fn get_folding_state(&self) -> FoldingState {
        let editor = self.executor.editor();
        let regions = editor.folding_manager().regions().to_vec();
        // Count hidden lines by union of collapsed ranges so nested/overlapping folds are not
        // double-counted (which could otherwise underflow `line_count - collapsed_line_count`).
        let collapsed_line_count = editor.folding_manager().collapsed_hidden_line_count();

        let visible_logical_lines = editor.line_count().saturating_sub(collapsed_line_count);

        FoldingState {
            regions,
            collapsed_line_count,
            visible_logical_lines,
            total_visual_lines: editor.visual_line_count(),
        }
    }

    /// Get style state
    pub fn get_style_state(&self) -> StyleState {
        let editor = self.executor.editor();
        let layered_count: usize = editor.style_layers().values().map(|t| t.len()).sum();
        StyleState {
            style_count: editor.interval_tree().len() + layered_count,
        }
    }

    /// Get diagnostics state.
    pub fn get_diagnostics_state(&self) -> DiagnosticsState {
        let editor = self.executor.editor();
        DiagnosticsState {
            diagnostics_count: editor.diagnostics().len(),
        }
    }

    /// Get decorations state.
    pub fn get_decorations_state(&self) -> DecorationsState {
        let editor = self.executor.editor();
        let decoration_count: usize = editor.decorations().values().map(|d| d.len()).sum();
        DecorationsState {
            layer_count: editor.decorations().len(),
            decoration_count,
        }
    }

    /// Get all styles within the specified range
    pub fn get_styles_in_range(&self, start: usize, end: usize) -> Vec<(usize, usize, StyleId)> {
        let editor = self.executor.editor();
        let mut result: Vec<(usize, usize, StyleId)> = editor
            .interval_tree()
            .query_range(start, end)
            .iter()
            .map(|interval| (interval.start, interval.end, interval.style_id))
            .collect();

        for tree in editor.style_layers().values() {
            result.extend(
                tree.query_range(start, end)
                    .iter()
                    .map(|interval| (interval.start, interval.end, interval.style_id)),
            );
        }

        result.sort_unstable_by_key(|(s, e, id)| (*s, *e, *id));
        result
    }

    /// Get all styles at the specified position
    pub fn get_styles_at(&self, offset: usize) -> Vec<StyleId> {
        let editor = self.executor.editor();
        let mut styles: Vec<StyleId> = editor
            .interval_tree()
            .query_point(offset)
            .iter()
            .map(|interval| interval.style_id)
            .collect();

        for tree in editor.style_layers().values() {
            styles.extend(
                tree.query_point(offset)
                    .iter()
                    .map(|interval| interval.style_id),
            );
        }

        styles.sort_unstable();
        styles.dedup();
        styles
    }

    /// Replace all intervals in the specified style layer.
    ///
    /// Suitable for scenarios such as LSP semantic highlighting and simple syntax highlighting that require "full layer refresh".
    /// This method only triggers `StyleChanged` once, avoiding version number explosion due to individual insertions.
    pub fn replace_style_layer(&mut self, layer: StyleLayerId, intervals: Vec<Interval>) {
        self.executor
            .editor_mut()
            .replace_style_layer(layer, intervals);
        self.mark_modified(StateChangeType::StyleChanged);
    }

    /// Clear the specified style layer.
    pub fn clear_style_layer(&mut self, layer: StyleLayerId) {
        self.executor.editor_mut().clear_style_layer(layer);
        self.mark_modified(StateChangeType::StyleChanged);
    }

    /// Replace diagnostics wholesale.
    pub fn replace_diagnostics(&mut self, diagnostics: Vec<Diagnostic>) {
        self.executor.editor_mut().replace_diagnostics(diagnostics);
        self.mark_modified(StateChangeType::DiagnosticsChanged);
    }

    /// Clear all diagnostics.
    pub fn clear_diagnostics(&mut self) {
        self.executor.editor_mut().clear_diagnostics();
        self.mark_modified(StateChangeType::DiagnosticsChanged);
    }

    /// Replace document symbols / outline wholesale.
    pub fn replace_document_symbols(&mut self, symbols: crate::DocumentOutline) {
        self.executor.editor_mut().replace_document_symbols(symbols);
        self.mark_modified(StateChangeType::SymbolsChanged);
    }

    /// Clear document symbols / outline.
    pub fn clear_document_symbols(&mut self) {
        self.executor.editor_mut().clear_document_symbols();
        self.mark_modified(StateChangeType::SymbolsChanged);
    }

    /// Replace a decoration layer wholesale.
    pub fn replace_decorations(&mut self, layer: DecorationLayerId, decorations: Vec<Decoration>) {
        self.executor
            .editor_mut()
            .replace_decorations(layer, decorations);
        self.mark_modified(StateChangeType::DecorationsChanged);
    }

    /// Clear a decoration layer.
    pub fn clear_decorations(&mut self, layer: DecorationLayerId) {
        self.executor.editor_mut().clear_decorations(layer);
        self.mark_modified(StateChangeType::DecorationsChanged);
    }

    /// Replace folding regions wholesale.
    ///
    /// If `preserve_collapsed` is true, matching existing collapsed derived regions stay collapsed
    /// after replacement, including conservative matches across small line-number drift.
    pub fn replace_folding_regions(&mut self, regions: Vec<FoldRegion>, preserve_collapsed: bool) {
        self.executor
            .editor_mut()
            .replace_folding_regions(regions, preserve_collapsed);
        self.mark_modified(StateChangeType::FoldingChanged);
    }

    /// Clear all *derived* folding regions (leaves user folds intact).
    pub fn clear_folding_regions(&mut self) {
        self.executor.editor_mut().clear_derived_folding_regions();
        self.mark_modified(StateChangeType::FoldingChanged);
    }

    /// Toggle the fold region that starts on the current cursor line.
    pub fn toggle_fold_at_current_line(&mut self) -> bool {
        let line = self.executor.editor().cursor_position().line;
        let toggled = self.executor.editor_mut().toggle_fold_at_line(line);
        if toggled {
            self.mark_modified(StateChangeType::FoldingChanged);
        }
        toggled
    }

    /// Expand all folding regions.
    pub fn expand_all_folds(&mut self) {
        let had_collapsed = self
            .executor
            .editor()
            .folding_manager()
            .regions()
            .iter()
            .any(|region| region.is_collapsed);
        self.executor.editor_mut().expand_all_folds();
        if had_collapsed {
            self.mark_modified(StateChangeType::FoldingChanged);
        }
    }

    /// Apply derived-state edits produced by a document processor (highlighting, folding, etc.).
    pub fn apply_processing_edits<I>(&mut self, edits: I)
    where
        I: IntoIterator<Item = ProcessingEdit>,
    {
        for edit in edits {
            match edit {
                ProcessingEdit::ReplaceStyleLayer { layer, intervals } => {
                    self.replace_style_layer(layer, intervals);
                }
                ProcessingEdit::ClearStyleLayer { layer } => {
                    self.clear_style_layer(layer);
                }
                ProcessingEdit::ReplaceFoldingRegions {
                    regions,
                    preserve_collapsed,
                } => {
                    self.replace_folding_regions(regions, preserve_collapsed);
                }
                ProcessingEdit::ClearFoldingRegions => {
                    self.clear_folding_regions();
                }
                ProcessingEdit::ReplaceDiagnostics { diagnostics } => {
                    self.replace_diagnostics(diagnostics);
                }
                ProcessingEdit::ClearDiagnostics => {
                    self.clear_diagnostics();
                }
                ProcessingEdit::ReplaceDecorations { layer, decorations } => {
                    self.replace_decorations(layer, decorations);
                }
                ProcessingEdit::ClearDecorations { layer } => {
                    self.clear_decorations(layer);
                }
                ProcessingEdit::ReplaceDocumentSymbols { symbols } => {
                    self.replace_document_symbols(symbols);
                }
                ProcessingEdit::ClearDocumentSymbols => {
                    self.clear_document_symbols();
                }
            }
        }
    }

    /// Run a [`DocumentProcessor`] against the current document and apply its edits.
    pub fn apply_processor<P>(&mut self, processor: &mut P) -> Result<(), P::Error>
    where
        P: DocumentProcessor,
    {
        let edits = processor.process(self)?;
        self.apply_processing_edits(edits);
        Ok(())
    }

    /// Get viewport content
    pub fn get_viewport_content(&self, start_row: usize, count: usize) -> HeadlessGrid {
        let editor = self.executor.editor();
        let text = editor.get_text();
        let generator = crate::SnapshotGenerator::from_text_with_layout_options(
            &text,
            editor.viewport_width(),
            editor.layout_engine().tab_width(),
            editor.layout_engine().wrap_mode(),
            editor.layout_engine().wrap_indent(),
        );
        generator.get_headless_grid(start_row, count)
    }

    /// Get a styled viewport clipped to a horizontal cell window.
    pub fn get_viewport_content_window(
        &self,
        start_row: usize,
        count: usize,
        left: usize,
        width: usize,
    ) -> HeadlessGrid {
        self.executor.editor().get_headless_grid_styled_window(
            start_row,
            count,
            Some((left, width)),
        )
    }

    /// Get styled viewport content (by visual line).
    ///
    /// - Supports soft wrapping (based on `LayoutEngine`)
    /// - `Cell.styles` will contain the merged result of `interval_tree` and all `style_layers`
    pub fn get_viewport_content_styled(
        &self,
        start_visual_row: usize,
        count: usize,
    ) -> HeadlessGrid {
        self.executor
            .editor()
            .get_headless_grid_styled(start_visual_row, count)
    }

    /// Get lightweight minimap content (by visual line).
    pub fn get_minimap_content(&self, start_visual_row: usize, count: usize) -> crate::MinimapGrid {
        self.executor
            .editor()
            .get_minimap_grid(start_visual_row, count)
    }

    /// Get a decoration-aware composed viewport snapshot (by composed visual line).
    ///
    /// See [`EditorCore::get_headless_grid_composed`](crate::EditorCore::get_headless_grid_composed)
    /// for detailed semantics and caveats.
    pub fn get_viewport_content_composed(
        &self,
        start_visual_row: usize,
        count: usize,
    ) -> ComposedGrid {
        self.executor
            .editor()
            .get_headless_grid_composed(start_visual_row, count)
    }

    /// Get total visual line count under current wrap/folding state.
    pub fn total_visual_lines(&self) -> usize {
        self.executor.editor().visual_line_count()
    }

    /// Map global visual row to `(logical_line, visual_in_logical)`.
    pub fn visual_to_logical_line(&self, visual_row: usize) -> (usize, usize) {
        self.executor.editor().visual_to_logical_line(visual_row)
    }

    /// Map logical position to visual `(row, x_cells)`.
    pub fn logical_position_to_visual(&self, line: usize, column: usize) -> Option<(usize, usize)> {
        self.executor
            .editor()
            .logical_position_to_visual(line, column)
    }

    /// Map visual `(row, x_cells)` back to logical position.
    pub fn visual_position_to_logical(
        &self,
        visual_row: usize,
        x_cells: usize,
    ) -> Option<Position> {
        self.executor
            .editor()
            .visual_position_to_logical(visual_row, x_cells)
    }

    /// Subscribe to state change notifications
    pub fn subscribe<F>(&mut self, callback: F)
    where
        F: FnMut(&StateChange) + Send + 'static,
    {
        self.callbacks.push(Box::new(callback));
    }

    /// Check if state has changed since a version
    pub fn has_changed_since(&self, version: u64) -> bool {
        self.state_version > version
    }

    /// Mark document as modified and increment version number
    pub fn mark_modified(&mut self, change_type: StateChangeType) {
        self.mark_modified_internal(change_type, None, None);
    }

    fn mark_modified_internal(
        &mut self,
        change_type: StateChangeType,
        is_modified_override: Option<bool>,
        delta: Option<Arc<TextDelta>>,
    ) {
        let old_version = self.state_version;
        self.state_version += 1;

        // Only mark as modified for document content changes
        if matches!(change_type, StateChangeType::DocumentModified) {
            self.is_modified = is_modified_override.unwrap_or(true);
        }

        let mut change = StateChange::new(change_type, old_version, self.state_version);
        if let Some(delta) = delta {
            change = change.with_text_delta(delta);
        }
        self.notify_callbacks(&change);
    }

    /// Mark document as unmodified (e.g., after saving)
    pub fn mark_saved(&mut self) {
        self.executor.mark_clean();
        self.is_modified = false;
    }

    /// Discard redundant UI-engine undo while preserving text, delta and cursor.
    pub fn discard_undo_history(&mut self) {
        self.executor.discard_undo_history();
    }

    /// Capture a persistable snapshot of the undo/redo history for this document.
    pub fn undo_history_snapshot(&self) -> UndoHistorySnapshot {
        self.executor.undo_history_snapshot()
    }

    /// Restore a previously captured [`UndoHistorySnapshot`].
    ///
    /// Notes:
    /// - This does **not** modify the current document text.
    /// - Callers should only restore a snapshot into the **same text** it was captured from.
    pub fn restore_undo_history(
        &mut self,
        snapshot: UndoHistorySnapshot,
    ) -> Result<(), UndoHistoryRestoreError> {
        self.last_text_delta = None;
        self.executor.restore_undo_history(snapshot)?;
        self.is_modified = !self.executor.is_clean();
        Ok(())
    }

    /// Notify state change (without modifying version number)
    fn notify_change(&mut self, change_type: StateChangeType) {
        let change = StateChange::new(change_type, self.state_version, self.state_version);
        self.notify_callbacks(&change);
    }

    /// Get the structured text delta produced by the last document edit, if any.
    pub fn last_text_delta(&self) -> Option<&TextDelta> {
        self.last_text_delta.as_deref()
    }

    /// Take the structured text delta produced by the last document edit, if any.
    pub fn take_last_text_delta(&mut self) -> Option<Arc<TextDelta>> {
        self.last_text_delta.take()
    }

    /// Toggle a bookmark at the current cursor line.
    ///
    /// Returns `true` if a bookmark was added, or `false` if an existing bookmark on that line was
    /// removed.
    pub fn toggle_bookmark_at_cursor_line(&mut self) -> bool {
        let line = self.executor.editor().cursor_position().line;
        let line_start = self
            .executor
            .editor()
            .line_index()
            .position_to_char_offset(line, 0);
        let added = self.bookmarks.toggle_line_start(line_start);
        self.mark_modified_internal(StateChangeType::NavigationChanged, None, None);
        added
    }

    /// Return all bookmark line numbers (0-based).
    pub fn bookmark_lines(&self) -> Vec<usize> {
        self.bookmarks
            .line_numbers(self.executor.editor().line_index())
    }

    /// Clear all bookmarks.
    pub fn clear_bookmarks(&mut self) {
        self.bookmarks.clear();
        self.mark_modified_internal(StateChangeType::NavigationChanged, None, None);
    }

    /// Move the cursor to the next bookmark (wrapping to the first bookmark).
    ///
    /// Returns the new cursor position, or `None` if there are no bookmarks.
    pub fn goto_next_bookmark(&mut self) -> Result<Option<Position>, CommandError> {
        let line = self.executor.editor().cursor_position().line;
        let current_line_start = self
            .executor
            .editor()
            .line_index()
            .position_to_char_offset(line, 0);

        let Some(target) = self.bookmarks.next_after_line_start(current_line_start) else {
            return Ok(None);
        };

        let (line, column) = self
            .executor
            .editor()
            .line_index()
            .char_offset_to_position(target.offset);
        self.execute(Command::Cursor(CursorCommand::MoveTo { line, column }))?;
        let _ = self.execute(Command::Cursor(CursorCommand::ClearSelection))?;
        Ok(Some(Position::new(line, column)))
    }

    /// Move the cursor to the previous bookmark (wrapping to the last bookmark).
    ///
    /// Returns the new cursor position, or `None` if there are no bookmarks.
    pub fn goto_prev_bookmark(&mut self) -> Result<Option<Position>, CommandError> {
        let line = self.executor.editor().cursor_position().line;
        let current_line_start = self
            .executor
            .editor()
            .line_index()
            .position_to_char_offset(line, 0);

        let Some(target) = self.bookmarks.prev_before_line_start(current_line_start) else {
            return Ok(None);
        };

        let (line, column) = self
            .executor
            .editor()
            .line_index()
            .char_offset_to_position(target.offset);
        self.execute(Command::Cursor(CursorCommand::MoveTo { line, column }))?;
        let _ = self.execute(Command::Cursor(CursorCommand::ClearSelection))?;
        Ok(Some(Position::new(line, column)))
    }

    /// Set (or replace) a named mark at the current cursor position.
    pub fn set_mark_at_cursor(&mut self, name: String) -> Result<(), CommandError> {
        if name.trim().is_empty() {
            return Err(CommandError::Other("Mark name cannot be empty".to_string()));
        }

        let pos = self.executor.editor().cursor_position();
        let offset = self
            .executor
            .editor()
            .line_index()
            .position_to_char_offset(pos.line, pos.column);
        self.marks.set(name, offset);
        self.mark_modified_internal(StateChangeType::NavigationChanged, None, None);
        Ok(())
    }

    /// Move the cursor to a named mark (if present).
    ///
    /// Returns the new cursor position, or `None` if the mark does not exist.
    pub fn goto_mark(&mut self, name: &str) -> Result<Option<Position>, CommandError> {
        let Some(anchor) = self.marks.get(name) else {
            return Ok(None);
        };
        let (line, column) = self
            .executor
            .editor()
            .line_index()
            .char_offset_to_position(anchor.offset);
        self.execute(Command::Cursor(CursorCommand::MoveTo { line, column }))?;
        let _ = self.execute(Command::Cursor(CursorCommand::ClearSelection))?;
        Ok(Some(Position::new(line, column)))
    }

    /// Remove a named mark.
    ///
    /// Returns `true` if the mark existed.
    pub fn clear_mark(&mut self, name: &str) -> bool {
        let existed = self.marks.remove(name);
        if existed {
            self.mark_modified_internal(StateChangeType::NavigationChanged, None, None);
        }
        existed
    }

    /// Return all mark names (deterministic order).
    pub fn mark_names(&self) -> Vec<String> {
        self.marks.names()
    }

    /// Clear all marks.
    pub fn clear_all_marks(&mut self) {
        self.marks.clear();
        self.mark_modified_internal(StateChangeType::NavigationChanged, None, None);
    }

    /// Record the current cursor position as a jump-list location.
    ///
    /// Typical usage: call this *before* performing a “jump” (go-to-definition, search result,
    /// symbol navigation, ...).
    pub fn push_jump_location(&mut self) {
        let pos = self.executor.editor().cursor_position();
        let offset = self
            .executor
            .editor()
            .line_index()
            .position_to_char_offset(pos.line, pos.column);
        self.jump_list.record(offset);
        self.mark_modified_internal(StateChangeType::NavigationChanged, None, None);
    }

    /// Jump back in the jump list.
    ///
    /// Returns the new cursor position, or `None` if there is no back entry.
    pub fn jump_back(&mut self) -> Result<Option<Position>, CommandError> {
        let pos = self.executor.editor().cursor_position();
        let current_offset = self
            .executor
            .editor()
            .line_index()
            .position_to_char_offset(pos.line, pos.column);

        let Some(target) = self.jump_list.back(current_offset) else {
            return Ok(None);
        };

        self.mark_modified_internal(StateChangeType::NavigationChanged, None, None);

        let (line, column) = self
            .executor
            .editor()
            .line_index()
            .char_offset_to_position(target.offset);
        self.execute(Command::Cursor(CursorCommand::MoveTo { line, column }))?;
        let _ = self.execute(Command::Cursor(CursorCommand::ClearSelection))?;
        Ok(Some(Position::new(line, column)))
    }

    /// Jump forward in the jump list.
    ///
    /// Returns the new cursor position, or `None` if there is no forward entry.
    pub fn jump_forward(&mut self) -> Result<Option<Position>, CommandError> {
        let pos = self.executor.editor().cursor_position();
        let current_offset = self
            .executor
            .editor()
            .line_index()
            .position_to_char_offset(pos.line, pos.column);

        let Some(target) = self.jump_list.forward(current_offset) else {
            return Ok(None);
        };

        self.mark_modified_internal(StateChangeType::NavigationChanged, None, None);

        let (line, column) = self
            .executor
            .editor()
            .line_index()
            .char_offset_to_position(target.offset);
        self.execute(Command::Cursor(CursorCommand::MoveTo { line, column }))?;
        let _ = self.execute(Command::Cursor(CursorCommand::ClearSelection))?;
        Ok(Some(Position::new(line, column)))
    }

    /// Clear the jump list (both back/forward stacks).
    pub fn clear_jump_list(&mut self) {
        self.jump_list.clear();
        self.mark_modified_internal(StateChangeType::NavigationChanged, None, None);
    }

    /// Notify all callbacks
    fn notify_callbacks(&mut self, change: &StateChange) {
        for callback in &mut self.callbacks {
            callback(change);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_document_state() {
        let manager = EditorStateManager::new("Hello World\nLine 2", 80);
        let doc_state = manager.get_document_state();

        assert_eq!(doc_state.line_count, 2);
        assert_eq!(doc_state.char_count, 18); // Including newline
        assert!(!doc_state.is_modified);
        assert_eq!(doc_state.version, 0);
    }

    #[test]
    fn test_cursor_state() {
        let manager = EditorStateManager::new("Hello World", 80);
        let cursor_state = manager.get_cursor_state();

        assert_eq!(cursor_state.position, Position::new(0, 0));
        assert_eq!(cursor_state.offset, 0);
        assert!(cursor_state.selection.is_none());
    }

    #[test]
    fn test_viewport_state() {
        let mut manager = EditorStateManager::new("Line 1\nLine 2\nLine 3", 80);
        manager.set_viewport_height(10);
        manager.set_scroll_top(1);

        let viewport_state = manager.get_viewport_state();

        assert_eq!(viewport_state.width, 80);
        assert_eq!(viewport_state.height, Some(10));
        assert_eq!(viewport_state.scroll_top, 1);
        assert_eq!(viewport_state.visible_lines, 1..3);
    }

    #[test]
    fn test_folding_state() {
        let manager = EditorStateManager::new("Line 1\nLine 2\nLine 3", 80);
        let folding_state = manager.get_folding_state();

        assert_eq!(folding_state.regions.len(), 0);
        assert_eq!(folding_state.collapsed_line_count, 0);
        assert_eq!(folding_state.visible_logical_lines, 3);
    }

    #[test]
    fn test_style_state() {
        let manager = EditorStateManager::new("Hello World", 80);
        let style_state = manager.get_style_state();

        assert_eq!(style_state.style_count, 0);
    }

    #[test]
    fn test_full_state() {
        let manager = EditorStateManager::new("Test", 80);
        let full_state = manager.get_full_state();

        assert_eq!(full_state.document.line_count, 1);
        assert_eq!(full_state.cursor.position, Position::new(0, 0));
        assert_eq!(full_state.viewport.width, 80);
    }

    #[test]
    fn test_version_tracking() {
        let mut manager = EditorStateManager::new("Test", 80);

        assert_eq!(manager.version(), 0);
        assert!(!manager.has_changed_since(0));

        manager.mark_modified(StateChangeType::DocumentModified);

        assert_eq!(manager.version(), 1);
        assert!(manager.has_changed_since(0));
        assert!(!manager.has_changed_since(1));
    }

    #[test]
    fn test_modification_tracking() {
        let mut manager = EditorStateManager::new("Test", 80);

        assert!(!manager.get_document_state().is_modified);

        manager.mark_modified(StateChangeType::DocumentModified);
        assert!(manager.get_document_state().is_modified);

        manager.mark_saved();
        assert!(!manager.get_document_state().is_modified);
    }

    #[test]
    fn test_undo_redo_state_and_dirty_tracking() {
        let mut manager = EditorStateManager::empty(80);

        let state = manager.get_undo_redo_state();
        assert!(!state.can_undo);
        assert!(!state.can_redo);

        manager
            .execute(Command::Edit(EditCommand::InsertText {
                text: "abc".to_string(),
            }))
            .unwrap();

        assert!(manager.get_document_state().is_modified);
        let state = manager.get_undo_redo_state();
        assert!(state.can_undo);
        assert!(!state.can_redo);
        assert_eq!(state.undo_depth, 1);

        manager.execute(Command::Edit(EditCommand::Undo)).unwrap();
        assert!(!manager.get_document_state().is_modified);
        let state = manager.get_undo_redo_state();
        assert!(!state.can_undo);
        assert!(state.can_redo);

        manager.execute(Command::Edit(EditCommand::Redo)).unwrap();
        assert!(manager.get_document_state().is_modified);
        let state = manager.get_undo_redo_state();
        assert!(state.can_undo);
        assert!(!state.can_redo);
    }

    #[test]
    fn test_insert_tab_undo_restores_clean_state() {
        let mut manager = EditorStateManager::empty(80);
        assert!(!manager.get_document_state().is_modified);

        manager
            .execute(Command::Edit(EditCommand::InsertTab))
            .unwrap();
        assert!(manager.get_document_state().is_modified);

        manager.execute(Command::Edit(EditCommand::Undo)).unwrap();
        assert!(!manager.get_document_state().is_modified);
    }

    #[test]
    fn test_insert_tab_spaces_undo_restores_clean_state() {
        let mut manager = EditorStateManager::empty(80);
        manager
            .execute(Command::View(ViewCommand::SetTabKeyBehavior {
                behavior: crate::TabKeyBehavior::Spaces,
            }))
            .unwrap();

        manager
            .execute(Command::Edit(EditCommand::InsertTab))
            .unwrap();
        assert!(manager.get_document_state().is_modified);

        manager.execute(Command::Edit(EditCommand::Undo)).unwrap();
        assert!(!manager.get_document_state().is_modified);
    }

    #[test]
    fn test_state_change_callback() {
        use std::sync::{Arc, Mutex};

        let mut manager = EditorStateManager::new("Test", 80);

        let callback_called = Arc::new(Mutex::new(false));
        let callback_called_clone = callback_called.clone();

        manager.subscribe(move |_change| {
            *callback_called_clone.lock().unwrap() = true;
        });

        manager.mark_modified(StateChangeType::CursorMoved);

        // Verify callback was called
        assert!(*callback_called.lock().unwrap());
    }

    #[test]
    fn test_execute_cursor_noop_does_not_bump_version() {
        let mut manager = EditorStateManager::new("A", 80);
        assert_eq!(manager.version(), 0);

        // Continue moving left at the beginning of file (unchanged after clamp), version should not change.
        manager
            .execute(Command::Cursor(CursorCommand::MoveBy {
                delta_line: 0,
                delta_column: -1,
            }))
            .unwrap();
        assert_eq!(manager.editor().cursor_position(), Position::new(0, 0));
        assert_eq!(manager.version(), 0);

        // Move to end of line (changed), version increments.
        manager
            .execute(Command::Cursor(CursorCommand::MoveTo {
                line: 0,
                column: usize::MAX,
            }))
            .unwrap();
        assert_eq!(manager.editor().cursor_position(), Position::new(0, 1));
        assert_eq!(manager.version(), 1);

        // Continue moving right at end of line (unchanged after clamp), version should not change.
        let version_before = manager.version();
        manager
            .execute(Command::Cursor(CursorCommand::MoveBy {
                delta_line: 0,
                delta_column: 1,
            }))
            .unwrap();
        assert_eq!(manager.editor().cursor_position(), Position::new(0, 1));
        assert_eq!(manager.version(), version_before);
    }

    #[test]
    fn test_viewport_height() {
        let mut manager = EditorStateManager::new("Test", 80);

        assert_eq!(manager.get_viewport_state().height, None);

        manager.set_viewport_height(20);
        assert_eq!(manager.get_viewport_state().height, Some(20));
    }

    #[test]
    fn test_scroll_position() {
        let mut manager = EditorStateManager::new("Line 1\nLine 2\nLine 3\nLine 4", 80);
        manager.set_viewport_height(2);

        assert_eq!(manager.get_viewport_state().scroll_top, 0);
        assert_eq!(manager.get_viewport_state().visible_lines, 0..2);

        manager.set_scroll_top(2);
        assert_eq!(manager.get_viewport_state().scroll_top, 2);
        assert_eq!(manager.get_viewport_state().visible_lines, 2..4);
    }

    #[test]
    fn test_get_styles() {
        let mut manager = EditorStateManager::new("Hello World", 80);

        // Add style through the controlled editor-core API.
        manager
            .editor_mut()
            .insert_style_interval(crate::intervals::Interval::new(0, 5, 1));

        let styles = manager.get_styles_in_range(0, 10);
        assert_eq!(styles.len(), 1);
        assert_eq!(styles[0], (0, 5, 1));

        let styles_at = manager.get_styles_at(3);
        assert_eq!(styles_at.len(), 1);
        assert_eq!(styles_at[0], 1);
    }

    #[test]
    fn test_replace_style_layer_affects_queries() {
        let mut manager = EditorStateManager::new("Hello", 80);

        manager.replace_style_layer(
            StyleLayerId::SEMANTIC_TOKENS,
            vec![Interval::new(0, 1, 100)],
        );

        assert_eq!(manager.get_styles_at(0), vec![100]);

        // Base layer + layered styles are merged.
        manager
            .editor_mut()
            .insert_style_interval(Interval::new(0, 5, 1));

        assert_eq!(manager.get_styles_at(0), vec![1, 100]);
    }

    #[test]
    fn test_viewport_content_styled_wraps_and_includes_styles() {
        let mut manager = EditorStateManager::new("abcdef", 3);

        // Highlight "bcd" across a wrap boundary: "abc" | "def"
        manager.replace_style_layer(StyleLayerId::SIMPLE_SYNTAX, vec![Interval::new(1, 4, 7)]);

        let grid = manager.get_viewport_content_styled(0, 10);
        assert_eq!(grid.actual_line_count(), 2);

        let line0 = &grid.lines[0];
        assert_eq!(line0.logical_line_index, 0);
        assert!(!line0.is_wrapped_part);
        assert_eq!(line0.cells.len(), 3);
        assert_eq!(line0.cells[0].ch, 'a');
        assert_eq!(line0.cells[1].ch, 'b');
        assert_eq!(line0.cells[2].ch, 'c');
        assert_eq!(line0.cells[0].styles, Vec::<StyleId>::new());
        assert_eq!(line0.cells[1].styles, vec![7]);
        assert_eq!(line0.cells[2].styles, vec![7]);

        let line1 = &grid.lines[1];
        assert_eq!(line1.logical_line_index, 0);
        assert!(line1.is_wrapped_part);
        assert_eq!(line1.cells.len(), 3);
        assert_eq!(line1.cells[0].ch, 'd');
        assert_eq!(line1.cells[0].styles, vec![7]);
        assert_eq!(line1.cells[1].ch, 'e');
        assert_eq!(line1.cells[1].styles, Vec::<StyleId>::new());
    }

    #[test]
    fn test_smooth_scroll_state_and_prefetch_lines() {
        let mut manager = EditorStateManager::new("a\nb\nc\nd\n", 80);
        manager.set_viewport_height(2);
        manager.set_scroll_top(1);
        manager.set_scroll_sub_row_offset(123);
        manager.set_overscan_rows(2);

        let smooth = manager.get_smooth_scroll_state();
        assert_eq!(
            smooth,
            SmoothScrollState {
                top_visual_row: 1,
                sub_row_offset: 123,
                overscan_rows: 2
            }
        );

        let viewport = manager.get_viewport_state();
        assert_eq!(viewport.visible_lines, 1..3);
        assert_eq!(viewport.sub_row_offset, 123);
        assert_eq!(viewport.overscan_rows, 2);
        assert_eq!(viewport.prefetch_lines, 0..5);
        assert_eq!(viewport.total_visual_lines, 5);
    }

    #[test]
    fn test_minimap_content_returns_lightweight_summary() {
        let mut manager = EditorStateManager::new("abc def\n", 80);
        manager.replace_style_layer(StyleLayerId::SIMPLE_SYNTAX, vec![Interval::new(0, 3, 9)]);

        let minimap = manager.get_minimap_content(0, 1);
        assert_eq!(minimap.actual_line_count(), 1);
        let line = &minimap.lines[0];
        assert_eq!(line.logical_line_index, 0);
        assert_eq!(line.visual_in_logical, 0);
        assert_eq!(line.char_offset_start, 0);
        assert_eq!(line.char_offset_end, 7);
        assert!(line.total_cells >= line.non_whitespace_cells);
        assert_eq!(line.dominant_style, Some(9));
        assert!(!line.is_fold_placeholder_appended);
    }
}
