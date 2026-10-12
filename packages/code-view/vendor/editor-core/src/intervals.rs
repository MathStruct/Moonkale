//! Phase 4: Styles and Folding (Intervals & Visibility)
//!
//! Uses Interval Tree to manage style metadata and code folding.

/// Style ID type
pub type StyleId = u32;

/// Built-in style id used for folding placeholder text (e.g. `/*...*/`, `use ...`).
///
/// Consumers should map this to a muted style.
pub const FOLD_PLACEHOLDER_STYLE_ID: StyleId = 0x0300_0001;

/// Built-in style id for LSP `textDocument/documentHighlight` (kind: Text/unspecified).
pub const DOCUMENT_HIGHLIGHT_TEXT_STYLE_ID: StyleId = 0x0400_0001;
/// Built-in style id for LSP `textDocument/documentHighlight` (kind: Read).
pub const DOCUMENT_HIGHLIGHT_READ_STYLE_ID: StyleId = 0x0400_0002;
/// Built-in style id for LSP `textDocument/documentHighlight` (kind: Write).
pub const DOCUMENT_HIGHLIGHT_WRITE_STYLE_ID: StyleId = 0x0400_0003;

/// Built-in style id for IME marked text (composition / preedit).
///
/// This is intended for underline/background styling of inline IME preedit strings.
pub const IME_MARKED_TEXT_STYLE_ID: StyleId = 0x0700_0001;

/// Built-in style id for LSP inlay hints virtual text (`textDocument/inlayHint`).
///
/// Renderers should map this to a muted / secondary foreground, and optionally a subtle background.
pub const INLAY_HINT_STYLE_ID: StyleId = 0x0800_0001;

/// Built-in style id for LSP code lens virtual text (`textDocument/codeLens`).
///
/// Renderers should map this to a muted / link-like style.
pub const CODE_LENS_STYLE_ID: StyleId = 0x0800_0002;

/// Built-in style id for LSP document links (`textDocument/documentLink`).
///
/// Renderers should typically draw this as an underline and/or link-like foreground color.
pub const DOCUMENT_LINK_STYLE_ID: StyleId = 0x0800_0003;

/// Built-in style id for match highlights (e.g. search matches, bracket matches).
pub const MATCH_HIGHLIGHT_STYLE_ID: StyleId = 0x0800_0004;

/// Built-in style id for added lines in headless diff projections.
pub const DIFF_ADD_LINE_STYLE_ID: StyleId = 0x0900_0001;
/// Built-in style id for removed lines in headless diff projections.
pub const DIFF_REMOVE_LINE_STYLE_ID: StyleId = 0x0900_0002;
/// Built-in style id for filler spacer rows in headless diff projections.
pub const DIFF_SPACER_STYLE_ID: StyleId = 0x0900_0003;

/// Style layer ID
///
/// Used to distinguish style sources (e.g., LSP semantic highlighting, simple syntax highlighting, diagnostics, etc.),
/// allowing replacement/clearing of one layer without affecting other style layers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StyleLayerId(pub u32);

impl StyleLayerId {
    /// Create a style layer id from a raw numeric identifier.
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    /// LSP `semanticTokens` style layer (recommended for semantic highlighting).
    pub const SEMANTIC_TOKENS: Self = Self(1);

    /// Simple syntax highlighting style layer (e.g., regex-based JSON/INI highlighting).
    pub const SIMPLE_SYNTAX: Self = Self(2);

    /// Sublime Text `.sublime-syntax` style layer (lightweight syntax highlighting/folding).
    pub const SUBLIME_SYNTAX: Self = Self(3);

    /// LSP diagnostics overlay layer.
    ///
    /// This is intended for underlines / gutter markers sourced from LSP diagnostics.
    pub const DIAGNOSTICS: Self = Self(4);

    /// LSP `textDocument/documentHighlight` overlay layer.
    pub const DOCUMENT_HIGHLIGHTS: Self = Self(5);

    /// Tree-sitter syntax highlighting style layer.
    pub const TREE_SITTER: Self = Self(6);

    /// IME marked text (composition / preedit) overlay layer.
    pub const IME_MARKED_TEXT: Self = Self(7);

    /// LSP document links overlay layer.
    ///
    /// This is intended for underlines / click targets sourced from `textDocument/documentLink`.
    pub const DOCUMENT_LINKS: Self = Self(8);

    /// Match highlights overlay layer (search matches, bracket highlights, etc).
    pub const MATCH_HIGHLIGHTS: Self = Self(9);

    /// Bracket match overlay layer (matching bracket highlights).
    pub const BRACKET_MATCHES: Self = Self(10);
}

/// Interval structure
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Interval {
    /// Start offset (bytes or characters, depending on usage scenario)
    pub start: usize,
    /// End offset (exclusive)
    pub end: usize,
    /// Style ID
    pub style_id: StyleId,
}

/// Text edit delta used to shift style intervals after text changes.
///
/// Coordinates are character offsets in the pre-edit document. A non-zero deletion is applied
/// first as `[start, start + delete_len)`, then `insert_len` characters are inserted at `start`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IntervalTextEdit {
    /// Character offset where the edit starts.
    pub start: usize,
    /// Number of characters deleted from `start`.
    pub delete_len: usize,
    /// Number of characters inserted at `start` after deletion.
    pub insert_len: usize,
}

impl IntervalTextEdit {
    /// Create a text edit delta for interval shifting.
    pub const fn new(start: usize, delete_len: usize, insert_len: usize) -> Self {
        Self {
            start,
            delete_len,
            insert_len,
        }
    }
}

impl Interval {
    /// Create a new interval with `[start, end)` offsets and a style id.
    pub fn new(start: usize, end: usize, style_id: StyleId) -> Self {
        Self {
            start,
            end,
            style_id,
        }
    }

    /// Check if interval contains a specific position
    pub fn contains(&self, pos: usize) -> bool {
        self.start <= pos && pos < self.end
    }

    /// Check if two intervals overlap
    pub fn overlaps(&self, other: &Interval) -> bool {
        self.start < other.end && other.start < self.end
    }
}

/// Interval tree - manages style intervals
///
/// Uses a sorted vector with binary search for efficient interval queries.
/// Query complexity: O(log n + k), where k is the number of overlapping intervals.
/// Insertion complexity: O(n) (requires maintaining sort order).
pub struct IntervalTree {
    /// List of intervals (kept sorted by start position)
    intervals: Vec<Interval>,
    /// Prefix maximum end position: `prefix_max_end[i] = max(intervals[0..=i].end)`
    ///
    /// Used for early pruning in `query_point` / `query_range`, avoiding degradation to O(n) scan when there are many style intervals.
    prefix_max_end: Vec<usize>,
}

impl IntervalTree {
    /// Create an empty interval tree.
    pub fn new() -> Self {
        Self {
            intervals: Vec::new(),
            prefix_max_end: Vec::new(),
        }
    }

    fn rebuild_prefix_max_end_from(&mut self, start_idx: usize) {
        if self.intervals.is_empty() {
            self.prefix_max_end.clear();
            return;
        }

        if self.prefix_max_end.len() != self.intervals.len() {
            self.prefix_max_end.resize(self.intervals.len(), 0);
        }

        let mut max_end = if start_idx == 0 {
            0
        } else {
            self.prefix_max_end[start_idx - 1]
        };

        for (idx, interval) in self.intervals.iter().enumerate().skip(start_idx) {
            max_end = max_end.max(interval.end);
            self.prefix_max_end[idx] = max_end;
        }
    }

    fn rebuild_prefix_max_end(&mut self) {
        self.rebuild_prefix_max_end_from(0);
    }

    fn apply_insertion_to_interval(interval: &mut Interval, pos: usize, delta: usize) {
        if interval.start >= pos {
            interval.start += delta;
            interval.end += delta;
        } else if interval.end > pos {
            // Interval spans insertion point, extend end position.
            interval.end += delta;
        }
    }

    fn apply_deletion_to_interval(interval: &mut Interval, start: usize, end: usize) -> bool {
        if start >= end {
            return true;
        }

        let delta = end - start;
        if interval.end <= start {
            // Interval is before deletion range, unaffected.
            true
        } else if interval.start >= end {
            // Interval is after deletion range, move forward.
            interval.start -= delta;
            interval.end -= delta;
            true
        } else if interval.start >= start && interval.end <= end {
            // Interval is completely within deletion range.
            false
        } else if interval.start < start && interval.end > end {
            // Interval spans deletion range, shrink.
            interval.end -= delta;
            true
        } else if interval.start < start {
            // Interval partially overlaps the deletion range at its end.
            interval.end = start;
            true
        } else {
            // Interval partially overlaps the deletion range at its start.
            interval.start = start;
            interval.end -= delta;
            true
        }
    }

    #[cfg(debug_assertions)]
    fn debug_assert_sorted(&self) {
        debug_assert!(
            self.intervals
                .windows(2)
                .all(|pair| pair[0].start <= pair[1].start),
            "IntervalTree intervals must remain sorted by start offset"
        );
    }

    /// Insert an interval
    pub fn insert(&mut self, interval: Interval) {
        // Find insertion position (maintaining sort order)
        let pos = self
            .intervals
            .binary_search_by_key(&interval.start, |i| i.start)
            .unwrap_or_else(|pos| pos);

        self.intervals.insert(pos, interval);
        self.prefix_max_end.insert(pos, 0);
        self.rebuild_prefix_max_end_from(pos);
    }

    /// Remove interval that exactly matches the specified interval
    pub fn remove(&mut self, start: usize, end: usize, style_id: StyleId) -> bool {
        if let Some(pos) = self
            .intervals
            .iter()
            .position(|i| i.start == start && i.end == end && i.style_id == style_id)
        {
            self.intervals.remove(pos);
            self.prefix_max_end.remove(pos);
            if pos < self.intervals.len() {
                self.rebuild_prefix_max_end_from(pos);
            }
            true
        } else {
            false
        }
    }

    /// Query all intervals containing a specific position
    /// Optimized version: uses binary search to locate interval range that may contain pos
    pub fn query_point(&self, pos: usize) -> Vec<&Interval> {
        self.query_point_impl(pos).0
    }

    fn query_point_impl(&self, pos: usize) -> (Vec<&Interval>, usize) {
        if self.intervals.is_empty() {
            return (Vec::new(), 0);
        }

        let mut result = Vec::new();
        let mut scanned = 0usize;

        // Use binary search to find first position where start > pos
        let search_key = pos.saturating_add(1);
        let idx = match self
            .intervals
            .binary_search_by_key(&search_key, |i| i.start)
        {
            Ok(idx) => idx,
            Err(idx) => idx,
        };

        // Starting from idx-1, check backward for all intervals that may contain pos
        // Because intervals are sorted by start, all intervals with start <= pos are before idx
        for i in (0..idx).rev() {
            scanned = scanned.saturating_add(1);

            // If maximum end of `intervals[0..=i]` is <= pos, earlier intervals cannot contain pos.
            if self.prefix_max_end[i] <= pos {
                break;
            }

            let interval = &self.intervals[i];
            if interval.contains(pos) {
                result.push(interval);
            }
        }

        (result, scanned)
    }

    #[cfg(test)]
    fn query_point_scan_count(&self, pos: usize) -> usize {
        self.query_point_impl(pos).1
    }

    /// Query all intervals overlapping with specified range
    /// Optimized version: uses binary search to locate interval range that may overlap
    pub fn query_range(&self, start: usize, end: usize) -> Vec<&Interval> {
        if self.intervals.is_empty() || start >= end {
            return Vec::new();
        }

        let mut result = Vec::new();

        // Use binary search to find first position where interval.start >= end
        // All intervals that may overlap are before this position
        let search_end = match self.intervals.binary_search_by_key(&end, |i| i.start) {
            Ok(idx) => idx,
            Err(idx) => idx,
        };

        if search_end == 0 {
            return result;
        }

        // First find position where start >= start, then expand backward,
        // until `prefix_max_end` indicates earlier intervals cannot cross start.
        let mut scan_start = match self.intervals.binary_search_by_key(&start, |i| i.start) {
            Ok(idx) | Err(idx) => idx.min(search_end),
        };

        while scan_start > 0 && self.prefix_max_end[scan_start - 1] > start {
            scan_start -= 1;
        }

        for interval in self.intervals[scan_start..search_end].iter() {
            if interval.start < end && interval.end > start {
                result.push(interval);
            }
        }

        result
    }

    /// Clear all intervals
    pub fn clear(&mut self) {
        self.intervals.clear();
        self.prefix_max_end.clear();
    }

    /// Get number of intervals
    pub fn len(&self) -> usize {
        self.intervals.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.intervals.is_empty()
    }

    /// Update offsets (when text changes)
    ///
    /// Call this method to update all intervals when inserting text of `delta` length at position `pos`
    pub fn update_for_insertion(&mut self, pos: usize, delta: usize) {
        if delta == 0 || self.intervals.is_empty() {
            return;
        }

        for interval in &mut self.intervals {
            Self::apply_insertion_to_interval(interval, pos, delta);
        }
        self.rebuild_prefix_max_end();
        #[cfg(debug_assertions)]
        self.debug_assert_sorted();
    }

    /// Update offsets (when text is deleted)
    ///
    /// Call this method to update all intervals when deleting text in range `[start, end)`
    pub fn update_for_deletion(&mut self, start: usize, end: usize) {
        if start >= end || self.intervals.is_empty() {
            return;
        }

        let mut updated = Vec::with_capacity(self.intervals.len());
        for mut interval in self.intervals.drain(..) {
            if Self::apply_deletion_to_interval(&mut interval, start, end) {
                updated.push(interval);
            }
        }
        if !updated
            .windows(2)
            .all(|pair| pair[0].start <= pair[1].start)
        {
            updated.sort_by_key(|interval| interval.start);
        }
        self.intervals = updated;

        self.rebuild_prefix_max_end();
        #[cfg(debug_assertions)]
        self.debug_assert_sorted();
    }

    /// Update offsets for multiple text edits in one tree pass.
    ///
    /// Edits are interpreted in pre-edit coordinates and applied from larger `start` offsets to
    /// smaller offsets, matching the command layer's descending mutation order. Each edit applies
    /// deletion first and insertion second, preserving the single-edit replacement semantics.
    pub fn update_for_text_edits(&mut self, edits: &[IntervalTextEdit]) {
        if self.intervals.is_empty()
            || !edits
                .iter()
                .any(|edit| edit.delete_len > 0 || edit.insert_len > 0)
        {
            return;
        }

        let mut ordered_edits: Vec<IntervalTextEdit> = edits
            .iter()
            .copied()
            .filter(|edit| edit.delete_len > 0 || edit.insert_len > 0)
            .collect();
        ordered_edits.sort_by_key(|edit| std::cmp::Reverse(edit.start));

        let mut updated = Vec::with_capacity(self.intervals.len());
        for mut interval in self.intervals.drain(..) {
            let mut keep = true;
            for edit in &ordered_edits {
                if edit.delete_len > 0 {
                    let end = edit.start.saturating_add(edit.delete_len);
                    keep = Self::apply_deletion_to_interval(&mut interval, edit.start, end);
                    if !keep {
                        break;
                    }
                }

                if edit.insert_len > 0 {
                    Self::apply_insertion_to_interval(&mut interval, edit.start, edit.insert_len);
                }
            }

            if keep {
                updated.push(interval);
            }
        }
        if !updated
            .windows(2)
            .all(|pair| pair[0].start <= pair[1].start)
        {
            updated.sort_by_key(|interval| interval.start);
        }
        self.intervals = updated;

        self.rebuild_prefix_max_end();
        #[cfg(debug_assertions)]
        self.debug_assert_sorted();
    }
}

impl Default for IntervalTree {
    fn default() -> Self {
        Self::new()
    }
}

/// Fold region
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoldRegion {
    /// Start line number
    pub start_line: usize,
    /// End line number (inclusive)
    pub end_line: usize,
    /// Whether folded
    pub is_collapsed: bool,
    /// Placeholder text shown when folded (e.g., "[...]")
    pub placeholder: String,
}

impl FoldRegion {
    /// Create a folding region for an inclusive line range.
    pub fn new(start_line: usize, end_line: usize) -> Self {
        Self {
            start_line,
            end_line,
            is_collapsed: false,
            placeholder: String::from("[...]"),
        }
    }

    /// Create a folding region with a custom placeholder string.
    pub fn with_placeholder(start_line: usize, end_line: usize, placeholder: String) -> Self {
        Self {
            start_line,
            end_line,
            is_collapsed: false,
            placeholder,
        }
    }

    /// Expand
    pub fn expand(&mut self) {
        self.is_collapsed = false;
    }

    /// Collapse
    pub fn collapse(&mut self) {
        self.is_collapsed = true;
    }

    /// Toggle fold state
    pub fn toggle(&mut self) {
        self.is_collapsed = !self.is_collapsed;
    }

    /// Check if line number is within fold region
    pub fn contains_line(&self, line: usize) -> bool {
        line >= self.start_line && line <= self.end_line
    }
}

/// Folding manager
pub struct FoldingManager {
    /// Fold regions sourced from external/derived providers (LSP, sublime syntax, etc.).
    derived_regions: Vec<FoldRegion>,
    /// Fold regions created explicitly by the user (via commands).
    user_regions: Vec<FoldRegion>,
    /// Cached merged view (sorted/deduplicated) used for rendering and coordinate mapping.
    merged_regions: Vec<FoldRegion>,
}

impl FoldingManager {
    /// Create an empty folding manager.
    pub fn new() -> Self {
        Self {
            derived_regions: Vec::new(),
            user_regions: Vec::new(),
            merged_regions: Vec::new(),
        }
    }

    fn rebuild_merged_regions(&mut self) {
        self.merged_regions.clear();
        // User folds should override derived folds when they share the same (start, end) range.
        // This makes toggling/collapsing derived regions from the UI deterministic: we can record
        // the user's collapsed/expanded choice as a user region and have it win in the merged view.
        self.merged_regions
            .extend(self.user_regions.iter().cloned());
        self.merged_regions
            .extend(self.derived_regions.iter().cloned());

        self.merged_regions
            .sort_by_key(|r| (r.start_line, r.end_line));
        self.merged_regions
            .dedup_by(|a, b| a.start_line == b.start_line && a.end_line == b.end_line);
    }

    fn normalize_regions(regions: &mut Vec<FoldRegion>) {
        regions.sort_by_key(|r| (r.start_line, r.end_line));
        regions.dedup_by(|a, b| a.start_line == b.start_line && a.end_line == b.end_line);
        regions.retain(|r| r.end_line > r.start_line);
    }

    fn clamp_regions(regions: &mut Vec<FoldRegion>, max_line: usize) {
        for r in regions.iter_mut() {
            r.start_line = r.start_line.min(max_line);
            r.end_line = r.end_line.min(max_line);
        }
        Self::normalize_regions(regions);
    }

    fn overlapping_line_count(left: &FoldRegion, right: &FoldRegion) -> usize {
        let start = left.start_line.max(right.start_line);
        let end = left.end_line.min(right.end_line);
        if start > end { 0 } else { end - start + 1 }
    }

    fn collapsed_fuzzy_match_score(
        existing: &FoldRegion,
        replacement: &FoldRegion,
    ) -> Option<(usize, usize, usize)> {
        if existing.placeholder != replacement.placeholder {
            return None;
        }

        let start_delta = existing.start_line.abs_diff(replacement.start_line);
        let overlapping_line_count = Self::overlapping_line_count(existing, replacement);
        if start_delta > 1 || overlapping_line_count < 2 {
            return None;
        }

        let existing_len = existing.end_line.saturating_sub(existing.start_line);
        let replacement_len = replacement.end_line.saturating_sub(replacement.start_line);
        let len_delta = existing_len.abs_diff(replacement_len);
        let max_len_delta = 2.max(existing_len.max(replacement_len) / 3);
        if len_delta > max_len_delta {
            return None;
        }

        Some((
            start_delta,
            len_delta,
            existing.end_line.abs_diff(replacement.end_line),
        ))
    }

    fn preserve_collapsed_states(existing: &[FoldRegion], replacements: &mut [FoldRegion]) {
        let collapsed = existing
            .iter()
            .filter(|region| region.is_collapsed)
            .collect::<Vec<_>>();
        if collapsed.is_empty() {
            return;
        }

        let mut used = vec![false; collapsed.len()];

        for replacement in replacements.iter_mut() {
            if let Some(source_idx) = collapsed.iter().enumerate().position(|(idx, existing)| {
                !used[idx]
                    && existing.start_line == replacement.start_line
                    && existing.end_line == replacement.end_line
            }) {
                replacement.is_collapsed = true;
                used[source_idx] = true;
            }
        }

        for replacement in replacements
            .iter_mut()
            .filter(|region| !region.is_collapsed)
        {
            let best = collapsed
                .iter()
                .enumerate()
                .filter_map(|(idx, existing)| {
                    if used[idx] {
                        return None;
                    }
                    Self::collapsed_fuzzy_match_score(existing, replacement)
                        .map(|score| (score, idx))
                })
                .min_by_key(|(score, idx)| (*score, *idx));

            if let Some((_score, source_idx)) = best {
                replacement.is_collapsed = true;
                used[source_idx] = true;
            }
        }
    }

    /// Add a user-created fold region.
    pub fn add_region(&mut self, region: FoldRegion) {
        // Keep sorted by start line.
        let pos = self
            .user_regions
            .binary_search_by_key(&region.start_line, |r| r.start_line)
            .unwrap_or_else(|pos| pos);

        self.user_regions.insert(pos, region);
        Self::normalize_regions(&mut self.user_regions);
        self.rebuild_merged_regions();
    }

    /// Remove a user-created fold region.
    pub fn remove_region(&mut self, start_line: usize, end_line: usize) -> bool {
        if let Some(pos) = self
            .user_regions
            .iter()
            .position(|r| r.start_line == start_line && r.end_line == end_line)
        {
            self.user_regions.remove(pos);
            self.rebuild_merged_regions();
            true
        } else {
            false
        }
    }

    /// Get fold region containing specified line (merged view).
    pub fn get_region_for_line(&self, line: usize) -> Option<&FoldRegion> {
        self.merged_regions.iter().find(|r| r.contains_line(line))
    }

    pub(crate) fn innermost_region_bounds_for_line(&self, line: usize) -> Option<(usize, usize)> {
        self.merged_regions
            .iter()
            .filter(|region| region.contains_line(line))
            .min_by_key(|region| {
                (
                    region.end_line.saturating_sub(region.start_line),
                    region.end_line,
                    region.start_line,
                )
            })
            .map(|region| (region.start_line, region.end_line))
    }

    /// Get mutable reference to a fold region containing specified line (prefers user folds).
    pub fn get_region_for_line_mut(&mut self, line: usize) -> Option<&mut FoldRegion> {
        // Important: a line can be covered by multiple (nested) fold regions.
        //
        // When a user clicks a fold marker on an inner block (nested inside an outer fold),
        // we must act on the *innermost* region for that line. Otherwise, unfolding the
        // inner region can accidentally target the outer region again (because it also
        // "contains" the line), making the inner fold appear "stuck".
        //
        // Choose the smallest region (end-start) among all regions that contain `line`,
        // preferring user regions on ties.
        let mut best: Option<(
            bool,  /*is_user*/
            usize, /*idx*/
            usize, /*len*/
            usize, /*end*/
            usize, /*start*/
        )> = None;

        for (is_user, regions) in [
            (true, self.user_regions.as_slice()),
            (false, self.derived_regions.as_slice()),
        ] {
            for (idx, region) in regions.iter().enumerate() {
                if !region.contains_line(line) {
                    continue;
                }
                let len = region.end_line.saturating_sub(region.start_line);
                let candidate = (is_user, idx, len, region.end_line, region.start_line);
                match best {
                    None => best = Some(candidate),
                    Some((best_is_user, _best_idx, best_len, best_end, best_start)) => {
                        let better_range = (len, region.end_line, region.start_line)
                            < (best_len, best_end, best_start);
                        let prefer_user_tie = (len, region.end_line, region.start_line)
                            == (best_len, best_end, best_start)
                            && is_user
                            && !best_is_user;
                        if better_range || prefer_user_tie {
                            best = Some(candidate);
                        }
                    }
                }
            }
        }

        let (is_user, idx, _len, _end, _start) = best?;

        if is_user {
            self.user_regions.get_mut(idx)
        } else {
            self.derived_regions.get_mut(idx)
        }
    }

    /// Collapse specified line
    pub fn collapse_line(&mut self, line: usize) -> bool {
        if let Some(region) = self.get_region_for_line_mut(line) {
            region.collapse();
            self.rebuild_merged_regions();
            true
        } else {
            false
        }
    }

    /// Expand specified line
    pub fn expand_line(&mut self, line: usize) -> bool {
        if let Some(region) = self.get_region_for_line_mut(line) {
            region.expand();
            self.rebuild_merged_regions();
            true
        } else {
            false
        }
    }

    /// Toggle fold state of specified line
    pub fn toggle_line(&mut self, line: usize) -> bool {
        if let Some(region) = self.get_region_for_line_mut(line) {
            region.toggle();
            self.rebuild_merged_regions();
            true
        } else {
            false
        }
    }

    /// Toggle fold region starting at specified line (preferring "innermost" region).
    ///
    /// `rust-analyzer` / LSP folding ranges often contain nested regions. To make TUI and other frontends
    /// behave more intuitively when "cursor is on a start line", we choose:
    /// - Among all regions with `start_line == line`, the one with smallest `end_line` (innermost)
    pub fn toggle_region_starting_at_line(&mut self, start_line: usize) -> bool {
        if self.merged_regions.is_empty() {
            return false;
        }

        // Find the innermost region among both sources, preferring user folds on ties.
        let mut best_source = None::<(bool, usize)>; // (is_user, index)
        let mut best_end = usize::MAX;

        for (is_user, regions) in [
            (true, &mut self.user_regions),
            (false, &mut self.derived_regions),
        ] {
            let Ok(mut idx) = regions.binary_search_by_key(&start_line, |r| r.start_line) else {
                continue;
            };

            while idx > 0 && regions[idx - 1].start_line == start_line {
                idx -= 1;
            }

            for (relative_i, region) in regions[idx..].iter().enumerate() {
                if region.start_line != start_line {
                    break;
                }
                if region.end_line <= region.start_line {
                    continue;
                }
                // `enumerate()` yields a slice-relative index; convert it back to an absolute
                // index into `regions` so the later `get_mut` targets the correct region.
                let absolute_i = idx + relative_i;
                if region.end_line < best_end
                    || (region.end_line == best_end
                        && best_source.is_some_and(|(prev_is_user, _)| !prev_is_user && is_user))
                {
                    best_end = region.end_line;
                    best_source = Some((is_user, absolute_i));
                }
            }
        }

        let Some((is_user, idx)) = best_source else {
            return false;
        };

        if is_user {
            if let Some(region) = self.user_regions.get_mut(idx) {
                region.toggle();
            }
        } else if let Some(region) = self.derived_regions.get_mut(idx) {
            region.toggle();
        }

        self.rebuild_merged_regions();
        true
    }

    /// Calculate mapping from logical line to visual line
    ///
    /// Returns the visual line number for the specified logical line number, or None if folded
    pub fn logical_to_visual(&self, logical_line: usize, base_visual: usize) -> Option<usize> {
        let mut hidden_lines = 0usize;

        for (hidden_start, hidden_end) in self.collapsed_hidden_ranges() {
            if logical_line >= hidden_start && logical_line < hidden_end {
                return None;
            }
            if logical_line <= hidden_start {
                break;
            }

            hidden_lines = hidden_lines.saturating_add(hidden_end.min(logical_line) - hidden_start);
        }

        Some(base_visual.saturating_add(logical_line.saturating_sub(hidden_lines)))
    }

    /// Calculate mapping from visual line to logical line
    pub fn visual_to_logical(&self, visual_line: usize, base_visual: usize) -> usize {
        let mut logical = visual_line.saturating_sub(base_visual);

        for (hidden_start, hidden_end) in self.collapsed_hidden_ranges() {
            if logical < hidden_start {
                break;
            }
            logical = logical.saturating_add(hidden_end - hidden_start);
        }

        logical
    }

    fn collapsed_hidden_ranges(&self) -> Vec<(usize, usize)> {
        let mut ranges: Vec<(usize, usize)> = Vec::new();

        for region in self
            .merged_regions
            .iter()
            .filter(|region| region.is_collapsed)
        {
            if region.end_line <= region.start_line {
                continue;
            }

            let hidden_start = region.start_line.saturating_add(1);
            let hidden_end = region.end_line.saturating_add(1);
            if hidden_start >= hidden_end {
                continue;
            }

            if let Some((_, last_end)) = ranges.last_mut()
                && hidden_start <= *last_end
            {
                *last_end = (*last_end).max(hidden_end);
                continue;
            }

            ranges.push((hidden_start, hidden_end));
        }

        ranges
    }

    /// Total number of logical lines hidden by collapsed folds.
    ///
    /// Overlapping and nested collapsed regions are counted once (the hidden ranges are merged
    /// first), so the result never exceeds the document's logical line count.
    pub fn collapsed_hidden_line_count(&self) -> usize {
        self.collapsed_hidden_ranges()
            .into_iter()
            .map(|(start, end)| end.saturating_sub(start))
            .sum()
    }

    /// Get all fold regions
    pub fn regions(&self) -> &[FoldRegion] {
        &self.merged_regions
    }

    /// Get all derived fold regions.
    pub fn derived_regions(&self) -> &[FoldRegion] {
        &self.derived_regions
    }

    /// Get all user-created fold regions.
    pub fn user_regions(&self) -> &[FoldRegion] {
        &self.user_regions
    }

    /// Clear all fold regions (derived + user).
    pub fn clear(&mut self) {
        self.derived_regions.clear();
        self.user_regions.clear();
        self.merged_regions.clear();
    }

    /// Clear all derived fold regions, leaving user folds intact.
    pub fn clear_derived_regions(&mut self) {
        self.derived_regions.clear();
        self.rebuild_merged_regions();
    }

    /// Replace derived fold regions (will be sorted by start line and deduplicated).
    pub fn replace_derived_regions(&mut self, mut regions: Vec<FoldRegion>) {
        Self::normalize_regions(&mut regions);
        self.derived_regions = regions;
        self.rebuild_merged_regions();
    }

    /// Replace derived fold regions while preserving collapsed state across small range drift.
    pub fn replace_derived_regions_preserving_collapsed(&mut self, mut regions: Vec<FoldRegion>) {
        Self::normalize_regions(&mut regions);
        Self::preserve_collapsed_states(&self.derived_regions, &mut regions);
        self.derived_regions = regions;
        self.rebuild_merged_regions();
    }

    /// Replace fold regions with new list (legacy API).
    ///
    /// This replaces *derived* fold regions, leaving user folds intact.
    pub fn replace_regions(&mut self, regions: Vec<FoldRegion>) {
        self.replace_derived_regions(regions);
    }

    /// Expand all folds
    pub fn expand_all(&mut self) {
        for region in &mut self.derived_regions {
            region.expand();
        }
        for region in &mut self.user_regions {
            region.expand();
        }
        self.rebuild_merged_regions();
    }

    /// Collapse all regions
    pub fn collapse_all(&mut self) {
        for region in &mut self.derived_regions {
            region.collapse();
        }
        for region in &mut self.user_regions {
            region.collapse();
        }
        self.rebuild_merged_regions();
    }

    /// Update fold regions to account for an edit that changes the number of logical lines.
    ///
    /// This is intended to keep **user folds** stable across newline insertions/deletions.
    ///
    /// - `edit_line` is the logical line where the edit occurred (pre-edit).
    /// - `line_delta` is the net change in line count (`+n` for inserted newlines, `-n` for deleted).
    pub fn apply_line_delta(&mut self, edit_line: usize, line_delta: isize) {
        if line_delta == 0 {
            return;
        }

        let apply = |regions: &mut Vec<FoldRegion>| {
            for region in regions.iter_mut() {
                if edit_line <= region.start_line {
                    let start = region.start_line as isize + line_delta;
                    let end = region.end_line as isize + line_delta;
                    region.start_line = start.max(0) as usize;
                    region.end_line = end.max(0) as usize;
                } else if edit_line <= region.end_line {
                    let end = region.end_line as isize + line_delta;
                    region.end_line = end.max(region.start_line as isize) as usize;
                }
            }
        };

        apply(&mut self.derived_regions);
        apply(&mut self.user_regions);
        self.rebuild_merged_regions();
    }

    /// Clamp fold regions to the given `line_count` after a text edit, dropping invalid regions.
    pub fn clamp_to_line_count(&mut self, line_count: usize) {
        let max_line = line_count.saturating_sub(1);
        Self::clamp_regions(&mut self.derived_regions, max_line);
        Self::clamp_regions(&mut self.user_regions, max_line);
        self.rebuild_merged_regions();
    }
}

impl Default for FoldingManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interval_contains() {
        let interval = Interval::new(10, 20, 1);
        assert!(interval.contains(10));
        assert!(interval.contains(15));
        assert!(interval.contains(19));
        assert!(!interval.contains(20));
        assert!(!interval.contains(9));
    }

    #[test]
    fn test_interval_overlaps() {
        let i1 = Interval::new(10, 20, 1);
        let i2 = Interval::new(15, 25, 2);
        let i3 = Interval::new(25, 30, 3);

        assert!(i1.overlaps(&i2));
        assert!(i2.overlaps(&i1));
        assert!(!i1.overlaps(&i3));
        assert!(!i3.overlaps(&i1));
    }

    #[test]
    fn test_interval_tree_insert() {
        let mut tree = IntervalTree::new();
        tree.insert(Interval::new(10, 20, 1));
        tree.insert(Interval::new(5, 15, 2));
        tree.insert(Interval::new(15, 25, 3));

        assert_eq!(tree.len(), 3);
    }

    #[test]
    fn test_interval_tree_query_point() {
        let mut tree = IntervalTree::new();
        tree.insert(Interval::new(10, 20, 1));
        tree.insert(Interval::new(5, 15, 2));
        tree.insert(Interval::new(15, 25, 3));

        let results = tree.query_point(12);
        assert_eq!(results.len(), 2); // intervals 1 and 2

        let results = tree.query_point(18);
        assert_eq!(results.len(), 2); // intervals 1 and 3
    }

    #[test]
    fn test_interval_tree_query_point_prunes_scan() {
        let mut tree = IntervalTree::new();

        // Construct many non-overlapping intervals: ideally point query should only need to check few candidate intervals.
        for i in 0..10_000usize {
            let start = i * 2;
            tree.insert(Interval::new(start, start + 1, 1));
        }

        let pos = 2 * 10_000 - 2; // Falls within last interval
        let results = tree.query_point(pos);
        assert_eq!(results.len(), 1);

        // After pruning with prefix_max_end, should avoid degrading to linear scan of all intervals.
        assert!(
            tree.query_point_scan_count(pos) <= 4,
            "scan should be pruned for disjoint intervals"
        );
    }

    #[test]
    fn test_interval_tree_query_range() {
        let mut tree = IntervalTree::new();
        tree.insert(Interval::new(10, 20, 1));
        tree.insert(Interval::new(25, 35, 2));
        tree.insert(Interval::new(40, 50, 3));

        let results = tree.query_range(15, 30);
        assert_eq!(results.len(), 2); // intervals 1 and 2

        let results = tree.query_range(0, 60);
        assert_eq!(results.len(), 3); // all intervals
    }

    #[test]
    fn test_interval_tree_update_insertion() {
        let mut tree = IntervalTree::new();
        tree.insert(Interval::new(10, 20, 1));
        tree.insert(Interval::new(30, 40, 2));

        tree.update_for_insertion(15, 5);

        assert_eq!(tree.intervals[0].start, 10);
        assert_eq!(tree.intervals[0].end, 25); // 20 + 5

        assert_eq!(tree.intervals[1].start, 35); // 30 + 5
        assert_eq!(tree.intervals[1].end, 45); // 40 + 5
    }

    #[test]
    fn test_interval_tree_update_deletion() {
        let mut tree = IntervalTree::new();
        tree.insert(Interval::new(10, 20, 1));
        tree.insert(Interval::new(30, 40, 2));
        tree.insert(Interval::new(50, 60, 3));

        tree.update_for_deletion(25, 35);

        assert_eq!(tree.intervals[0].start, 10);
        assert_eq!(tree.intervals[0].end, 20); // Unaffected

        assert_eq!(tree.intervals[1].start, 25); // 30 - (35-25)
        assert_eq!(tree.intervals[1].end, 30); // 40 - (35-25)

        assert_eq!(tree.intervals[2].start, 40); // 50 - 10
        assert_eq!(tree.intervals[2].end, 50); // 60 - 10
    }

    #[test]
    fn test_interval_tree_batch_update_matches_sequential_updates() {
        fn build_tree() -> IntervalTree {
            let mut tree = IntervalTree::new();
            tree.insert(Interval::new(0, 4, 1));
            tree.insert(Interval::new(6, 12, 2));
            tree.insert(Interval::new(15, 25, 3));
            tree.insert(Interval::new(20, 24, 4));
            tree.insert(Interval::new(30, 38, 5));
            tree.insert(Interval::new(38, 45, 6));
            tree
        }

        let edits = vec![
            IntervalTextEdit::new(40, 5, 2),
            IntervalTextEdit::new(18, 10, 0),
            IntervalTextEdit::new(5, 0, 3),
        ];

        let mut sequential = build_tree();
        for edit in &edits {
            if edit.delete_len > 0 {
                sequential.update_for_deletion(edit.start, edit.start + edit.delete_len);
            }
            if edit.insert_len > 0 {
                sequential.update_for_insertion(edit.start, edit.insert_len);
            }
        }

        let mut batched = build_tree();
        batched.update_for_text_edits(&edits);

        assert_eq!(batched.intervals, sequential.intervals);
        assert_eq!(batched.prefix_max_end, sequential.prefix_max_end);
    }

    #[test]
    fn test_interval_tree_batch_update_keeps_queries_correct() {
        let mut tree = IntervalTree::new();
        tree.insert(Interval::new(2, 8, 1));
        tree.insert(Interval::new(10, 18, 2));
        tree.insert(Interval::new(20, 30, 3));

        tree.update_for_text_edits(&[
            IntervalTextEdit::new(12, 4, 1),
            IntervalTextEdit::new(4, 2, 0),
        ]);

        let point_styles: Vec<StyleId> = tree
            .query_point(10)
            .into_iter()
            .map(|interval| interval.style_id)
            .collect();
        assert_eq!(point_styles, vec![2]);

        let range_styles: Vec<StyleId> = tree
            .query_range(0, 32)
            .into_iter()
            .map(|interval| interval.style_id)
            .collect();
        assert_eq!(range_styles, vec![1, 2, 3]);
    }

    #[test]
    fn test_fold_region() {
        let mut region = FoldRegion::new(5, 10);
        assert!(!region.is_collapsed);

        region.collapse();
        assert!(region.is_collapsed);

        region.expand();
        assert!(!region.is_collapsed);

        region.toggle();
        assert!(region.is_collapsed);
    }

    #[test]
    fn test_folding_manager() {
        let mut manager = FoldingManager::new();

        manager.add_region(FoldRegion::new(5, 10));
        manager.add_region(FoldRegion::new(15, 20));

        assert!(manager.collapse_line(7));
        assert!(manager.get_region_for_line(7).unwrap().is_collapsed);

        assert!(manager.expand_line(7));
        assert!(!manager.get_region_for_line(7).unwrap().is_collapsed);
    }

    #[test]
    fn test_nested_folds_can_unfold_inner_after_outer_unfold() {
        let mut manager = FoldingManager::new();

        // Outer region: 1..=10
        let mut outer = FoldRegion::new(1, 10);
        outer.collapse();
        manager.add_region(outer);

        // Inner region: 3..=5 (nested inside outer)
        let mut inner = FoldRegion::new(3, 5);
        inner.collapse();
        manager.add_region(inner);

        // Step 1: inner is folded.
        let inner_before = manager
            .regions()
            .iter()
            .find(|r| r.start_line == 3 && r.end_line == 5)
            .unwrap();
        assert!(inner_before.is_collapsed);

        // Step 2/3: unfold outer.
        assert!(manager.expand_line(1));
        let outer_after = manager
            .regions()
            .iter()
            .find(|r| r.start_line == 1 && r.end_line == 10)
            .unwrap();
        assert!(!outer_after.is_collapsed);

        // Step 4: now we should still be able to unfold the inner region by line 3.
        assert!(manager.expand_line(3));
        let inner_after = manager
            .regions()
            .iter()
            .find(|r| r.start_line == 3 && r.end_line == 5)
            .unwrap();
        assert!(!inner_after.is_collapsed);
    }

    #[test]
    fn test_toggle_region_starting_at_line_targets_correct_region() {
        // Regression: `toggle_region_starting_at_line` used to store a slice-relative index and
        // write it back as an absolute index, collapsing the wrong region when the target was not
        // the first region in the source.
        let mut manager = FoldingManager::new();
        manager.add_region(FoldRegion::new(1, 3));
        manager.add_region(FoldRegion::new(5, 8));
        manager.add_region(FoldRegion::new(10, 15));

        assert!(manager.toggle_region_starting_at_line(10));

        let collapsed: Vec<usize> = manager
            .user_regions()
            .iter()
            .filter(|r| r.is_collapsed)
            .map(|r| r.start_line)
            .collect();
        assert_eq!(
            collapsed,
            vec![10],
            "only the region starting at line 10 should be collapsed"
        );

        // Toggling the middle region must likewise only affect that region.
        assert!(manager.toggle_region_starting_at_line(5));
        let collapsed: Vec<usize> = manager
            .user_regions()
            .iter()
            .filter(|r| r.is_collapsed)
            .map(|r| r.start_line)
            .collect();
        assert_eq!(collapsed, vec![5, 10]);
    }

    #[test]
    fn test_collapsed_hidden_line_count_dedups_nested_regions() {
        // Regression: summing `end_line - start_line` per collapsed region double-counted nested
        // folds and could exceed the document line count (underflowing the visible-line math).
        let mut manager = FoldingManager::new();
        let mut outer = FoldRegion::new(0, 10);
        outer.collapse();
        manager.add_region(outer);
        let mut inner = FoldRegion::new(2, 5);
        inner.collapse();
        manager.add_region(inner);

        // Hidden lines are the union of (1..=10), i.e. 10 lines — the inner region adds nothing.
        assert_eq!(manager.collapsed_hidden_line_count(), 10);
    }

    #[test]
    fn test_logical_to_visual_with_folding() {
        let mut manager = FoldingManager::new();

        let mut region = FoldRegion::new(5, 10);
        region.collapse();
        manager.add_region(region);

        // Line before fold
        assert_eq!(manager.logical_to_visual(3, 0), Some(3));

        // Fold start line
        assert_eq!(manager.logical_to_visual(5, 0), Some(5));

        // Middle line of fold should return None
        assert_eq!(manager.logical_to_visual(7, 0), None);

        // Line after fold should have adjusted position
        assert_eq!(manager.logical_to_visual(15, 0), Some(10)); // 15 - 5 hidden lines
    }

    #[test]
    fn test_multiple_overlapping_styles() {
        let mut tree = IntervalTree::new();

        // Add overlapping style intervals
        tree.insert(Interval::new(0, 100, 1)); // Syntax highlighting
        tree.insert(Interval::new(20, 30, 2)); // Search highlighting
        tree.insert(Interval::new(25, 35, 3)); // Selection region

        // Query position 27, should have 3 styles
        let styles = tree.query_point(27);
        assert_eq!(styles.len(), 3);

        // Verify all styles were found
        let style_ids: Vec<StyleId> = styles.iter().map(|i| i.style_id).collect();
        assert!(style_ids.contains(&1));
        assert!(style_ids.contains(&2));
        assert!(style_ids.contains(&3));
    }
}
