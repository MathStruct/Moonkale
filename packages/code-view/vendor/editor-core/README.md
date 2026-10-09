# editor-core

`editor-core` is a **headless** editor engine focused on state management, Unicode-aware text
measurement, and coordinate conversion. It is intentionally UI-agnostic: consumers render from
snapshots and drive edits through the command/state APIs.

## Features

- **Efficient text storage** via a Piece Table (`PieceTable`) for inserts/deletes.
- **Fast line indexing** via a rope-backed `LineIndex` for line access and conversions.
- **Soft wrapping layout** (`LayoutEngine`) with Unicode-aware cell widths.
- **Style + folding metadata** via interval trees (`IntervalTree`) and fold regions (`FoldingManager`)
  (derived folds + stable user folds).
- **Symbols/outline model** (`DocumentOutline`, `DocumentSymbol`, `WorkspaceSymbol`) for building
  outline trees and symbol search UIs (typically populated from LSP).
- **Language intelligence result sets** (`WorkspaceIntelligence`) for storing cross-file results
  like references, call hierarchy, and type hierarchy (with staleness tracking on buffer edits).
- **Headless snapshots** (`SnapshotGenerator` → `HeadlessGrid`) for building “text grid” UIs.
- **Lightweight minimap snapshots** (`MinimapGrid`) for overview rendering without per-cell payload.
- **Decoration-aware composed snapshots** (`ComposedGrid`) that inject virtual text (inlay hints,
  code lens) so hosts can render from snapshot data without re-implementing layout rules.
- **Command interface** (`CommandExecutor`) and **state/query layer** (`EditorStateManager`).
- **Undo history persistence ("hot exit")** via `undo_history_snapshot` / `restore_undo_history`
  (optional `serde` feature for easy JSON serialization).
- **Undo tree (branching history)** (optional power feature):
  - new edits after undo create alternate branches instead of discarding the previous redo path
  - select which branch `Redo` follows via `CommandExecutor::{redo_branch_count, select_redo_branch}`
- **Workspace model** (`Workspace`) for multi-buffer + multi-view (split panes):
  - open buffers: `Workspace::open_buffer` → `OpenBufferResult { buffer_id, view_id }`
  - create additional views: `Workspace::create_view`
  - execute commands against a view: `Workspace::execute`
  - render from a view: `Workspace::get_viewport_content_styled`
  - query visual-row mappings from a view: `Workspace::{total_visual_lines_for_view, visual_to_logical_for_view, logical_to_visual_for_view, visual_position_to_logical_for_view}`
  - query viewport + smooth-scroll metadata: `Workspace::viewport_state_for_view`
  - render minimap summaries from a view: `Workspace::get_minimap_content`
  - search across open buffers: `Workspace::search_all_open_buffers`
  - apply workspace edits (per-buffer undo grouping): `Workspace::apply_text_edits`
- **Kernel-level editing commands** for common editor UX:
  - line ops: `DuplicateLines`, `DeleteLines`, `MoveLinesUp/Down`, `JoinLines`, `SplitLine`
  - comment toggling: `ToggleComment` (language-config driven)
  - selection/multi-cursor ops: `SelectLine`, `SelectWord`, `ExpandSelection`, `AddCursorAbove/Below`,
    `AddNextOccurrence`, `AddAllOccurrences`
  - language-aware auto-indent for newlines:
    - `EditCommand::InsertNewline { auto_indent: true }` uses view-local `IndentationConfig`
      (set via `ViewCommand::SetIndentationConfig`)
  - auto-pairs + bracket matching:
    - `EditCommand::TypeChar` + `AutoPairsConfig` (auto-close, wrap selection, skip over closing, delete-pair)
    - `CursorCommand::MoveToMatchingBracket`
    - `StyleCommand::UpdateBracketMatchHighlights` (writes `StyleLayerId::BRACKET_MATCHES` with `MATCH_HIGHLIGHT_STYLE_ID`)
  - snippet engine (placeholders + navigation):
    - `EditCommand::ApplySnippet` (for LSP completion snippets / template inserts)
    - `CursorCommand::SnippetNextPlaceholder` / `SnippetPrevPlaceholder`
- **Bookmarks / marks / jump list** (anchors stable under edits):
  - `EditorStateManager::{toggle_bookmark_at_cursor_line, goto_next_bookmark, goto_prev_bookmark, set_mark_at_cursor, goto_mark, push_jump_location, jump_back, jump_forward}`
  - `Workspace::{toggle_bookmark_at_cursor_line, bookmark_lines, goto_next_bookmark, goto_prev_bookmark, set_mark_at_cursor, goto_mark, push_jump_location, jump_back, jump_forward}`
  - emits `StateChangeType::NavigationChanged` when bookmark/mark/jump-list state changes
- **Search utilities** (`find_next`, `find_prev`, `find_all`) operating on character offsets.

## Choosing an API surface (single view vs workspace)

`editor-core` is intentionally UI-agnostic, but there are a few different “levels” of API you can
build on:

- `CommandExecutor`: the lowest-level command runner for **one buffer**, including undo/redo.
  - own it directly if you want maximum control and you already have your own state manager
  - used internally by both `EditorStateManager` and `Workspace`
- `EditorStateManager`: ergonomic **single-buffer / single-view** wrapper.
  - adds `version`, `is_modified`, subscriptions, and structured query helpers
  - great starting point for simple apps and tests
- `Workspace`: **multi-buffer + multi-view** model (tabs + split panes).
  - buffers are identified by `BufferId` and own text + undo + derived metadata
  - views are identified by `ViewId` and own cursor/selection + viewport config + scroll

### Relation to the older “single view” interface

If you previously integrated `editor-core` by treating `EditorStateManager` as “the editor”, that
model still works and is still supported. Conceptually, it corresponds to “a `Workspace` with one
buffer and one view”. Moving to `Workspace` makes those identities explicit so that multiple views
can share the same buffer state.

### Migration cheat sheet (`EditorStateManager` → `Workspace`)

- Run a command: `state.execute(cmd)` → `ws.execute(view_id, cmd)`
- Render a viewport: `state.get_viewport_content_styled(start, count)` →
  `ws.get_viewport_content_styled(view_id, start, count)`
- Render lightweight minimap summaries: `state.get_minimap_content(start, count)` →
  `ws.get_minimap_content(view_id, start, count)`
- Visual row mapping queries: `state.visual_to_logical_line(...)` →
  `ws.visual_to_logical_for_view(view_id, ...)`
- Subscribe to changes: `state.subscribe(cb)` → `ws.subscribe_view(view_id, cb)`
- Apply derived state: `state.apply_processing_edits(edits)` →
  `ws.apply_processing_edits(buffer_id, edits)`
- Observe text deltas: `state.take_last_text_delta()` →
  `ws.take_last_text_delta_for_buffer(buffer_id)` (once per buffer edit) or
  `ws.take_last_text_delta_for_view(view_id)` (per view)

## Design overview

`editor-core` is organized as a set of small layers:

- **Storage**: Piece Table holds the document text.
- **Indexing**: `LineIndex` provides line access + offset/position conversions.
- **Layout**: `LayoutEngine` computes wrap points and logical↔visual mappings.
- **Intervals**: styles/folding are represented as ranges and queried efficiently.
- **Snapshots**: a UI-facing “text grid” snapshot (`HeadlessGrid`) can be rendered by any frontend.
- **State/commands**: public APIs for edits, queries, versioning, and change notifications.

### Offsets and coordinates

- Many public APIs use **character offsets** (not byte offsets) for robustness with Unicode.
- Rendering uses **cell widths** (`Cell.width` is typically 1 or 2) to support CJK and emoji.
- There is a distinction between **logical lines** (document lines) and **visual lines**
  (after soft wrapping and/or folding).

### Derived state pipeline

Higher-level integrations (like LSP semantic tokens or syntax highlighting) can compute derived
editor metadata and apply it through:

- `DocumentProcessor` (produce edits)
- `ProcessingEdit` (apply edits)
- `EditorStateManager::apply_processing_edits` (update state consistently)

## Quick start

### Command-driven editing

```rust
use editor_core::{Command, CommandExecutor, CursorCommand, EditCommand, Position};

let mut executor = CommandExecutor::empty(80);

executor.execute(Command::Edit(EditCommand::Insert {
    offset: 0,
    text: "Hello\nWorld".to_string(),
})).unwrap();

executor.execute(Command::Cursor(CursorCommand::MoveTo {
    line: 1,
    column: 2,
})).unwrap();

assert_eq!(executor.editor().cursor_position(), Position::new(1, 2));
```

### Undo history persistence ("hot exit")

For "hot exit" / restore workflows, persist the document text plus a snapshot of the undo/redo
history, then restore both on startup.

```rust
use editor_core::{Command, CommandExecutor, EditCommand};

let mut executor = CommandExecutor::new("hello", 80);
executor.execute(Command::Edit(EditCommand::InsertText { text: "!".into() })).unwrap();

let snapshot = executor.undo_history_snapshot();

// Persist `executor.editor().get_text()` + `snapshot` in your host application.
let text = executor.editor().get_text().to_string();

let mut restored = CommandExecutor::new(&text, 80);
restored.restore_undo_history(snapshot).unwrap();
```

### State queries + change notifications

```rust
use editor_core::{Command, EditCommand, EditorStateManager, StateChangeType};

let mut manager = EditorStateManager::new("Initial text", 80);
manager.subscribe(|change| {
    println!("change={:?} version {}->{}", change.change_type, change.old_version, change.new_version);
});

manager.execute(Command::Edit(EditCommand::Insert {
    offset: 0,
    text: "New: ".to_string(),
})).unwrap();
assert!(manager.get_document_state().is_modified);

// Manual edits are possible, but callers must preserve invariants and call `mark_modified`.
manager.editor_mut().piece_table.insert(0, "X");
manager.mark_modified(StateChangeType::DocumentModified);
```

### Multi-view workspace (split panes)

```rust
use editor_core::{Command, CursorCommand, EditCommand, Workspace};

let mut ws = Workspace::new();
let opened = ws.open_buffer(Some("file:///demo.txt".to_string()), "0123456789\n", 10).unwrap();

let left = opened.view_id;
let right = ws.create_view(opened.buffer_id, 5).unwrap();

ws.execute(left, Command::Cursor(CursorCommand::MoveTo { line: 0, column: 1 })).unwrap();
ws.execute(right, Command::Cursor(CursorCommand::MoveTo { line: 0, column: 5 })).unwrap();

ws.execute(left, Command::Edit(EditCommand::InsertText { text: "X".to_string() })).unwrap();
assert_eq!(ws.buffer_text(opened.buffer_id).unwrap(), "0X123456789\n");
```

### Decoration-aware composed snapshots (virtual text)

If you apply decorations that include `Decoration.text` (e.g. inlay hints or code lens), you can
render them via `ComposedGrid`:

```rust
use editor_core::{
    Decoration, DecorationKind, DecorationLayerId, DecorationPlacement, DecorationRange,
    EditorStateManager, ProcessingEdit,
};

let mut manager = EditorStateManager::new("a = 1\n", 80);
let anchor = manager.editor().line_index.position_to_char_offset(0, 1);

manager.apply_processing_edits(vec![ProcessingEdit::ReplaceDecorations {
    layer: DecorationLayerId::INLAY_HINTS,
    decorations: vec![Decoration {
        range: DecorationRange::new(anchor, anchor),
        placement: DecorationPlacement::After,
        kind: DecorationKind::InlayHint,
        text: Some(": i32".to_string()),
        styles: vec![],
        tooltip: None,
        data_json: None,
    }],
}]);

let composed = manager.get_viewport_content_composed(0, 10);
assert!(composed.actual_line_count() > 0);
```

## Performance & benches

`editor-core` aims to keep the common editor hot paths **incremental**:

- Text edits update `LineIndex` and `LayoutEngine` incrementally (instead of rebuilding from a full
  `get_text()` copy on every keystroke).
- Viewport rendering streams visible lines from `LineIndex` + `LayoutEngine` (no full-document
  intermediate strings in the viewport path).

Run the P1.5 benchmark suite:

```bash
cargo bench -p editor-core --bench performance
```

For a quick local sanity run (smaller sample sizes):

```bash
cargo bench -p editor-core --bench performance -- --sample-size 10 --warm-up-time 0.1 --measurement-time 0.1
```

There is also a small runtime example (prints timings for observation):

```bash
cargo run -p editor-core --example performance_milestones
```

## Related crates

- `editor-core-lsp`: LSP integration (UTF-16 conversions, semantic tokens helpers, stdio JSON-RPC).
- `editor-core-sublime`: `.sublime-syntax` highlighting + folding engine.
- `editor-core-treesitter`: Tree-sitter integration (incremental parsing → highlighting + folding).
