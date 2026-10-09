//! Bounded synthetic presentation provider and explicit scalar source mapping.
use crate::native_decorations::{Batch, Decoration, InlineStyle, Kind, Provider};
use moonkale_ext_api::editor::DocumentRevision;
use serde::Serialize;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Mode {
    Fixture,
    Markdown,
}

pub(crate) fn provider(mode: Mode, revision: DocumentRevision, source: &str) -> Batch {
    match mode {
        Mode::Fixture => fixture(revision, source),
        Mode::Markdown => crate::native_markdown::batch(revision, source),
    }
}

#[derive(Clone, PartialEq, Serialize)]
pub(crate) struct Replacement {
    pub start: usize,
    pub end: usize,
    pub widget: Option<String>,
    #[serde(skip)]
    pub style: Option<InlineStyle>,
}

pub(crate) fn valid_replacements(text: &str, values: Vec<Replacement>) -> Vec<Replacement> {
    let mut boundaries = vec![0];
    let mut offset = 0;
    for grapheme in text.graphemes(true) {
        offset += grapheme.chars().count();
        boundaries.push(offset);
    }
    values
        .into_iter()
        .filter(|value| {
            value.start < value.end
                && boundaries.contains(&value.start)
                && boundaries.contains(&value.end)
        })
        .collect()
}

/// Move view boundaries past replacements without changing engine layout or history.
/// Source cells come from the complete bounded source prefix, so this also works
/// when a virtual window begins in a continuation or ends before the token ends.
pub(crate) fn map_runs(
    lines: &mut [editor_core::HeadlessLine],
    source: &[editor_core::Cell],
    replacements: &[Replacement],
) {
    let boundary = |offset: usize| {
        replacements
            .iter()
            .find(|r| r.start < offset && offset < r.end)
            .map_or(offset, |r| r.end)
    };
    for line in lines {
        if line.is_fold_placeholder_appended {
            continue;
        }
        let start = boundary(line.char_offset_start);
        let end = boundary(line.char_offset_end);
        if let Some(cells) = source.get(start..end) {
            line.char_offset_start = start;
            line.char_offset_end = end;
            line.cells = cells.to_vec();
        }
    }
}

/// Each bit identifies a replacement still in preview. Inclusive caret
/// boundaries reveal a clicked widget before the next input reaches the engine.
pub(crate) fn preview_mask(
    values: &[Replacement],
    caret: usize,
    selection: Option<std::ops::Range<usize>>,
) -> u64 {
    values
        .iter()
        .take(63)
        .enumerate()
        .fold(0, |mask, (index, value)| {
            let active = value.start <= caret && caret <= value.end
                || selection
                    .as_ref()
                    .is_some_and(|range| range.start < value.end && value.start < range.end);
            if active {
                mask
            } else {
                mask | (1 << index)
            }
        })
}

/// Complete source lines within the bounded prefix. Never cut a token or
/// claim the rest of a logical line outside the measurement budget.
pub(crate) fn source_length(editor: &editor_core::EditorStateManager) -> usize {
    let index = editor.editor().line_index();
    let length = index.char_count();
    if length <= 4096 {
        return length;
    }
    let (line, _) = index.char_offset_to_position(4096);
    index.position_to_char_offset(line, 0)
}

pub(crate) fn last_line(
    mode: Mode,
    editor: &editor_core::EditorStateManager,
    revision: DocumentRevision,
) -> usize {
    replacements(mode, editor, revision)
        .iter()
        .map(|value| {
            editor
                .editor()
                .line_index()
                .char_offset_to_position(value.end)
                .0
        })
        .max()
        .unwrap_or(0)
}

pub(crate) fn replacements(
    mode: Mode,
    editor: &editor_core::EditorStateManager,
    revision: DocumentRevision,
) -> Vec<Replacement> {
    use crate::native_decorations::{Decorations, Window};
    let length = source_length(editor);
    let source = editor.editor().text_range(0, length);
    let values = Decorations::compose(
        revision,
        Window {
            start: 0,
            end: length,
            first_line: 0,
            last_line: editor
                .editor()
                .line_index()
                .char_offset_to_position(length)
                .0,
        },
        vec![provider(mode, revision, &source)],
    );
    valid_replacements(&source, values.replacements(0..length))
}

pub(crate) fn state(
    mode: Mode,
    editor: &editor_core::EditorStateManager,
    revision: DocumentRevision,
) -> u64 {
    let cursor = editor.get_cursor_state();
    let index = editor.editor().line_index();
    let selection = cursor.selection.map(|selection| {
        let a = index.position_to_char_offset(selection.start.line, selection.start.column);
        let b = index.position_to_char_offset(selection.end.line, selection.end.column);
        a.min(b)..a.max(b)
    });
    let mask = preview_mask(
        &replacements(mode, editor, revision),
        cursor.offset,
        selection,
    );
    if mode == Mode::Markdown {
        mask | (1 << 63)
    } else {
        mask
    }
}

pub(crate) fn in_run(
    values: &[Replacement],
    mask: u64,
    run: std::ops::Range<usize>,
) -> Vec<Replacement> {
    values
        .iter()
        .enumerate()
        .filter(|(index, value)| {
            mask & (1 << index) != 0 && run.start <= value.start && value.end <= run.end
        })
        .map(|(_, value)| Replacement {
            start: value.start - run.start,
            end: value.end - run.start,
            widget: value.widget.clone(),
            style: value.style,
        })
        .collect()
}

pub(crate) struct MappedRuns {
    pub lines: Vec<editor_core::HeadlessLine>,
    pub collapsed: Vec<usize>,
}

/// Complete bounded mapping shared by rendering and the height index.
pub(crate) fn mapped_runs(
    mode: Mode,
    editor: &editor_core::EditorStateManager,
    revision: DocumentRevision,
    mask: u64,
) -> Option<MappedRuns> {
    let index = editor.editor().line_index();
    let last = last_line(mode, editor, revision);
    let end = index.position_to_char_offset(last, index.get_line(last)?.char_count);
    if end > source_length(editor) {
        return None;
    }
    let values = replacements(mode, editor, revision);
    let replacements = in_run(&values, mask, 0..end);
    let count = editor
        .logical_position_to_visual(last, index.get_line(last)?.char_count)?
        .0
        + 1;
    let mut lines = editor.get_viewport_content_styled(0, count).lines;
    if lines.iter().any(|line| line.is_fold_placeholder_appended) {
        return None;
    }
    // Newlines are engine source scalars even though ordinary visual runs omit
    // them. An owner spanning logical lines must carry these canonical offsets.
    let cells = editor
        .editor()
        .text_range(0, end)
        .chars()
        .map(|ch| editor_core::Cell::new(ch, 1))
        .collect::<Vec<_>>();
    let nonempty = lines
        .iter()
        .map(|line| {
            !line.cells.is_empty()
                || replacements.iter().any(|value| {
                    value.start < line.char_offset_start && line.char_offset_start < value.end
                })
        })
        .collect::<Vec<_>>();
    map_runs(&mut lines, &cells, &replacements);
    let collapsed = lines
        .iter()
        .enumerate()
        .filter_map(|(row, line)| (nonempty[row] && line.cells.is_empty()).then_some(row))
        .collect();
    Some(MappedRuns { lines, collapsed })
}

pub(crate) fn fixture(revision: DocumentRevision, source: &str) -> Batch {
    let mut values = Vec::new();
    for (id, token, widget) in [
        (0, "[[hidden]]", None),
        (1, "[[chip]]", Some("Chip".to_string())),
    ] {
        let prefix = token.trim_end_matches("]]");
        let found = source
            .find(token)
            .map(|byte| (byte, token.len()))
            .or_else(|| {
                let byte = source.find(&format!("{prefix}:"))?;
                let end = source[byte..].find("]]")? + 2;
                Some((byte, end))
            });
        if let Some((byte, bytes)) = found {
            let start = source[..byte].chars().count();
            values.push(Decoration {
                provider: Provider::Fixture,
                id,
                kind: Kind::Replace {
                    range: start..start + source[byte..byte + bytes].chars().count(),
                    widget,
                    style: None,
                },
            });
        }
    }
    values.push(Decoration {
        provider: Provider::Fixture,
        id: 2,
        kind: Kind::BlockWidget {
            anchor: 0,
            widget: "Source-anchored block widget".to_string(),
        },
    });
    Batch { revision, values }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn presentation_priority_revision_and_run_boundaries_are_explicit() {
        use crate::native_decorations::{Decorations, Window};
        let source = "Wi [[hidden]] [[chip]] tail";
        let revision = DocumentRevision(2);
        let mut batch = fixture(revision, source);
        batch.values.push(Decoration {
            provider: Provider::Fixture,
            id: 99,
            kind: Kind::Replace {
                style: None,
                range: 5..18,
                widget: Some("overlap".into()),
            },
        });
        let result = Decorations::compose(
            revision,
            Window {
                start: 0,
                end: source.chars().count(),
                first_line: 0,
                last_line: 0,
            },
            vec![batch, fixture(DocumentRevision(1), source)],
        );
        let values = result.replacements(0..source.chars().count());
        assert_eq!(
            values
                .iter()
                .map(|value| (value.start, value.end))
                .collect::<Vec<_>>(),
            vec![(3, 13), (14, 22)]
        );
        assert!(
            result.replacements(4..12).is_empty(),
            "a cross-run replacement must fall back to source, not be clipped"
        );
        assert_eq!(result.block(0), Some("Source-anchored block widget"));
        assert_eq!(result.block(1), None);
    }
    #[test]
    fn cross_run_mapping_preserves_source_and_has_one_owner_in_virtual_slices() {
        use editor_core::{Cell, HeadlessLine};
        let source: Vec<Cell> = "ab[[chip]]xy".chars().map(|ch| Cell::new(ch, 1)).collect();
        let replacement = Replacement {
            style: None,
            start: 2,
            end: 10,
            widget: Some("Chip".into()),
        };
        let make = |start, end| {
            let mut line = HeadlessLine::new(0, start != 0);
            line.char_offset_start = start;
            line.char_offset_end = end;
            line.cells = source[start..end].to_vec();
            line
        };
        let mut lines = vec![make(0, 4), make(4, 8), make(8, 12)];
        map_runs(&mut lines, &source, std::slice::from_ref(&replacement));
        assert_eq!(
            lines
                .iter()
                .map(|l| (l.char_offset_start, l.char_offset_end))
                .collect::<Vec<_>>(),
            vec![(0, 10), (10, 10), (10, 12)]
        );
        assert_eq!(
            lines
                .iter()
                .flat_map(|l| l.cells.iter().map(|c| c.ch))
                .collect::<String>(),
            "ab[[chip]]xy"
        );
        assert_eq!(
            lines
                .iter()
                .filter(|l| l.char_offset_start <= replacement.start
                    && l.char_offset_end >= replacement.end)
                .count(),
            1
        );
        let mut window = vec![make(4, 8), make(8, 12)];
        map_runs(&mut window, &source, &[replacement]);
        assert_eq!(window[0].cells.len(), 0);
        assert_eq!(
            window[1].cells.iter().map(|c| c.ch).collect::<String>(),
            "xy"
        );
    }

    #[test]
    fn complete_mapping_identifies_consumed_rows_but_keeps_empty_source_lines() {
        let source = format!("ab[[hidden:{}]] tail\nnext\n", "x".repeat(60));
        let editor = editor_core::EditorStateManager::new(&source, 10);
        let mapped = mapped_runs(Mode::Fixture, &editor, DocumentRevision(0), 3).unwrap();
        assert!(mapped.collapsed.len() >= 3);
        for row in &mapped.collapsed {
            assert!(mapped.lines[*row].cells.is_empty());
        }
        let empty = editor_core::EditorStateManager::new("\nnext", 10);
        assert!(mapped_runs(Mode::Fixture, &empty, DocumentRevision(0), 3)
            .unwrap()
            .collapsed
            .is_empty());
    }

    #[test]
    fn reveal_is_local_to_caret_boundaries_and_overlapping_selection() {
        let values = vec![
            Replacement {
                style: None,
                start: 3,
                end: 13,
                widget: None,
            },
            Replacement {
                style: None,
                start: 14,
                end: 22,
                widget: Some("Chip".into()),
            },
        ];
        assert_eq!(preview_mask(&values, 0, None), 3);
        for caret in [3, 8, 13] {
            assert_eq!(preview_mask(&values, caret, None), 2);
        }
        for caret in [14, 18, 22] {
            assert_eq!(preview_mask(&values, caret, None), 1);
        }
        assert_eq!(preview_mask(&values, 30, Some(4..8)), 2);
        assert_eq!(preview_mask(&values, 30, Some(13..14)), 3);
        assert_eq!(preview_mask(&values, 30, Some(0..30)), 0);
    }

    #[test]
    fn multiline_owner_includes_newlines_and_collapses_empty_continuations() {
        let source = "Wi [[hidden:a\n\nb]] gap\n[[chip:c\nd]] tail\nnext\n";
        let editor = editor_core::EditorStateManager::new(source, 12);
        let revision = DocumentRevision(0);
        let values = replacements(Mode::Fixture, &editor, revision);
        assert_eq!(values.len(), 2);
        assert_eq!(last_line(Mode::Fixture, &editor, revision), 4);
        let mapped = mapped_runs(Mode::Fixture, &editor, revision, 3).unwrap();
        assert!(!mapped.collapsed.is_empty());
        for replacement in &values {
            let owners = mapped
                .lines
                .iter()
                .filter(|line| {
                    line.char_offset_start <= replacement.start
                        && replacement.end <= line.char_offset_end
                })
                .collect::<Vec<_>>();
            assert_eq!(owners.len(), 1);
            let owner = owners[0];
            let text = owner.cells.iter().map(|cell| cell.ch).collect::<String>();
            assert_eq!(
                text,
                editor
                    .editor()
                    .text_range(owner.char_offset_start, owner.cells.len())
            );
            assert!(text.contains('\n'));
        }
        let hidden_only = mapped_runs(Mode::Fixture, &editor, revision, 1).unwrap();
        assert!(hidden_only
            .lines
            .iter()
            .filter(|line| line.logical_line_index >= 3)
            .all(|line| line.cells.iter().all(|cell| cell.ch != '\n')));
        assert!(
            mapped
                .collapsed
                .iter()
                .any(|row| mapped.lines[*row].logical_line_index == 1),
            "blank source lines inside replacements collapse"
        );
    }

    #[test]
    fn folded_multiline_ranges_keep_engine_source_ownership() {
        let mut editor = editor_core::EditorStateManager::new("Wi [[chip:a\nb]] tail\nnext", 20);
        editor
            .execute(editor_core::Command::Style(
                editor_core::StyleCommand::Fold {
                    start_line: 0,
                    end_line: 1,
                },
            ))
            .unwrap();
        assert!(mapped_runs(Mode::Fixture, &editor, DocumentRevision(0), 1).is_none());
        editor
            .execute(editor_core::Command::Style(
                editor_core::StyleCommand::UnfoldAll,
            ))
            .unwrap();
        assert!(mapped_runs(Mode::Fixture, &editor, DocumentRevision(0), 1).is_some());
    }

    #[test]
    fn bounded_prefix_excludes_partial_tokens() {
        let source = format!("before\n[[chip:{}]] tail\n", "x".repeat(4100));
        let editor = editor_core::EditorStateManager::new(&source, 80);
        assert_eq!(source_length(&editor), 7);
        assert!(replacements(Mode::Fixture, &editor, DocumentRevision(0)).is_empty());
    }

    #[test]
    fn markdown_ownership_and_provider_identity_survive_engine_wraps() {
        let editor = editor_core::EditorStateManager::new(
            "before\n*猫 &amp; dog* **bold** `code`\nafter",
            7,
        );
        let revision = DocumentRevision(0);
        let mask = state(Mode::Markdown, &editor, revision);
        assert_eq!(mask, (1 << 63) | 7);
        assert_ne!(mask, state(Mode::Fixture, &editor, revision));
        let mapped = mapped_runs(Mode::Markdown, &editor, revision, mask).unwrap();
        let values = replacements(Mode::Markdown, &editor, revision);
        assert_eq!(values.len(), 3);
        for value in values {
            assert_eq!(
                mapped
                    .lines
                    .iter()
                    .filter(|line| line.char_offset_start <= value.start
                        && value.end <= line.char_offset_end)
                    .count(),
                1
            );
            assert!(value.style.is_some());
        }
        let long = format!("before\n*{}*\n", "x".repeat(4100));
        let editor = editor_core::EditorStateManager::new(&long, 80);
        assert!(replacements(Mode::Markdown, &editor, revision).is_empty());
        assert_eq!(state(Mode::Markdown, &editor, revision), 1 << 63);
    }

    #[test]
    fn replacements_reject_ranges_inside_extended_graphemes() {
        let text = "a👩🏽‍💻é";
        let make = |start, end| Replacement {
            style: None,
            start,
            end,
            widget: None,
        };
        let values = valid_replacements(
            text,
            vec![make(1, 5), make(1, 2), make(5, 6), make(5, 7), make(0, 99)],
        );
        assert_eq!(
            values
                .iter()
                .map(|value| (value.start, value.end))
                .collect::<Vec<_>>(),
            vec![(1, 5), (5, 7)]
        );
    }
}
