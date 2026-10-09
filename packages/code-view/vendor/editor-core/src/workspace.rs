//! Workspace and multi-buffer / multi-view model.
//!
//! `editor-core` is intentionally UI-agnostic, but a full-featured editor typically needs a
//! kernel-level model for:
//!
//! - managing multiple open buffers (text + undo + derived metadata)
//! - managing multiple views into the same buffer (split panes)
//!
//! This module provides a small [`Workspace`] that owns:
//! - `BufferId` + `CommandExecutor` (buffer text + undo + derived state)
//! - `ViewId` + per-view state (selections/cursors, wrap config, scroll)
//!
//! The workspace executes commands **against a specific view**. Text edits are applied to the
//! underlying buffer, and any resulting [`crate::TextDelta`] is broadcast to all views of the
//! same buffer.

use crate::commands::{
    AutoPairsConfig, Command, CommandExecutor, CommandResult, CursorCommand, EditCommand,
    TextEditSpec, UndoHistoryRestoreError, UndoHistorySnapshot,
};
use crate::decorations::{Decoration, DecorationLayerId};
use crate::delta::TextDelta;
use crate::intervals::FoldRegion;
use crate::processing::ProcessingEdit;
use crate::search::{SearchError, SearchMatch, SearchOptions, find_all};
use crate::selection_set::selection_direction;
use crate::snippets::SnippetSession;
use crate::state::CursorState;
use crate::{AnchorBias, TextAnchor};
use crate::{
    IndentationConfig, LineEnding, LineIndex, Position, Selection, SelectionDirection,
    TabKeyBehavior, ViewCommand,
};
use crate::{StateChange, StateChangeCallback, StateChangeType, WrapIndent, WrapMode};
use std::collections::{BTreeMap, HashMap};
use std::ops::Range;
use std::sync::Arc;

/// Opaque identifier for an open buffer in a [`Workspace`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BufferId(u64);

impl BufferId {
    /// Create a `BufferId` from a raw numeric id.
    ///
    /// This is intended for interoperability boundaries (e.g. FFI) that persist ids externally.
    pub const fn from_raw(id: u64) -> Self {
        Self(id)
    }

    /// Get the underlying numeric id.
    pub fn get(self) -> u64 {
        self.0
    }
}

/// Opaque identifier for a view into a buffer in a [`Workspace`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ViewId(u64);

impl ViewId {
    /// Create a `ViewId` from a raw numeric id.
    ///
    /// This is intended for interoperability boundaries (e.g. FFI) that persist ids externally.
    pub const fn from_raw(id: u64) -> Self {
        Self(id)
    }

    /// Get the underlying numeric id.
    pub fn get(self) -> u64 {
        self.0
    }
}

/// Metadata attached to a workspace buffer.
#[derive(Debug, Clone)]
pub struct BufferMetadata {
    /// Optional buffer URI/path (host-provided).
    pub uri: Option<String>,
}

/// Result of opening a buffer (a buffer always starts with a default view).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenBufferResult {
    /// The created buffer id.
    pub buffer_id: BufferId,
    /// The initial view id into that buffer.
    pub view_id: ViewId,
}

/// A navigation target produced by jump-list operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JumpTarget {
    /// Target buffer id.
    pub buffer_id: BufferId,
    /// Target position in logical coordinates.
    pub position: Position,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ViewCore {
    cursor_position: Position,
    selection: Option<Selection>,
    secondary_selections: Vec<Selection>,
    viewport_width: usize,
    wrap_mode: WrapMode,
    wrap_indent: WrapIndent,
    tab_width: usize,
    tab_key_behavior: TabKeyBehavior,
    indentation_config: IndentationConfig,
    auto_pairs: AutoPairsConfig,
    snippet_session: Option<SnippetSession>,
    preferred_x_cells: Option<usize>,
}

impl ViewCore {
    fn from_executor(executor: &CommandExecutor) -> Self {
        let editor = executor.editor();
        Self {
            cursor_position: editor.cursor_position(),
            selection: editor.selection().cloned(),
            secondary_selections: editor.secondary_selections().to_vec(),
            viewport_width: editor.viewport_width(),
            wrap_mode: editor.layout_engine().wrap_mode(),
            wrap_indent: editor.layout_engine().wrap_indent(),
            tab_width: editor.layout_engine().tab_width(),
            tab_key_behavior: executor.tab_key_behavior(),
            indentation_config: executor.indentation_config().clone(),
            auto_pairs: executor.auto_pairs_config().clone(),
            snippet_session: executor.snippet_session().cloned(),
            preferred_x_cells: executor.preferred_x_cells(),
        }
    }

    fn apply_to_executor(&self, executor: &mut CommandExecutor) {
        let mut invalidate_visual_rows = false;
        let editor = executor.editor_mut();
        editor.set_cursor_state(
            self.cursor_position,
            self.selection.clone(),
            self.secondary_selections.clone(),
        );

        if editor.viewport_width() != self.viewport_width {
            invalidate_visual_rows = true;
        }

        let before_wrap_mode = editor.layout_engine().wrap_mode();
        let before_wrap_indent = editor.layout_engine().wrap_indent();
        let before_tab_width = editor.layout_engine().tab_width();
        let before_viewport_width = editor.layout_engine().viewport_width();
        if before_wrap_mode != self.wrap_mode
            || before_wrap_indent != self.wrap_indent
            || before_tab_width != self.tab_width
            || before_viewport_width != self.viewport_width
        {
            invalidate_visual_rows = true;
        }

        if invalidate_visual_rows {
            editor.set_view_options(
                self.viewport_width,
                self.wrap_mode,
                self.wrap_indent,
                self.tab_width,
            );
        }

        executor.set_tab_key_behavior(self.tab_key_behavior);
        executor.set_indentation_config(self.indentation_config.clone());
        executor.set_auto_pairs_config(self.auto_pairs.clone());
        executor.set_snippet_session(self.snippet_session.clone());
        executor.set_preferred_x_cells(self.preferred_x_cells);
    }
}

struct BufferEntry {
    meta: BufferMetadata,
    executor: CommandExecutor,
    version: u64,
    last_text_delta: Option<Arc<TextDelta>>,
    bookmarks: BookmarkSet,
    marks: MarkSet,
    /// Deterministic baseline view config for new views into this buffer.
    ///
    /// Captured once from the buffer's initial executor state, so `create_view` does not inherit
    /// whichever view most recently loaded its scratch state into the shared executor (which would
    /// make new views' config depend on call order).
    default_view_core: ViewCore,
}

struct ViewEntry {
    buffer: BufferId,
    core: ViewCore,
    version: u64,
    callbacks: Vec<StateChangeCallback>,
    scroll_top: usize,
    scroll_sub_row_offset: u16,
    overscan_rows: usize,
    viewport_height: Option<usize>,
    last_text_delta: Option<Arc<TextDelta>>,
    jump_list: JumpList,
}

/// Workspace-level errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceError {
    /// A buffer with this uri already exists.
    UriAlreadyOpen(String),
    /// A buffer id was not found.
    BufferNotFound(BufferId),
    /// A view id was not found.
    ViewNotFound(ViewId),
    /// Executing a command failed.
    CommandFailed {
        /// Target view id.
        view: ViewId,
        /// Error message.
        message: String,
    },
    /// Applying edits to a buffer failed.
    ApplyEditsFailed {
        /// Target buffer id.
        buffer: BufferId,
        /// Error message.
        message: String,
    },
}

/// Errors produced when restoring undo history for a workspace buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceUndoHistoryRestoreError {
    /// A buffer id was not found.
    BufferNotFound(BufferId),
    /// Restoring the undo history failed (corrupt snapshot or version mismatch).
    RestoreFailed {
        /// Target buffer id.
        buffer: BufferId,
        /// Underlying restore error.
        error: UndoHistoryRestoreError,
    },
}

impl std::fmt::Display for WorkspaceUndoHistoryRestoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BufferNotFound(id) => write!(f, "Buffer not found (id={})", id.get()),
            Self::RestoreFailed { buffer, error } => {
                write!(
                    f,
                    "Restore undo history failed (buffer={}): {}",
                    buffer.get(),
                    error
                )
            }
        }
    }
}

impl std::error::Error for WorkspaceUndoHistoryRestoreError {}

/// Search matches for a single open buffer in a [`Workspace`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceSearchResult {
    /// Buffer id.
    pub id: BufferId,
    /// Optional URI/path metadata.
    pub uri: Option<String>,
    /// All matches in this buffer (character offsets, half-open).
    pub matches: Vec<SearchMatch>,
}

/// Smooth-scrolling state for a view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewSmoothScrollState {
    /// Top visual row anchor.
    pub top_visual_row: usize,
    /// Sub-row offset within `top_visual_row` (0..=65535, normalized).
    pub sub_row_offset: u16,
    /// Overscan rows for prefetching.
    pub overscan_rows: usize,
}

/// Viewport state for a workspace view, including visual totals and smooth-scrolling metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceViewportState {
    /// Viewport width (in cells).
    pub width: usize,
    /// Viewport height (line count, host-provided).
    pub height: Option<usize>,
    /// Current top visual row.
    pub scroll_top: usize,
    /// Visible visual range.
    pub visible_lines: Range<usize>,
    /// Total visual line count under current view config (wrap + folding aware).
    pub total_visual_lines: usize,
    /// Smooth-scroll metadata.
    pub smooth_scroll: ViewSmoothScrollState,
    /// Recommended prefetch range using overscan rows.
    pub prefetch_lines: Range<usize>,
}

fn apply_char_offset_delta(mut offset: usize, delta: &TextDelta) -> usize {
    for edit in &delta.edits {
        let start = edit.start;
        let end = edit.end();
        let deleted_len = edit.deleted_len();
        let inserted_len = edit.inserted_len();

        if offset < start {
            continue;
        }

        if offset < end {
            // If the caret was inside the replaced range, anchor it at the end of the inserted text.
            offset = start.saturating_add(inserted_len);
            continue;
        }

        // After the replaced range: shift by the net length delta.
        if inserted_len >= deleted_len {
            offset = offset.saturating_add(inserted_len - deleted_len);
        } else {
            offset = offset.saturating_sub(deleted_len - inserted_len);
        }
    }

    offset
}

fn apply_position_delta(
    old_index: &LineIndex,
    new_index: &LineIndex,
    pos: Position,
    delta: &TextDelta,
) -> Position {
    let before = old_index.position_to_char_offset(pos.line, pos.column);
    let after = apply_char_offset_delta(before, delta);
    let (line, column) = new_index.char_offset_to_position(after);
    Position::new(line, column)
}

fn apply_selection_delta(
    old_index: &LineIndex,
    new_index: &LineIndex,
    selection: &Selection,
    delta: &TextDelta,
) -> Selection {
    let start = apply_position_delta(old_index, new_index, selection.start, delta);
    let end = apply_position_delta(old_index, new_index, selection.end, delta);
    Selection {
        start,
        end,
        direction: selection_direction(start, end),
    }
}

/// Coalesce a freshly produced delta into a slot that may still hold a previously produced but
/// not-yet-consumed delta.
///
/// A consumer that only reads the latest stored delta (e.g. LSP incremental sync, search anchor
/// shifting) would otherwise lose the earlier edit when two edits happen between consumptions.
/// Merging keeps the slot equivalent to "all edits since the last take".
fn coalesce_delta_slot(slot: &mut Option<Arc<TextDelta>>, next: &Arc<TextDelta>) {
    *slot = Some(match slot.take() {
        Some(existing) => Arc::new(TextDelta::merge(&existing, next)),
        None => next.clone(),
    });
}

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

    fn line_numbers(&self, line_index: &LineIndex) -> Vec<usize> {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct JumpEntry {
    buffer_id: BufferId,
    anchor: TextAnchor,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct JumpList {
    back: Vec<JumpEntry>,
    forward: Vec<JumpEntry>,
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

    fn record(&mut self, entry: JumpEntry) {
        if self.back.last().is_some_and(|last| *last == entry) {
            return;
        }

        self.back.push(entry);
        self.forward.clear();

        if self.back.len() > self.max_len {
            let overflow = self.back.len() - self.max_len;
            self.back.drain(0..overflow);
        }
    }

    fn back(&mut self, current: JumpEntry) -> Option<JumpEntry> {
        let target = self.back.pop()?;
        if !self.forward.last().is_some_and(|last| *last == current) {
            self.forward.push(current);
        }
        Some(target)
    }

    fn forward(&mut self, current: JumpEntry) -> Option<JumpEntry> {
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

    fn apply_delta(&mut self, buffer_id: BufferId, delta: &TextDelta) {
        for entry in self
            .back
            .iter_mut()
            .chain(self.forward.iter_mut())
            .filter(|e| e.buffer_id == buffer_id)
        {
            entry.anchor.apply_delta(delta);
        }
    }
}

/// A collection of open buffers and their views.
#[derive(Default)]
pub struct Workspace {
    next_buffer_id: u64,
    buffers: BTreeMap<BufferId, BufferEntry>,
    uri_to_buffer: HashMap<String, BufferId>,

    next_view_id: u64,
    views: BTreeMap<ViewId, ViewEntry>,
    active_view: Option<ViewId>,

    intelligence: crate::WorkspaceIntelligence,
}

impl std::fmt::Debug for Workspace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Workspace")
            .field("buffer_count", &self.buffers.len())
            .field("view_count", &self.views.len())
            .field("uri_count", &self.uri_to_buffer.len())
            .field("active_view", &self.active_view)
            .field("intelligence_set_count", &self.intelligence.len())
            .finish()
    }
}

impl Workspace {
    /// Create an empty workspace.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the number of open buffers.
    pub fn len(&self) -> usize {
        self.buffers.len()
    }

    /// Returns `true` if there are no open buffers.
    pub fn is_empty(&self) -> bool {
        self.buffers.is_empty()
    }

    /// Returns the number of open views.
    pub fn view_count(&self) -> usize {
        self.views.len()
    }

    /// Return the active view id (if any).
    pub fn active_view_id(&self) -> Option<ViewId> {
        self.active_view
    }

    /// Return the active buffer id (if any).
    pub fn active_buffer_id(&self) -> Option<BufferId> {
        let view_id = self.active_view?;
        self.views.get(&view_id).map(|v| v.buffer)
    }

    /// Read workspace-scoped language intelligence result sets (references/call hierarchy/etc.).
    pub fn intelligence(&self) -> &crate::WorkspaceIntelligence {
        &self.intelligence
    }

    /// Mutate workspace-scoped language intelligence result sets (references/call hierarchy/etc.).
    pub fn intelligence_mut(&mut self) -> &mut crate::WorkspaceIntelligence {
        &mut self.intelligence
    }

    /// Set the active view.
    pub fn set_active_view(&mut self, id: ViewId) -> Result<(), WorkspaceError> {
        if !self.views.contains_key(&id) {
            return Err(WorkspaceError::ViewNotFound(id));
        }
        self.active_view = Some(id);
        Ok(())
    }

    /// Open a new buffer in the workspace, creating an initial view.
    ///
    /// - `uri` is optional and host-provided (e.g. `file:///...`).
    /// - `text` is the initial contents.
    /// - `viewport_width` is the initial view's wrap width.
    pub fn open_buffer(
        &mut self,
        uri: Option<String>,
        text: &str,
        viewport_width: usize,
    ) -> Result<OpenBufferResult, WorkspaceError> {
        if let Some(uri) = uri.as_ref()
            && self.uri_to_buffer.contains_key(uri)
        {
            return Err(WorkspaceError::UriAlreadyOpen(uri.clone()));
        }

        let buffer_id = BufferId(self.next_buffer_id);
        self.next_buffer_id = self.next_buffer_id.saturating_add(1);

        let executor = CommandExecutor::new(text, viewport_width);
        // Capture the pristine view config now, before any command mutates the shared executor.
        let default_view_core = ViewCore::from_executor(&executor);
        let meta = BufferMetadata { uri: uri.clone() };
        self.buffers.insert(
            buffer_id,
            BufferEntry {
                meta,
                executor,
                version: 0,
                last_text_delta: None,
                bookmarks: BookmarkSet::default(),
                marks: MarkSet::default(),
                default_view_core,
            },
        );

        if let Some(uri) = uri {
            self.uri_to_buffer.insert(uri, buffer_id);
        }

        let view_id = self.create_view(buffer_id, viewport_width)?;

        if self.active_view.is_none() {
            self.active_view = Some(view_id);
        }

        Ok(OpenBufferResult { buffer_id, view_id })
    }

    /// Close a buffer (and all its views).
    pub fn close_buffer(&mut self, id: BufferId) -> Result<(), WorkspaceError> {
        let Some(entry) = self.buffers.remove(&id) else {
            return Err(WorkspaceError::BufferNotFound(id));
        };

        if let Some(uri) = entry.meta.uri.as_ref() {
            self.uri_to_buffer.remove(uri);
        }

        let views_to_remove: Vec<ViewId> = self
            .views
            .iter()
            .filter_map(|(vid, v)| if v.buffer == id { Some(*vid) } else { None })
            .collect();
        for view_id in views_to_remove {
            self.views.remove(&view_id);
        }

        if self
            .active_view
            .is_some_and(|active| !self.views.contains_key(&active))
        {
            self.active_view = self.views.keys().next().copied();
        }

        Ok(())
    }

    /// Close a view. If it was the last view of its buffer, the buffer is also closed.
    pub fn close_view(&mut self, id: ViewId) -> Result<(), WorkspaceError> {
        let Some(view) = self.views.remove(&id) else {
            return Err(WorkspaceError::ViewNotFound(id));
        };

        if self.active_view == Some(id) {
            self.active_view = self.views.keys().next().copied();
        }

        let still_has_views = self.views.values().any(|v| v.buffer == view.buffer);
        if !still_has_views {
            self.close_buffer(view.buffer)?;
        }

        Ok(())
    }

    /// Return all open buffer ids in deterministic order.
    pub fn buffer_ids(&self) -> Vec<BufferId> {
        let mut ids: Vec<BufferId> = self.buffers.keys().copied().collect();
        ids.sort_by_key(|id| id.get());
        ids
    }

    /// Return all open view ids in deterministic order.
    pub fn view_ids(&self) -> Vec<ViewId> {
        let mut ids: Vec<ViewId> = self.views.keys().copied().collect();
        ids.sort_by_key(|id| id.get());
        ids
    }

    /// Create a new view into an existing buffer.
    ///
    /// The new view starts from the buffer's deterministic default view config (see
    /// [`BufferEntry::default_view_core`]) — independent of which view most recently executed a
    /// command. To clone another view's config instead, use [`Workspace::create_view_from`].
    pub fn create_view(
        &mut self,
        buffer: BufferId,
        viewport_width: usize,
    ) -> Result<ViewId, WorkspaceError> {
        let Some(buffer_entry) = self.buffers.get(&buffer) else {
            return Err(WorkspaceError::BufferNotFound(buffer));
        };
        let core = buffer_entry.default_view_core.clone();
        Ok(self.insert_view_with_core(buffer, core, viewport_width))
    }

    /// Create a new view into the same buffer as `parent_view`, cloning that view's view-local
    /// config (wrap mode, tab width, indentation, auto-pairs, …) but with an independent
    /// cursor/selection and its own viewport width.
    ///
    /// This is the explicit "split / clone this view" operation, with predictable config
    /// provenance (unlike deriving from shared executor scratch state).
    pub fn create_view_from(
        &mut self,
        parent_view: ViewId,
        viewport_width: usize,
    ) -> Result<ViewId, WorkspaceError> {
        let Some(parent) = self.views.get(&parent_view) else {
            return Err(WorkspaceError::ViewNotFound(parent_view));
        };
        let buffer = parent.buffer;
        let core = parent.core.clone();
        Ok(self.insert_view_with_core(buffer, core, viewport_width))
    }

    /// Insert a new view for `buffer` from a base `ViewCore`, resetting cursor/selection/scroll and
    /// applying `viewport_width`.
    fn insert_view_with_core(
        &mut self,
        buffer: BufferId,
        mut core: ViewCore,
        viewport_width: usize,
    ) -> ViewId {
        core.cursor_position = Position::new(0, 0);
        core.selection = None;
        core.secondary_selections.clear();
        core.snippet_session = None;
        core.viewport_width = viewport_width.max(1);
        core.preferred_x_cells = None;

        let view_id = ViewId(self.next_view_id);
        self.next_view_id = self.next_view_id.saturating_add(1);

        self.views.insert(
            view_id,
            ViewEntry {
                buffer,
                core,
                version: 0,
                callbacks: Vec::new(),
                scroll_top: 0,
                scroll_sub_row_offset: 0,
                overscan_rows: 0,
                viewport_height: None,
                last_text_delta: None,
                jump_list: JumpList::new(200),
            },
        );

        view_id
    }

    /// Look up a buffer by uri.
    pub fn buffer_id_for_uri(&self, uri: &str) -> Option<BufferId> {
        self.uri_to_buffer.get(uri).copied()
    }

    /// Get a reference to a buffer's line index (logical line/column <-> char offsets).
    pub fn buffer_line_index(&self, buffer_id: BufferId) -> Result<&LineIndex, WorkspaceError> {
        let Some(buffer) = self.buffers.get(&buffer_id) else {
            return Err(WorkspaceError::BufferNotFound(buffer_id));
        };
        Ok(buffer.executor.editor().line_index())
    }

    /// Get the document length for a buffer in Unicode scalar values (Rust `char`s).
    pub fn buffer_char_count(&self, buffer_id: BufferId) -> Result<usize, WorkspaceError> {
        let Some(buffer) = self.buffers.get(&buffer_id) else {
            return Err(WorkspaceError::BufferNotFound(buffer_id));
        };
        Ok(buffer.executor.editor().char_count())
    }

    /// Get a slice of the buffer text as a `String` by character offset + length.
    ///
    /// Notes:
    /// - `start` and `len` are in Unicode scalar indices (Rust `char`s), not bytes.
    /// - Out-of-bounds ranges are clamped by the underlying text buffer.
    pub fn buffer_text_range(
        &self,
        buffer_id: BufferId,
        start: usize,
        len: usize,
    ) -> Result<String, WorkspaceError> {
        let Some(buffer) = self.buffers.get(&buffer_id) else {
            return Err(WorkspaceError::BufferNotFound(buffer_id));
        };
        Ok(buffer.executor.editor().text_range(start, len))
    }

    /// Get all decoration layers for a buffer.
    pub fn buffer_decorations(
        &self,
        buffer_id: BufferId,
    ) -> Result<&BTreeMap<DecorationLayerId, Vec<Decoration>>, WorkspaceError> {
        let Some(buffer) = self.buffers.get(&buffer_id) else {
            return Err(WorkspaceError::BufferNotFound(buffer_id));
        };
        Ok(buffer.executor.editor().decorations())
    }

    /// Get the current folding regions for a buffer (user folds + derived folds).
    pub fn folding_regions_for_buffer(
        &self,
        buffer_id: BufferId,
    ) -> Result<Vec<FoldRegion>, WorkspaceError> {
        let Some(buffer) = self.buffers.get(&buffer_id) else {
            return Err(WorkspaceError::BufferNotFound(buffer_id));
        };
        Ok(buffer
            .executor
            .editor()
            .folding_manager()
            .regions()
            .to_vec())
    }

    /// Returns whether a buffer has unsaved text edits.
    ///
    /// Notes:
    /// - This tracks the executor's "clean point" (usually the last `mark_saved_*` call),
    ///   and is restored by undoing back to that clean point.
    pub fn buffer_is_modified(&self, buffer_id: BufferId) -> Result<bool, WorkspaceError> {
        let Some(buffer) = self.buffers.get(&buffer_id) else {
            return Err(WorkspaceError::BufferNotFound(buffer_id));
        };
        Ok(!buffer.executor.is_clean())
    }

    /// Return the preferred line ending for saving this buffer.
    pub fn line_ending_for_buffer(
        &self,
        buffer_id: BufferId,
    ) -> Result<LineEnding, WorkspaceError> {
        let Some(buffer) = self.buffers.get(&buffer_id) else {
            return Err(WorkspaceError::BufferNotFound(buffer_id));
        };
        Ok(buffer.executor.line_ending())
    }

    /// Override the preferred line ending for saving this buffer.
    pub fn set_line_ending_for_buffer(
        &mut self,
        buffer_id: BufferId,
        line_ending: LineEnding,
    ) -> Result<(), WorkspaceError> {
        let Some(buffer) = self.buffers.get_mut(&buffer_id) else {
            return Err(WorkspaceError::BufferNotFound(buffer_id));
        };
        buffer.executor.set_line_ending(line_ending);
        Ok(())
    }

    /// Returns whether the view's underlying buffer has unsaved text edits.
    pub fn is_modified_for_view(&self, view_id: ViewId) -> Result<bool, WorkspaceError> {
        let buffer_id = self.buffer_id_for_view(view_id)?;
        self.buffer_is_modified(buffer_id)
    }

    /// Return the preferred line ending for saving this view's underlying buffer.
    pub fn line_ending_for_view(&self, view_id: ViewId) -> Result<LineEnding, WorkspaceError> {
        let buffer_id = self.buffer_id_for_view(view_id)?;
        self.line_ending_for_buffer(buffer_id)
    }

    /// Override the preferred line ending for saving this view's underlying buffer.
    pub fn set_line_ending_for_view(
        &mut self,
        view_id: ViewId,
        line_ending: LineEnding,
    ) -> Result<(), WorkspaceError> {
        let buffer_id = self.buffer_id_for_view(view_id)?;
        self.set_line_ending_for_buffer(buffer_id, line_ending)
    }

    /// Mark the current state of a buffer as saved (clean point).
    pub fn mark_saved_for_buffer(&mut self, buffer_id: BufferId) -> Result<(), WorkspaceError> {
        let Some(buffer) = self.buffers.get_mut(&buffer_id) else {
            return Err(WorkspaceError::BufferNotFound(buffer_id));
        };
        buffer.executor.mark_clean();
        Ok(())
    }

    /// Mark the current state of a view's buffer as saved (clean point).
    pub fn mark_saved_for_view(&mut self, view_id: ViewId) -> Result<(), WorkspaceError> {
        let buffer_id = self.buffer_id_for_view(view_id)?;
        self.mark_saved_for_buffer(buffer_id)
    }

    /// Capture a persistable snapshot of a buffer's undo/redo history.
    pub fn undo_history_snapshot_for_buffer(
        &self,
        buffer_id: BufferId,
    ) -> Result<UndoHistorySnapshot, WorkspaceError> {
        let Some(buffer) = self.buffers.get(&buffer_id) else {
            return Err(WorkspaceError::BufferNotFound(buffer_id));
        };
        Ok(buffer.executor.undo_history_snapshot())
    }

    /// Restore a buffer's undo/redo history from a previously captured snapshot.
    ///
    /// Notes:
    /// - This does **not** modify the current buffer text.
    /// - Callers should only restore a snapshot into the **same text** it was captured from.
    pub fn restore_undo_history_for_buffer(
        &mut self,
        buffer_id: BufferId,
        snapshot: UndoHistorySnapshot,
    ) -> Result<(), WorkspaceUndoHistoryRestoreError> {
        let Some(buffer) = self.buffers.get_mut(&buffer_id) else {
            return Err(WorkspaceUndoHistoryRestoreError::BufferNotFound(buffer_id));
        };

        buffer.last_text_delta = None;
        for view in self.views.values_mut() {
            if view.buffer == buffer_id {
                view.last_text_delta = None;
            }
        }

        buffer
            .executor
            .restore_undo_history(snapshot)
            .map_err(|err| WorkspaceUndoHistoryRestoreError::RestoreFailed {
                buffer: buffer_id,
                error: err,
            })?;

        Ok(())
    }

    /// Get a buffer's metadata.
    pub fn buffer_metadata(&self, id: BufferId) -> Option<&BufferMetadata> {
        self.buffers.get(&id).map(|e| &e.meta)
    }

    /// Get the buffer id that a view is pointing at.
    pub fn buffer_id_for_view(&self, id: ViewId) -> Result<BufferId, WorkspaceError> {
        self.views
            .get(&id)
            .map(|v| v.buffer)
            .ok_or(WorkspaceError::ViewNotFound(id))
    }

    /// Get the primary cursor position for a view.
    pub fn cursor_position_for_view(&self, id: ViewId) -> Result<Position, WorkspaceError> {
        self.views
            .get(&id)
            .map(|v| v.core.cursor_position)
            .ok_or(WorkspaceError::ViewNotFound(id))
    }

    /// Get the primary selection for a view (None means "empty selection / caret only").
    pub fn selection_for_view(&self, id: ViewId) -> Result<Option<Selection>, WorkspaceError> {
        self.views
            .get(&id)
            .map(|v| v.core.selection.clone())
            .ok_or(WorkspaceError::ViewNotFound(id))
    }

    /// Get the current tab width setting for a view (in monospace cells).
    pub fn tab_width_for_view(&self, id: ViewId) -> Result<usize, WorkspaceError> {
        self.views
            .get(&id)
            .map(|v| v.core.tab_width)
            .ok_or(WorkspaceError::ViewNotFound(id))
    }

    /// Get the current viewport width setting for a view (in monospace cells).
    pub fn viewport_width_for_view(&self, id: ViewId) -> Result<usize, WorkspaceError> {
        self.views
            .get(&id)
            .map(|v| v.core.viewport_width)
            .ok_or(WorkspaceError::ViewNotFound(id))
    }

    /// Get the current soft wrap mode for a view.
    pub fn wrap_mode_for_view(&self, id: ViewId) -> Result<WrapMode, WorkspaceError> {
        self.views
            .get(&id)
            .map(|v| v.core.wrap_mode)
            .ok_or(WorkspaceError::ViewNotFound(id))
    }

    /// Get the current wrapped-line indentation policy for a view.
    pub fn wrap_indent_for_view(&self, id: ViewId) -> Result<WrapIndent, WorkspaceError> {
        self.views
            .get(&id)
            .map(|v| v.core.wrap_indent)
            .ok_or(WorkspaceError::ViewNotFound(id))
    }

    /// Get the current tab key behavior for a view.
    pub fn tab_key_behavior_for_view(&self, id: ViewId) -> Result<TabKeyBehavior, WorkspaceError> {
        self.views
            .get(&id)
            .map(|v| v.core.tab_key_behavior)
            .ok_or(WorkspaceError::ViewNotFound(id))
    }

    /// Get the current indentation configuration for a view.
    pub fn indentation_config_for_view(
        &self,
        id: ViewId,
    ) -> Result<IndentationConfig, WorkspaceError> {
        self.views
            .get(&id)
            .map(|v| v.core.indentation_config.clone())
            .ok_or(WorkspaceError::ViewNotFound(id))
    }

    /// Get the current auto-pairs configuration for a view.
    pub fn auto_pairs_config_for_view(
        &self,
        id: ViewId,
    ) -> Result<AutoPairsConfig, WorkspaceError> {
        self.views
            .get(&id)
            .map(|v| v.core.auto_pairs.clone())
            .ok_or(WorkspaceError::ViewNotFound(id))
    }

    /// Get a view's normalized cursor/selection snapshot.
    ///
    /// This matches the semantics of `EditorStateManager::get_cursor_state`, but for workspace views.
    pub fn cursor_state_for_view(&self, id: ViewId) -> Result<CursorState, WorkspaceError> {
        let Some(view) = self.views.get(&id) else {
            return Err(WorkspaceError::ViewNotFound(id));
        };
        let Some(buffer) = self.buffers.get(&view.buffer) else {
            return Err(WorkspaceError::BufferNotFound(view.buffer));
        };

        let line_index = buffer.executor.editor().line_index();

        let mut selections: Vec<Selection> =
            Vec::with_capacity(1 + view.core.secondary_selections.len());
        let primary = view.core.selection.clone().unwrap_or(Selection {
            start: view.core.cursor_position,
            end: view.core.cursor_position,
            direction: SelectionDirection::Forward,
        });
        selections.push(primary);
        selections.extend(view.core.secondary_selections.iter().cloned());

        let (selections, primary_selection_index) =
            crate::selection_set::normalize_selections(selections, 0);
        let primary = selections
            .get(primary_selection_index)
            .cloned()
            .unwrap_or(Selection {
                start: view.core.cursor_position,
                end: view.core.cursor_position,
                direction: SelectionDirection::Forward,
            });

        let position = primary.end;
        let offset = line_index.position_to_char_offset(position.line, position.column);

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

        Ok(CursorState {
            position,
            offset,
            multi_cursors,
            selection,
            selections,
            primary_selection_index,
        })
    }

    /// Get the scroll position (top visual row) for a view.
    pub fn scroll_top_for_view(&self, id: ViewId) -> Result<usize, WorkspaceError> {
        self.views
            .get(&id)
            .map(|v| v.scroll_top)
            .ok_or(WorkspaceError::ViewNotFound(id))
    }

    /// Get the sub-row smooth-scroll offset for a view.
    pub fn scroll_sub_row_offset_for_view(&self, id: ViewId) -> Result<u16, WorkspaceError> {
        self.views
            .get(&id)
            .map(|v| v.scroll_sub_row_offset)
            .ok_or(WorkspaceError::ViewNotFound(id))
    }

    /// Get overscan rows for a view.
    pub fn overscan_rows_for_view(&self, id: ViewId) -> Result<usize, WorkspaceError> {
        self.views
            .get(&id)
            .map(|v| v.overscan_rows)
            .ok_or(WorkspaceError::ViewNotFound(id))
    }

    /// Get smooth-scroll state for a view.
    pub fn smooth_scroll_state_for_view(
        &self,
        id: ViewId,
    ) -> Result<ViewSmoothScrollState, WorkspaceError> {
        let Some(view) = self.views.get(&id) else {
            return Err(WorkspaceError::ViewNotFound(id));
        };
        Ok(ViewSmoothScrollState {
            top_visual_row: view.scroll_top,
            sub_row_offset: view.scroll_sub_row_offset,
            overscan_rows: view.overscan_rows,
        })
    }

    /// Update a buffer's uri/path.
    pub fn set_buffer_uri(
        &mut self,
        id: BufferId,
        uri: Option<String>,
    ) -> Result<(), WorkspaceError> {
        let Some(entry) = self.buffers.get_mut(&id) else {
            return Err(WorkspaceError::BufferNotFound(id));
        };

        if let Some(next) = uri.as_ref()
            && self.uri_to_buffer.contains_key(next)
            && entry.meta.uri.as_deref() != Some(next.as_str())
        {
            return Err(WorkspaceError::UriAlreadyOpen(next.clone()));
        }

        if let Some(prev) = entry.meta.uri.take() {
            self.uri_to_buffer.remove(&prev);
        }

        if let Some(next) = uri.clone() {
            self.uri_to_buffer.insert(next, id);
        }

        entry.meta.uri = uri;
        Ok(())
    }

    /// Get a view's current version (increments on view-local changes and buffer changes).
    pub fn view_version(&self, id: ViewId) -> Option<u64> {
        self.views.get(&id).map(|v| v.version)
    }

    /// Get the last broadcast text delta for this view (if any).
    pub fn last_text_delta_for_view(&self, id: ViewId) -> Option<&Arc<TextDelta>> {
        self.views.get(&id)?.last_text_delta.as_ref()
    }

    /// Take the last broadcast text delta for this view (if any).
    pub fn take_last_text_delta_for_view(&mut self, id: ViewId) -> Option<Arc<TextDelta>> {
        self.views.get_mut(&id)?.last_text_delta.take()
    }

    /// Take the last text delta for a buffer (if any).
    ///
    /// This is useful for incremental consumers (e.g. LSP sync) that want to observe each buffer
    /// edit exactly once, regardless of how many views exist for that buffer.
    pub fn take_last_text_delta_for_buffer(
        &mut self,
        id: BufferId,
    ) -> Result<Option<Arc<TextDelta>>, WorkspaceError> {
        let Some(buffer) = self.buffers.get_mut(&id) else {
            return Err(WorkspaceError::BufferNotFound(id));
        };
        Ok(buffer.last_text_delta.take())
    }

    /// Subscribe to changes for a view.
    pub fn subscribe_view<F>(&mut self, id: ViewId, callback: F) -> Result<(), WorkspaceError>
    where
        F: FnMut(&StateChange) + Send + 'static,
    {
        let Some(view) = self.views.get_mut(&id) else {
            return Err(WorkspaceError::ViewNotFound(id));
        };

        view.callbacks.push(Box::new(callback));
        Ok(())
    }

    fn notify_view(
        view: &mut ViewEntry,
        change_type: StateChangeType,
        delta: Option<Arc<TextDelta>>,
    ) {
        let old_version = view.version;
        view.version = view.version.saturating_add(1);

        let mut change = StateChange::new(change_type, old_version, view.version);
        if let Some(delta) = delta {
            change = change.with_text_delta(delta);
        }

        for cb in &mut view.callbacks {
            cb(&change);
        }
    }

    fn command_change_type(command: &Command) -> Option<StateChangeType> {
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
                | CursorCommand::MoveToMatchingBracket
                | CursorCommand::FindNext { .. }
                | CursorCommand::FindPrev { .. },
            ) => Some(StateChangeType::CursorMoved),
            Command::Cursor(_) => Some(StateChangeType::SelectionChanged),
            Command::View(ViewCommand::ScrollTo { .. } | ViewCommand::GetViewport { .. }) => None,
            Command::View(_) => Some(StateChangeType::ViewportChanged),
            Command::Style(
                crate::StyleCommand::AddStyle { .. }
                | crate::StyleCommand::RemoveStyle { .. }
                | crate::StyleCommand::UpdateBracketMatchHighlights
                | crate::StyleCommand::ClearBracketMatchHighlights,
            ) => Some(StateChangeType::StyleChanged),
            Command::Style(
                crate::StyleCommand::Fold { .. }
                | crate::StyleCommand::Unfold { .. }
                | crate::StyleCommand::UnfoldAll,
            ) => Some(StateChangeType::FoldingChanged),
        }
    }

    /// Execute a command against a specific view.
    ///
    /// - Cursor/selection state is view-local.
    /// - Text edits and derived-state edits are applied to the underlying buffer.
    /// - Any text delta is broadcast to all views of that buffer.
    pub fn execute(
        &mut self,
        view_id: ViewId,
        command: Command,
    ) -> Result<CommandResult, WorkspaceError> {
        let Some(buffer_id) = self.views.get(&view_id).map(|v| v.buffer) else {
            return Err(WorkspaceError::ViewNotFound(view_id));
        };

        let change_type = Self::command_change_type(&command);
        if change_type.is_none() {
            // Still run command because it may validate (e.g. ScrollTo), but treat as no version bump.
        }

        // Borrow maps separately so we can mutably access a view and its buffer.
        let views = &mut self.views;
        let buffers = &mut self.buffers;

        let Some(view) = views.get_mut(&view_id) else {
            return Err(WorkspaceError::ViewNotFound(view_id));
        };
        let Some(buffer) = buffers.get_mut(&buffer_id) else {
            return Err(WorkspaceError::BufferNotFound(buffer_id));
        };

        let before_view_core = view.core.clone();
        let before_line_index = buffer.executor.editor().line_index().clone();
        let before_char_count = buffer.executor.editor().char_count();

        // Load view-local state into the executor, execute, then snapshot it back.
        view.core.apply_to_executor(&mut buffer.executor);

        let result = buffer.executor.execute(command.clone()).map_err(|err| {
            WorkspaceError::CommandFailed {
                view: view_id,
                message: err.to_string(),
            }
        })?;

        view.core = ViewCore::from_executor(&buffer.executor);

        let delta = buffer.executor.take_last_text_delta().map(Arc::new);
        let after_char_count = buffer.executor.editor().char_count();

        // Detect no-ops: successful execution but no meaningful state change.
        let view_changed = view.core != before_view_core;
        let buffer_text_changed = delta.is_some()
            // `Backspace`/`DeleteForward` can succeed as boundary no-ops; detect via char count.
            || after_char_count != before_char_count;

        let buffer_derived_changed = matches!(command, Command::Style(_));

        if !(view_changed || buffer_text_changed || buffer_derived_changed) {
            return Ok(result);
        }

        let change_type = if buffer_text_changed {
            StateChangeType::DocumentModified
        } else {
            change_type.unwrap_or(StateChangeType::ViewportChanged)
        };

        if buffer_text_changed || buffer_derived_changed {
            // Broadcast to all views of this buffer.
            //
            // Coalesce (merge) into the delta slots rather than overwrite: a consumer that has not
            // taken the previous delta yet must still observe this edit. A change without a text
            // delta (e.g. a style-only change) must NOT clear an unconsumed text delta.
            if let Some(delta_arc) = delta.clone() {
                coalesce_delta_slot(&mut buffer.last_text_delta, &delta_arc);
                for other in views.values_mut() {
                    if other.buffer != buffer_id {
                        continue;
                    }
                    coalesce_delta_slot(&mut other.last_text_delta, &delta_arc);
                }
            }

            // Shift other views' cursor/selections through the delta (if any).
            if let Some(ref delta_arc) = delta {
                let new_index = buffer.executor.editor().line_index();
                for (other_id, other) in views.iter_mut() {
                    if other.buffer != buffer_id || *other_id == view_id {
                        continue;
                    }

                    other.core.cursor_position = apply_position_delta(
                        &before_line_index,
                        new_index,
                        other.core.cursor_position,
                        delta_arc,
                    );

                    if let Some(ref sel) = other.core.selection {
                        other.core.selection = Some(apply_selection_delta(
                            &before_line_index,
                            new_index,
                            sel,
                            delta_arc,
                        ));
                    }

                    for sel in &mut other.core.secondary_selections {
                        *sel = apply_selection_delta(&before_line_index, new_index, sel, delta_arc);
                    }

                    if let Some(ref mut session) = other.core.snippet_session {
                        session.apply_delta(delta_arc);
                    }
                }

                // Keep navigation state stable under edits.
                buffer.bookmarks.apply_delta(delta_arc);
                buffer.marks.apply_delta(delta_arc);
                for other in views.values_mut() {
                    if other.buffer != buffer_id {
                        continue;
                    }
                    other.jump_list.apply_delta(buffer_id, delta_arc);
                }
            }

            for other in views.values_mut() {
                if other.buffer != buffer_id {
                    continue;
                }
                Self::notify_view(other, change_type, delta.clone());
            }

            if buffer_text_changed && let Some(uri) = buffer.meta.uri.as_deref() {
                self.intelligence.mark_stale_for_uri(uri);
            }

            buffer.version = buffer.version.saturating_add(1);
        } else {
            Self::notify_view(view, change_type, None);
        }

        Ok(result)
    }

    /// Return `true` if the given view currently has an active snippet session.
    ///
    /// Snippet sessions are created by snippet inserts (for example LSP completion items with
    /// `insertTextFormat == 2`) and allow tab/shift-tab navigation between placeholders.
    pub fn has_active_snippet_session(&self, view_id: ViewId) -> Result<bool, WorkspaceError> {
        let Some(view) = self.views.get(&view_id) else {
            return Err(WorkspaceError::ViewNotFound(view_id));
        };
        Ok(view
            .core
            .snippet_session
            .as_ref()
            .map(|s| s.is_active())
            .unwrap_or(false))
    }

    /// Toggle a bookmark at the **current cursor line** for the given view.
    ///
    /// Returns `true` if a bookmark was added, or `false` if an existing bookmark on that line was
    /// removed.
    pub fn toggle_bookmark_at_cursor_line(
        &mut self,
        view_id: ViewId,
    ) -> Result<bool, WorkspaceError> {
        let Some(buffer_id) = self.views.get(&view_id).map(|v| v.buffer) else {
            return Err(WorkspaceError::ViewNotFound(view_id));
        };
        let Some(buffer) = self.buffers.get_mut(&buffer_id) else {
            return Err(WorkspaceError::BufferNotFound(buffer_id));
        };
        let Some(view) = self.views.get(&view_id) else {
            return Err(WorkspaceError::ViewNotFound(view_id));
        };

        let line_start = buffer
            .executor
            .editor()
            .line_index()
            .position_to_char_offset(view.core.cursor_position.line, 0);

        let added = buffer.bookmarks.toggle_line_start(line_start);

        for v in self.views.values_mut() {
            if v.buffer == buffer_id {
                Self::notify_view(v, StateChangeType::NavigationChanged, None);
            }
        }

        Ok(added)
    }

    /// Return all bookmark line numbers (0-based) for a buffer.
    pub fn bookmark_lines(&self, buffer_id: BufferId) -> Result<Vec<usize>, WorkspaceError> {
        let Some(buffer) = self.buffers.get(&buffer_id) else {
            return Err(WorkspaceError::BufferNotFound(buffer_id));
        };
        Ok(buffer
            .bookmarks
            .line_numbers(buffer.executor.editor().line_index()))
    }

    /// Clear all bookmarks for a buffer.
    pub fn clear_bookmarks(&mut self, buffer_id: BufferId) -> Result<(), WorkspaceError> {
        let Some(buffer) = self.buffers.get_mut(&buffer_id) else {
            return Err(WorkspaceError::BufferNotFound(buffer_id));
        };
        buffer.bookmarks.clear();

        for v in self.views.values_mut() {
            if v.buffer == buffer_id {
                Self::notify_view(v, StateChangeType::NavigationChanged, None);
            }
        }

        Ok(())
    }

    fn move_view_cursor_to_anchor(
        view: &mut ViewEntry,
        buffer: &BufferEntry,
        anchor: TextAnchor,
    ) -> Position {
        let (line, column) = buffer
            .executor
            .editor()
            .line_index()
            .char_offset_to_position(anchor.offset);
        view.core.cursor_position = Position::new(line, column);
        view.core.preferred_x_cells = buffer
            .executor
            .editor()
            .logical_position_to_visual(line, column)
            .map(|(_, x)| x);
        view.core.selection = None;
        view.core.secondary_selections.clear();
        view.core.cursor_position
    }

    /// Move the cursor to the next bookmark (wrapping to the first bookmark).
    ///
    /// Returns the new cursor position, or `None` if there are no bookmarks.
    pub fn goto_next_bookmark(
        &mut self,
        view_id: ViewId,
    ) -> Result<Option<Position>, WorkspaceError> {
        let Some(buffer_id) = self.views.get(&view_id).map(|v| v.buffer) else {
            return Err(WorkspaceError::ViewNotFound(view_id));
        };
        let Some(buffer) = self.buffers.get(&buffer_id) else {
            return Err(WorkspaceError::BufferNotFound(buffer_id));
        };

        let current_line_start = buffer
            .executor
            .editor()
            .line_index()
            .position_to_char_offset(
                self.views
                    .get(&view_id)
                    .ok_or(WorkspaceError::ViewNotFound(view_id))?
                    .core
                    .cursor_position
                    .line,
                0,
            );

        let Some(target) = buffer.bookmarks.next_after_line_start(current_line_start) else {
            return Ok(None);
        };

        let Some(view) = self.views.get_mut(&view_id) else {
            return Err(WorkspaceError::ViewNotFound(view_id));
        };
        let pos = Self::move_view_cursor_to_anchor(view, buffer, target);
        Self::notify_view(view, StateChangeType::SelectionChanged, None);
        Ok(Some(pos))
    }

    /// Move the cursor to the previous bookmark (wrapping to the last bookmark).
    ///
    /// Returns the new cursor position, or `None` if there are no bookmarks.
    pub fn goto_prev_bookmark(
        &mut self,
        view_id: ViewId,
    ) -> Result<Option<Position>, WorkspaceError> {
        let Some(buffer_id) = self.views.get(&view_id).map(|v| v.buffer) else {
            return Err(WorkspaceError::ViewNotFound(view_id));
        };
        let Some(buffer) = self.buffers.get(&buffer_id) else {
            return Err(WorkspaceError::BufferNotFound(buffer_id));
        };

        let current_line_start = buffer
            .executor
            .editor()
            .line_index()
            .position_to_char_offset(
                self.views
                    .get(&view_id)
                    .ok_or(WorkspaceError::ViewNotFound(view_id))?
                    .core
                    .cursor_position
                    .line,
                0,
            );

        let Some(target) = buffer.bookmarks.prev_before_line_start(current_line_start) else {
            return Ok(None);
        };

        let Some(view) = self.views.get_mut(&view_id) else {
            return Err(WorkspaceError::ViewNotFound(view_id));
        };
        let pos = Self::move_view_cursor_to_anchor(view, buffer, target);
        Self::notify_view(view, StateChangeType::SelectionChanged, None);
        Ok(Some(pos))
    }

    /// Set (or replace) a named mark at the current cursor position of the given view.
    pub fn set_mark_at_cursor(
        &mut self,
        view_id: ViewId,
        name: String,
    ) -> Result<(), WorkspaceError> {
        if name.trim().is_empty() {
            return Err(WorkspaceError::CommandFailed {
                view: view_id,
                message: "Mark name cannot be empty".to_string(),
            });
        }

        let Some(buffer_id) = self.views.get(&view_id).map(|v| v.buffer) else {
            return Err(WorkspaceError::ViewNotFound(view_id));
        };
        let Some(buffer) = self.buffers.get_mut(&buffer_id) else {
            return Err(WorkspaceError::BufferNotFound(buffer_id));
        };
        let Some(view) = self.views.get(&view_id) else {
            return Err(WorkspaceError::ViewNotFound(view_id));
        };

        let pos = view.core.cursor_position;
        let offset = buffer
            .executor
            .editor()
            .line_index()
            .position_to_char_offset(pos.line, pos.column);
        buffer.marks.set(name, offset);

        for v in self.views.values_mut() {
            if v.buffer == buffer_id {
                Self::notify_view(v, StateChangeType::NavigationChanged, None);
            }
        }

        Ok(())
    }

    /// Move the cursor to a named mark (if present).
    ///
    /// Returns the new cursor position, or `None` if the mark does not exist.
    pub fn goto_mark(
        &mut self,
        view_id: ViewId,
        name: &str,
    ) -> Result<Option<Position>, WorkspaceError> {
        let Some(buffer_id) = self.views.get(&view_id).map(|v| v.buffer) else {
            return Err(WorkspaceError::ViewNotFound(view_id));
        };
        let Some(buffer) = self.buffers.get(&buffer_id) else {
            return Err(WorkspaceError::BufferNotFound(buffer_id));
        };

        let Some(anchor) = buffer.marks.get(name) else {
            return Ok(None);
        };

        let Some(view) = self.views.get_mut(&view_id) else {
            return Err(WorkspaceError::ViewNotFound(view_id));
        };
        let pos = Self::move_view_cursor_to_anchor(view, buffer, anchor);
        Self::notify_view(view, StateChangeType::SelectionChanged, None);
        Ok(Some(pos))
    }

    /// Remove a named mark from a buffer.
    ///
    /// Returns `true` if the mark existed.
    pub fn clear_mark(&mut self, buffer_id: BufferId, name: &str) -> Result<bool, WorkspaceError> {
        let Some(buffer) = self.buffers.get_mut(&buffer_id) else {
            return Err(WorkspaceError::BufferNotFound(buffer_id));
        };
        let existed = buffer.marks.remove(name);
        if existed {
            for v in self.views.values_mut() {
                if v.buffer == buffer_id {
                    Self::notify_view(v, StateChangeType::NavigationChanged, None);
                }
            }
        }
        Ok(existed)
    }

    /// Return all mark names for a buffer (deterministic order).
    pub fn mark_names(&self, buffer_id: BufferId) -> Result<Vec<String>, WorkspaceError> {
        let Some(buffer) = self.buffers.get(&buffer_id) else {
            return Err(WorkspaceError::BufferNotFound(buffer_id));
        };
        Ok(buffer.marks.names())
    }

    /// Clear all marks for a buffer.
    pub fn clear_all_marks(&mut self, buffer_id: BufferId) -> Result<(), WorkspaceError> {
        let Some(buffer) = self.buffers.get_mut(&buffer_id) else {
            return Err(WorkspaceError::BufferNotFound(buffer_id));
        };
        buffer.marks.clear();
        for v in self.views.values_mut() {
            if v.buffer == buffer_id {
                Self::notify_view(v, StateChangeType::NavigationChanged, None);
            }
        }
        Ok(())
    }

    /// Record the current cursor position as a jump-list location for a view.
    ///
    /// Typical usage: call this *before* performing a “jump” (go-to-definition, search result,
    /// symbol navigation, ...).
    pub fn push_jump_location(&mut self, view_id: ViewId) -> Result<(), WorkspaceError> {
        let Some(buffer_id) = self.views.get(&view_id).map(|v| v.buffer) else {
            return Err(WorkspaceError::ViewNotFound(view_id));
        };
        let Some(buffer) = self.buffers.get(&buffer_id) else {
            return Err(WorkspaceError::BufferNotFound(buffer_id));
        };
        let Some(view) = self.views.get_mut(&view_id) else {
            return Err(WorkspaceError::ViewNotFound(view_id));
        };

        let pos = view.core.cursor_position;
        let offset = buffer
            .executor
            .editor()
            .line_index()
            .position_to_char_offset(pos.line, pos.column);

        view.jump_list.record(JumpEntry {
            buffer_id,
            anchor: TextAnchor::new(offset, AnchorBias::Right),
        });

        Self::notify_view(view, StateChangeType::NavigationChanged, None);
        Ok(())
    }

    /// Jump back in the view's jump list.
    ///
    /// Returns the navigation target (including the buffer id). If the target belongs to the
    /// current view's buffer, this method also moves the cursor and clears selection.
    pub fn jump_back(&mut self, view_id: ViewId) -> Result<Option<JumpTarget>, WorkspaceError> {
        let Some(buffer_id) = self.views.get(&view_id).map(|v| v.buffer) else {
            return Err(WorkspaceError::ViewNotFound(view_id));
        };
        let Some(buffer) = self.buffers.get(&buffer_id) else {
            return Err(WorkspaceError::BufferNotFound(buffer_id));
        };

        let current_pos = self
            .views
            .get(&view_id)
            .ok_or(WorkspaceError::ViewNotFound(view_id))?
            .core
            .cursor_position;
        let current_offset = buffer
            .executor
            .editor()
            .line_index()
            .position_to_char_offset(current_pos.line, current_pos.column);
        let current = JumpEntry {
            buffer_id,
            anchor: TextAnchor::new(current_offset, AnchorBias::Right),
        };

        let Some(view) = self.views.get_mut(&view_id) else {
            return Err(WorkspaceError::ViewNotFound(view_id));
        };
        let Some(target) = view.jump_list.back(current) else {
            return Ok(None);
        };

        let Some(target_buffer) = self.buffers.get(&target.buffer_id) else {
            Self::notify_view(view, StateChangeType::NavigationChanged, None);
            return Ok(None);
        };

        let (line, column) = target_buffer
            .executor
            .editor()
            .line_index()
            .char_offset_to_position(target.anchor.offset);
        let target_pos = Position::new(line, column);

        let out = JumpTarget {
            buffer_id: target.buffer_id,
            position: target_pos,
        };

        if target.buffer_id == buffer_id {
            Self::move_view_cursor_to_anchor(view, buffer, target.anchor);
            Self::notify_view(view, StateChangeType::SelectionChanged, None);
        } else {
            Self::notify_view(view, StateChangeType::NavigationChanged, None);
        }

        Ok(Some(out))
    }

    /// Jump forward in the view's jump list.
    ///
    /// Returns the navigation target (including the buffer id). If the target belongs to the
    /// current view's buffer, this method also moves the cursor and clears selection.
    pub fn jump_forward(&mut self, view_id: ViewId) -> Result<Option<JumpTarget>, WorkspaceError> {
        let Some(buffer_id) = self.views.get(&view_id).map(|v| v.buffer) else {
            return Err(WorkspaceError::ViewNotFound(view_id));
        };
        let Some(buffer) = self.buffers.get(&buffer_id) else {
            return Err(WorkspaceError::BufferNotFound(buffer_id));
        };

        let current_pos = self
            .views
            .get(&view_id)
            .ok_or(WorkspaceError::ViewNotFound(view_id))?
            .core
            .cursor_position;
        let current_offset = buffer
            .executor
            .editor()
            .line_index()
            .position_to_char_offset(current_pos.line, current_pos.column);
        let current = JumpEntry {
            buffer_id,
            anchor: TextAnchor::new(current_offset, AnchorBias::Right),
        };

        let Some(view) = self.views.get_mut(&view_id) else {
            return Err(WorkspaceError::ViewNotFound(view_id));
        };
        let Some(target) = view.jump_list.forward(current) else {
            return Ok(None);
        };

        let Some(target_buffer) = self.buffers.get(&target.buffer_id) else {
            Self::notify_view(view, StateChangeType::NavigationChanged, None);
            return Ok(None);
        };

        let (line, column) = target_buffer
            .executor
            .editor()
            .line_index()
            .char_offset_to_position(target.anchor.offset);
        let target_pos = Position::new(line, column);

        let out = JumpTarget {
            buffer_id: target.buffer_id,
            position: target_pos,
        };

        if target.buffer_id == buffer_id {
            Self::move_view_cursor_to_anchor(view, buffer, target.anchor);
            Self::notify_view(view, StateChangeType::SelectionChanged, None);
        } else {
            Self::notify_view(view, StateChangeType::NavigationChanged, None);
        }

        Ok(Some(out))
    }

    /// Clear the jump list (both back/forward stacks) for a view.
    pub fn clear_jump_list(&mut self, view_id: ViewId) -> Result<(), WorkspaceError> {
        let Some(view) = self.views.get_mut(&view_id) else {
            return Err(WorkspaceError::ViewNotFound(view_id));
        };
        view.jump_list.clear();
        Self::notify_view(view, StateChangeType::NavigationChanged, None);
        Ok(())
    }

    /// Apply a previously produced [`JumpTarget`] to a view (moves the cursor and clears
    /// selection).
    pub fn apply_jump_target(
        &mut self,
        view_id: ViewId,
        target: JumpTarget,
    ) -> Result<(), WorkspaceError> {
        let Some(view) = self.views.get_mut(&view_id) else {
            return Err(WorkspaceError::ViewNotFound(view_id));
        };
        if view.buffer != target.buffer_id {
            return Err(WorkspaceError::CommandFailed {
                view: view_id,
                message: "JumpTarget buffer does not match view buffer".to_string(),
            });
        }

        let Some(buffer) = self.buffers.get(&view.buffer) else {
            return Err(WorkspaceError::BufferNotFound(view.buffer));
        };

        view.core.cursor_position = target.position;
        view.core.preferred_x_cells = buffer
            .executor
            .editor()
            .logical_position_to_visual(target.position.line, target.position.column)
            .map(|(_, x)| x);
        view.core.selection = None;
        view.core.secondary_selections.clear();

        Self::notify_view(view, StateChangeType::SelectionChanged, None);
        Ok(())
    }

    /// Set the viewport height for a view (used for `ViewportState` calculations).
    pub fn set_viewport_height(
        &mut self,
        view_id: ViewId,
        height: usize,
    ) -> Result<(), WorkspaceError> {
        let Some(view) = self.views.get_mut(&view_id) else {
            return Err(WorkspaceError::ViewNotFound(view_id));
        };
        view.viewport_height = Some(height);
        Ok(())
    }

    /// Set the scroll position (top visual row) for a view.
    pub fn set_scroll_top(
        &mut self,
        view_id: ViewId,
        scroll_top: usize,
    ) -> Result<(), WorkspaceError> {
        let Some(view) = self.views.get_mut(&view_id) else {
            return Err(WorkspaceError::ViewNotFound(view_id));
        };
        view.scroll_top = scroll_top;
        Ok(())
    }

    /// Set sub-row smooth-scroll offset for a view.
    pub fn set_scroll_sub_row_offset(
        &mut self,
        view_id: ViewId,
        sub_row_offset: u16,
    ) -> Result<(), WorkspaceError> {
        let Some(view) = self.views.get_mut(&view_id) else {
            return Err(WorkspaceError::ViewNotFound(view_id));
        };
        view.scroll_sub_row_offset = sub_row_offset;
        Ok(())
    }

    /// Set overscan rows for a view.
    pub fn set_overscan_rows(
        &mut self,
        view_id: ViewId,
        overscan_rows: usize,
    ) -> Result<(), WorkspaceError> {
        let Some(view) = self.views.get_mut(&view_id) else {
            return Err(WorkspaceError::ViewNotFound(view_id));
        };
        view.overscan_rows = overscan_rows;
        Ok(())
    }

    /// Set smooth-scroll state for a view.
    pub fn set_smooth_scroll_state(
        &mut self,
        view_id: ViewId,
        state: ViewSmoothScrollState,
    ) -> Result<(), WorkspaceError> {
        let Some(view) = self.views.get_mut(&view_id) else {
            return Err(WorkspaceError::ViewNotFound(view_id));
        };
        view.scroll_top = state.top_visual_row;
        view.scroll_sub_row_offset = state.sub_row_offset;
        view.overscan_rows = state.overscan_rows;
        Ok(())
    }

    /// Get viewport state for a view, including total visual lines and overscan prefetch range.
    pub fn viewport_state_for_view(
        &mut self,
        view_id: ViewId,
    ) -> Result<WorkspaceViewportState, WorkspaceError> {
        let Some((
            buffer_id,
            view_core,
            scroll_top,
            viewport_height,
            sub_row_offset,
            overscan_rows,
        )) = self.views.get(&view_id).map(|v| {
            (
                v.buffer,
                v.core.clone(),
                v.scroll_top,
                v.viewport_height,
                v.scroll_sub_row_offset,
                v.overscan_rows,
            )
        })
        else {
            return Err(WorkspaceError::ViewNotFound(view_id));
        };

        let Some(buffer) = self.buffers.get_mut(&buffer_id) else {
            return Err(WorkspaceError::BufferNotFound(buffer_id));
        };
        view_core.apply_to_executor(&mut buffer.executor);
        let editor = buffer.executor.editor();

        let total_visual_lines = editor.visual_line_count();
        let visible_end = if let Some(height) = viewport_height {
            scroll_top.saturating_add(height).min(total_visual_lines)
        } else {
            total_visual_lines
        };
        let visible_lines = scroll_top.min(total_visual_lines)..visible_end;
        let prefetch_start = visible_lines.start.saturating_sub(overscan_rows);
        let prefetch_end = visible_lines
            .end
            .saturating_add(overscan_rows)
            .min(total_visual_lines);

        Ok(WorkspaceViewportState {
            width: editor.viewport_width(),
            height: viewport_height,
            scroll_top,
            visible_lines,
            total_visual_lines,
            smooth_scroll: ViewSmoothScrollState {
                top_visual_row: scroll_top,
                sub_row_offset,
                overscan_rows,
            },
            prefetch_lines: prefetch_start..prefetch_end,
        })
    }

    /// Get total visual lines for a view (wrap + folding aware).
    pub fn total_visual_lines_for_view(
        &mut self,
        view_id: ViewId,
    ) -> Result<usize, WorkspaceError> {
        Ok(self.viewport_state_for_view(view_id)?.total_visual_lines)
    }

    /// Map global visual row to `(logical_line, visual_in_logical)` for a view.
    pub fn visual_to_logical_for_view(
        &mut self,
        view_id: ViewId,
        visual_row: usize,
    ) -> Result<(usize, usize), WorkspaceError> {
        let Some((buffer_id, view_core)) =
            self.views.get(&view_id).map(|v| (v.buffer, v.core.clone()))
        else {
            return Err(WorkspaceError::ViewNotFound(view_id));
        };
        let Some(buffer) = self.buffers.get_mut(&buffer_id) else {
            return Err(WorkspaceError::BufferNotFound(buffer_id));
        };
        view_core.apply_to_executor(&mut buffer.executor);
        Ok(buffer.executor.editor().visual_to_logical_line(visual_row))
    }

    /// Map logical position to global visual `(row, x_cells)` for a view.
    pub fn logical_to_visual_for_view(
        &mut self,
        view_id: ViewId,
        line: usize,
        column: usize,
    ) -> Result<Option<(usize, usize)>, WorkspaceError> {
        let Some((buffer_id, view_core)) =
            self.views.get(&view_id).map(|v| (v.buffer, v.core.clone()))
        else {
            return Err(WorkspaceError::ViewNotFound(view_id));
        };
        let Some(buffer) = self.buffers.get_mut(&buffer_id) else {
            return Err(WorkspaceError::BufferNotFound(buffer_id));
        };
        view_core.apply_to_executor(&mut buffer.executor);
        Ok(buffer
            .executor
            .editor()
            .logical_position_to_visual(line, column))
    }

    /// Map visual `(row, x_cells)` back to logical position for a view.
    pub fn visual_position_to_logical_for_view(
        &mut self,
        view_id: ViewId,
        visual_row: usize,
        x_cells: usize,
    ) -> Result<Option<Position>, WorkspaceError> {
        let Some((buffer_id, view_core)) =
            self.views.get(&view_id).map(|v| (v.buffer, v.core.clone()))
        else {
            return Err(WorkspaceError::ViewNotFound(view_id));
        };
        let Some(buffer) = self.buffers.get_mut(&buffer_id) else {
            return Err(WorkspaceError::BufferNotFound(buffer_id));
        };
        view_core.apply_to_executor(&mut buffer.executor);
        Ok(buffer
            .executor
            .editor()
            .visual_position_to_logical(visual_row, x_cells))
    }

    /// Get the full document text for a buffer.
    pub fn buffer_text(&self, buffer_id: BufferId) -> Result<String, WorkspaceError> {
        let Some(buffer) = self.buffers.get(&buffer_id) else {
            return Err(WorkspaceError::BufferNotFound(buffer_id));
        };
        Ok(buffer.executor.editor().get_text())
    }

    /// Get the full document text converted to the buffer's preferred line ending for saving.
    pub fn buffer_text_for_saving(&self, buffer_id: BufferId) -> Result<String, WorkspaceError> {
        let Some(buffer) = self.buffers.get(&buffer_id) else {
            return Err(WorkspaceError::BufferNotFound(buffer_id));
        };
        let text = buffer.executor.editor().get_text();
        Ok(buffer.executor.line_ending().apply_to_text(&text))
    }

    /// Get the full document text converted to the view's preferred line ending for saving.
    pub fn text_for_saving_for_view(&self, view_id: ViewId) -> Result<String, WorkspaceError> {
        let buffer_id = self.buffer_id_for_view(view_id)?;
        self.buffer_text_for_saving(buffer_id)
    }

    /// Get styled viewport content for a view (by visual line).
    pub fn get_viewport_content_styled(
        &mut self,
        view_id: ViewId,
        start_visual_row: usize,
        count: usize,
    ) -> Result<crate::HeadlessGrid, WorkspaceError> {
        let Some(buffer_id) = self.views.get(&view_id).map(|v| v.buffer) else {
            return Err(WorkspaceError::ViewNotFound(view_id));
        };

        let view_core = self
            .views
            .get(&view_id)
            .map(|v| v.core.clone())
            .ok_or(WorkspaceError::ViewNotFound(view_id))?;

        let Some(buffer) = self.buffers.get_mut(&buffer_id) else {
            return Err(WorkspaceError::BufferNotFound(buffer_id));
        };

        view_core.apply_to_executor(&mut buffer.executor);
        Ok(buffer
            .executor
            .editor()
            .get_headless_grid_styled(start_visual_row, count))
    }

    /// Get lightweight minimap content for a view (by visual line).
    pub fn get_minimap_content(
        &mut self,
        view_id: ViewId,
        start_visual_row: usize,
        count: usize,
    ) -> Result<crate::MinimapGrid, WorkspaceError> {
        let Some(buffer_id) = self.views.get(&view_id).map(|v| v.buffer) else {
            return Err(WorkspaceError::ViewNotFound(view_id));
        };

        let view_core = self
            .views
            .get(&view_id)
            .map(|v| v.core.clone())
            .ok_or(WorkspaceError::ViewNotFound(view_id))?;

        let Some(buffer) = self.buffers.get_mut(&buffer_id) else {
            return Err(WorkspaceError::BufferNotFound(buffer_id));
        };

        view_core.apply_to_executor(&mut buffer.executor);
        Ok(buffer
            .executor
            .editor()
            .get_minimap_grid(start_visual_row, count))
    }

    /// Get a decoration-aware composed viewport snapshot for a view (by composed visual line).
    ///
    /// This snapshot can include virtual text (inlay hints, code lens) injected from the buffer's
    /// decoration layers. See [`crate::EditorCore::get_headless_grid_composed`] for details.
    pub fn get_viewport_content_composed(
        &mut self,
        view_id: ViewId,
        start_visual_row: usize,
        count: usize,
    ) -> Result<crate::ComposedGrid, WorkspaceError> {
        let Some(buffer_id) = self.views.get(&view_id).map(|v| v.buffer) else {
            return Err(WorkspaceError::ViewNotFound(view_id));
        };

        let view_core = self
            .views
            .get(&view_id)
            .map(|v| v.core.clone())
            .ok_or(WorkspaceError::ViewNotFound(view_id))?;

        let Some(buffer) = self.buffers.get_mut(&buffer_id) else {
            return Err(WorkspaceError::BufferNotFound(buffer_id));
        };

        view_core.apply_to_executor(&mut buffer.executor);
        Ok(buffer
            .executor
            .editor()
            .get_headless_grid_composed(start_visual_row, count))
    }

    /// Apply derived-state edits to a buffer and broadcast them to all views of that buffer.
    pub fn apply_processing_edits<I>(
        &mut self,
        buffer_id: BufferId,
        edits: I,
    ) -> Result<(), WorkspaceError>
    where
        I: IntoIterator<Item = ProcessingEdit>,
    {
        let Some(buffer) = self.buffers.get_mut(&buffer_id) else {
            return Err(WorkspaceError::BufferNotFound(buffer_id));
        };

        let mut style_changed = false;
        let mut folding_changed = false;
        let mut diagnostics_changed = false;
        let mut decorations_changed = false;
        let mut symbols_changed = false;

        for edit in edits {
            match edit {
                ProcessingEdit::ReplaceStyleLayer { layer, intervals } => {
                    buffer
                        .executor
                        .editor_mut()
                        .replace_style_layer(layer, intervals);
                    style_changed = true;
                }
                ProcessingEdit::ClearStyleLayer { layer } => {
                    buffer.executor.editor_mut().clear_style_layer(layer);
                    style_changed = true;
                }
                ProcessingEdit::ReplaceFoldingRegions {
                    regions,
                    preserve_collapsed,
                } => {
                    buffer
                        .executor
                        .editor_mut()
                        .replace_folding_regions(regions, preserve_collapsed);
                    folding_changed = true;
                }
                ProcessingEdit::ClearFoldingRegions => {
                    buffer.executor.editor_mut().clear_derived_folding_regions();
                    folding_changed = true;
                }
                ProcessingEdit::ReplaceDiagnostics { diagnostics } => {
                    buffer
                        .executor
                        .editor_mut()
                        .replace_diagnostics(diagnostics);
                    diagnostics_changed = true;
                }
                ProcessingEdit::ClearDiagnostics => {
                    buffer.executor.editor_mut().clear_diagnostics();
                    diagnostics_changed = true;
                }
                ProcessingEdit::ReplaceDecorations { layer, decorations } => {
                    buffer
                        .executor
                        .editor_mut()
                        .replace_decorations(layer, decorations);
                    decorations_changed = true;
                }
                ProcessingEdit::ClearDecorations { layer } => {
                    buffer.executor.editor_mut().clear_decorations(layer);
                    decorations_changed = true;
                }
                ProcessingEdit::ReplaceDocumentSymbols { symbols } => {
                    buffer
                        .executor
                        .editor_mut()
                        .replace_document_symbols(symbols);
                    symbols_changed = true;
                }
                ProcessingEdit::ClearDocumentSymbols => {
                    buffer.executor.editor_mut().clear_document_symbols();
                    symbols_changed = true;
                }
            }
        }

        let change_type = if folding_changed {
            Some(StateChangeType::FoldingChanged)
        } else if style_changed {
            Some(StateChangeType::StyleChanged)
        } else if decorations_changed {
            Some(StateChangeType::DecorationsChanged)
        } else if diagnostics_changed {
            Some(StateChangeType::DiagnosticsChanged)
        } else if symbols_changed {
            Some(StateChangeType::SymbolsChanged)
        } else {
            None
        };

        if let Some(change_type) = change_type {
            for view in self.views.values_mut() {
                if view.buffer == buffer_id {
                    Self::notify_view(view, change_type, None);
                }
            }
            buffer.version = buffer.version.saturating_add(1);
        }

        Ok(())
    }

    /// Search across all open buffers in the workspace.
    ///
    /// - This is purely in-memory (no file I/O).
    /// - Match ranges are returned as **character offsets** (half-open).
    pub fn search_all_open_buffers(
        &self,
        query: &str,
        options: SearchOptions,
    ) -> Result<Vec<WorkspaceSearchResult>, SearchError> {
        let mut out: Vec<WorkspaceSearchResult> = Vec::new();

        for (id, entry) in &self.buffers {
            let text = entry.executor.editor().get_text();
            let matches = find_all(&text, query, options)?;
            if matches.is_empty() {
                continue;
            }

            out.push(WorkspaceSearchResult {
                id: *id,
                uri: entry.meta.uri.clone(),
                matches,
            });
        }

        Ok(out)
    }

    /// Apply a set of text edits to multiple open buffers.
    ///
    /// - This is purely in-memory (no file I/O).
    /// - Edits are applied as a single undoable step **per buffer**.
    /// - Buffers are applied in deterministic `BufferId` order.
    pub fn apply_text_edits<I>(
        &mut self,
        edits: I,
    ) -> Result<Vec<(BufferId, usize)>, WorkspaceError>
    where
        I: IntoIterator<Item = (BufferId, Vec<TextEditSpec>)>,
    {
        let mut by_id: BTreeMap<BufferId, Vec<TextEditSpec>> = BTreeMap::new();
        for (id, mut buffer_edits) in edits {
            by_id.entry(id).or_default().append(&mut buffer_edits);
        }

        let mut applied: Vec<(BufferId, usize)> = Vec::new();
        for (buffer_id, buffer_edits) in by_id {
            let edit_count = buffer_edits.len();
            if edit_count == 0 {
                continue;
            }

            let Some(buffer) = self.buffers.get_mut(&buffer_id) else {
                return Err(WorkspaceError::BufferNotFound(buffer_id));
            };

            let before_line_index = buffer.executor.editor().line_index().clone();
            let before_char_count = buffer.executor.editor().char_count();

            // Apply without relying on any specific view selection: load a neutral view state.
            let neutral = ViewCore {
                cursor_position: Position::new(0, 0),
                selection: None,
                secondary_selections: Vec::new(),
                viewport_width: buffer.executor.editor().viewport_width().max(1),
                wrap_mode: buffer.executor.editor().layout_engine().wrap_mode(),
                wrap_indent: buffer.executor.editor().layout_engine().wrap_indent(),
                tab_width: buffer.executor.editor().layout_engine().tab_width(),
                tab_key_behavior: buffer.executor.tab_key_behavior(),
                indentation_config: buffer.executor.indentation_config().clone(),
                auto_pairs: buffer.executor.auto_pairs_config().clone(),
                snippet_session: None,
                preferred_x_cells: None,
            };
            neutral.apply_to_executor(&mut buffer.executor);

            buffer
                .executor
                .execute(Command::Edit(EditCommand::ApplyTextEdits {
                    edits: buffer_edits,
                }))
                .map_err(|err| WorkspaceError::ApplyEditsFailed {
                    buffer: buffer_id,
                    message: err.to_string(),
                })?;

            let delta = buffer.executor.take_last_text_delta().map(Arc::new);
            let after_char_count = buffer.executor.editor().char_count();
            let changed = delta.is_some() || after_char_count != before_char_count;

            if changed {
                if let Some(uri) = buffer.meta.uri.as_deref() {
                    self.intelligence.mark_stale_for_uri(uri);
                }

                if let Some(ref delta_arc) = delta {
                    // Coalesce into the delta slots (see `coalesce_delta_slot`) so an unconsumed
                    // delta from a prior edit is not lost.
                    coalesce_delta_slot(&mut buffer.last_text_delta, delta_arc);
                    let new_index = buffer.executor.editor().line_index();
                    for view in self.views.values_mut() {
                        if view.buffer != buffer_id {
                            continue;
                        }

                        coalesce_delta_slot(&mut view.last_text_delta, delta_arc);

                        view.core.cursor_position = apply_position_delta(
                            &before_line_index,
                            new_index,
                            view.core.cursor_position,
                            delta_arc,
                        );
                        if let Some(ref sel) = view.core.selection {
                            view.core.selection = Some(apply_selection_delta(
                                &before_line_index,
                                new_index,
                                sel,
                                delta_arc,
                            ));
                        }
                        for sel in &mut view.core.secondary_selections {
                            *sel = apply_selection_delta(
                                &before_line_index,
                                new_index,
                                sel,
                                delta_arc,
                            );
                        }

                        Self::notify_view(
                            view,
                            StateChangeType::DocumentModified,
                            Some(delta_arc.clone()),
                        );
                    }
                } else {
                    // No text delta for this change; do not clear an unconsumed text delta.
                    for view in self.views.values_mut() {
                        if view.buffer == buffer_id {
                            Self::notify_view(view, StateChangeType::DocumentModified, None);
                        }
                    }
                }

                buffer.version = buffer.version.saturating_add(1);
            }

            applied.push((buffer_id, edit_count));
        }

        Ok(applied)
    }
}
