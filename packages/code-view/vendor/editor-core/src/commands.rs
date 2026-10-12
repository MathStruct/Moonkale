//! Command Interface Layer
//!
//! Provides a unified command interface for convenient frontend integration.
//!
//! # Overview
//!
//! The Command Interface Layer is the primary entry point for Editor Core, wrapping all underlying components in a unified command pattern.
//! It supports the following types of operations:
//!
//! - **Text Editing**: Insert, delete, and replace text
//! - **Cursor Operations**: Move cursor and set selection range
//! - **View Management**: Set viewport, scroll, and get visible content
//! - **Style Control**: Add/remove styles and code folding
//!
//! # Example
//!
//! ```rust
//! use editor_core::{CommandExecutor, Command, EditCommand};
//!
//! let mut executor = CommandExecutor::empty(80);
//!
//! // Insert text
//! executor.execute(Command::Edit(EditCommand::Insert {
//!     offset: 0,
//!     text: "Hello, World!".to_string(),
//! })).unwrap();
//!
//! // Batch execute commands
//! let commands = vec![
//!     Command::Edit(EditCommand::Insert { offset: 0, text: "Line 1\n".to_string() }),
//!     Command::Edit(EditCommand::Insert { offset: 7, text: "Line 2\n".to_string() }),
//! ];
//! executor.execute_batch(commands).unwrap();
//! ```

use crate::decorations::{Decoration, DecorationLayerId, DecorationPlacement};
use crate::delta::{TextDelta, TextDeltaEdit};
use crate::diagnostics::Diagnostic;
use crate::intervals::{FoldRegion, IntervalTextEdit, StyleId, StyleLayerId};
use crate::layout::{
    cell_width_at, char_width, visual_x_for_column, wrap_indent_cells_for_line_text,
};
use crate::line_ending::LineEnding;
use crate::search::{CharIndex, SearchMatch, SearchOptions, find_all, find_next, find_prev};
use crate::snapshot::{
    Cell, ComposedCell, ComposedCellSource, ComposedGrid, ComposedLine, ComposedLineKind,
    HeadlessGrid, HeadlessLine, MinimapGrid, MinimapLine,
};
use crate::snippets::{SnippetNavigation, SnippetSession, parse_snippet};
#[cfg(debug_assertions)]
use crate::storage::PieceTable;
use crate::visual_rows::VisualRowIndex;
use crate::{FOLD_PLACEHOLDER_STYLE_ID, FoldingManager, IntervalTree, LayoutEngine, LineIndex};
use editor_core_lang::{CommentConfig, IndentStyle, IndentationConfig};
use regex::RegexBuilder;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::time::Duration;
use unicode_segmentation::UnicodeSegmentation;

const DEFAULT_COMMAND_HISTORY_LIMIT: usize = 1000;
#[path = "model.rs"]
mod model;
pub use self::model::{
    AutoPair, AutoPairsConfig, Command, CommandError, CommandResult, CursorCommand, EditCommand,
    ExpandSelectionDirection, ExpandSelectionUnit, Position, Selection, SelectionDirection,
    StyleCommand, TabKeyBehavior, TextEditSpec, ViewCommand,
};

#[path = "undo.rs"]
mod undo;
use self::undo::{TextEdit, UndoRedoManager, UndoStep};
pub use self::undo::{
    UndoHistoryRestoreError, UndoHistorySelectionSet, UndoHistorySnapshot, UndoHistoryStep,
    UndoHistoryTextEdit,
};

#[path = "render_grid.rs"]
mod render_grid;

#[path = "cursor_ops.rs"]
mod cursor_ops;
pub use self::cursor_ops::WordBoundaryConfig;
use self::cursor_ops::{
    TextBoundary, leading_horizontal_whitespace, next_boundary_column, prev_boundary_column,
};

#[path = "line_ops.rs"]
mod line_ops;

#[path = "edit_ops.rs"]
mod edit_ops;

#[derive(Debug, Clone, PartialEq, Eq)]
struct SelectionSetSnapshot {
    selections: Vec<Selection>,
    primary_index: usize,
}

/// Editor Core state
///
/// `EditorCore` aggregates all underlying editor components, including:
///
/// - **LineIndex**: Rope-backed canonical text buffer with fast line access
/// - **LayoutEngine**: Soft wrapping and text layout calculation
/// - **IntervalTree**: Style interval management
/// - **FoldingManager**: Code folding management
/// - **Cursor & Selection**: Cursor and selection state
///
/// # Example
///
/// ```rust
/// use editor_core::EditorCore;
///
/// let mut core = EditorCore::new("Hello\nWorld", 80);
/// assert_eq!(core.line_count(), 2);
/// assert_eq!(core.get_text(), "Hello\nWorld");
/// ```
pub struct EditorCore {
    /// Debug-only deprecated PieceTable shadow used to catch migration regressions.
    #[cfg(debug_assertions)]
    piece_table_shadow: PieceTable,
    /// Line index
    line_index: LineIndex,
    /// Layout engine
    layout_engine: LayoutEngine,
    /// Interval tree (style management)
    interval_tree: IntervalTree,
    /// Layered styles (for semantic highlighting/simple syntax highlighting, etc.)
    style_layers: BTreeMap<StyleLayerId, IntervalTree>,
    /// Derived diagnostics for this document (character-offset ranges + metadata).
    diagnostics: Vec<Diagnostic>,
    /// Derived decorations for this document (virtual text, links, etc.).
    decorations: BTreeMap<DecorationLayerId, Vec<Decoration>>,
    /// Derived document symbols / outline for this document.
    document_symbols: crate::DocumentOutline,
    /// Folding manager
    folding_manager: FoldingManager,
    /// Current cursor position
    cursor_position: Position,
    /// Current selection range
    selection: Option<Selection>,
    /// Secondary selections/cursors (multi-cursor). Each Selection can be empty (start==end), representing a caret.
    secondary_selections: Vec<Selection>,
    /// Viewport width
    viewport_width: usize,
    word_boundary: WordBoundaryConfig,
    visual_row_index_cache: RefCell<Option<VisualRowIndex>>,
}

impl EditorCore {
    /// Create a new Editor Core
    pub fn new(text: &str, viewport_width: usize) -> Self {
        let normalized = crate::text::normalize_crlf_to_lf(text);
        let text = normalized.as_ref();

        #[cfg(debug_assertions)]
        let piece_table_shadow = PieceTable::new(text);
        let line_index = LineIndex::from_text(text);
        let mut layout_engine = LayoutEngine::new(viewport_width);

        // Initialize layout engine to be consistent with initial text (including trailing empty line).
        let lines = crate::text::split_lines_preserve_trailing(text);
        let line_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
        layout_engine.from_lines(&line_refs);

        Self {
            #[cfg(debug_assertions)]
            piece_table_shadow,
            line_index,
            layout_engine,
            interval_tree: IntervalTree::new(),
            style_layers: BTreeMap::new(),
            diagnostics: Vec::new(),
            decorations: BTreeMap::new(),
            document_symbols: crate::DocumentOutline::default(),
            folding_manager: FoldingManager::new(),
            cursor_position: Position::new(0, 0),
            selection: None,
            secondary_selections: Vec::new(),
            viewport_width,
            word_boundary: WordBoundaryConfig::default(),
            visual_row_index_cache: RefCell::new(None),
        }
    }

    /// Create an empty Editor Core
    pub fn empty(viewport_width: usize) -> Self {
        Self::new("", viewport_width)
    }

    /// Get text content
    pub fn get_text(&self) -> String {
        self.line_index.text_buffer().get_text()
    }

    /// Get a text range by character offset and length.
    pub fn text_range(&self, start: usize, len: usize) -> String {
        self.line_index.text_buffer().get_range(start, len)
    }

    /// Get total line count
    pub fn line_count(&self) -> usize {
        self.line_index.line_count()
    }

    /// Get total character count
    pub fn char_count(&self) -> usize {
        self.line_index.text_buffer().len_chars()
    }

    /// Override the ASCII word-boundary character set used by editor-friendly "word" operations.
    ///
    /// See [`WordBoundaryConfig::set_ascii_boundary_chars`].
    pub fn set_word_boundary_ascii_boundary_chars(&mut self, boundary_chars: &str) {
        self.word_boundary.set_ascii_boundary_chars(boundary_chars);
    }

    /// Reset word-boundary configuration to the default (ASCII identifier-like words).
    pub fn reset_word_boundary_defaults(&mut self) {
        self.word_boundary = WordBoundaryConfig::default();
    }

    /// Get cursor position
    pub fn cursor_position(&self) -> Position {
        self.cursor_position
    }

    /// Get selection range
    pub fn selection(&self) -> Option<&Selection> {
        self.selection.as_ref()
    }

    /// Get secondary selections/cursors (multi-cursor)
    pub fn secondary_selections(&self) -> &[Selection] {
        &self.secondary_selections
    }

    /// Replace cursor and selection state as one view-local snapshot.
    pub(crate) fn set_cursor_state(
        &mut self,
        cursor_position: Position,
        selection: Option<Selection>,
        secondary_selections: Vec<Selection>,
    ) {
        self.cursor_position = cursor_position;
        self.selection = selection;
        self.secondary_selections = secondary_selections;
    }

    /// Get the canonical line index and text access facade.
    pub fn line_index(&self) -> &LineIndex {
        &self.line_index
    }

    /// Get the current layout engine state.
    pub fn layout_engine(&self) -> &LayoutEngine {
        &self.layout_engine
    }

    /// Get the base style interval tree.
    pub fn interval_tree(&self) -> &IntervalTree {
        &self.interval_tree
    }

    /// Get all style layers.
    pub fn style_layers(&self) -> &BTreeMap<StyleLayerId, IntervalTree> {
        &self.style_layers
    }

    /// Get one style layer by id.
    pub fn style_layer(&self, layer: StyleLayerId) -> Option<&IntervalTree> {
        self.style_layers.get(&layer)
    }

    /// Get the current diagnostics list.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Get all decoration layers.
    pub fn decorations(&self) -> &BTreeMap<DecorationLayerId, Vec<Decoration>> {
        &self.decorations
    }

    /// Get all decorations for a given layer.
    pub fn decorations_for_layer(&self, layer: DecorationLayerId) -> &[Decoration] {
        self.decorations
            .get(&layer)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Get the current document outline.
    pub fn document_symbols(&self) -> &crate::DocumentOutline {
        &self.document_symbols
    }

    /// Get the folding manager as a read-only view.
    pub fn folding_manager(&self) -> &FoldingManager {
        &self.folding_manager
    }

    /// Get the current viewport width in cells.
    pub fn viewport_width(&self) -> usize {
        self.viewport_width
    }

    /// Replace view layout options and reflow from the canonical text source when needed.
    pub(crate) fn set_view_options(
        &mut self,
        viewport_width: usize,
        wrap_mode: crate::WrapMode,
        wrap_indent: crate::WrapIndent,
        tab_width: usize,
    ) {
        let viewport_width = viewport_width.max(1);
        let tab_width = tab_width.max(1);

        let changed = self.viewport_width != viewport_width
            || self.layout_engine.viewport_width() != viewport_width
            || self.layout_engine.wrap_mode() != wrap_mode
            || self.layout_engine.wrap_indent() != wrap_indent
            || self.layout_engine.tab_width() != tab_width;

        self.viewport_width = viewport_width;
        self.layout_engine.set_viewport_width(viewport_width);
        self.layout_engine.set_wrap_mode(wrap_mode);
        self.layout_engine.set_wrap_indent(wrap_indent);
        self.layout_engine.set_tab_width(tab_width);

        if changed {
            self.reflow_layout_from_line_index();
        }
    }

    /// Insert a base style interval through the controlled style path.
    pub(crate) fn insert_style_interval(&mut self, interval: crate::intervals::Interval) {
        self.interval_tree.insert(interval);
    }

    /// Remove a base style interval through the controlled style path.
    pub(crate) fn remove_style_interval(&mut self, start: usize, end: usize, style_id: StyleId) {
        self.interval_tree.remove(start, end, style_id);
    }

    /// Replace one derived style layer; empty intervals clear the layer.
    pub(crate) fn replace_style_layer(
        &mut self,
        layer: StyleLayerId,
        intervals: Vec<crate::intervals::Interval>,
    ) {
        if intervals.is_empty() {
            self.style_layers.remove(&layer);
            return;
        }

        let tree = self.style_layers.entry(layer).or_default();
        tree.clear();
        for interval in intervals {
            if interval.start < interval.end {
                tree.insert(interval);
            }
        }
    }

    /// Clear one derived style layer.
    pub(crate) fn clear_style_layer(&mut self, layer: StyleLayerId) {
        self.style_layers.remove(&layer);
    }

    /// Replace diagnostics wholesale.
    pub(crate) fn replace_diagnostics(&mut self, diagnostics: Vec<Diagnostic>) {
        self.diagnostics = diagnostics;
    }

    /// Clear all diagnostics.
    pub(crate) fn clear_diagnostics(&mut self) {
        self.diagnostics.clear();
    }

    /// Replace a decoration layer, sorting it into deterministic range order.
    pub(crate) fn replace_decorations(
        &mut self,
        layer: DecorationLayerId,
        mut decorations: Vec<Decoration>,
    ) {
        decorations.sort_unstable_by_key(|d| (d.range.start, d.range.end));
        self.decorations.insert(layer, decorations);
    }

    /// Clear one decoration layer.
    pub(crate) fn clear_decorations(&mut self, layer: DecorationLayerId) {
        self.decorations.remove(&layer);
    }

    /// Replace document symbols / outline wholesale.
    pub(crate) fn replace_document_symbols(&mut self, symbols: crate::DocumentOutline) {
        self.document_symbols = symbols;
    }

    /// Clear document symbols / outline.
    pub(crate) fn clear_document_symbols(&mut self) {
        self.document_symbols = crate::DocumentOutline::default();
    }

    /// Replace derived folding regions and invalidate visual-row mappings.
    pub(crate) fn replace_folding_regions(
        &mut self,
        regions: Vec<FoldRegion>,
        preserve_collapsed: bool,
    ) {
        if preserve_collapsed {
            self.folding_manager
                .replace_derived_regions_preserving_collapsed(regions);
        } else {
            self.folding_manager.replace_derived_regions(regions);
        }
        self.invalidate_visual_row_index_cache();
    }

    /// Clear derived folding regions and invalidate visual-row mappings.
    pub(crate) fn clear_derived_folding_regions(&mut self) {
        self.folding_manager.clear_derived_regions();
        self.invalidate_visual_row_index_cache();
    }

    /// Toggle a fold starting at `line` and refresh affected visual-row mappings.
    pub(crate) fn toggle_fold_at_line(&mut self, line: usize) -> bool {
        let affected = self
            .folding_manager
            .regions()
            .iter()
            .filter(|region| region.start_line == line && region.end_line > region.start_line)
            .min_by_key(|region| region.end_line)
            .map(|region| (region.start_line, region.end_line));
        let toggled = self.folding_manager.toggle_region_starting_at_line(line);
        if toggled {
            if let Some((start, end)) = affected {
                self.sync_visual_row_index_for_logical_range(start, end);
            } else {
                self.invalidate_visual_row_index_cache();
            }
        }
        toggled
    }

    /// Expand all folds and refresh visual-row mappings.
    pub(crate) fn expand_all_folds(&mut self) {
        let had_collapsed = self
            .folding_manager
            .regions()
            .iter()
            .any(|region| region.is_collapsed);
        self.folding_manager.expand_all();
        if had_collapsed {
            self.invalidate_visual_row_index_cache();
        }
    }

    /// Invalidate cached visual-row index (wrap/folding derived mapping).
    pub fn invalidate_visual_row_index_cache(&mut self) {
        *self.visual_row_index_cache.borrow_mut() = None;
    }

    fn visual_row_count_for_logical_line(&self, logical_line: usize) -> usize {
        if logical_line >= self.layout_engine.logical_line_count() {
            return 0;
        }
        if Self::is_logical_line_hidden(self.folding_manager.regions(), logical_line) {
            return 0;
        }

        self.layout_engine
            .get_line_layout(logical_line)
            .map(|layout| layout.visual_line_count)
            .unwrap_or(1)
            .max(1)
    }

    fn sync_visual_row_index_for_logical_range(&mut self, start_line: usize, end_line: usize) {
        if self.visual_row_index_cache.borrow().is_none() {
            return;
        }

        let line_count = self.layout_engine.logical_line_count();
        if line_count == 0 || start_line >= line_count {
            return;
        }

        let end_line = end_line.min(line_count.saturating_sub(1));
        let counts = (start_line..=end_line)
            .map(|line| (line, self.visual_row_count_for_logical_line(line)))
            .collect::<Vec<_>>();

        let mut cache = self.visual_row_index_cache.borrow_mut();
        let Some(index) = cache.as_mut() else {
            return;
        };

        if index.logical_line_count() != line_count {
            *cache = None;
            return;
        }

        for (line, count) in counts {
            if !index.set_line_visual_count(line, count) {
                *cache = None;
                return;
            }
        }
    }

    fn sync_visual_row_index_after_text_change(
        &mut self,
        start_line: usize,
        deleted_newlines: usize,
        inserted_newlines: usize,
    ) {
        if self.visual_row_index_cache.borrow().is_none() {
            return;
        }

        let line_delta = inserted_newlines as isize - deleted_newlines as isize;
        if line_delta != 0 {
            let line_count = self.layout_engine.logical_line_count();
            let mut cache = self.visual_row_index_cache.borrow_mut();
            let Some(index) = cache.as_mut() else {
                return;
            };

            if line_delta > 0 {
                let inserted = line_delta as usize;
                if index.logical_line_count().saturating_add(inserted) != line_count {
                    *cache = None;
                    return;
                }
                index.insert_lines(
                    start_line.saturating_add(1),
                    std::iter::repeat_n(0, inserted),
                );
            } else {
                let removed = (-line_delta) as usize;
                if index.logical_line_count().saturating_sub(removed) != line_count
                    || !index.remove_lines(start_line.saturating_add(1), removed)
                {
                    *cache = None;
                    return;
                }
            }
        }

        let touch_lines = deleted_newlines.max(inserted_newlines).saturating_add(1);
        self.sync_visual_row_index_for_logical_range(
            start_line,
            start_line.saturating_add(touch_lines),
        );
    }

    /// Reflow every logical line from the canonical line index after layout options change.
    pub(crate) fn reflow_layout_from_line_index(&mut self) {
        let lines: Vec<String> = (0..self.line_index.line_count())
            .map(|line| self.line_index.get_line_text(line).unwrap_or_default())
            .collect();
        self.layout_engine
            .recalculate_all_from_lines(lines.iter().map(String::as_str));
        self.invalidate_visual_row_index_cache();
    }

    fn with_visual_row_index<R>(&self, f: impl FnOnce(&VisualRowIndex) -> R) -> R {
        if self.visual_row_index_cache.borrow().is_none() {
            let index = self.build_visual_row_index();
            *self.visual_row_index_cache.borrow_mut() = Some(index);
        }
        let cache = self.visual_row_index_cache.borrow();
        let index = cache
            .as_ref()
            .expect("visual-row cache should be initialized");
        f(index)
    }

    fn build_visual_row_index(&self) -> VisualRowIndex {
        let counts = (0..self.layout_engine.logical_line_count())
            .map(|logical_line| self.visual_row_count_for_logical_line(logical_line))
            .collect();
        VisualRowIndex::from_line_visual_counts(counts)
    }

    /// Get total visual line count (considering soft wrapping + folding).
    pub fn visual_line_count(&self) -> usize {
        self.with_visual_row_index(|index| index.total_visual_lines())
    }

    /// Map visual line number back to (logical_line, visual_in_logical), considering folding.
    pub fn visual_to_logical_line(&self, visual_line: usize) -> (usize, usize) {
        self.with_visual_row_index(|index| {
            if index.total_visual_lines() == 0 {
                return (0, 0);
            }
            let clamped_visual = visual_line.min(index.total_visual_lines().saturating_sub(1));
            index
                .span_for_visual_row(clamped_visual)
                .map(|(span, visual_in_logical)| (span.logical_line, visual_in_logical))
                .unwrap_or((0, 0))
        })
    }

    /// Convert logical coordinates (line, column) to visual coordinates (visual line number, in-line x cell offset), considering folding.
    pub fn logical_position_to_visual(
        &self,
        logical_line: usize,
        column: usize,
    ) -> Option<(usize, usize)> {
        let regions = self.folding_manager.regions();
        let logical_line = Self::closest_visible_line(regions, logical_line)?;
        let visual_start = self.visual_start_for_logical_line(logical_line)?;

        let tab_width = self.layout_engine.tab_width();

        let layout = self.layout_engine.get_line_layout(logical_line)?;
        let line_text = self
            .line_index
            .get_line_text(logical_line)
            .unwrap_or_default();

        let line_char_len = line_text.chars().count();
        let column = column.min(line_char_len);

        let mut wrapped_offset = 0usize;
        let mut segment_start_col = 0usize;
        for wrap_point in &layout.wrap_points {
            if column >= wrap_point.char_index {
                wrapped_offset = wrapped_offset.saturating_add(1);
                segment_start_col = wrap_point.char_index;
            } else {
                break;
            }
        }

        let seg_start_x_in_line = visual_x_for_column(&line_text, segment_start_col, tab_width);
        let mut x_in_line = seg_start_x_in_line;
        let mut x_in_segment = 0usize;
        for ch in line_text
            .chars()
            .skip(segment_start_col)
            .take(column.saturating_sub(segment_start_col))
        {
            let w = cell_width_at(ch, x_in_line, tab_width);
            x_in_line = x_in_line.saturating_add(w);
            x_in_segment = x_in_segment.saturating_add(w);
        }

        let indent = if wrapped_offset == 0 {
            0
        } else {
            wrap_indent_cells_for_line_text(
                &line_text,
                self.layout_engine.wrap_indent(),
                self.viewport_width,
                tab_width,
            )
        };

        Some((
            visual_start.saturating_add(wrapped_offset),
            indent.saturating_add(x_in_segment),
        ))
    }

    /// Convert logical coordinates (line, column) to visual coordinates (visual line number, in-line x cell offset), considering folding.
    ///
    /// Difference from [`logical_position_to_visual`](Self::logical_position_to_visual) is that it allows `column`
    /// to exceed the line end: the exceeding part is treated as `' '` (width=1) virtual spaces, suitable for rectangular selection / column editing.
    pub fn logical_position_to_visual_allow_virtual(
        &self,
        logical_line: usize,
        column: usize,
    ) -> Option<(usize, usize)> {
        let regions = self.folding_manager.regions();
        let logical_line = Self::closest_visible_line(regions, logical_line)?;
        let visual_start = self.visual_start_for_logical_line(logical_line)?;

        let tab_width = self.layout_engine.tab_width();

        let layout = self.layout_engine.get_line_layout(logical_line)?;
        let line_text = self
            .line_index
            .get_line_text(logical_line)
            .unwrap_or_default();

        let line_char_len = line_text.chars().count();
        let clamped_column = column.min(line_char_len);

        let mut wrapped_offset = 0usize;
        let mut segment_start_col = 0usize;
        for wrap_point in &layout.wrap_points {
            if clamped_column >= wrap_point.char_index {
                wrapped_offset = wrapped_offset.saturating_add(1);
                segment_start_col = wrap_point.char_index;
            } else {
                break;
            }
        }

        let seg_start_x_in_line = visual_x_for_column(&line_text, segment_start_col, tab_width);
        let mut x_in_line = seg_start_x_in_line;
        let mut x_in_segment = 0usize;
        for ch in line_text
            .chars()
            .skip(segment_start_col)
            .take(clamped_column.saturating_sub(segment_start_col))
        {
            let w = cell_width_at(ch, x_in_line, tab_width);
            x_in_line = x_in_line.saturating_add(w);
            x_in_segment = x_in_segment.saturating_add(w);
        }

        let x_in_segment = x_in_segment + column.saturating_sub(line_char_len);

        let indent = if wrapped_offset == 0 {
            0
        } else {
            wrap_indent_cells_for_line_text(
                &line_text,
                self.layout_engine.wrap_indent(),
                self.viewport_width,
                tab_width,
            )
        };

        Some((
            visual_start.saturating_add(wrapped_offset),
            indent.saturating_add(x_in_segment),
        ))
    }

    /// Convert visual coordinates (global visual row + x in cells) back to logical `(line, column)`.
    ///
    /// - `visual_row` is the global visual row (after soft wrapping and folding).
    /// - `x_in_cells` is the cell offset within that visual row (0-based).
    ///
    /// Returns `None` if layout information is unavailable.
    pub fn visual_position_to_logical(
        &self,
        visual_row: usize,
        x_in_cells: usize,
    ) -> Option<Position> {
        let total_visual = self.visual_line_count();
        if total_visual == 0 {
            return Some(Position::new(0, 0));
        }

        let clamped_row = visual_row.min(total_visual.saturating_sub(1));
        let (logical_line, visual_in_logical) = self.visual_to_logical_line(clamped_row);

        let layout = self.layout_engine.get_line_layout(logical_line)?;
        let line_text = self
            .line_index
            .get_line_text(logical_line)
            .unwrap_or_default();
        let line_char_len = line_text.chars().count();

        let segment_start_col = if visual_in_logical == 0 {
            0
        } else {
            layout
                .wrap_points
                .get(visual_in_logical - 1)
                .map(|wp| wp.char_index)
                .unwrap_or(0)
        };

        let segment_end_col = layout
            .wrap_points
            .get(visual_in_logical)
            .map(|wp| wp.char_index)
            .unwrap_or(line_char_len)
            .max(segment_start_col)
            .min(line_char_len);

        let tab_width = self.layout_engine.tab_width();
        let x_in_cells = if visual_in_logical == 0 {
            x_in_cells
        } else {
            let indent = wrap_indent_cells_for_line_text(
                &line_text,
                self.layout_engine.wrap_indent(),
                self.viewport_width,
                tab_width,
            );
            x_in_cells.saturating_sub(indent)
        };
        let seg_start_x_in_line = visual_x_for_column(&line_text, segment_start_col, tab_width);
        let mut x_in_line = seg_start_x_in_line;
        let mut x_in_segment = 0usize;
        let mut column = segment_start_col;

        for (char_idx, ch) in line_text.chars().enumerate().skip(segment_start_col) {
            if char_idx >= segment_end_col {
                break;
            }

            let w = cell_width_at(ch, x_in_line, tab_width);
            if x_in_segment.saturating_add(w) > x_in_cells {
                break;
            }

            x_in_line = x_in_line.saturating_add(w);
            x_in_segment = x_in_segment.saturating_add(w);
            column = column.saturating_add(1);
        }

        Some(Position::new(logical_line, column))
    }

    fn visual_start_for_logical_line(&self, logical_line: usize) -> Option<usize> {
        if logical_line >= self.layout_engine.logical_line_count() {
            return None;
        }
        self.with_visual_row_index(|index| {
            index
                .span_for_logical_line(logical_line)
                .map(|span| span.start_visual_row)
        })
    }

    fn is_logical_line_hidden(regions: &[FoldRegion], logical_line: usize) -> bool {
        regions.iter().any(|region| {
            region.is_collapsed
                && logical_line > region.start_line
                && logical_line <= region.end_line
        })
    }

    fn collapsed_region_starting_at(
        regions: &[FoldRegion],
        start_line: usize,
    ) -> Option<&FoldRegion> {
        regions
            .iter()
            .filter(|region| {
                region.is_collapsed
                    && region.start_line == start_line
                    && region.end_line > start_line
            })
            .min_by_key(|region| region.end_line)
    }

    fn closest_visible_line(regions: &[FoldRegion], logical_line: usize) -> Option<usize> {
        let mut line = logical_line;
        if regions.is_empty() {
            return Some(line);
        }

        while Self::is_logical_line_hidden(regions, line) {
            let Some(start) = regions
                .iter()
                .filter(|region| {
                    region.is_collapsed && line > region.start_line && line <= region.end_line
                })
                .map(|region| region.start_line)
                .max()
            else {
                break;
            };
            line = start;
        }

        if Self::is_logical_line_hidden(regions, line) {
            None
        } else {
            Some(line)
        }
    }

    fn fold_right_boundary_bracket_char(&self, region: &FoldRegion) -> Option<char> {
        let end_line_text = self.line_index.get_line_text(region.end_line)?;

        // Common formatting: closing brace is the first non-whitespace char on the end line.
        if let Some(ch) = end_line_text.chars().find(|c| !c.is_whitespace())
            && matches!(ch, '}' | ')' | ']')
        {
            return Some(ch);
        }

        // Fallback: scan from the end, skipping common trailing punctuation.
        for ch in end_line_text.chars().rev() {
            if ch.is_whitespace() {
                continue;
            }
            if matches!(ch, '}' | ')' | ']') {
                return Some(ch);
            }
            if matches!(ch, ';' | ',') {
                continue;
            }
            break;
        }

        None
    }

    fn styles_at_offset(&self, offset: usize) -> Vec<StyleId> {
        let mut styles: Vec<StyleId> = self
            .interval_tree
            .query_point(offset)
            .iter()
            .map(|interval| interval.style_id)
            .collect();

        for tree in self.style_layers.values() {
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
}

/// Command executor
///
/// `CommandExecutor` is the main interface for the editor, responsible for:
///
/// - Execute various editor commands
/// - Maintain command history
/// - Handle errors and exceptions
/// - Ensure editor state consistency
///
/// # Command Types
///
/// - [`EditCommand`] - Text insertion, deletion, replacement
/// - [`CursorCommand`] - Cursor movement, selection operations
/// - [`ViewCommand`] - Viewport management and scroll control
/// - [`StyleCommand`] - Style and folding management
///
/// # Example
///
/// ```rust
/// use editor_core::{CommandExecutor, Command, EditCommand, CursorCommand, Position};
///
/// let mut executor = CommandExecutor::empty(80);
///
/// // Insert text
/// executor.execute(Command::Edit(EditCommand::Insert {
///     offset: 0,
///     text: "fn main() {}".to_string(),
/// })).unwrap();
///
/// // Move cursor
/// executor.execute(Command::Cursor(CursorCommand::MoveTo {
///     line: 0,
///     column: 3,
/// })).unwrap();
///
/// assert_eq!(executor.editor().cursor_position(), Position::new(0, 3));
/// ```
pub struct CommandExecutor {
    /// Editor Core
    editor: EditorCore,
    /// Bounded command history for debug/inspection APIs.
    command_history: Vec<Command>,
    /// Maximum number of commands retained in `command_history`; zero disables history.
    command_history_limit: usize,
    /// Undo/redo manager (only records CommandExecutor edit commands executed via)
    undo_redo: UndoRedoManager,
    /// Controls how [`EditCommand::InsertTab`] behaves.
    tab_key_behavior: TabKeyBehavior,
    /// Language-aware indentation config used by [`EditCommand::InsertNewline`] when `auto_indent=true`.
    indentation_config: IndentationConfig,
    /// Auto-pairs configuration used by [`EditCommand::TypeChar`] and delete-pair behavior.
    auto_pairs: AutoPairsConfig,
    /// Active snippet session (placeholders + navigation), if any.
    snippet_session: Option<SnippetSession>,
    /// Preferred line ending for saving (internal storage is always LF).
    line_ending: LineEnding,
    /// Sticky x position for visual-row cursor movement (in cells).
    preferred_x_cells: Option<usize>,
    /// Structured delta for the last executed text modification (cleared on each `execute()` call).
    last_text_delta: Option<TextDelta>,
}

impl CommandExecutor {
    /// Create a new command executor
    pub fn new(text: &str, viewport_width: usize) -> Self {
        Self {
            editor: EditorCore::new(text, viewport_width),
            command_history: Vec::with_capacity(DEFAULT_COMMAND_HISTORY_LIMIT),
            command_history_limit: DEFAULT_COMMAND_HISTORY_LIMIT,
            undo_redo: UndoRedoManager::new(1000),
            tab_key_behavior: TabKeyBehavior::Spaces,
            indentation_config: IndentationConfig::default(),
            auto_pairs: AutoPairsConfig::default(),
            snippet_session: None,
            line_ending: LineEnding::detect_in_text(text),
            preferred_x_cells: None,
            last_text_delta: None,
        }
    }

    /// Create an empty command executor
    pub fn empty(viewport_width: usize) -> Self {
        Self::new("", viewport_width)
    }

    fn update_interval_trees_for_text_edits(&mut self, edits: &[IntervalTextEdit]) {
        if edits.is_empty() {
            return;
        }

        self.editor.interval_tree.update_for_text_edits(edits);
        for layer_tree in self.editor.style_layers.values_mut() {
            layer_tree.update_for_text_edits(edits);
        }
    }

    fn record_command_history(&mut self, command: &Command) {
        if self.command_history_limit == 0 {
            return;
        }

        self.command_history.push(command.history_summary());
        self.trim_command_history_to_limit();
    }

    fn trim_command_history_to_limit(&mut self) {
        if self.command_history_limit == 0 {
            self.command_history.clear();
            return;
        }

        let excess = self
            .command_history
            .len()
            .saturating_sub(self.command_history_limit);
        if excess > 0 {
            self.command_history.drain(..excess);
        }
    }

    /// Execute command
    pub fn execute(&mut self, command: Command) -> Result<CommandResult, CommandError> {
        self.last_text_delta = None;

        // Snippet sessions are view-local and should generally end when the user performs an
        // explicit navigation outside snippet tabstop traversal, or when history/programmatic
        // edits occur (undo/redo, bulk apply edits, ...).
        if matches!(
            &command,
            Command::Cursor(
                CursorCommand::SnippetNextPlaceholder | CursorCommand::SnippetPrevPlaceholder
            )
        ) {
            // keep session
        } else if matches!(&command, Command::Cursor(_))
            || matches!(
                &command,
                Command::Edit(
                    EditCommand::Undo | EditCommand::Redo | EditCommand::ApplyTextEdits { .. }
                )
            )
        {
            self.snippet_session = None;
        }

        // Save a bounded summary before execution so failed commands remain observable.
        self.record_command_history(&command);

        let skip_snippet_delta =
            matches!(&command, Command::Edit(EditCommand::ApplySnippet { .. }));

        // Undo grouping:
        //
        // Coalescing groups are meant to represent a "continuous editing" session (typing / IME
        // composition updates). UI frameworks may issue non-edit commands during editing (e.g.
        // viewport width updates every frame for soft-wrapping), and those should *not* break
        // the current coalescing group.
        //
        // Cursor/selection/navigation and history traversal commands indicate the next insertion
        // should start a fresh group; other non-insert edit commands close the group when pushed.
        if matches!(
            command,
            Command::Cursor(_) | Command::Edit(EditCommand::Undo | EditCommand::Redo)
        ) {
            self.undo_redo.end_group();
        }

        // Execute command
        let result = match command {
            Command::Edit(edit_cmd) => self.execute_edit(edit_cmd),
            Command::Cursor(cursor_cmd) => self.execute_cursor(cursor_cmd),
            Command::View(view_cmd) => self.execute_view(view_cmd),
            Command::Style(style_cmd) => self.execute_style(style_cmd),
        }?;

        // Keep snippet placeholder ranges stable under subsequent edits.
        //
        // Note: snippet insertion itself (`ApplySnippet`) creates anchors in **post-edit**
        // coordinates, so we must not apply the delta again for that command.
        if !skip_snippet_delta
            && let (Some(delta), Some(session)) =
                (self.last_text_delta.as_ref(), self.snippet_session.as_mut())
        {
            session.apply_delta(delta);
        }

        Ok(result)
    }

    /// Get the structured text delta produced by the last successful `execute()` call, if any.
    pub fn last_text_delta(&self) -> Option<&TextDelta> {
        self.last_text_delta.as_ref()
    }

    /// Take the structured text delta produced by the last successful `execute()` call, if any.
    pub fn take_last_text_delta(&mut self) -> Option<TextDelta> {
        self.last_text_delta.take()
    }

    /// Batch execute commands (transactional)
    pub fn execute_batch(
        &mut self,
        commands: Vec<Command>,
    ) -> Result<Vec<CommandResult>, CommandError> {
        let mut results = Vec::new();

        for command in commands {
            let result = self.execute(command)?;
            results.push(result);
        }

        Ok(results)
    }

    /// Get the bounded command history.
    ///
    /// Large text payloads are stored as summaries so this debug-oriented history does not keep
    /// another full copy of pasted or inserted text.
    pub fn get_command_history(&self) -> &[Command] {
        &self.command_history
    }

    /// Get the maximum number of commands retained in history.
    pub fn command_history_limit(&self) -> usize {
        self.command_history_limit
    }

    /// Set the maximum number of commands retained in history; `0` disables history recording.
    pub fn set_command_history_limit(&mut self, limit: usize) {
        self.command_history_limit = limit;
        self.trim_command_history_to_limit();
    }

    /// Can undo
    pub fn can_undo(&self) -> bool {
        self.undo_redo.can_undo()
    }

    /// Discard UI-engine history when the host owns canonical undo transactions.
    pub fn discard_undo_history(&mut self) {
        self.undo_redo = UndoRedoManager::new(1);
        self.command_history.clear();
    }

    /// Can redo
    pub fn can_redo(&self) -> bool {
        self.undo_redo.can_redo()
    }

    /// Undo stack depth (counted by undo steps; grouped undo may pop multiple steps at once)
    pub fn undo_depth(&self) -> usize {
        self.undo_redo.undo_depth()
    }

    /// Redo stack depth (counted by undo steps)
    pub fn redo_depth(&self) -> usize {
        self.undo_redo.redo_depth()
    }

    /// Number of redo branches available at the current history node.
    ///
    /// - In a purely linear history, this is `0` or `1`.
    /// - When you undo and then make a new edit, the previous redo path becomes an **alternate
    ///   branch** (undo tree).
    pub fn redo_branch_count(&self) -> usize {
        self.undo_redo.redo_branch_count()
    }

    /// Index of the currently selected redo branch at the current node, if any.
    pub fn selected_redo_branch_index(&self) -> Option<usize> {
        self.undo_redo.selected_redo_branch_index()
    }

    /// Select which redo branch `EditCommand::Redo` will follow from the current node.
    pub fn select_redo_branch(&mut self, index: usize) -> Result<(), CommandError> {
        self.undo_redo.end_group();
        self.undo_redo.select_redo_branch(index)
    }

    /// Currently open undo group ID (for insert coalescing only)
    pub fn current_change_group(&self) -> Option<usize> {
        self.undo_redo.current_group_id()
    }

    /// Timeout used when deciding whether adjacent insertion commands remain in one undo group.
    pub fn undo_coalescing_timeout(&self) -> Duration {
        self.undo_redo.coalescing_timeout()
    }

    /// Configure the insertion coalescing timeout; `Duration::ZERO` disables time-based merging.
    pub fn set_undo_coalescing_timeout(&mut self, timeout: Duration) {
        self.undo_redo.set_coalescing_timeout(timeout);
    }

    /// Whether current state is at clean point (for dirty tracking)
    pub fn is_clean(&self) -> bool {
        self.undo_redo.is_clean()
    }

    /// Mark current state as clean point (call after saving file)
    pub fn mark_clean(&mut self) {
        self.undo_redo.mark_clean();
    }

    /// Capture a persistable snapshot of the undo/redo history for this document.
    ///
    /// Callers are expected to persist the current document text separately.
    pub fn undo_history_snapshot(&self) -> UndoHistorySnapshot {
        self.undo_redo.snapshot()
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
        self.undo_redo.restore_from_snapshot(snapshot)
    }

    /// Get a reference to the Editor Core
    pub fn editor(&self) -> &EditorCore {
        &self.editor
    }

    /// Get a mutable reference to the field-private Editor Core.
    ///
    /// Prefer [`execute`](Self::execute) for command-driven mutations that must keep text, layout,
    /// cursor, selection, folding, style, and undo state synchronized. This accessor is intended for
    /// advanced callers that need to invoke public [`EditorCore`] methods directly; it does not
    /// expose private fields.
    pub fn editor_mut(&mut self) -> &mut EditorCore {
        &mut self.editor
    }

    /// Get current tab key behavior used by [`EditCommand::InsertTab`].
    pub fn tab_key_behavior(&self) -> TabKeyBehavior {
        self.tab_key_behavior
    }

    /// Set tab key behavior used by [`EditCommand::InsertTab`].
    pub fn set_tab_key_behavior(&mut self, behavior: TabKeyBehavior) {
        self.tab_key_behavior = behavior;
    }

    /// Get the current indentation configuration used by [`EditCommand::InsertNewline`] when
    /// `auto_indent=true`.
    pub fn indentation_config(&self) -> &IndentationConfig {
        &self.indentation_config
    }

    /// Replace the indentation configuration used by [`EditCommand::InsertNewline`] when
    /// `auto_indent=true`.
    pub fn set_indentation_config(&mut self, config: IndentationConfig) {
        self.indentation_config = config;
    }

    /// Get the current auto-pairs configuration.
    pub fn auto_pairs_config(&self) -> &AutoPairsConfig {
        &self.auto_pairs
    }

    /// Replace the auto-pairs configuration.
    pub fn set_auto_pairs_config(&mut self, config: AutoPairsConfig) {
        self.auto_pairs = config;
    }

    /// Enable/disable auto-pairs behavior (convenience wrapper).
    pub fn set_auto_pairs_enabled(&mut self, enabled: bool) {
        self.auto_pairs.enabled = enabled;
    }

    /// Return `true` if a snippet session is currently active for this view.
    pub fn has_active_snippet_session(&self) -> bool {
        self.snippet_session
            .as_ref()
            .map(|s| s.is_active())
            .unwrap_or(false)
    }

    /// Get the current snippet session (placeholders + navigation), if any.
    pub fn snippet_session(&self) -> Option<&SnippetSession> {
        self.snippet_session.as_ref()
    }

    /// Replace the current snippet session.
    pub fn set_snippet_session(&mut self, session: Option<SnippetSession>) {
        self.snippet_session = session;
    }

    /// Get the sticky x position (in cells) used by visual-row cursor movement.
    pub fn preferred_x_cells(&self) -> Option<usize> {
        self.preferred_x_cells
    }

    /// Set the sticky x position (in cells) used by visual-row cursor movement.
    pub fn set_preferred_x_cells(&mut self, preferred_x_cells: Option<usize>) {
        self.preferred_x_cells = preferred_x_cells;
    }

    /// Get the preferred line ending for saving this document.
    pub fn line_ending(&self) -> LineEnding {
        self.line_ending
    }

    /// Override the preferred line ending for saving this document.
    pub fn set_line_ending(&mut self, line_ending: LineEnding) {
        self.line_ending = line_ending;
    }

    // Private method: execute edit command
    fn execute_view(&mut self, command: ViewCommand) -> Result<CommandResult, CommandError> {
        match command {
            ViewCommand::SetViewportWidth { width } => {
                if width == 0 {
                    return Err(CommandError::Other(
                        "Viewport width must be greater than 0".to_string(),
                    ));
                }

                self.editor.set_view_options(
                    width,
                    self.editor.layout_engine.wrap_mode(),
                    self.editor.layout_engine.wrap_indent(),
                    self.editor.layout_engine.tab_width(),
                );
                Ok(CommandResult::Success)
            }
            ViewCommand::SetWrapMode { mode } => {
                self.editor.set_view_options(
                    self.editor.viewport_width,
                    mode,
                    self.editor.layout_engine.wrap_indent(),
                    self.editor.layout_engine.tab_width(),
                );
                Ok(CommandResult::Success)
            }
            ViewCommand::SetWrapIndent { indent } => {
                self.editor.set_view_options(
                    self.editor.viewport_width,
                    self.editor.layout_engine.wrap_mode(),
                    indent,
                    self.editor.layout_engine.tab_width(),
                );
                Ok(CommandResult::Success)
            }
            ViewCommand::SetTabWidth { width } => {
                if width == 0 {
                    return Err(CommandError::Other(
                        "Tab width must be greater than 0".to_string(),
                    ));
                }

                self.editor.set_view_options(
                    self.editor.viewport_width,
                    self.editor.layout_engine.wrap_mode(),
                    self.editor.layout_engine.wrap_indent(),
                    width,
                );
                Ok(CommandResult::Success)
            }
            ViewCommand::SetTabKeyBehavior { behavior } => {
                self.tab_key_behavior = behavior;
                Ok(CommandResult::Success)
            }
            ViewCommand::SetIndentationConfig { config } => {
                self.indentation_config = config;
                Ok(CommandResult::Success)
            }
            ViewCommand::SetAutoPairsConfig { config } => {
                self.set_auto_pairs_config(config);
                Ok(CommandResult::Success)
            }
            ViewCommand::SetAutoPairsEnabled { enabled } => {
                self.set_auto_pairs_enabled(enabled);
                Ok(CommandResult::Success)
            }
            ViewCommand::SetWordBoundaryAsciiBoundaryChars { boundary_chars } => {
                self.editor
                    .set_word_boundary_ascii_boundary_chars(&boundary_chars);
                Ok(CommandResult::Success)
            }
            ViewCommand::ResetWordBoundaryDefaults => {
                self.editor.reset_word_boundary_defaults();
                Ok(CommandResult::Success)
            }
            ViewCommand::ScrollTo { line } => {
                if line >= self.editor.line_index.line_count() {
                    return Err(CommandError::InvalidPosition { line, column: 0 });
                }

                // Scroll operation only validates line number validity
                // Actual scrolling handled by frontend
                Ok(CommandResult::Success)
            }
            ViewCommand::GetViewport { start_row, count } => {
                let grid = self.editor.get_headless_grid_styled(start_row, count);
                Ok(CommandResult::Viewport(grid))
            }
        }
    }

    // Private method: execute style command
    fn execute_style(&mut self, command: StyleCommand) -> Result<CommandResult, CommandError> {
        match command {
            StyleCommand::AddStyle {
                start,
                end,
                style_id,
            } => {
                if start >= end {
                    return Err(CommandError::InvalidRange { start, end });
                }

                let interval = crate::intervals::Interval::new(start, end, style_id);
                self.editor.insert_style_interval(interval);
                Ok(CommandResult::Success)
            }
            StyleCommand::RemoveStyle {
                start,
                end,
                style_id,
            } => {
                self.editor.remove_style_interval(start, end, style_id);
                Ok(CommandResult::Success)
            }
            StyleCommand::Fold {
                start_line,
                end_line,
            } => {
                if start_line >= end_line {
                    return Err(CommandError::InvalidRange {
                        start: start_line,
                        end: end_line,
                    });
                }

                let mut region = crate::intervals::FoldRegion::new(start_line, end_line);
                region.collapse();
                self.editor.folding_manager.add_region(region);
                self.editor
                    .sync_visual_row_index_for_logical_range(start_line, end_line);
                Ok(CommandResult::Success)
            }
            StyleCommand::Unfold { start_line } => {
                let affected = self
                    .editor
                    .folding_manager
                    .innermost_region_bounds_for_line(start_line);
                self.editor.folding_manager.expand_line(start_line);
                if let Some((start, end)) = affected {
                    self.editor
                        .sync_visual_row_index_for_logical_range(start, end);
                }
                Ok(CommandResult::Success)
            }
            StyleCommand::UnfoldAll => {
                let affected = self
                    .editor
                    .folding_manager
                    .regions()
                    .iter()
                    .filter(|region| region.is_collapsed)
                    .fold(None::<(usize, usize)>, |acc, region| match acc {
                        Some((start, end)) => {
                            Some((start.min(region.start_line), end.max(region.end_line)))
                        }
                        None => Some((region.start_line, region.end_line)),
                    });
                self.editor.folding_manager.expand_all();
                if let Some((start, end)) = affected {
                    self.editor
                        .sync_visual_row_index_for_logical_range(start, end);
                }
                Ok(CommandResult::Success)
            }
            StyleCommand::UpdateBracketMatchHighlights => {
                self.execute_update_bracket_match_highlights_command()
            }
            StyleCommand::ClearBracketMatchHighlights => {
                self.execute_clear_bracket_match_highlights_command()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_edit_insert() {
        let mut executor = CommandExecutor::new("Hello", 80);

        let result = executor.execute(Command::Edit(EditCommand::Insert {
            offset: 5,
            text: " World".to_string(),
        }));

        assert!(result.is_ok());
        assert_eq!(executor.editor().get_text(), "Hello World");
    }

    #[test]
    fn test_edit_delete() {
        let mut executor = CommandExecutor::new("Hello World", 80);

        let result = executor.execute(Command::Edit(EditCommand::Delete {
            start: 5,
            length: 6,
        }));

        assert!(result.is_ok());
        assert_eq!(executor.editor().get_text(), "Hello");
    }

    #[test]
    fn test_edit_replace() {
        let mut executor = CommandExecutor::new("Hello World", 80);

        let result = executor.execute(Command::Edit(EditCommand::Replace {
            start: 6,
            length: 5,
            text: "Rust".to_string(),
        }));

        assert!(result.is_ok());
        assert_eq!(executor.editor().get_text(), "Hello Rust");
    }

    #[test]
    fn test_cursor_move_to() {
        let mut executor = CommandExecutor::new("Line 1\nLine 2\nLine 3", 80);

        let result = executor.execute(Command::Cursor(CursorCommand::MoveTo {
            line: 1,
            column: 3,
        }));

        assert!(result.is_ok());
        assert_eq!(executor.editor().cursor_position(), Position::new(1, 3));
    }

    #[test]
    fn test_cursor_selection() {
        let mut executor = CommandExecutor::new("Hello World", 80);

        let result = executor.execute(Command::Cursor(CursorCommand::SetSelection {
            start: Position::new(0, 0),
            end: Position::new(0, 5),
        }));

        assert!(result.is_ok());
        assert!(executor.editor().selection().is_some());
    }

    #[test]
    fn test_view_set_width() {
        let mut executor = CommandExecutor::new("Test", 80);

        let result = executor.execute(Command::View(ViewCommand::SetViewportWidth { width: 40 }));

        assert!(result.is_ok());
        assert_eq!(executor.editor().viewport_width(), 40);
    }

    #[test]
    fn test_style_add_remove() {
        let mut executor = CommandExecutor::new("Hello World", 80);

        // Add style
        let result = executor.execute(Command::Style(StyleCommand::AddStyle {
            start: 0,
            end: 5,
            style_id: 1,
        }));
        assert!(result.is_ok());

        // Remove style
        let result = executor.execute(Command::Style(StyleCommand::RemoveStyle {
            start: 0,
            end: 5,
            style_id: 1,
        }));
        assert!(result.is_ok());
    }

    #[test]
    fn test_batch_execution() {
        let mut executor = CommandExecutor::new("", 80);

        let commands = vec![
            Command::Edit(EditCommand::Insert {
                offset: 0,
                text: "Hello".to_string(),
            }),
            Command::Edit(EditCommand::Insert {
                offset: 5,
                text: " World".to_string(),
            }),
        ];

        let results = executor.execute_batch(commands);
        assert!(results.is_ok());
        assert_eq!(executor.editor().get_text(), "Hello World");
    }

    #[test]
    fn test_error_invalid_offset() {
        let mut executor = CommandExecutor::new("Hello", 80);

        let result = executor.execute(Command::Edit(EditCommand::Insert {
            offset: 100,
            text: "X".to_string(),
        }));

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            CommandError::InvalidOffset(_)
        ));
    }
}
