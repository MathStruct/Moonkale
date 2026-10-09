use crate::commands::{Position, Selection, SelectionDirection};

pub fn selection_direction(start: Position, end: Position) -> SelectionDirection {
    if start <= end {
        SelectionDirection::Forward
    } else {
        SelectionDirection::Backward
    }
}

pub fn selection_min_max(selection: &Selection) -> (Position, Position) {
    if selection.start <= selection.end {
        (selection.start, selection.end)
    } else {
        (selection.end, selection.start)
    }
}

pub fn selection_contains_position_inclusive(selection: &Selection, pos: Position) -> bool {
    let (min_pos, max_pos) = selection_min_max(selection);
    min_pos <= pos && pos <= max_pos
}

pub fn normalize_selections(
    mut selections: Vec<Selection>,
    primary_index: usize,
) -> (Vec<Selection>, usize) {
    if selections.is_empty() {
        return (selections, 0);
    }

    let primary_active = selections
        .get(primary_index)
        .map(|s| s.end)
        .unwrap_or(selections[0].end);

    for sel in &mut selections {
        sel.direction = selection_direction(sel.start, sel.end);
    }

    selections.sort_by(|a, b| {
        let (a_min, a_max) = selection_min_max(a);
        let (b_min, b_max) = selection_min_max(b);
        a_min
            .cmp(&b_min)
            .then_with(|| a_max.cmp(&b_max))
            .then_with(|| a.end.cmp(&b.end))
            .then_with(|| a.start.cmp(&b.start))
    });

    // Merge overlapping selections (half-open-ish: do not merge when next starts exactly at current end).
    let mut merged: Vec<Selection> = Vec::with_capacity(selections.len());
    for sel in selections {
        if merged.is_empty() {
            merged.push(sel);
            continue;
        }

        let last = merged.last_mut().expect("non-empty");
        let (last_min, last_max) = selection_min_max(last);
        let (sel_min, sel_max) = selection_min_max(&sel);

        if sel_min < last_max {
            // Merge to union range; canonicalize to Forward.
            let new_min = last_min.min(sel_min);
            let new_max = last_max.max(sel_max);
            *last = Selection {
                start: new_min,
                end: new_max,
                direction: SelectionDirection::Forward,
            };
        } else if sel_min == last_min && sel_max == last_max {
            // Exact duplicate - drop.
            continue;
        } else {
            merged.push(sel);
        }
    }

    let new_primary_index = merged
        .iter()
        .position(|s| selection_contains_position_inclusive(s, primary_active))
        .unwrap_or_else(|| merged.len().saturating_sub(1));

    (merged, new_primary_index)
}

pub fn rect_selections(anchor: Position, active: Position) -> (Vec<Selection>, usize) {
    let start_line = anchor.line.min(active.line);
    let end_line = anchor.line.max(active.line);

    let direction = selection_direction(
        Position::new(0, anchor.column),
        Position::new(0, active.column),
    );

    let mut selections = Vec::with_capacity(end_line.saturating_sub(start_line) + 1);
    for line in start_line..=end_line {
        selections.push(Selection {
            start: Position::new(line, anchor.column),
            end: Position::new(line, active.column),
            direction,
        });
    }

    let primary_active = active;
    let primary_index = selections
        .iter()
        .position(|s| s.end.line == primary_active.line)
        .unwrap_or(0);

    normalize_selections(selections, primary_index)
}
