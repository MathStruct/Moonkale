//! Small Dioxus host for the editor-core M0 spike.
//!
//! Unlike the M1 textarea prototype, this component owns text, cursor and
//! selection in `EditorStateManager`. Dioxus events dispatch Rust commands;
//! the DOM is only a rendering of the visible snapshot rows.

use dioxus::html::input_data::MouseButton;
use dioxus::prelude::*;
use editor_core::{
    char_width, Command, CursorCommand, EditCommand, EditorStateManager, Position, Selection,
};
use std::{cell::RefCell, rc::Rc};

const ROWS_PER_VIEW: usize = 40;
const VISIBLE_ROWS: usize = 27;

/// Development-only prototype proving a Rust-owned Dioxus editing loop.
#[component]
pub fn EditorCoreSpike() -> Element {
    let mut model = use_signal(|| {
        let tail = (0..200)
            .map(|row| format!("// virtual viewport row {row:03}"))
            .collect::<Vec<_>>()
            .join("\n");
        EditorStateManager::new(
            &format!("fn main() {{\n    println!(\"hello Moonkale\");\n}}\n\nSelection and edits live in Rust.\n😀 Unicode stays in the model.\n{tail}"),
            100,
        )
    });
    let mut first_row = use_signal(|| 0usize);
    let mut dragging = use_signal(|| false);
    let mut hovered_position = use_signal(|| None::<(usize, usize)>);
    let mut composing = use_signal(|| false);
    let mut composition_preview = use_signal(String::new);
    let mut last_composition_commit = use_signal(String::new);
    let mut input_value = use_signal(String::new);
    let input_mounted = use_hook(|| Rc::new(RefCell::new(None::<Rc<MountedData>>)));
    let viewport_mounted = use_hook(|| Rc::new(RefCell::new(None::<Rc<MountedData>>)));
    let mount_on_mount_input = input_mounted.clone();
    let viewport_on_key = viewport_mounted.clone();
    let viewport_on_mount = viewport_mounted.clone();
    let focus_input_on_down = input_mounted.clone();

    let (grid, cursor, text) = model.with(|editor| {
        (
            editor.get_viewport_content(first_row(), ROWS_PER_VIEW),
            editor.get_cursor_state(),
            editor.editor().get_text().to_string(),
        )
    });
    let visible_rows = grid.lines.len();
    let total_rows = model.with(|editor| editor.get_viewport_state().total_visual_lines);
    let selection = cursor.selection.clone();
    let selected_text = selection
        .as_ref()
        .map(|range| selection_text(&text, range))
        .unwrap_or_default();
    let hovered_position_text = hovered_position()
        .map(|(line, column)| format!("{line}:{column}"))
        .unwrap_or_default();
    let copy_selected_text = selected_text.clone();
    let cut_selected_text = selected_text.clone();
    #[cfg(feature = "native-desktop")]
    let keyboard_selected_text = selected_text.clone();
    let preedit = composition_preview();

    rsx! {
        div {
            class: "mk-editor-core-spike",
            "data-editor": "editor-core-spike",
            "data-cursor-offset": "{cursor.offset}",
            "data-selection": "{selected_text}",
            "data-composing": "{composing}",
            "data-preedit": "{preedit}",
            "data-first-row": "{first_row}",
            "data-hover-position": "{hovered_position_text}",
            tabindex: 0,
            onmousedown: move |_| {
                if let Some(element) = focus_input_on_down.borrow().as_ref().cloned() {
                    spawn(async move { let _ = element.set_focus(true).await; });
                }
            },
            oncopy: move |event| {
                if copy_selected_text.is_empty() || !write_clipboard_event(&event, &copy_selected_text) {
                    return;
                }
                event.prevent_default();
            },
            oncut: move |event| {
                if cut_selected_text.is_empty() || !write_clipboard_event(&event, &cut_selected_text) {
                    return;
                }
                event.prevent_default();
                let _ = model.with_mut(|editor| editor.execute(Command::Edit(EditCommand::Backspace)));
            },
            onkeydown: move |event| {
                if composing() { return; }
                let key = event.key();
                let modifiers = event.modifiers();
                let primary = modifiers.contains(Modifiers::CONTROL) || modifiers.contains(Modifiers::META);
                let shift = modifiers.contains(Modifiers::SHIFT);
                let handled = model.with_mut(|editor| {
                    #[cfg(feature = "native-desktop")]
                    if primary {
                        match key {
                            Key::Character(ref value) if value.eq_ignore_ascii_case("c") => {
                                if !keyboard_selected_text.is_empty()
                                    && with_native_clipboard(|clipboard| clipboard.set_text(keyboard_selected_text.clone()).ok()).is_some()
                                {
                                    return true;
                                }
                            }
                            Key::Character(ref value) if value.eq_ignore_ascii_case("x") => {
                                if !keyboard_selected_text.is_empty()
                                    && with_native_clipboard(|clipboard| clipboard.set_text(keyboard_selected_text.clone()).ok()).is_some()
                                {
                                    let _ = editor.execute(Command::Edit(EditCommand::Backspace));
                                    return true;
                                }
                            }
                            Key::Character(ref value) if value.eq_ignore_ascii_case("v") => {
                                if let Some(text) = with_native_clipboard(|clipboard| clipboard.get_text().ok()) {
                                    if !text.is_empty() {
                                        let _ = editor.execute(Command::Edit(EditCommand::InsertText { text }));
                                        return true;
                                    }
                                }
                            }
                            _ => {}
                        }
                    }

                    if primary {
                        match key {
                            Key::Character(ref value) if value.eq_ignore_ascii_case("z") => {
                                let command = if shift { EditCommand::Redo } else { EditCommand::Undo };
                                let _ = editor.execute(Command::Edit(command));
                                return true;
                            }
                            Key::Character(ref value) if value.eq_ignore_ascii_case("y") => {
                                let _ = editor.execute(Command::Edit(EditCommand::Redo));
                                return true;
                            }
                            Key::Character(ref value) if value.eq_ignore_ascii_case("a") => {
                                let last_line = editor.editor().line_count().saturating_sub(1);
                                let end_column = editor.editor().char_count();
                                let _ = editor.execute(Command::Cursor(CursorCommand::SetSelection {
                                    start: Position::new(0, 0),
                                    end: Position::new(last_line, end_column),
                                }));
                                return true;
                            }
                            _ => {}
                        }
                    }

                    let movement = match key {
                        Key::ArrowLeft => Some(CursorCommand::MoveGraphemeLeft),
                        Key::ArrowRight => Some(CursorCommand::MoveGraphemeRight),
                        Key::ArrowUp => Some(CursorCommand::MoveBy { delta_line: -1, delta_column: 0 }),
                        Key::ArrowDown => Some(CursorCommand::MoveBy { delta_line: 1, delta_column: 0 }),
                        Key::Home => Some(CursorCommand::MoveToLineStart),
                        Key::End => Some(CursorCommand::MoveToLineEnd),
                        _ => None,
                    };
                    if let Some(command) = movement {
                        move_cursor(editor, command, shift);
                        return true;
                    }

                    let edit = match key {
                        Key::Backspace => Some(EditCommand::Backspace),
                        Key::Delete => Some(EditCommand::DeleteForward),
                        Key::Enter => Some(EditCommand::InsertNewline { auto_indent: false }),
                        Key::Tab => Some(EditCommand::InsertTab),
                        Key::Character(ref value) if !primary && !modifiers.contains(Modifiers::ALT) && !value.is_empty() => {
                            Some(EditCommand::InsertText { text: value.clone() })
                        }
                        _ => None,
                    };
                    if let Some(command) = edit {
                        let _ = editor.execute(Command::Edit(command));
                        return true;
                    }
                    false
                });
                if handled {
                    event.prevent_default();
                    let cursor_line = model.with(|editor| editor.get_cursor_state().position.line);
                    let current_start = first_row();
                    let next_start = if cursor_line < current_start {
                        cursor_line
                    } else if cursor_line >= current_start + VISIBLE_ROWS {
                        cursor_line + 1 - VISIBLE_ROWS
                    } else {
                        current_start
                    };
                    if next_start != current_start {
                        first_row.set(next_start);
                        if let Some(element) = viewport_on_key.borrow().as_ref().cloned() {
                            let top = next_start as f64 * 22.0;
                            spawn(async move {
                                let target = dioxus::html::geometry::PixelsVector2D::new(0.0, top);
                                let _ = element.scroll(target, ScrollBehavior::Instant).await;
                            });
                        }
                    }
                }
            },
            onmouseup: move |_| dragging.set(false),
            div { class: "mk-editor-core-toolbar",
                span { "Rows {first_row}–{first_row() + visible_rows} of {total_rows}" }
                span { "Cursor {cursor.position.line + 1}:{cursor.position.column + 1}" }
            }
            div {
                class: "mk-editor-core-viewport",
                role: "textbox",
                aria_label: "Editor core spike",
                aria_multiline: "true",
                style: "height: 600px; overflow: auto; background: #1a1b26; color: #c0caf5; font: 14px/22px monospace; white-space: pre; user-select: none;",
                onmounted: move |event: MountedEvent| {
                    *viewport_on_mount.borrow_mut() = Some(event.data.clone());
                },
                onmouseleave: move |_| hovered_position.set(None),
                onscroll: move |event| {
                    first_row.set((event.data().scroll_top().max(0.0) / 22.0).floor() as usize);
                },
                div {
                    style: "height: {total_rows * 22}px; position: relative;",
                    div {
                        style: "position: absolute; top: {first_row() * 22}px; left: 0; right: 0;",
                        for line in grid.lines {
                            div {
                                class: "mk-editor-core-row",
                                "data-line": "{line.logical_line_index}",
                                style: "height: 22px; line-height: 22px; position: relative;",
                                span { class: "mk-editor-core-gutter", "{line.logical_line_index + 1:>4} " }
                                for (column, cell) in line.cells.iter().enumerate() {
                                    if cursor.offset == line.char_offset_start + column {
                                        span {
                                            class: "mk-editor-core-caret",
                                            style: "display: inline-block; position: relative; width: 0; height: 22px; vertical-align: top; pointer-events: none;",
                                            span { class: "mk-editor-core-caret-bar", style: "position: absolute; top: 0; left: 0; width: 1px; height: 22px; background: currentColor;" }
                                        }
                                        if !preedit.is_empty() {
                                            span { class: "mk-editor-core-preedit", "{preedit}" }
                                        }
                                    }
                                    SpikeCell {
                                        model,
                                        dragging,
                                        hovered_position,
                                        line: line.logical_line_index,
                                        column,
                                        selected: selection.as_ref().is_some_and(|range| contains(range, Position::new(line.logical_line_index, column))),
                                        ch: cell.ch,
                                    }
                                }
                                if line.char_offset_end == cursor.offset {
                                    span {
                                        class: "mk-editor-core-caret",
                                        style: "display: inline-block; position: relative; width: 0; height: 22px; vertical-align: top; pointer-events: none;",
                                        span { class: "mk-editor-core-caret-bar", style: "position: absolute; top: 0; left: 0; width: 1px; height: 22px; background: currentColor;" }
                                    }
                                    if !preedit.is_empty() {
                                        span { class: "mk-editor-core-preedit", "{preedit}" }
                                    }
                                }
                                span {
                                    class: "mk-editor-core-row-trailing-space",
                                    style: "position: absolute; left: {(line.logical_line_index + 1).to_string().len().max(4) + 1 + line.cells.iter().map(|cell| char_width(cell.ch)).sum::<usize>()}ch; right: 0; top: 0; height: 22px;",
                                    onmousedown: move |_| {
                                        dragging.set(true);
                                        model.with_mut(|editor| {
                                            place_caret_at(
                                                editor,
                                                Position::new(
                                                    line.logical_line_index,
                                                    line.char_offset_end - line.char_offset_start,
                                                ),
                                            )
                                        });
                                    },
                                }
                            }
                        }
                    }
                }
            }
            textarea {
                class: "mk-editor-core-input-sink",
                aria_label: "Editor text input",
                tabindex: -1,
                value: "{input_value}",
                style: "position: fixed; left: -10000px; top: 0; width: 1px; height: 1px; opacity: 0;",
                onmounted: move |event: MountedEvent| {
                    *mount_on_mount_input.borrow_mut() = Some(event.data.clone());
                },
                oncompositionstart: move |_| {
                    composing.set(true);
                    composition_preview.set(String::new());
                },
                oncompositionupdate: move |event| composition_preview.set(event.data().data()),
                oncompositionend: move |event| {
                    let text = event.data().data();
                    if !text.is_empty() {
                        let _ = model.with_mut(|editor| editor.execute(Command::Edit(EditCommand::InsertText { text: text.clone() })));
                    }
                    last_composition_commit.set(text);
                    composition_preview.set(String::new());
                    composing.set(false);
                    input_value.set(String::new());
                },
                oninput: move |event| {
                    let text = event.value();
                    if text.is_empty() || composing() { return; }
                    if text == last_composition_commit() {
                        last_composition_commit.set(String::new());
                        input_value.set(String::new());
                        return;
                    }
                    let _ = model.with_mut(|editor| editor.execute(Command::Edit(EditCommand::InsertText { text })));
                    last_composition_commit.set(String::new());
                    input_value.set(String::new());
                },
            }
            p { class: "mk-editor-core-status", "Editor state, cursor and selection are Rust-owned. Keyboard and composition input use Dioxus events; the offscreen input sink is never queried for selection. No eval is used." }
        }
    }
}

#[cfg(feature = "native-desktop")]
pub(crate) fn with_native_clipboard<T>(
    operation: impl FnOnce(&mut arboard::Clipboard) -> Option<T>,
) -> Option<T> {
    thread_local! {
        static CLIPBOARD: RefCell<Option<arboard::Clipboard>> = const { RefCell::new(None) };
    }

    CLIPBOARD.with(|cell| {
        let mut clipboard = cell.borrow_mut();
        if clipboard.is_none() {
            *clipboard = arboard::Clipboard::new().ok();
        }
        clipboard.as_mut().and_then(operation)
    })
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn write_clipboard_event(event: &ClipboardEvent, text: &str) -> bool {
    use wasm_bindgen::JsCast;

    let data = event.data();
    let Some(native_event) = data.downcast::<web_sys::Event>() else {
        return false;
    };
    let Some(clipboard_event) = native_event.dyn_ref::<web_sys::ClipboardEvent>() else {
        return false;
    };
    clipboard_event
        .clipboard_data()
        .is_some_and(|clipboard| clipboard.set_data("text/plain", text).is_ok())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn write_clipboard_event(_event: &ClipboardEvent, _text: &str) -> bool {
    // Dioxus Desktop serializes clipboard events without native clipboard data.
    // The web spike proves the event contract; desktop clipboard support remains
    // an input-host integration gate rather than silently deleting selected text.
    false
}

#[component]
fn SpikeCell(
    model: Signal<EditorStateManager>,
    dragging: Signal<bool>,
    hovered_position: Signal<Option<(usize, usize)>>,
    line: usize,
    column: usize,
    selected: bool,
    ch: char,
) -> Element {
    rsx! {
        span {
            class: if selected { "mk-editor-core-cell mk-editor-core-selected" } else { "mk-editor-core-cell" },
            style: "display: inline-block; width: {char_width(ch)}ch;",
            background_color: if selected { "rgba(122, 162, 247, 0.38)" } else { "transparent" },
            onmousedown: move |event| {
                dragging.set(true);
                let target_column = hit_test_column(column, ch, event.element_coordinates().x);
                model.with_mut(|editor| place_caret_at(editor, Position::new(line, target_column)));
            },
            onmousemove: move |event| {
                let target_column = hit_test_column(column, ch, event.element_coordinates().x);
                if hovered_position() != Some((line, target_column)) {
                    hovered_position.set(Some((line, target_column)));
                }
                if dragging() && event.held_buttons().contains(MouseButton::Primary) {
                    let _ = model.with_mut(|editor| editor.execute(Command::Cursor(CursorCommand::ExtendSelection { to: Position::new(line, target_column) })));
                } else {
                    if dragging() {
                        dragging.set(false);
                    }
                }
            },
            "{ch}"
        }
    }
}

pub(crate) fn hit_test_column(column: usize, ch: char, x: f64) -> usize {
    let width = char_width(ch);
    // The spike uses 14px monospace text, whose CSS `ch` cell is about 8.4px.
    // Use 4px per cell as the midpoint threshold to absorb subpixel rounding.
    // Cell spans are explicitly sized in `ch` to match editor-core's width model.
    if width == 0 || x >= width as f64 * 4.0 {
        column + 1
    } else {
        column
    }
}

pub(crate) fn place_caret_at(editor: &mut EditorStateManager, position: Position) {
    let _ = editor.execute(Command::Cursor(CursorCommand::ClearSelection));
    let _ = editor.execute(Command::Cursor(CursorCommand::MoveTo {
        line: position.line,
        column: position.column,
    }));
}

fn selection_anchor(selection: &Selection) -> Position {
    selection.start
}

pub(crate) fn contains(selection: &Selection, position: Position) -> bool {
    let (start, end) = selection_bounds(selection);
    start <= position && position < end
}

fn selection_bounds(selection: &Selection) -> (Position, Position) {
    if selection.start <= selection.end {
        (selection.start, selection.end)
    } else {
        (selection.end, selection.start)
    }
}

pub(crate) fn move_cursor(editor: &mut EditorStateManager, command: CursorCommand, extend: bool) {
    // editor-core can retain an empty selection even though CursorState reports
    // it as None. MoveTo preserves that hidden selection, masking the new caret.
    if editor
        .editor()
        .selection()
        .is_some_and(|selection| selection.start == selection.end)
    {
        let _ = editor.execute(Command::Cursor(CursorCommand::ClearSelection));
    }
    let before = editor.get_cursor_state();
    let selection = before.selection;
    let anchor = selection
        .as_ref()
        .map(selection_anchor)
        .unwrap_or(before.position);

    if let Some(selection) = selection {
        let active = selection.end;
        if extend {
            // Keep the existing anchor in the editor while moving from its active end.
            // MoveTo intentionally preserves selections, so avoid clearing and rebuilding
            // the selection on each repeated Shift+Arrow key event.
            let _ = editor.execute(Command::Cursor(CursorCommand::MoveTo {
                line: active.line,
                column: active.column,
            }));
            let _ = editor.execute(Command::Cursor(command));
            let to = editor.editor().cursor_position();
            let selection_command = if anchor == to {
                CursorCommand::ClearSelection
            } else {
                CursorCommand::SetSelection {
                    start: anchor,
                    end: to,
                }
            };
            let _ = editor.execute(Command::Cursor(selection_command));
            return;
        }

        let collapse_to = match &command {
            CursorCommand::MoveGraphemeLeft => selection_bounds(&selection).0,
            CursorCommand::MoveGraphemeRight => selection_bounds(&selection).1,
            _ => active,
        };
        let _ = editor.execute(Command::Cursor(CursorCommand::ClearSelection));
        let _ = editor.execute(Command::Cursor(CursorCommand::MoveTo {
            line: collapse_to.line,
            column: collapse_to.column,
        }));

        // Plain left/right collapses to the corresponding edge without moving past it.
        if !extend
            && matches!(
                &command,
                CursorCommand::MoveGraphemeLeft | CursorCommand::MoveGraphemeRight
            )
        {
            return;
        }
    }

    let _ = editor.execute(Command::Cursor(command));
    if extend {
        let to = editor.get_cursor_state().position;
        let selection_command = if anchor == to {
            CursorCommand::ClearSelection
        } else {
            CursorCommand::SetSelection {
                start: anchor,
                end: to,
            }
        };
        let _ = editor.execute(Command::Cursor(selection_command));
    }
}

fn selection_text(text: &str, selection: &Selection) -> String {
    let (start_position, end_position) = if selection.start <= selection.end {
        (selection.start, selection.end)
    } else {
        (selection.end, selection.start)
    };
    let start = line_column_to_char_offset(text, start_position);
    let end = line_column_to_char_offset(text, end_position);
    text.chars()
        .skip(start)
        .take(end.saturating_sub(start))
        .collect()
}

fn line_column_to_char_offset(text: &str, position: Position) -> usize {
    text.split('\n')
        .take(position.line)
        .map(|line| line.chars().count() + 1)
        .sum::<usize>()
        + position.column
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mouse_hit_testing_uses_unicode_cell_width() {
        assert_eq!(hit_test_column(3, 'a', 3.0), 3);
        assert_eq!(hit_test_column(3, 'a', 5.0), 4);
        assert_eq!(hit_test_column(3, '界', 7.0), 3);
        assert_eq!(hit_test_column(3, '界', 10.0), 4);
        assert_eq!(hit_test_column(3, '\u{301}', 0.0), 4);
    }

    #[test]
    fn mouse_placement_clears_an_existing_selection() {
        let mut editor = EditorStateManager::new("abcdef", 80);
        editor
            .execute(Command::Cursor(CursorCommand::SetSelection {
                start: Position::new(0, 1),
                end: Position::new(0, 4),
            }))
            .unwrap();

        place_caret_at(&mut editor, Position::new(0, 5));
        let cursor = editor.get_cursor_state();
        assert_eq!(cursor.position, Position::new(0, 5));
        assert_eq!(cursor.selection, None);
    }

    #[test]
    fn backward_selection_highlights_and_arrow_keys_collapse_to_edges() {
        let mut editor = EditorStateManager::new("abcdef", 80);
        editor
            .execute(Command::Cursor(CursorCommand::SetSelection {
                start: Position::new(0, 4),
                end: Position::new(0, 1),
            }))
            .unwrap();
        let selection = editor.get_cursor_state().selection.unwrap();
        assert_eq!(selection_text("abcdef", &selection), "bcd");
        assert!(!contains(&selection, Position::new(0, 0)));
        assert!(contains(&selection, Position::new(0, 1)));
        assert!(contains(&selection, Position::new(0, 3)));
        assert!(!contains(&selection, Position::new(0, 4)));

        move_cursor(&mut editor, CursorCommand::MoveGraphemeRight, false);
        let cursor = editor.get_cursor_state();
        assert_eq!(cursor.position, Position::new(0, 4));
        assert_eq!(cursor.selection, None);

        editor
            .execute(Command::Cursor(CursorCommand::SetSelection {
                start: Position::new(0, 4),
                end: Position::new(0, 1),
            }))
            .unwrap();
        move_cursor(&mut editor, CursorCommand::MoveGraphemeLeft, false);
        assert_eq!(editor.get_cursor_state().position, Position::new(0, 1));

        editor
            .execute(Command::Cursor(CursorCommand::SetSelection {
                start: Position::new(0, 4),
                end: Position::new(0, 1),
            }))
            .unwrap();
        move_cursor(&mut editor, CursorCommand::MoveGraphemeLeft, true);
        let cursor = editor.get_cursor_state();
        let selection = cursor.selection.unwrap();
        assert_eq!(selection_text("abcdef", &selection), "abcd");
        assert_eq!(cursor.position, Position::new(0, 0));
    }

    #[test]
    fn repeated_shift_arrows_extend_and_contract_without_losing_the_anchor() {
        let source = "a".repeat(120);
        let mut editor = EditorStateManager::new(&source, 120);
        for _ in 0..100 {
            move_cursor(&mut editor, CursorCommand::MoveGraphemeRight, true);
        }
        let cursor = editor.get_cursor_state();
        assert_eq!(cursor.position, Position::new(0, 100));
        assert_eq!(
            selection_text(&source, cursor.selection.as_ref().unwrap()),
            "a".repeat(100)
        );

        for _ in 0..100 {
            move_cursor(&mut editor, CursorCommand::MoveGraphemeLeft, true);
        }
        let cursor = editor.get_cursor_state();
        assert_eq!(cursor.position, Position::new(0, 0));
        assert_eq!(cursor.selection, None);

        move_cursor(&mut editor, CursorCommand::MoveGraphemeRight, false);
        assert_eq!(editor.get_cursor_state().offset, 1);
    }

    #[test]
    fn rust_model_owns_unicode_selection_and_edit_undo() {
        let mut editor = EditorStateManager::new("a😀bc", 80);
        editor
            .execute(Command::Cursor(CursorCommand::SetSelection {
                start: Position::new(0, 1),
                end: Position::new(0, 2),
            }))
            .unwrap();

        let selection = editor.get_cursor_state().selection.unwrap();
        assert_eq!(selection_text("a😀bc", &selection), "😀");

        editor
            .execute(Command::Edit(EditCommand::InsertText { text: "λ".into() }))
            .unwrap();
        assert_eq!(editor.editor().get_text(), "aλbc");
        editor.execute(Command::Edit(EditCommand::Undo)).unwrap();
        assert_eq!(editor.editor().get_text(), "a😀bc");
        editor.execute(Command::Edit(EditCommand::Redo)).unwrap();
        assert_eq!(editor.editor().get_text(), "aλbc");
    }

    #[test]
    fn selected_text_is_direction_independent_and_spans_lines() {
        let text = "alpha\nβeta";
        let forward = Selection {
            start: Position::new(0, 1),
            end: Position::new(1, 2),
            direction: editor_core::SelectionDirection::Forward,
        };
        let backward = Selection {
            start: Position::new(1, 2),
            end: Position::new(0, 1),
            direction: editor_core::SelectionDirection::Backward,
        };

        assert_eq!(selection_text(text, &forward), "lpha\nβe");
        assert_eq!(selection_text(text, &backward), "lpha\nβe");
    }

    #[test]
    fn cursor_movement_uses_grapheme_boundaries() {
        let mut editor = EditorStateManager::new("a😀e\u{301}b", 80);
        for expected_offset in [1, 2, 4] {
            editor
                .execute(Command::Cursor(CursorCommand::MoveGraphemeRight))
                .unwrap();
            assert_eq!(editor.get_cursor_state().offset, expected_offset);
        }
        editor
            .execute(Command::Cursor(CursorCommand::MoveGraphemeLeft))
            .unwrap();
        assert_eq!(editor.get_cursor_state().offset, 2);
    }

    #[test]
    fn rendered_snapshot_is_limited_to_the_requested_viewport() {
        let source = (0..100)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let editor = EditorStateManager::new(&source, 80);
        let grid = editor.get_viewport_content(30, ROWS_PER_VIEW);

        assert_eq!(grid.start_visual_row, 30);
        assert_eq!(grid.actual_line_count(), ROWS_PER_VIEW);
        assert_eq!(grid.lines[0].logical_line_index, 30);
    }

    #[test]
    fn three_megabyte_document_returns_bounded_rows_and_local_edit_delta() {
        let line = "0123456789".repeat(10);
        let source = std::iter::repeat_n(line.as_str(), 30_000)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(source.len() >= 3_000_000);

        let mut editor = EditorStateManager::new(&source, 120);
        let last_line = editor.editor().line_count() - 1;
        let last_column = line.chars().count();
        editor
            .execute(Command::Cursor(CursorCommand::MoveTo {
                line: last_line,
                column: last_column,
            }))
            .unwrap();
        editor
            .execute(Command::Edit(EditCommand::InsertText { text: "x".into() }))
            .unwrap();

        let delta = editor.take_last_text_delta().unwrap();
        assert_eq!(delta.edits.len(), 1);
        assert_eq!(delta.edits[0].inserted_text, "x");
        assert_eq!(delta.edits[0].deleted_text, "");
        assert_eq!(delta.edits[0].start, delta.before_char_count);

        let grid = editor.get_viewport_content(last_line - 10, 40);
        assert_eq!(grid.actual_line_count(), 11);
        assert!(grid.actual_line_count() <= 40);
    }
}
