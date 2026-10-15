//! Bounded, canonical context for assistive technology; never a second document.
use crate::native_model::NativeModel;

pub(crate) struct Context {
    pub line: usize,
    pub column: usize,
    pub excerpt: String,
    pub selection: Option<(usize, usize, usize, usize)>,
}

pub(crate) fn context(model: &NativeModel) -> Context {
    let editor = &model.engine;
    let cursor = editor.get_cursor_state();
    let index = editor.editor().line_index();
    let line_start = index.position_to_char_offset(cursor.position.line, 0);
    let start = line_start.max(cursor.offset.saturating_sub(80));
    // text_range is bounded even for spec 018's three-million-character line.
    let mut excerpt = editor.editor().text_range(start, 160);
    if let Some(end) = excerpt.find('\n') {
        excerpt.truncate(end);
    }
    if start > line_start {
        excerpt.insert(0, '…');
    }
    Context {
        line: cursor.position.line + 1,
        column: cursor.position.column + 1,
        excerpt,
        selection: cursor
            .selection
            .as_ref()
            .filter(|s| s.start != s.end)
            .map(|s| {
                (
                    s.start.line + 1,
                    s.start.column + 1,
                    s.end.line + 1,
                    s.end.column + 1,
                )
            }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use editor_core::{Command, CursorCommand, Position};
    use moonkale_ext_api::editor::EditorSnapshot;

    #[test]
    fn canonical_context_is_bounded_and_reports_unicode_selection() {
        let mut model = NativeModel::new(
            &EditorSnapshot {
                node: moonkale_core::NodeId::derive(
                    &moonkale_core::SourceId::new("test:a11y"),
                    "a",
                ),
                revision: Default::default(),
                text: format!("😀中\r\n{}", "x".repeat(3_000_000)),
                selection: None,
            },
            None,
        );
        model
            .engine
            .execute(Command::Cursor(CursorCommand::SetSelection {
                start: Position::new(0, 0),
                end: Position::new(0, 2),
            }))
            .unwrap();
        let value = context(&model);
        assert_eq!(value.excerpt, "😀中");
        assert_eq!(value.selection, Some((1, 1, 1, 3)));
        model
            .engine
            .execute(Command::Cursor(CursorCommand::ClearSelection))
            .unwrap();
        model
            .engine
            .execute(Command::Cursor(CursorCommand::MoveTo {
                line: 1,
                column: 2_000_000,
            }))
            .unwrap();
        let value = context(&model);
        assert_eq!((value.line, value.column), (2, 2_000_001));
        assert!(value.excerpt.chars().count() <= 161);
        assert!(value.excerpt.starts_with('…'));
    }
}
