//! View-only wrap affinity and pixel-column movement for the bounded fixture.
use crate::{
    native_layout::RowHeights,
    native_proportional_run::{RunHeight, RunKey},
};
use editor_core::EditorStateManager;
use std::collections::BTreeMap;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy, PartialEq)]
pub(crate) struct CaretAffinity {
    pub key: RunKey,
    pub offset: usize,
}

#[derive(Clone, Copy)]
pub(crate) struct Target {
    pub offset: usize,
    pub x: f64,
    pub y: f64,
    pub affinity: Option<CaretAffinity>,
}

pub(crate) fn positions(run: &RunHeight, heights: &RowHeights) -> Vec<Target> {
    let mut values = Vec::new();
    for rect in &run.geometry.boxes {
        for (offset, backward) in [(rect.start, false), (rect.end, true)] {
            let point = if backward {
                run.geometry.caret_backward(offset)
            } else {
                run.geometry.caret(offset)
            };
            if let Some((x, y, _)) = point {
                let backward = backward
                    && (offset == run.length && !run.include_end
                        || run
                            .geometry
                            .caret(offset)
                            .is_some_and(|(_, forward_y, _)| (forward_y - y).abs() > 0.5));
                values.push(Target {
                    offset: run.key.start + offset,
                    x,
                    y: heights.row_top(run.key.row) + y,
                    affinity: backward.then_some(CaretAffinity {
                        key: run.key,
                        offset: run.key.start + offset,
                    }),
                });
            }
        }
    }
    if values.is_empty() {
        values.push(Target {
            offset: run.key.start,
            x: 0.0,
            y: heights.row_top(run.key.row),
            affinity: None,
        });
    }
    values
}

pub(crate) fn origin(
    run: &RunHeight,
    offset: usize,
    affinity: Option<CaretAffinity>,
    heights: &RowHeights,
) -> Option<Target> {
    let local = offset.checked_sub(run.key.start)?;
    let backward = affinity.is_some_and(|value| value.key == run.key && value.offset == offset);
    if local > run.length || local == run.length && !run.include_end && !backward {
        return None;
    }
    let (x, y, _) = if backward {
        run.geometry.caret_backward(local)
    } else {
        run.geometry.caret(local)
    }?;
    Some(Target {
        offset,
        x,
        y: heights.row_top(run.key.row) + y,
        affinity: backward.then_some(CaretAffinity {
            key: run.key,
            offset,
        }),
    })
}

pub(crate) fn edge(
    run: &RunHeight,
    origin: Target,
    end: bool,
    heights: &RowHeights,
) -> Option<Target> {
    edge_from_positions(positions(run, heights), origin, end)
}

fn edge_from_positions(points: Vec<Target>, origin: Target, end: bool) -> Option<Target> {
    points
        .into_iter()
        .filter(|point| (point.y - origin.y).abs() < 0.5)
        .min_by(|a, b| {
            if end {
                b.x.total_cmp(&a.x).then_with(|| b.offset.cmp(&a.offset))
            } else {
                a.x.total_cmp(&b.x).then_with(|| a.offset.cmp(&b.offset))
            }
        })
}

pub(crate) fn vertical(
    runs: &BTreeMap<usize, RunHeight>,
    key: RunKey,
    origin: Target,
    down: bool,
    x: f64,
    heights: &RowHeights,
) -> Option<Target> {
    let points = runs
        .values()
        .filter(|run| {
            run.key.model == key.model
                && run.key.revision == key.revision
                && run.key.viewport_width == key.viewport_width
                && run.key.font_epoch == key.font_epoch
                && run.key.presentation == key.presentation
        })
        .flat_map(|run| positions(run, heights))
        .collect::<Vec<_>>();
    let y = points
        .iter()
        .map(|point| point.y)
        .filter(|y| {
            if down {
                *y > origin.y + 0.5
            } else {
                *y < origin.y - 0.5
            }
        })
        .min_by(|a, b| (a - origin.y).abs().total_cmp(&(b - origin.y).abs()))?;
    nearest(
        points.into_iter().filter(|point| (point.y - y).abs() < 0.5),
        x,
    )
}

pub(crate) fn enter(run: &RunHeight, down: bool, x: f64, heights: &RowHeights) -> Option<Target> {
    let points = positions(run, heights);
    let y = points.iter().map(|point| point.y).min_by(|a, b| {
        if down {
            a.total_cmp(b)
        } else {
            b.total_cmp(a)
        }
    })?;
    nearest(
        points.into_iter().filter(|point| (point.y - y).abs() < 0.5),
        x,
    )
}

fn nearest(points: impl Iterator<Item = Target>, x: f64) -> Option<Target> {
    points.min_by(|a, b| {
        (a.x - x)
            .abs()
            .total_cmp(&(b.x - x).abs())
            .then_with(|| a.affinity.is_some().cmp(&b.affinity.is_some()))
    })
}

/// Uniform targets use the engine's visible source cells, including tabs and
/// wide characters, but only expose Unicode grapheme boundaries on this row.
pub(crate) fn uniform(
    editor: &EditorStateManager,
    row: usize,
    x: f64,
    cell_pixels: f64,
) -> Option<Target> {
    let grid = editor.get_viewport_content_styled(row, 1);
    let line = grid.lines.first()?;
    if line.is_fold_placeholder_appended {
        return None;
    }
    let text = line.cells.iter().map(|cell| cell.ch).collect::<String>();
    let mut column = 0;
    let mut pixel = line.segment_x_start_cells as f64 * cell_pixels;
    let mut values = Vec::new();
    for grapheme in text.graphemes(true) {
        values.push(Target {
            offset: line.char_offset_start + column,
            x: pixel,
            y: 0.0,
            affinity: None,
        });
        let length = grapheme.chars().count();
        pixel += line.cells[column..column + length]
            .iter()
            .map(|cell| cell.width as f64 * cell_pixels)
            .sum::<f64>();
        column += length;
    }
    let offset = line.char_offset_start + column;
    let (source_line, source_column) = editor.editor().line_index().char_offset_to_position(offset);
    if editor
        .logical_position_to_visual(source_line, source_column)
        .is_some_and(|(mapped, _)| mapped == row)
    {
        values.push(Target {
            offset,
            x: pixel,
            y: 0.0,
            affinity: None,
        });
    }
    nearest(values.into_iter(), x)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn visual_edges_include_zero_width_trailing_source_but_exclude_other_rows() {
        let point = |offset, x, y| Target {
            offset,
            x,
            y,
            affinity: None,
        };
        let points = vec![
            point(0, 0.0, 0.0),
            point(4, 20.0, 0.0),
            point(5, 20.0, 0.0),
            point(6, 0.0, 32.0),
        ];
        assert_eq!(
            edge_from_positions(points.clone(), point(2, 8.0, 0.0), true)
                .unwrap()
                .offset,
            5
        );
        assert_eq!(
            edge_from_positions(points, point(2, 8.0, 0.0), false)
                .unwrap()
                .offset,
            0
        );
    }
    #[test]
    fn uniform_targets_snap_to_graphemes_and_clamp_short_rows() {
        let editor = EditorStateManager::new("a😀e\u{301}b\nxy", 80);
        let source = editor.get_viewport_content_styled(0, 1);
        let emoji_width = source.lines[0].cells[1].width as f64 * 8.0;
        assert_eq!(
            uniform(&editor, 0, 8.0 + emoji_width * 0.2, 8.0)
                .unwrap()
                .offset,
            1
        );
        assert_eq!(
            uniform(&editor, 0, 8.0 + emoji_width * 0.8, 8.0)
                .unwrap()
                .offset,
            2
        );
        assert_eq!(
            uniform(&editor, 0, 8.0 + emoji_width + 8.0, 8.0)
                .unwrap()
                .offset,
            4
        );
        assert_eq!(uniform(&editor, 1, 1000.0, 8.0).unwrap().offset, 8);
    }
}
