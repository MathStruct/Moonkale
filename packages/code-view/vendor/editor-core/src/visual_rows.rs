//! Visual-row prefix index for soft wrapping and folding.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VisualRowSpan {
    pub(crate) logical_line: usize,
    pub(crate) start_visual_row: usize,
    pub(crate) visual_line_count: usize,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct VisualRowIndex {
    line_visual_counts: Vec<usize>,
    tree: FenwickTree,
}

impl VisualRowIndex {
    pub(crate) fn from_line_visual_counts(line_visual_counts: Vec<usize>) -> Self {
        let tree = FenwickTree::from_values(&line_visual_counts);
        Self {
            line_visual_counts,
            tree,
        }
    }

    pub(crate) fn logical_line_count(&self) -> usize {
        self.line_visual_counts.len()
    }

    pub(crate) fn total_visual_lines(&self) -> usize {
        self.tree.total()
    }

    pub(crate) fn visual_rows_before_logical_line(&self, logical_line: usize) -> usize {
        self.tree
            .prefix_sum(logical_line.min(self.line_visual_counts.len()))
    }

    pub(crate) fn set_line_visual_count(
        &mut self,
        logical_line: usize,
        visual_line_count: usize,
    ) -> bool {
        let Some(current) = self.line_visual_counts.get_mut(logical_line) else {
            return false;
        };

        let previous = *current;
        if previous == visual_line_count {
            return true;
        }

        *current = visual_line_count;
        self.tree
            .add_delta(logical_line, previous, visual_line_count);
        true
    }

    pub(crate) fn insert_lines<I>(&mut self, logical_line: usize, visual_line_counts: I)
    where
        I: IntoIterator<Item = usize>,
    {
        let counts = visual_line_counts.into_iter().collect::<Vec<_>>();
        if counts.is_empty() {
            return;
        }

        let pos = logical_line.min(self.line_visual_counts.len());
        self.line_visual_counts.splice(pos..pos, counts);
        self.tree = FenwickTree::from_values(&self.line_visual_counts);
    }

    pub(crate) fn remove_lines(&mut self, logical_line: usize, count: usize) -> bool {
        if count == 0 {
            return true;
        }
        if logical_line >= self.line_visual_counts.len()
            || logical_line.saturating_add(count) > self.line_visual_counts.len()
        {
            return false;
        }

        self.line_visual_counts
            .drain(logical_line..logical_line.saturating_add(count));
        self.tree = FenwickTree::from_values(&self.line_visual_counts);
        true
    }

    pub(crate) fn span_for_visual_row(&self, visual_row: usize) -> Option<(VisualRowSpan, usize)> {
        if visual_row >= self.total_visual_lines() {
            return None;
        }

        let logical_line = self.tree.find_line_containing_visual_row(visual_row)?;
        let start_visual_row = self.tree.prefix_sum(logical_line);
        let visual_line_count = self.line_visual_counts[logical_line];
        Some((
            VisualRowSpan {
                logical_line,
                start_visual_row,
                visual_line_count,
            },
            visual_row.saturating_sub(start_visual_row),
        ))
    }

    pub(crate) fn span_for_logical_line(&self, logical_line: usize) -> Option<VisualRowSpan> {
        let visual_line_count = *self.line_visual_counts.get(logical_line)?;
        if visual_line_count == 0 {
            return None;
        }

        Some(VisualRowSpan {
            logical_line,
            start_visual_row: self.tree.prefix_sum(logical_line),
            visual_line_count,
        })
    }

    pub(crate) fn next_span_after_logical_line(
        &self,
        logical_line: usize,
    ) -> Option<VisualRowSpan> {
        let next_visual_row = self.tree.prefix_sum(logical_line.saturating_add(1));
        self.span_for_visual_row(next_visual_row)
            .map(|(span, _)| span)
    }
}

#[derive(Debug, Clone, Default)]
struct FenwickTree {
    tree: Vec<usize>,
}

impl FenwickTree {
    fn from_values(values: &[usize]) -> Self {
        let mut tree = Self {
            tree: vec![0; values.len().saturating_add(1)],
        };
        for (idx, value) in values.iter().copied().enumerate() {
            tree.add(idx, value);
        }
        tree
    }

    fn total(&self) -> usize {
        self.prefix_sum(self.tree.len().saturating_sub(1))
    }

    fn prefix_sum(&self, end_exclusive: usize) -> usize {
        let mut idx = end_exclusive.min(self.tree.len().saturating_sub(1));
        let mut sum = 0usize;
        while idx > 0 {
            sum = sum.saturating_add(self.tree[idx]);
            idx -= Self::lowbit(idx);
        }
        sum
    }

    fn add_delta(&mut self, idx: usize, previous: usize, next: usize) {
        if next >= previous {
            self.add(idx, next - previous);
        } else {
            self.sub(idx, previous - next);
        }
    }

    fn add(&mut self, idx: usize, delta: usize) {
        let mut tree_idx = idx.saturating_add(1);
        while tree_idx < self.tree.len() {
            self.tree[tree_idx] = self.tree[tree_idx].saturating_add(delta);
            tree_idx += Self::lowbit(tree_idx);
        }
    }

    fn sub(&mut self, idx: usize, delta: usize) {
        let mut tree_idx = idx.saturating_add(1);
        while tree_idx < self.tree.len() {
            self.tree[tree_idx] = self.tree[tree_idx].saturating_sub(delta);
            tree_idx += Self::lowbit(tree_idx);
        }
    }

    fn find_line_containing_visual_row(&self, visual_row: usize) -> Option<usize> {
        if visual_row >= self.total() {
            return None;
        }

        let mut idx = 0usize;
        let mut sum = 0usize;
        let mut bit = Self::highest_power_of_two_le(self.tree.len().saturating_sub(1));
        while bit > 0 {
            let next = idx + bit;
            if next < self.tree.len() && sum.saturating_add(self.tree[next]) <= visual_row {
                idx = next;
                sum = sum.saturating_add(self.tree[next]);
            }
            bit >>= 1;
        }

        Some(idx)
    }

    fn highest_power_of_two_le(n: usize) -> usize {
        if n == 0 {
            return 0;
        }
        1usize << (usize::BITS - 1 - n.leading_zeros())
    }

    fn lowbit(value: usize) -> usize {
        value & value.wrapping_neg()
    }
}

#[cfg(test)]
mod tests {
    use super::VisualRowIndex;

    #[test]
    fn maps_visual_rows_with_hidden_lines() {
        let index = VisualRowIndex::from_line_visual_counts(vec![1, 0, 3, 2]);

        assert_eq!(index.total_visual_lines(), 6);
        assert_eq!(index.span_for_visual_row(0).unwrap().0.logical_line, 0);
        assert_eq!(index.span_for_visual_row(1).unwrap().0.logical_line, 2);
        assert_eq!(index.span_for_visual_row(3).unwrap().1, 2);
        assert_eq!(index.span_for_visual_row(4).unwrap().0.logical_line, 3);
        assert!(index.span_for_logical_line(1).is_none());
        assert_eq!(index.span_for_logical_line(2).unwrap().start_visual_row, 1);
    }

    #[test]
    fn updates_line_counts_without_rebuilding_callers() {
        let mut index = VisualRowIndex::from_line_visual_counts(vec![1, 1, 1]);

        assert!(index.set_line_visual_count(1, 4));
        assert_eq!(index.total_visual_lines(), 6);
        assert_eq!(index.span_for_visual_row(4).unwrap().0.logical_line, 1);
        assert_eq!(index.span_for_logical_line(2).unwrap().start_visual_row, 5);

        index.insert_lines(2, [2]);
        assert_eq!(index.logical_line_count(), 4);
        assert_eq!(index.span_for_logical_line(2).unwrap().start_visual_row, 5);
        assert_eq!(index.span_for_logical_line(3).unwrap().start_visual_row, 7);

        assert!(index.remove_lines(1, 1));
        assert_eq!(index.logical_line_count(), 3);
        assert_eq!(index.total_visual_lines(), 4);
        assert_eq!(index.span_for_visual_row(1).unwrap().0.logical_line, 1);
    }
}
