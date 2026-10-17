//! Source-owned presentation provider and explicit scalar source mapping.
use crate::native_decorations::{Batch, Decoration, InlineStyle, Kind, Provider};
use crate::native_layout::PresentationState;
use dioxus::prelude::ReadableExt;
use moonkale_ext_api::editor::DocumentRevision;
use serde::Serialize;

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Mode {
    Fixture,
    Markdown,
    Documentation(dioxus::prelude::Signal<crate::native_model::NativeModel>),
}

pub(crate) fn provider(mode: Mode, revision: DocumentRevision, source: &str) -> Batch {
    match mode {
        Mode::Fixture => fixture(revision, source),
        Mode::Markdown => crate::native_markdown::batch(revision, source),
        Mode::Documentation(model) => {
            let state = model.peek();
            crate::native_doc_comments::Language::from_slug(state.language.unwrap().slug())
                .map(|language| state.docs.borrow_mut().batch(language, revision, source))
                .unwrap_or(Batch {
                    revision,
                    values: Vec::new(),
                })
        }
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
    // Only replacement endpoints need validation. Scanning every grapheme and
    // allocating a boundary for every plain-source character dominates long
    // notes, even though nearly all of those positions are never queried.
    let mut endpoints: Vec<_> = values
        .iter()
        .flat_map(|value| [value.start, value.end])
        .collect();
    endpoints.sort_unstable();
    endpoints.dedup();
    let mut boundaries = Vec::with_capacity(endpoints.len());
    let mut remaining = text.chars();
    let mut previous = 0;
    for scalar in endpoints {
        // Chars::nth skips UTF-8 runs in chunks; avoid a debug-build branch and
        // endpoint comparison for every plain-source scalar in a long note.
        if scalar > previous && remaining.nth(scalar - previous - 1).is_none() {
            break;
        }
        let byte = text.len() - remaining.as_str().len();
        previous = scalar;
        let mut cursor = unicode_segmentation::GraphemeCursor::new(byte, text.len(), true);
        if cursor.is_boundary(text, 0).unwrap_or(false) {
            boundaries.push(scalar);
        }
    }
    values
        .into_iter()
        .filter(|value| {
            value.start < value.end
                && boundaries.binary_search(&value.start).is_ok()
                && boundaries.binary_search(&value.end).is_ok()
        })
        .collect()
}

/// Move view boundaries past replacements without changing engine layout or history.
/// Source cells come from canonical source ranges, so this also works
/// when a virtual window begins in a continuation or ends before the token ends.
#[cfg(test)]
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

/// Inclusive caret boundaries and overlapping selections reveal whole constructs.
pub(crate) fn preview_state(
    values: &[Replacement],
    caret: usize,
    selection: Option<std::ops::Range<usize>>,
    provider: u8,
) -> PresentationState {
    let mut revealed: Option<(usize, usize)> = None;
    for value in values {
        if value.start <= caret && caret <= value.end
            || selection
                .as_ref()
                .is_some_and(|range| range.start < value.end && value.start < range.end)
        {
            revealed = Some(revealed.map_or((value.start, value.end), |(a, b)| {
                (a.min(value.start), b.max(value.end))
            }));
        }
    }
    PresentationState { provider, revealed }
}

pub(crate) fn source_length(editor: &editor_core::EditorStateManager) -> usize {
    editor.editor().line_index().char_count()
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

type ReplacementCache = Vec<(Mode, String, std::rc::Rc<Vec<Replacement>>)>;
thread_local! {
    static REPLACEMENTS: std::cell::RefCell<ReplacementCache> = const { std::cell::RefCell::new(Vec::new()) };
}

pub(crate) fn replacements(
    mode: Mode,
    editor: &editor_core::EditorStateManager,
    revision: DocumentRevision,
) -> std::rc::Rc<Vec<Replacement>> {
    use crate::native_decorations::{Decorations, Window};
    if let Mode::Documentation(model) = mode {
        let state = model.peek();
        if let Some(values) = state.docs.borrow().cached(revision) {
            return values;
        }
        // Local deltas already updated the normalized syntax source. Only a
        // replacement or unsupported batch needs another engine snapshot.
        let source = state
            .docs
            .borrow()
            .updated_source(revision)
            .unwrap_or_else(|| editor.editor().get_text());
        return state.docs.borrow_mut().replacements(
            crate::native_doc_comments::Language::from_slug(state.language.unwrap().slug())
                .unwrap(),
            revision,
            &source,
        );
    }
    let length = source_length(editor);
    let source = editor.editor().text_range(0, length);
    if let Some(values) = REPLACEMENTS.with(|cache| {
        cache
            .borrow()
            .iter()
            .find(|(key, text, _)| *key == mode && *text == source)
            .map(|(_, _, values)| values.clone())
    }) {
        return values;
    }
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
    let mut values = valid_replacements(&source, values.replacements(0..length));
    values.sort_by_key(|value| value.start);
    let result = std::rc::Rc::new(values);
    REPLACEMENTS.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() == 4 {
            cache.remove(0);
        }
        cache.push((mode, source, result.clone()));
    });
    result
}

#[cfg(test)]
pub(crate) fn state(
    mode: Mode,
    editor: &editor_core::EditorStateManager,
    revision: DocumentRevision,
) -> PresentationState {
    state_for_values(
        &replacements(mode, editor, revision),
        editor,
        match mode {
            Mode::Markdown => 2,
            Mode::Documentation(_) => 3,
            Mode::Fixture => 1,
        },
    )
}

pub(crate) fn state_for_values(
    values: &[Replacement],
    editor: &editor_core::EditorStateManager,
    provider: u8,
) -> PresentationState {
    let cursor = editor.get_cursor_state();
    let index = editor.editor().line_index();
    let selection = cursor.selection.map(|selection| {
        let a = index.position_to_char_offset(selection.start.line, selection.start.column);
        let b = index.position_to_char_offset(selection.end.line, selection.end.column);
        a.min(b)..a.max(b)
    });
    preview_state(values, cursor.offset, selection, provider)
}

pub(crate) fn in_run(
    values: &[Replacement],
    mask: PresentationState,
    run: std::ops::Range<usize>,
) -> Vec<Replacement> {
    values
        .iter()
        .filter(|value| {
            mask.previews(value.start, value.end)
                && run.start <= value.start
                && value.end <= run.end
        })
        .map(|value| Replacement {
            start: value.start - run.start,
            end: value.end - run.start,
            widget: value.widget.clone(),
            style: value.style.clone(),
        })
        .collect()
}

pub(crate) struct MappedRuns {
    pub lines: Vec<editor_core::HeadlessLine>,
    #[cfg_attr(not(test), allow(dead_code))]
    pub collapsed: Vec<usize>,
}

pub(crate) fn mapped_boundary(
    values: &[Replacement],
    state: PresentationState,
    offset: usize,
) -> usize {
    let index = values.partition_point(|value| value.end <= offset);
    values
        .get(index)
        .filter(|value| {
            value.start < offset && offset < value.end && state.previews(value.start, value.end)
        })
        .map_or(offset, |value| value.end)
}

/// Only materialize source cells for visible rows (and caller's margin).
pub(crate) fn mapped_window(
    mode: Mode,
    editor: &editor_core::EditorStateManager,
    revision: DocumentRevision,
    state: PresentationState,
    first: usize,
    count: usize,
) -> Option<MappedRuns> {
    let values = replacements(mode, editor, revision);
    let mut lines = editor.get_viewport_content_styled(first, count).lines;
    if lines.iter().any(|line| line.is_fold_placeholder_appended) {
        return None;
    }
    let mut collapsed = Vec::new();
    for (row, line) in lines.iter_mut().enumerate() {
        let start = mapped_boundary(&values, state, line.char_offset_start);
        let end = mapped_boundary(&values, state, line.char_offset_end);
        let consumed = start == end && (line.char_offset_start != start || !line.cells.is_empty());
        if consumed {
            collapsed.push(first + row);
        }
        // Engine styled cells can contain synthetic continuation indentation.
        // Geometry offsets must map only canonical source scalars, as in the
        // complete mapper; copy just this visible run rather than the document.
        line.char_offset_start = start;
        line.char_offset_end = end;
        line.cells = editor
            .editor()
            .text_range(start, end - start)
            .chars()
            .map(|ch| editor_core::Cell::new(ch, 1))
            .collect();
    }
    Some(MappedRuns { lines, collapsed })
}

/// Code keeps its bounded horizontal windows; only documentation materializes an owner.
pub(crate) fn map_documentation_window(
    editor: &editor_core::EditorStateManager,
    lines: &mut [editor_core::HeadlessLine],
    values: &[Replacement],
    state: PresentationState,
) {
    let index = editor.editor().line_index();
    for line in lines {
        if line.is_fold_placeholder_appended {
            continue;
        }
        let logical_start = index.position_to_char_offset(line.logical_line_index, 0);
        let logical_end = if line.logical_line_index + 1 < index.line_count() {
            index
                .position_to_char_offset(line.logical_line_index + 1, 0)
                .saturating_sub(1)
        } else {
            index.char_count()
        };
        let candidate = values.get(values.partition_point(|value| value.end <= logical_start));
        let Some(owner) = candidate
            .filter(|value| value.start < logical_end && state.previews(value.start, value.end))
        else {
            continue;
        };
        let start = if logical_start == owner.start && !line.is_wrapped_part {
            owner.start
        } else {
            mapped_boundary(values, state, line.char_offset_start)
        };
        let end = mapped_boundary(
            values,
            state,
            line.char_offset_end.max(logical_end.min(owner.end)),
        );
        line.char_offset_start = start;
        line.char_offset_end = end;
        line.cells = editor
            .editor()
            .text_range(start, end.saturating_sub(start))
            .chars()
            .map(|ch| editor_core::Cell::new(ch, 1))
            .collect();
    }
}

/// Determine collapsed continuation rows without allocating source cell arrays.
pub(crate) fn collapsed_rows(
    editor: &editor_core::EditorStateManager,
    values: &[Replacement],
    state: PresentationState,
) -> Vec<usize> {
    let index = editor.editor().line_index();
    // Resolve the many presentation endpoints against one scalar line index.
    // Repeated rope line lookups otherwise rescan leaf text for every token
    // and every continuation row on each caret/measurement update.
    let text = index.get_text();
    let mut starts = vec![0];
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        offset += line.chars().count();
        if line.ends_with('\n') {
            starts.push(offset);
        }
    }
    let position = |offset: usize| {
        let line = starts
            .partition_point(|start| *start <= offset)
            .saturating_sub(1);
        (line, offset.saturating_sub(starts[line]))
    };
    let line_end = |line: usize| starts.get(line + 1).map_or(offset, |next| next - 1);
    let folds = editor.get_folding_state();
    // Expanded fold regions do not affect presentation. Filter once rather
    // than scanning every heading/list region for every replacement.
    let collapsed_folds: Vec<_> = folds
        .regions
        .iter()
        .filter(|fold| fold.is_collapsed)
        .collect();
    let mut rows = Vec::new();
    let unwrapped = collapsed_folds.is_empty()
        && editor.editor().layout_engine().wrap_mode() == editor_core::WrapMode::None;
    for value in values
        .iter()
        .filter(|value| state.previews(value.start, value.end))
    {
        let (start_line, start_col) = position(value.start);
        let (end_line, end_col) = position(value.end);
        if unwrapped {
            let first = start_line + usize::from(value.widget.is_some());
            for (row, &start) in starts.iter().enumerate().take(end_line + 1).skip(first) {
                let end = line_end(row);
                if (value.start < start || value.widget.is_none() && value.start == start)
                    && start < value.end
                    && end <= value.end
                {
                    rows.push(row);
                }
            }
            continue;
        }
        if collapsed_folds
            .iter()
            .any(|fold| fold.start_line <= end_line && start_line <= fold.end_line)
        {
            continue;
        }
        let Some(first) = editor.logical_position_to_visual_row(start_line, start_col) else {
            continue;
        };
        let Some(last) = editor.logical_position_to_visual_row(end_line, end_col) else {
            continue;
        };
        for row in (if value.widget.is_none() {
            first
        } else {
            first + 1
        })..=last
        {
            let Some(position) = editor.visual_position_to_logical(row, 0) else {
                continue;
            };
            let start = starts[position.line] + position.column;
            let end = editor
                .visual_position_to_logical(row + 1, 0)
                .filter(|next| next.line == position.line)
                .map(|next| starts[next.line] + next.column)
                .unwrap_or_else(|| line_end(position.line));
            if (value.start < start || (value.widget.is_none() && value.start == start))
                && start < value.end
                && end <= value.end
            {
                rows.push(row);
            }
        }
    }
    rows.sort_unstable();
    rows.dedup();
    rows
}

#[cfg(test)]
pub(crate) fn mapped_runs(
    mode: Mode,
    editor: &editor_core::EditorStateManager,
    revision: DocumentRevision,
    state: PresentationState,
) -> Option<MappedRuns> {
    mapped_window(
        mode,
        editor,
        revision,
        state,
        0,
        editor.get_viewport_state().total_visual_lines,
    )
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
    // Compact expectations for the two-token fixture; production reveal state
    // is an uncapped interval and does not use these bits.
    fn preview_mask(
        values: &[Replacement],
        caret: usize,
        selection: Option<std::ops::Range<usize>>,
    ) -> u64 {
        let state = preview_state(values, caret, selection, 1);
        values.iter().enumerate().fold(0, |mask, (i, v)| {
            if state.previews(v.start, v.end) {
                mask | (1 << i)
            } else {
                mask
            }
        })
    }
    fn all(provider: u8) -> PresentationState {
        PresentationState {
            provider,
            revealed: None,
        }
    }
    fn revealed() -> PresentationState {
        PresentationState {
            provider: 2,
            revealed: Some((0, usize::MAX)),
        }
    }

    #[test]
    fn row_only_mapping_matches_cell_mapping_for_wrapping_unicode_and_folds() {
        let source = "wide界😀 words words\n\n\t   nested words words words\nend";
        for width in [7, 20, 80] {
            let mut editor = editor_core::EditorStateManager::new(source, width);
            for folded in [false, true] {
                if folded {
                    editor
                        .execute(editor_core::Command::Style(
                            editor_core::StyleCommand::Fold {
                                start_line: 0,
                                end_line: 2,
                            },
                        ))
                        .unwrap();
                }
                // A zero-cell query must land exactly on the wrapped source
                // boundary, including indented and folded Unicode lines.
                for row in 0..editor.get_viewport_state().total_visual_lines {
                    let position = editor.visual_position_to_logical(row, 0).unwrap();
                    assert_eq!(
                        editor.logical_position_to_visual_row(position.line, position.column),
                        Some(row)
                    );
                }
                let index = editor.editor().line_index();
                for line in 0..index.line_count() {
                    for column in 0..=index.get_line(line).unwrap().char_count + 1 {
                        assert_eq!(
                            editor.logical_position_to_visual_row(line, column),
                            editor
                                .logical_position_to_visual(line, column)
                                .map(|(row, _)| row)
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn visible_mapping_does_not_invent_continuation_source_characters() {
        let source = format!("# Heading\n\n    {}\n\n# Later", "words ".repeat(100));
        let editor = editor_core::EditorStateManager::new(&source, 20);
        let mapped =
            mapped_window(Mode::Markdown, &editor, DocumentRevision(0), all(2), 2, 20).unwrap();
        for line in mapped.lines {
            assert_eq!(
                line.cells.iter().map(|cell| cell.ch).collect::<String>(),
                editor.editor().text_range(
                    line.char_offset_start,
                    line.char_offset_end - line.char_offset_start
                )
            );
        }
    }

    #[test]
    fn long_note_has_uncapped_reveal_and_only_materializes_visible_cells() {
        let source = (0..240)
            .map(|n| {
                format!(
                    "# Section {n} 猫\n\n{} **bold** and `code`.\n\n",
                    "plain words ".repeat(75)
                )
            })
            .collect::<String>();
        assert!(source.len() > 200_000);
        let editor = editor_core::EditorStateManager::new(&source, 80);
        let revision = DocumentRevision(0);
        let values = replacements(Mode::Markdown, &editor, revision);
        assert_eq!(values.len(), 720);
        let target = &values[700];
        let state = preview_state(&values, target.start, None, 2);
        assert!(!state.previews(target.start, target.end));
        assert!(state.previews(values[699].start, values[699].end));
        let index = editor.editor().line_index();
        let (line, col) = index.char_offset_to_position(target.start);
        let row = editor.logical_position_to_visual(line, col).unwrap().0;
        let mapped = mapped_window(Mode::Markdown, &editor, revision, all(2), row, 24).unwrap();
        assert!(mapped.lines.len() <= 24);
        assert!(
            mapped
                .lines
                .iter()
                .map(|line| line.cells.len())
                .sum::<usize>()
                < 10_000
        );
        assert_eq!(
            mapped
                .lines
                .iter()
                .filter(|line| line.char_offset_start <= target.start
                    && target.end <= line.char_offset_end)
                .count(),
            1
        );
        let preview = preview_state(
            &values,
            usize::MAX,
            Some(values[650].start..values[710].end),
            2,
        );
        assert!(!preview.previews(values[700].start, values[700].end));
        assert!(preview.previews(values[649].start, values[649].end));
    }

    #[test]
    fn sparse_collapsed_row_index_matches_complete_mapping() {
        let source = "before\n\nTitle **猫**\n===\n\n```rust\nfn main() {}\n\n```\n\nafter";
        for width in [7, 20, 80] {
            let editor = editor_core::EditorStateManager::new(source, width);
            let values = replacements(Mode::Markdown, &editor, DocumentRevision(0));
            let mapped = mapped_runs(Mode::Markdown, &editor, DocumentRevision(0), all(2)).unwrap();
            assert_eq!(collapsed_rows(&editor, &values, all(2)), mapped.collapsed);
        }
    }

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
        let mapped = mapped_runs(Mode::Fixture, &editor, DocumentRevision(0), all(1)).unwrap();
        assert!(mapped.collapsed.len() >= 3);
        for row in &mapped.collapsed {
            assert!(mapped.lines[*row].cells.is_empty());
        }
        let empty = editor_core::EditorStateManager::new("\nnext", 10);
        assert!(
            mapped_runs(Mode::Fixture, &empty, DocumentRevision(0), all(1))
                .unwrap()
                .collapsed
                .is_empty()
        );
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
    fn unwrapped_collapsed_rows_include_blank_and_complete_continuations() {
        let source = "before\n/// First\n///\n/// Last\nafter\n";
        let mut editor = editor_core::EditorStateManager::new(source, 12);
        editor
            .execute(editor_core::Command::View(
                editor_core::ViewCommand::SetWrapMode {
                    mode: editor_core::WrapMode::None,
                },
            ))
            .unwrap();
        let start = source.find("///").unwrap();
        let end = source.find("\nafter").unwrap();
        let values = vec![Replacement {
            start,
            end,
            widget: Some("FirstLast".into()),
            style: None,
        }];
        assert_eq!(collapsed_rows(&editor, &values, all(3)), vec![2, 3]);
        assert!(
            collapsed_rows(&editor, &values, preview_state(&values, start, None, 3)).is_empty()
        );
        editor.replace_folding_regions(
            vec![editor_core::FoldRegion {
                start_line: 1,
                end_line: 3,
                is_collapsed: true,
                placeholder: "[...]".into(),
            }],
            false,
        );
        assert!(collapsed_rows(&editor, &values, all(3)).is_empty());
    }

    #[test]
    fn documentation_mapping_preserves_bounded_code_windows() {
        let prefix = "/// First\n/// Second\n";
        let source = format!("{prefix}let value = {};\n", "x".repeat(100_000));
        let editor = editor_core::EditorStateManager::new(&source, 30);
        let mut lines = editor.get_viewport_content_window(0, 3, 0, 30).lines;
        let code = lines[2].cells.clone();
        let values = vec![Replacement {
            start: 0,
            end: prefix.chars().count() - 1,
            widget: Some("FirstSecond".into()),
            style: None,
        }];
        map_documentation_window(&editor, &mut lines, &values, all(3));
        assert_eq!(lines[0].cells.len(), prefix.chars().count() - 1);
        assert!(lines[1].cells.is_empty());
        assert_eq!(lines[2].cells, code);
        assert!(lines[2].cells.len() <= 31);
        assert_eq!(editor.editor().get_text(), source);
    }

    #[test]
    fn properties_hide_complete_rows_and_reveal_canonical_source() {
        let source = "---\ntitle: 猫\n---\nBody\n";
        let editor = editor_core::EditorStateManager::new(source, 12);
        let values = replacements(Mode::Markdown, &editor, DocumentRevision(0));
        assert_eq!(values.len(), 1);
        assert!(values[0].widget.is_none());
        assert_eq!(collapsed_rows(&editor, &values, all(2)), vec![0, 1, 2]);
        let revealed = preview_state(&values, 5, None, 2);
        assert!(collapsed_rows(&editor, &values, revealed).is_empty());
        let selected = preview_state(
            &values,
            source.chars().count(),
            Some(0..source.chars().count()),
            2,
        );
        assert!(collapsed_rows(&editor, &values, selected).is_empty());
        assert_eq!(editor.editor().get_text(), source);
    }

    #[test]
    fn multiline_owner_includes_newlines_and_collapses_empty_continuations() {
        let source = "Wi [[hidden:a\n\nb]] gap\n[[chip:c\nd]] tail\nnext\n";
        let editor = editor_core::EditorStateManager::new(source, 12);
        let revision = DocumentRevision(0);
        let values = replacements(Mode::Fixture, &editor, revision);
        assert_eq!(values.len(), 2);
        assert_eq!(last_line(Mode::Fixture, &editor, revision), 4);
        let mapped = mapped_runs(Mode::Fixture, &editor, revision, all(1)).unwrap();
        assert!(!mapped.collapsed.is_empty());
        for replacement in values.iter() {
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
        let hidden_only = mapped_runs(
            Mode::Fixture,
            &editor,
            revision,
            preview_state(&values, values[1].start, None, 1),
        )
        .unwrap();
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
        assert!(mapped_runs(Mode::Fixture, &editor, DocumentRevision(0), all(1)).is_none());
        editor
            .execute(editor_core::Command::Style(
                editor_core::StyleCommand::UnfoldAll,
            ))
            .unwrap();
        assert!(mapped_runs(Mode::Fixture, &editor, DocumentRevision(0), all(1)).is_some());
    }

    #[test]
    fn complete_source_keeps_tokens_beyond_the_old_prefix() {
        let source = format!("before\n[[chip:{}]] tail\n", "x".repeat(4100));
        let editor = editor_core::EditorStateManager::new(&source, 80);
        assert_eq!(source_length(&editor), source.chars().count());
        assert_eq!(
            replacements(Mode::Fixture, &editor, DocumentRevision(0)).len(),
            1
        );
    }

    #[test]
    fn markdown_ownership_and_provider_identity_survive_engine_wraps() {
        let editor = editor_core::EditorStateManager::new(
            "before\n*猫 &amp; dog* **bold** `code`\nafter",
            7,
        );
        let revision = DocumentRevision(0);
        let mask = state(Mode::Markdown, &editor, revision);
        assert_eq!(mask, all(2));
        assert_ne!(mask, state(Mode::Fixture, &editor, revision));
        let mapped = mapped_runs(Mode::Markdown, &editor, revision, mask).unwrap();
        let values = replacements(Mode::Markdown, &editor, revision);
        assert_eq!(values.len(), 3);
        for value in values.iter() {
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
        assert_eq!(replacements(Mode::Markdown, &editor, revision).len(), 1);
        assert_eq!(state(Mode::Markdown, &editor, revision), all(2));
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

    #[test]
    fn setext_underline_collapses_and_reveals_with_its_heading() {
        let source = "before\n\nTitle **猫**\n===\n\nafter";
        let editor = editor_core::EditorStateManager::new(source, 80);
        let revision = DocumentRevision(0);
        let values = replacements(Mode::Markdown, &editor, revision);
        assert_eq!(values.len(), 1);
        let mask = state(Mode::Markdown, &editor, revision);
        let mapped = mapped_runs(Mode::Markdown, &editor, revision, mask).unwrap();
        assert!(mapped
            .collapsed
            .iter()
            .any(|row| mapped.lines[*row].logical_line_index == 3));
        assert!(
            !mapped
                .collapsed
                .iter()
                .any(|row| mapped.lines[*row].logical_line_index == 4),
            "real blank row stays visible"
        );
        let underline = source[..source.find("===").unwrap()].chars().count();
        assert_eq!(preview_mask(&values, underline, None), 0);
        assert_eq!(preview_mask(&values, 0, Some(underline..underline + 1)), 0);
        let revealed = mapped_runs(Mode::Markdown, &editor, revision, revealed()).unwrap();
        assert!(revealed.collapsed.is_empty());
        assert!(revealed
            .lines
            .iter()
            .any(|line| line.logical_line_index == 3
                && line.cells.iter().map(|cell| cell.ch).collect::<String>() == "==="));
    }

    #[test]
    fn fenced_blocks_collapse_body_rows_keep_blank_after_and_fall_back_when_folded() {
        let source = "before\n\n```rust\nfn main() {}\n\n```\n\nafter";
        let mut editor = editor_core::EditorStateManager::new(source, 80);
        let revision = DocumentRevision(0);
        let values = replacements(Mode::Markdown, &editor, revision);
        assert_eq!(values.len(), 1);
        let mask = state(Mode::Markdown, &editor, revision);
        let mapped = mapped_runs(Mode::Markdown, &editor, revision, mask).unwrap();
        for line in [3, 4, 5] {
            assert!(mapped
                .collapsed
                .iter()
                .any(|row| mapped.lines[*row].logical_line_index == line));
        }
        assert!(!mapped
            .collapsed
            .iter()
            .any(|row| mapped.lines[*row].logical_line_index == 6));
        let body = source[..source.find("fn main").unwrap()].chars().count();
        assert_eq!(preview_mask(&values, body, None), 0);
        assert_eq!(preview_mask(&values, 0, Some(body..body + 1)), 0);
        assert!(mapped_runs(Mode::Markdown, &editor, revision, revealed())
            .unwrap()
            .collapsed
            .is_empty());
        editor
            .execute(editor_core::Command::Style(
                editor_core::StyleCommand::Fold {
                    start_line: 2,
                    end_line: 5,
                },
            ))
            .unwrap();
        assert!(mapped_runs(Mode::Markdown, &editor, revision, mask).is_none());
        editor
            .execute(editor_core::Command::Style(
                editor_core::StyleCommand::UnfoldAll,
            ))
            .unwrap();
        assert!(mapped_runs(Mode::Markdown, &editor, revision, mask).is_some());
        let incomplete = format!("```rust\n{}\n```\n", "x".repeat(4100));
        let editor = editor_core::EditorStateManager::new(&incomplete, 80);
        assert!(
            replacements(Mode::Markdown, &editor, revision).len() == 1,
            "a complete fence beyond the former prefix retains its owner"
        );
    }
    #[test]
    fn endpoint_validation_matches_full_grapheme_segmentation() {
        use unicode_segmentation::UnicodeSegmentation;
        for text in ["", "a\r\nb", "a👩🏽‍💻é", "🇩🇪🇨🇳🇺🇸", "क्‍षि", "\u{600}a\u{301}b"]
        {
            let mut expected = vec![0];
            let mut offset = 0;
            for grapheme in text.graphemes(true) {
                offset += grapheme.chars().count();
                expected.push(offset);
            }
            let values = (0..=offset + 1)
                .flat_map(|start| {
                    (start + 1..=offset + 2).map(move |end| Replacement {
                        start,
                        end,
                        widget: None,
                        style: None,
                    })
                })
                .collect();
            let actual = valid_replacements(text, values);
            let wanted = expected
                .iter()
                .flat_map(|start| {
                    expected
                        .iter()
                        .filter(move |end| *end > start)
                        .map(move |end| (*start, *end))
                })
                .collect::<Vec<_>>();
            assert_eq!(
                actual
                    .iter()
                    .map(|value| (value.start, value.end))
                    .collect::<Vec<_>>(),
                wanted,
                "{text:?}"
            );
        }
    }
    #[test]
    fn derived_fold_refresh_updates_visibility_after_row_index_is_cached() {
        let mut editor = editor_core::EditorStateManager::new("a\nb\nc\nd\n", 80);
        editor.replace_folding_regions(vec![editor_core::FoldRegion::new(0, 2)], false);
        assert_eq!(editor.total_visual_lines(), 5);
        editor.replace_folding_regions(vec![editor_core::FoldRegion::new(0, 3)], false);
        assert_eq!(editor.logical_position_to_visual_row(3, 0), Some(3));
        let mut region = editor_core::FoldRegion::new(0, 3);
        region.collapse();
        editor.replace_folding_regions(vec![region], false);
        assert_eq!(editor.total_visual_lines(), 2);
        assert_eq!(editor.logical_position_to_visual_row(3, 0), Some(0));
        let mut region = editor_core::FoldRegion::new(0, 2);
        region.collapse();
        editor.replace_folding_regions(vec![region], false);
        assert_eq!(editor.total_visual_lines(), 3);
        assert_eq!(editor.logical_position_to_visual_row(3, 0), Some(1));
        editor.replace_folding_regions(vec![editor_core::FoldRegion::new(0, 2)], false);
        assert_eq!(editor.total_visual_lines(), 5);
        assert_eq!(editor.logical_position_to_visual_row(3, 0), Some(3));
    }
}
