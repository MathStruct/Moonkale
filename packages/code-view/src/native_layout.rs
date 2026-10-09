//! Uniform horizontal geometry and a sparse variable-height row index.
//! Source-to-visual-cell mapping remains owned by editor-core.
use editor_core::Position;
use moonkale_ext_api::editor::DocumentRevision;

/// Retained source anchor for a viewport across view-only layout changes.
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct ViewAnchor {
    pub row: usize,
    pub source: Position,
    pub revision: DocumentRevision,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct UniformLayout {
    row_pixels: f64,
    cell_pixels: f64,
}

/// Fixture-only presentation settings; never enabled by production hosts.
#[cfg(feature = "layout-fixture")]
#[derive(Clone, Copy, Default, PartialEq)]
pub struct LayoutFixture {
    /// Logical source line anchoring the taller row and block.
    pub line: usize,
    /// Extra height of the text row, in pixels.
    pub extra_height: f64,
    /// Height of a noneditable block preceding the source row, in pixels.
    pub block_height: f64,
    /// Artificial latency for stale-measurement regression coverage.
    pub measurement_delay_ms: u64,
    /// Opt-in integration target; other source lines retain uniform layout.
    pub proportional_line: Option<usize>,
    /// Synthetic replacement and anchored-widget acceptance provider.
    pub presentation: bool,
    /// View-local controls and content inside the anchored block.
    pub interactive_widget: bool,
}

/// Sparse prefix-sum index: uniform documents allocate no row entries.
/// Entries contain cumulative height differences at changed row boundaries.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RowHeights {
    rows: usize,
    height: f64,
    changes: Vec<(usize, f64)>,
}

impl RowHeights {
    pub(crate) fn new(rows: usize, height: f64, overrides: &[(usize, f64)]) -> Self {
        let mut values: Vec<_> = overrides
            .iter()
            .copied()
            .filter(|(row, pixels)| *row < rows && pixels.is_finite() && *pixels >= 0.0)
            .collect();
        values.sort_by_key(|(row, _)| *row);
        values.dedup_by_key(|(row, _)| *row);
        let mut delta = 0.0;
        let changes = values
            .into_iter()
            .filter_map(|(row, pixels)| {
                if pixels == height {
                    return None;
                }
                delta += pixels - height;
                Some((row + 1, delta))
            })
            .collect();
        Self {
            rows,
            height,
            changes,
        }
    }

    pub(crate) fn row_top(&self, row: usize) -> f64 {
        let row = row.min(self.rows);
        let count = self
            .changes
            .partition_point(|(boundary, _)| *boundary <= row);
        row as f64 * self.height
            + count
                .checked_sub(1)
                .map(|i| self.changes[i].1)
                .unwrap_or(0.0)
    }

    pub(crate) fn row_height(&self, row: usize) -> f64 {
        self.row_top(row + 1) - self.row_top(row)
    }

    pub(crate) fn total_height(&self) -> f64 {
        self.row_top(self.rows)
    }

    pub(crate) fn row_at_y(&self, y: f64) -> usize {
        if self.rows == 0 {
            return 0;
        }
        if self.changes.is_empty() {
            return ((y.max(0.0) / self.height).floor() as usize).min(self.rows - 1);
        }
        let total = self.total_height();
        if total <= 0.0 {
            return 0;
        }
        // Exact boundaries choose the next visible row; beyond the document
        // choose the final visible row, even with collapsed trailing rows.
        let y = y.max(0.0).min((total - 0.000001).max(0.0));
        let (mut low, mut high) = (0, self.rows);
        while low < high {
            let middle = low + (high - low) / 2;
            if self.row_top(middle + 1) <= y.max(0.0) {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        low.min(self.rows - 1)
    }

    #[cfg(any(feature = "layout-fixture", test))]
    pub(crate) fn adjacent_visible(&self, row: usize, down: bool) -> Option<usize> {
        let y = if down {
            self.row_top(row + 1)
        } else {
            self.row_top(row) - 0.000001
        };
        if y < 0.0 || y >= self.total_height() {
            return None;
        }
        let next = self.row_at_y(y);
        (next != row).then_some(next)
    }

    pub(crate) fn visible_rows(&self, start: usize, pixels: f64) -> usize {
        if self.changes.is_empty() {
            return (pixels / self.height).ceil() as usize;
        }
        self.row_at_y(self.row_top(start) + pixels.max(0.0))
            .saturating_sub(start)
            + 1
    }

    pub(crate) fn max_start(&self, viewport: f64) -> usize {
        self.row_at_y((self.total_height() - viewport).max(0.0))
    }

    pub(crate) fn reveal_start(&self, start: usize, viewport: f64, target: usize) -> usize {
        if target < start {
            return target;
        }
        if self.row_top(target + 1) > self.row_top(start) + viewport {
            // Round upward to keep the whole target visible, unless it is itself
            // taller than the viewport. In that case reveal its beginning.
            let y = (self.row_top(target + 1) - viewport).max(0.0);
            let row = self.row_at_y(y);
            return (row + usize::from(self.row_top(row) < y)).min(target);
        }
        start
    }
}

impl UniformLayout {
    pub(crate) fn new(row_pixels: f64, cell_pixels: f64) -> Self {
        Self {
            row_pixels,
            cell_pixels,
        }
    }

    pub(crate) fn row_height(self) -> f64 {
        self.row_pixels
    }

    pub(crate) fn column_x(self, column: usize) -> f64 {
        column as f64 * self.cell_pixels
    }

    pub(crate) fn column_at_x(self, x: f64) -> usize {
        (x.max(0.0) / self.cell_pixels).floor() as usize
    }

    /// A rendered cell represents one source character even when it occupies
    /// multiple visual cells (a tab or a wide Unicode character).
    pub(crate) fn hit_column(self, column: usize, width: usize, local_x: f64) -> usize {
        column + usize::from(local_x >= self.column_x(width) / 2.0)
    }

    pub(crate) fn reveal_start(start: usize, capacity: usize, target: usize) -> usize {
        if target < start {
            target
        } else if target >= start + capacity {
            target + 1 - capacity
        } else {
            start
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixed_heights_match_linear_reference() {
        let heights = [22.0, 176.0, 22.0, 5.0, 44.0, 22.0];
        let index = RowHeights::new(heights.len(), 22.0, &[(4, 44.0), (1, 176.0), (3, 5.0)]);
        let mut top = 0.0;
        for (row, height) in heights.into_iter().enumerate() {
            assert_eq!(index.row_top(row), top);
            assert_eq!(index.row_height(row), height);
            assert_eq!(index.row_at_y(top), row);
            assert_eq!(index.row_at_y(top + height - 0.01), row);
            top += height;
        }
        assert_eq!(index.total_height(), top);
        assert_eq!(index.row_at_y(top + 100.0), 5);
        assert_eq!(index.row_at_y(-10.0), 0);
        assert_eq!(index.reveal_start(0, 60.0, 1), 1);
        assert_eq!(index.reveal_start(0, 60.0, 4), 3);
        assert_eq!(index.visible_rows(0, 60.0), 2);
    }

    #[test]
    fn collapsed_rows_have_no_pixels_or_hit_targets() {
        let index = RowHeights::new(7, 22.0, &[(0, 0.0), (2, 0.0), (3, 0.0), (6, 0.0)]);
        assert_eq!(index.total_height(), 66.0);
        assert_eq!(index.row_at_y(0.0), 1);
        assert_eq!(index.row_at_y(21.9), 1);
        assert_eq!(index.row_at_y(22.0), 4);
        assert_eq!(index.row_at_y(44.0), 5);
        assert_eq!(index.row_at_y(1000.0), 5);
        assert_eq!(index.row_height(3), 0.0);
        assert_eq!(index.adjacent_visible(1, true), Some(4));
        assert_eq!(index.adjacent_visible(4, false), Some(1));
        assert_eq!(index.adjacent_visible(1, false), None);
        assert_eq!(index.adjacent_visible(5, true), None);
        assert_eq!(index.visible_rows(1, 30.0), 4);
        assert_eq!(
            RowHeights::new(2, 22.0, &[(0, 0.0), (1, 0.0)]).row_at_y(1.0),
            0
        );
        assert_eq!(RowHeights::new(2, 22.0, &[(0, -1.0)]).total_height(), 44.0);
    }

    #[test]
    fn uniform_fast_path_and_height_change_anchor() {
        let before = RowHeights::new(100_000, 22.0, &[]);
        assert!(before.changes.is_empty());
        assert_eq!(before.row_at_y(before.row_top(90_000)), 90_000);
        assert_eq!(before.visible_rows(0, 45.0), 3);
        let after = RowHeights::new(100_000, 22.0, &[(1, 220.0)]);
        let displacement = 7.0;
        let anchored_y = after.row_top(50) + displacement;
        assert_eq!(after.row_at_y(anchored_y), 50);
        assert_eq!(anchored_y - before.row_top(50) - displacement, 198.0);
        assert_eq!(RowHeights::new(0, 22.0, &[]).row_at_y(50.0), 0);
        assert_eq!(
            RowHeights::new(2, 22.0, &[(3, 100.0), (0, f64::NAN)]).total_height(),
            44.0
        );
    }

    #[test]
    fn wide_cell_hit_testing_and_reveal() {
        let layout = UniformLayout::new(22.0, 8.0);
        assert_eq!(layout.hit_column(5, 4, 15.9), 5);
        assert_eq!(layout.hit_column(5, 4, 16.0), 6);
        assert_eq!(UniformLayout::reveal_start(10, 4, 9), 9);
        assert_eq!(UniformLayout::reveal_start(10, 4, 13), 10);
        assert_eq!(UniformLayout::reveal_start(10, 4, 14), 11);
    }
}

/// Retained measured run geometry. Populated only by the opt-in layout feature.
#[derive(Clone, Copy, PartialEq)]
#[allow(dead_code)]
pub(crate) struct CachedRunHeight {
    pub revision: DocumentRevision,
    pub row: usize,
    pub start: usize,
    pub viewport_width: f64,
    pub height: f64,
    /// Bit mask of fixture constructs still in preview.
    pub presentation: u64,
}
