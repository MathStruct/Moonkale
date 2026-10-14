//! The virtualised grid's arithmetic, kept apart from the component so it
//! is tested without a DOM: which rows and columns a viewport shows, the
//! columns' starting widths, the sorted and filtered row order, cursor
//! moves, and the scroll that reveals a cell.
//!
//! Rows have one fixed height, so a row window is a division; columns have
//! their own widths, so a column window is a binary search over their
//! prefix sums. Only the window (plus a little overscan) is in the DOM —
//! 50 000 rows cost the same to draw as 50.

use moonkale_core::Value;
use std::cmp::Ordering;
use std::ops::Range;

/// One row's height in pixels; the CSS sets the same.
pub const ROW_H: f64 = 24.0;
/// The header row's height.
pub const HEAD_H: f64 = 26.0;
/// The row-number column's width (it stays put while the rest scrolls).
pub const NUM_W: f64 = 56.0;
/// Rows drawn beyond each edge of the viewport, so a fast scroll does not
/// show blank rows before the next render.
pub const OVERSCAN_ROWS: usize = 10;
/// Columns drawn beyond each edge.
pub const OVERSCAN_COLS: usize = 2;
/// The narrowest a column can be dragged to, and the widest it starts.
pub const MIN_W: f64 = 48.0;
pub const MAX_START_W: f64 = 420.0;

/// The rows a viewport shows: `scroll_top` and `height` in pixels of the
/// grid's scroll box (the header included), over `n` rows.
pub fn row_window(scroll_top: f64, height: f64, n: usize) -> Range<usize> {
    let top = (scroll_top.max(0.0) / ROW_H).floor() as usize;
    let visible = (height.max(0.0) / ROW_H).ceil() as usize + 1;
    let start = top.saturating_sub(OVERSCAN_ROWS).min(n);
    let end = (top + visible + OVERSCAN_ROWS).min(n);
    start..end
}

/// Each column's left edge, after the row-number column: `lefts[i]` is
/// where column `i` starts and `lefts[n]` is the total width.
pub fn lefts(widths: &[f64]) -> Vec<f64> {
    let mut out = Vec::with_capacity(widths.len() + 1);
    let mut x = 0.0;
    out.push(x);
    for w in widths {
        x += w;
        out.push(x);
    }
    out
}

/// The columns a viewport shows: `scroll_left` and `width` of the scroll
/// box; the row-number column covers the first `NUM_W` pixels.
pub fn col_window(lefts: &[f64], scroll_left: f64, width: f64) -> Range<usize> {
    let n = lefts.len().saturating_sub(1);
    let from = scroll_left.max(0.0);
    let to = from + (width - NUM_W).max(0.0);
    // The first column whose right edge is past `from`, the first whose
    // left edge is past `to`.
    let first = lefts[1..].partition_point(|&right| right <= from);
    let last = lefts[..n].partition_point(|&left| left < to);
    first.saturating_sub(OVERSCAN_COLS)..(last + OVERSCAN_COLS).min(n)
}

/// What a cell shows.
pub fn cell_text(v: &Value) -> String {
    v.to_string()
}

/// Starting widths from the header and a sample of rows: about one
/// character's width per character, clamped so a long text does not take
/// the whole screen. The sample is the first and last rows plus a stride
/// through the rest, so values that grow along the result (ids, counts)
/// fit at the end too.
pub fn initial_widths(columns: &[String], rows: &[Vec<Value>]) -> Vec<f64> {
    const CHAR: f64 = 7.6;
    const PAD: f64 = 22.0;
    columns
        .iter()
        .enumerate()
        .map(|(c, name)| {
            let longest = sample(rows.len())
                .filter_map(|i| rows[i].get(c))
                .map(|v| cell_text(v).chars().count())
                .max()
                .unwrap_or(0)
                // The header carries a sort arrow.
                .max(name.chars().count() + 2);
            (longest as f64 * CHAR + PAD).clamp(MIN_W, MAX_START_W)
        })
        .collect()
}

/// About 400 row indices out of `n`: the first and last 100 and an even
/// stride through the middle.
fn sample(n: usize) -> impl Iterator<Item = usize> {
    let stride = (n / 200).max(1);
    let tail = n.saturating_sub(100).max(n.min(100));
    (0..n.min(100))
        .chain((100..n.saturating_sub(100)).step_by(stride))
        .chain(tail..n)
}

/// How two cells order: numbers by value (an integer and a float
/// compare), booleans false first, text case-insensitively (ties by
/// case), bytes by size; across kinds, booleans < numbers < text < bytes.
/// NULL is not ordered here — [`order`] keeps it last either way.
pub fn compare(a: &Value, b: &Value) -> Ordering {
    use Value::*;
    let rank = |v: &Value| match v {
        Null => 4,
        Bool(_) => 0,
        Int(_) | Float(_) => 1,
        Text(_) => 2,
        Bytes { .. } => 3,
    };
    let num = |v: &Value| match v {
        Int(i) => *i as f64,
        Float(x) => *x,
        _ => 0.0,
    };
    match (a, b) {
        (Bool(x), Bool(y)) => x.cmp(y),
        (Int(x), Int(y)) => x.cmp(y),
        (Int(_) | Float(_), Int(_) | Float(_)) => num(a).total_cmp(&num(b)),
        (Text(x), Text(y)) => x
            .to_lowercase()
            .cmp(&y.to_lowercase())
            .then_with(|| x.cmp(y)),
        (Bytes { len: x }, Bytes { len: y }) => x.cmp(y),
        _ => rank(a).cmp(&rank(b)),
    }
}

/// The rows to show, as indices into `rows`: those with a cell containing
/// `filter` (case-insensitive; empty keeps all), sorted by column `sort.0`
/// ascending when `sort.1`, descending otherwise. The sort is stable, and
/// NULLs stay last in both directions.
pub fn order(rows: &[Vec<Value>], sort: Option<(usize, bool)>, filter: &str) -> Vec<usize> {
    let needle = filter.trim().to_lowercase();
    let mut out: Vec<usize> = (0..rows.len())
        .filter(|&i| {
            needle.is_empty()
                || rows[i]
                    .iter()
                    .any(|v| cell_text(v).to_lowercase().contains(&needle))
        })
        .collect();
    if let Some((c, ascending)) = sort {
        out.sort_by(|&i, &j| {
            let (a, b) = (rows[i].get(c), rows[j].get(c));
            let null = |v: Option<&Value>| matches!(v, None | Some(Value::Null));
            match (null(a), null(b)) {
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                _ => {
                    let o = compare(a.unwrap(), b.unwrap());
                    if ascending {
                        o
                    } else {
                        o.reverse()
                    }
                }
            }
        });
    }
    out
}

/// The next sort when a header is clicked: ascending, then descending,
/// then unsorted; another column starts ascending.
pub fn next_sort(current: Option<(usize, bool)>, column: usize) -> Option<(usize, bool)> {
    match current {
        Some((c, true)) if c == column => Some((column, false)),
        Some((c, false)) if c == column => None,
        _ => Some((column, true)),
    }
}

/// A cursor move on the keyboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Move {
    Up,
    Down,
    Left,
    Right,
    PageUp,
    PageDown,
    /// The row's first cell.
    Home,
    /// The row's last cell.
    End,
    /// The grid's first cell.
    First,
    /// The grid's last cell.
    Last,
}

/// Where the cursor `(row, column)` goes — rows are positions in the shown
/// order — clamped to `rows × cols`; `page` is the rows one screen holds.
pub fn step(
    (r, c): (usize, usize),
    mv: Move,
    rows: usize,
    cols: usize,
    page: usize,
) -> (usize, usize) {
    if rows == 0 || cols == 0 {
        return (0, 0);
    }
    let (last_r, last_c) = (rows - 1, cols - 1);
    let (r, c) = (r.min(last_r), c.min(last_c));
    let page = page.max(1);
    match mv {
        Move::Up => (r.saturating_sub(1), c),
        Move::Down => ((r + 1).min(last_r), c),
        Move::Left => (r, c.saturating_sub(1)),
        Move::Right => (r, (c + 1).min(last_c)),
        Move::PageUp => (r.saturating_sub(page), c),
        Move::PageDown => ((r + page).min(last_r), c),
        Move::Home => (r, 0),
        Move::End => (r, last_c),
        Move::First => (0, 0),
        Move::Last => (last_r, last_c),
    }
}

/// The scroll `(left, top)` that brings cell `(row, column)` fully into
/// view, or `None` when it already is. The sticky header and row-number
/// column cover the top and left edges, so a cell under them is hidden.
pub fn reveal(
    (left, top): (f64, f64),
    (row, col): (usize, usize),
    lefts: &[f64],
    (width, height): (f64, f64),
) -> Option<(f64, f64)> {
    let mut to = (left, top);
    let y0 = row as f64 * ROW_H;
    let view_h = (height - HEAD_H).max(ROW_H);
    if y0 < top {
        to.1 = y0;
    } else if y0 + ROW_H > top + view_h {
        to.1 = y0 + ROW_H - view_h;
    }
    if let (Some(&x0), Some(&x1)) = (lefts.get(col), lefts.get(col + 1)) {
        let view_w = (width - NUM_W).max(0.0);
        if x0 < left {
            to.0 = x0;
        } else if x1 > left + view_w {
            // A column wider than the view shows its start.
            to.0 = (x1 - view_w).min(x0);
        }
    }
    (to != (left, top)).then_some(to)
}

#[cfg(test)]
mod tests {
    use super::*;
    use Value::*;

    #[test]
    fn the_row_window_covers_the_viewport_plus_overscan() {
        // 600 px over 50 000 rows at the top: 26 visible rows, 10 below.
        let w = row_window(0.0, 600.0, 50_000);
        assert_eq!(w.start, 0);
        assert_eq!(w.end, 26 + OVERSCAN_ROWS);
        // Scrolled to row 1000: 10 rows of overscan above too.
        let w = row_window(1000.0 * ROW_H, 600.0, 50_000);
        assert_eq!(w.start, 990);
        assert!(w.contains(&1000) && w.contains(&1025));
        // The end clamps, and few rows draw all of them.
        assert_eq!(row_window(1e9, 600.0, 50_000).end, 50_000);
        assert_eq!(row_window(0.0, 600.0, 3), 0..3);
        assert_eq!(row_window(0.0, 600.0, 0), 0..0);
    }

    #[test]
    fn the_column_window_finds_the_columns_under_the_viewport() {
        let l = lefts(&[100.0; 50]);
        assert_eq!(l.len(), 51);
        assert_eq!(l[50], 5000.0);
        // 556 px wide: 500 px of columns after the row numbers → 0..5,
        // plus two of overscan on the right.
        assert_eq!(col_window(&l, 0.0, 556.0), 0..7);
        // Scrolled to 1050: columns 10..16 show (10 is half hidden).
        let w = col_window(&l, 1050.0, 556.0);
        assert_eq!(w, 8..18);
        assert_eq!(col_window(&l, 1e9, 556.0).end, 50);
        assert_eq!(col_window(&lefts(&[]), 0.0, 556.0), 0..0);
    }

    #[test]
    fn widths_follow_the_content_within_bounds() {
        let cols = vec!["id".to_string(), "description".to_string()];
        let rows = vec![
            vec![Int(1), Text("short".into())],
            vec![Int(22), Text("x".repeat(400))],
        ];
        let w = initial_widths(&cols, &rows);
        // "id" plus room for the sort arrow: four characters and padding.
        assert!((w[0] - (4.0 * 7.6 + 22.0)).abs() < 1e-9, "{}", w[0]);
        assert_eq!(
            initial_widths(&["x".into()], &[])[0],
            MIN_W,
            "never under the minimum"
        );
        // Ids that grow: the last row's width counts, not only the first's.
        let ids: Vec<Vec<Value>> = (1..=50_000).map(|i| vec![Int(i)]).collect();
        let iw = initial_widths(&["i".into()], &ids)[0];
        assert!((iw - (5.0 * 7.6 + 22.0)).abs() < 1e-9, "{iw}");
        assert!(sample(50_000).count() < 450);
        assert_eq!(sample(3).collect::<Vec<_>>(), vec![0, 1, 2]);
        assert_eq!(
            sample(150).collect::<Vec<_>>(),
            (0..150).collect::<Vec<_>>(),
            "a short result is read whole"
        );
        assert_eq!(w[1], MAX_START_W, "a long text is clamped");
    }

    #[test]
    fn values_compare_by_kind_then_value() {
        assert_eq!(compare(&Int(2), &Float(1.5)), Ordering::Greater);
        assert_eq!(
            compare(&Int(10), &Int(9)),
            Ordering::Greater,
            "numbers, not text"
        );
        assert_eq!(
            compare(&Text("apple".into()), &Text("Banana".into())),
            Ordering::Less
        );
        assert_eq!(
            compare(&Text("a".into()), &Text("A".into())),
            Ordering::Greater,
            "case breaks ties"
        );
        assert_eq!(compare(&Bool(true), &Int(0)), Ordering::Less);
        assert_eq!(compare(&Int(5), &Text("5".into())), Ordering::Less);
    }

    #[test]
    fn order_filters_then_sorts_stably_with_nulls_last() {
        let rows = vec![
            vec![Int(3), Text("c".into())],
            vec![Null, Text("null".into())],
            vec![Int(1), Text("a".into())],
            vec![Int(3), Text("c2".into())],
        ];
        assert_eq!(order(&rows, None, ""), vec![0, 1, 2, 3]);
        assert_eq!(order(&rows, Some((0, true)), ""), vec![2, 0, 3, 1]);
        assert_eq!(
            order(&rows, Some((0, false)), ""),
            vec![0, 3, 2, 1],
            "ties keep their order, NULL stays last"
        );
        assert_eq!(
            order(&rows, None, "C"),
            vec![0, 3],
            "case-insensitive filter"
        );
        assert_eq!(order(&rows, Some((0, false)), "c"), vec![0, 3]);
        assert_eq!(order(&rows, None, "nothing"), Vec::<usize>::new());
    }

    #[test]
    fn a_header_click_cycles_ascending_descending_off() {
        let s = next_sort(None, 2);
        assert_eq!(s, Some((2, true)));
        let s = next_sort(s, 2);
        assert_eq!(s, Some((2, false)));
        assert_eq!(next_sort(s, 2), None);
        assert_eq!(next_sort(Some((1, false)), 2), Some((2, true)));
    }

    #[test]
    fn the_cursor_moves_and_clamps() {
        let at = (5, 2);
        assert_eq!(step(at, Move::Up, 100, 4, 20), (4, 2));
        assert_eq!(step((0, 0), Move::Up, 100, 4, 20), (0, 0));
        assert_eq!(step(at, Move::Right, 100, 4, 20), (5, 3));
        assert_eq!(step((5, 3), Move::Right, 100, 4, 20), (5, 3));
        assert_eq!(step(at, Move::PageDown, 100, 4, 20), (25, 2));
        assert_eq!(step((95, 2), Move::PageDown, 100, 4, 20), (99, 2));
        assert_eq!(step(at, Move::PageUp, 100, 4, 20), (0, 2));
        assert_eq!(step(at, Move::Home, 100, 4, 20), (5, 0));
        assert_eq!(step(at, Move::End, 100, 4, 20), (5, 3));
        assert_eq!(step(at, Move::Last, 100, 4, 20), (99, 3));
        assert_eq!(step(at, Move::First, 100, 4, 20), (0, 0));
        // A filter that shrank the grid pulls a stale cursor in.
        assert_eq!(step((500, 9), Move::Down, 10, 4, 20), (9, 3));
        assert_eq!(step(at, Move::Down, 0, 4, 20), (0, 0));
    }

    #[test]
    fn reveal_scrolls_only_as_far_as_needed() {
        let l = lefts(&[100.0; 20]);
        let view = (556.0, 26.0 + 10.0 * ROW_H); // 5 columns, 10 rows
        assert_eq!(
            reveal((0.0, 0.0), (3, 2), &l, view),
            None,
            "already visible"
        );
        // Down past the last visible row: the row lands on the bottom edge.
        assert_eq!(reveal((0.0, 0.0), (10, 0), &l, view), Some((0.0, ROW_H)));
        // Up above the top: it lands on the top edge (under the header).
        assert_eq!(
            reveal((0.0, 50.0 * ROW_H), (40, 0), &l, view),
            Some((0.0, 40.0 * ROW_H))
        );
        // Right past the edge, and back left.
        assert_eq!(reveal((0.0, 0.0), (0, 7), &l, view), Some((300.0, 0.0)));
        assert_eq!(reveal((300.0, 0.0), (0, 1), &l, view), Some((100.0, 0.0)));
    }
}
