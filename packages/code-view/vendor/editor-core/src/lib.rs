#![warn(missing_docs)]
//! Editor Core - Industrial-Grade Headless Code Editor Kernel
//!
//! # Overview
//!
//! `editor-core` is a headless code editor kernel focused on state management, text metrics, and
//! coordinate transformations.
//!
//! It does not render UI. Hosts render from snapshots (e.g. [`HeadlessGrid`]) and drive edits via
//! the command/state APIs.
//!
//! # Core Features
//!
//! - **Efficient Text Storage**: Rope-backed text buffer with `char`-indexed access
//! - **Fast Line Index**: Rope-based line access and coordinate conversion
//! - **Soft Wrapping Support**: Headless layout engine, supporting arbitrary container widths
//! - **Style Management**: Interval tree structure, O(log n + k) query complexity
//! - **Code Folding**: Supports arbitrary levels of code folding
//! - **State Tracking**: Version number mechanism and Change Notifications system
//! - **Workspace model**: Multi-buffer + multi-view orchestration (`Workspace`, `BufferId`,
//!   `ViewId`) for tabs and split panes
//!
//! # Architecture Layers
//!
//! ```text
//! ┌─────────────────────────────────────────────┐
//! │  Command Interface & State Management       │  ← Public API
//! ├─────────────────────────────────────────────┤
//! │  Snapshot API (HeadlessGrid)                │  ← Rendering Data
//! ├─────────────────────────────────────────────┤
//! │  Intervals & Visibility (Styles + Folding)  │  ← Visual Enhancement
//! ├─────────────────────────────────────────────┤
//! │  Layout Engine (Soft Wrapping)              │  ← Text Layout
//! ├─────────────────────────────────────────────┤
//! │  Line Index + TextBuffer (Rope-based)       │  ← Text Storage / Line Access
//! └─────────────────────────────────────────────┘
//! ```
//!
//! # Quick Start
//!
//! ## Using Command Interface
//!
//! ```rust
//! use editor_core::{CommandExecutor, Command, EditCommand, CursorCommand, Position};
//!
//! let mut executor = CommandExecutor::empty(80);
//!
//! // Insert text
//! executor.execute(Command::Edit(EditCommand::Insert {
//!     offset: 0,
//!     text: "fn main() {\n    println!(\"Hello\");\n}\n".to_string(),
//! })).unwrap();
//!
//! // Move cursor
//! executor.execute(Command::Cursor(CursorCommand::MoveTo {
//!     line: 1,
//!     column: 4,
//! })).unwrap();
//!
//! assert_eq!(executor.editor().cursor_position(), Position::new(1, 4));
//! ```
//!
//! ## Using State Management
//!
//! ```rust
//! use editor_core::{EditorStateManager, StateChangeType};
//!
//! let mut manager = EditorStateManager::new("Initial text", 80);
//!
//! // Subscribe to state changes
//! manager.subscribe(|change| {
//!     println!("State changed: {:?}", change.change_type);
//! });
//!
//! // Query state
//! let doc_state = manager.get_document_state();
//! println!("Line count: {}, Characters: {}", doc_state.line_count, doc_state.char_count);
//! ```
//!
//! ## Using Workspace (multi-buffer / multi-view)
//!
//! ```rust
//! use editor_core::{Command, CursorCommand, EditCommand, Workspace};
//!
//! let mut ws = Workspace::new();
//! let opened = ws
//!     .open_buffer(Some("file:///demo.txt".to_string()), "Hello\nWorld\n", 80)
//!     .unwrap();
//!
//! // Commands always target a view.
//! let view = opened.view_id;
//! ws.execute(view, Command::Cursor(CursorCommand::MoveTo { line: 1, column: 0 }))
//!     .unwrap();
//! ws.execute(view, Command::Edit(EditCommand::InsertText { text: ">> ".into() }))
//!     .unwrap();
//!
//! let grid = ws.get_viewport_content_styled(view, 0, 10).unwrap();
//! assert!(grid.actual_line_count() > 0);
//! ```
//!
//! # API Visibility
//!
//! `EditorCore` keeps its storage, layout, style, folding, and cursor fields private so hosts
//! cannot bypass synchronization invariants. Use read-only getters such as
//! [`EditorCore::line_index`], [`EditorCore::layout_engine`], [`EditorCore::folding_manager`], and
//! [`EditorCore::viewport_width`] for inspection. Use [`CommandExecutor`], [`EditorStateManager`],
//! [`Workspace`], or narrowly scoped public methods for mutations that must update text, layout,
//! folding, styles, and notifications together. Layout and interval/folding types are exposed from
//! the crate root as facade re-exports rather than through public `layout` or `intervals` modules.
//!
//! # Module Description
//!
//! - [`storage`] - deprecated Piece Table compatibility layer; it is not on the main editing path
//! - [`line_index`] - Rope-based line index and canonical text access facade
//! - [`LayoutEngine`], [`WrapMode`], and related root re-exports - soft wrapping layout facade
//! - [`IntervalTree`], [`FoldingManager`], and related root re-exports - style intervals and code folding facade
//! - [`snapshot`] - Headless snapshot API (HeadlessGrid)
//! - [`commands`] - Unified command interface
//! - [`state`] - State management and query interface
//!
//! # Performance Goals
//!
//! - **Loading**: 1000 line document < 100ms
//! - **insertion**: 100 random insertions < 100ms
//! - **line access**: 1000 line accesses < 10ms
//! - **Memory**: bounded command history with rope-backed text storage on the main editing path
//!
//! # Unicode Support
//!
//! - UTF-8 internal encoding
//! - Proper handling of CJK double-width characters
//! - Public offsets and positions are `char`-indexed Unicode scalar coordinates
//! - Layout columns and snapshot ranges use Unicode scalar positions plus rendered cell widths, not
//!   grapheme-cluster indices
//! - Dedicated grapheme cursor/delete commands use UAX #29 boundaries; dedicated word movement and
//!   deletion commands use Unicode word boundaries
//! - Editor-friendly word selection and expansion use configurable ASCII token boundaries, treating
//!   non-ASCII scalars as single-character word units
//! - `editor-core-lsp` provides UTF-16 code unit coordinate conversion for LSP integrations
//! - `editor-core-sublime` provides optional `.sublime-syntax` syntax highlighting and folding

pub mod anchors;
pub mod commands;
pub mod decorations;
pub mod delta;
pub mod diagnostics;
pub mod intelligence;
pub(crate) mod intervals;
pub(crate) mod layout;
pub mod line_ending;
pub mod line_index;
pub mod processing;
pub mod search;
mod selection_set;
pub mod snapshot;
pub mod snippets;
pub mod state;
pub mod storage;
pub mod symbols;
mod text;
mod text_buffer;
mod visual_rows;
pub mod workspace;

pub use anchors::{AnchorBias, TextAnchor};
pub use commands::{
    AutoPair, AutoPairsConfig, Command, CommandError, CommandExecutor, CommandResult,
    CursorCommand, EditCommand, EditorCore, ExpandSelectionDirection, ExpandSelectionUnit,
    Position, Selection, SelectionDirection, StyleCommand, TabKeyBehavior, TextEditSpec,
    UndoHistoryRestoreError, UndoHistorySelectionSet, UndoHistorySnapshot, UndoHistoryStep,
    UndoHistoryTextEdit, ViewCommand,
};
pub use decorations::{
    Decoration, DecorationKind, DecorationLayerId, DecorationPlacement, DecorationRange,
};
pub use delta::{TextDelta, TextDeltaEdit};
pub use diagnostics::{Diagnostic, DiagnosticRange, DiagnosticSeverity};
pub use editor_core_lang::{CommentConfig, IndentStyle, IndentationConfig};
pub use intelligence::{
    CallHierarchyIncomingCall, CallHierarchyOutgoingCall, CallHierarchyResultSet, HierarchyItem,
    IntelligenceResultSet, ReferencesResultSet, ResultSetId, ResultSetKind, TypeHierarchyResultSet,
    WorkspaceIntelligence,
};
pub use intervals::{
    CODE_LENS_STYLE_ID, DIFF_ADD_LINE_STYLE_ID, DIFF_REMOVE_LINE_STYLE_ID, DIFF_SPACER_STYLE_ID,
    DOCUMENT_HIGHLIGHT_READ_STYLE_ID, DOCUMENT_HIGHLIGHT_TEXT_STYLE_ID,
    DOCUMENT_HIGHLIGHT_WRITE_STYLE_ID, DOCUMENT_LINK_STYLE_ID, FOLD_PLACEHOLDER_STYLE_ID,
    FoldRegion, FoldingManager, IME_MARKED_TEXT_STYLE_ID, INLAY_HINT_STYLE_ID, Interval,
    IntervalTextEdit, IntervalTree, MATCH_HIGHLIGHT_STYLE_ID, StyleId, StyleLayerId,
};
pub use layout::{
    DEFAULT_TAB_WIDTH, LayoutEngine, VisualLineInfo, WrapIndent, WrapMode, WrapPoint,
    calculate_wrap_points, calculate_wrap_points_with_tab_width,
    calculate_wrap_points_with_tab_width_and_mode,
    calculate_wrap_points_with_tab_width_mode_and_indent, cell_width_at, char_width, str_width,
    str_width_with_tab_width, visual_x_for_column,
};
pub use line_ending::LineEnding;
pub use line_index::LineIndex;
pub use processing::{DocumentProcessor, ProcessingEdit};
pub use search::{CompiledSearch, SearchError, SearchMatch, SearchOptions};
pub use snapshot::{
    Cell, ComposedCell, ComposedCellSource, ComposedGrid, ComposedLine, ComposedLineKind,
    HeadlessGrid, HeadlessLine, MinimapGrid, MinimapLine, SnapshotGenerator,
};
pub use snippets::{SnippetRange, SnippetSession, SnippetTabstop, SnippetTemplate, parse_snippet};
pub use state::{
    CursorState, DecorationsState, DiagnosticsState, DocumentState, EditorState,
    EditorStateManager, FoldingState, SmoothScrollState, StateChange, StateChangeCallback,
    StateChangeType, StyleState, UndoRedoState, ViewportState,
};
#[deprecated(
    note = "PieceTable is no longer on the main editing path; use EditorCore/LineIndex text APIs instead"
)]
pub use storage::PieceTable;
pub use symbols::{
    DocumentOutline, DocumentSymbol, SymbolKind, SymbolLocation, SymbolRange, Utf16Position,
    Utf16Range, WorkspaceSymbol,
};
pub use workspace::{
    BufferId, BufferMetadata, JumpTarget, OpenBufferResult, ViewId, ViewSmoothScrollState,
    Workspace, WorkspaceError, WorkspaceSearchResult, WorkspaceUndoHistoryRestoreError,
    WorkspaceViewportState,
};
