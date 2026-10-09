//! Phase 3: Layout and Soft Wrapping (Headless Layout Engine)
//!
//! Calculates the visual representation of text given a container width.
//! Computes character widths based on UAX #11 and implements headless reflow algorithm.

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthChar;

/// Default tab width (in cells) used when a caller does not specify a tab width.
pub const DEFAULT_TAB_WIDTH: usize = 4;

/// Soft wrapping mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WrapMode {
    /// No soft wrapping (each logical line is a single visual line).
    None,
    /// Wrap at Unicode grapheme boundaries.
    #[default]
    Char,
    /// Prefer wrapping at word boundaries (whitespace), falling back to character wrap.
    Word,
}

/// Wrapped-line indentation policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WrapIndent {
    /// No indentation for wrapped continuations.
    #[default]
    None,
    /// Indent wrapped continuations by the width (in cells) of the logical line's leading
    /// whitespace prefix (spaces + tabs).
    SameAsLineIndent,
    /// Indent wrapped continuations by a fixed number of cells.
    FixedCells(usize),
}

/// Wrap point
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WrapPoint {
    /// Character index where wrapping occurs (within the logical line)
    pub char_index: usize,
    /// Byte offset where wrapping occurs (within the logical line)
    pub byte_offset: usize,
}

/// Visual line information
#[derive(Debug, Clone)]
pub struct VisualLineInfo {
    /// Number of visual lines corresponding to this logical line
    pub visual_line_count: usize,
    /// List of wrap points
    pub wrap_points: Vec<WrapPoint>,
}

impl VisualLineInfo {
    /// Create an empty layout (a single visual line, no wrap points).
    pub fn new() -> Self {
        Self {
            visual_line_count: 1,
            wrap_points: Vec::new(),
        }
    }

    /// Calculate visual line information from text and width constraint
    pub fn from_text(text: &str, viewport_width: usize) -> Self {
        let wrap_points = calculate_wrap_points(text, viewport_width);
        let visual_line_count = wrap_points.len() + 1;

        Self {
            visual_line_count,
            wrap_points,
        }
    }

    /// Calculate visual line information from text and width constraint, with explicit `tab_width`.
    pub fn from_text_with_tab_width(text: &str, viewport_width: usize, tab_width: usize) -> Self {
        let wrap_points = calculate_wrap_points_with_tab_width(text, viewport_width, tab_width);
        let visual_line_count = wrap_points.len() + 1;

        Self {
            visual_line_count,
            wrap_points,
        }
    }

    /// Calculate visual line information with explicit options.
    pub fn from_text_with_options(
        text: &str,
        viewport_width: usize,
        tab_width: usize,
        wrap_mode: WrapMode,
    ) -> Self {
        Self::from_text_with_layout_options(
            text,
            viewport_width,
            tab_width,
            wrap_mode,
            WrapIndent::None,
        )
    }

    /// Calculate visual line information with explicit layout options.
    pub fn from_text_with_layout_options(
        text: &str,
        viewport_width: usize,
        tab_width: usize,
        wrap_mode: WrapMode,
        wrap_indent: WrapIndent,
    ) -> Self {
        let wrap_points = calculate_wrap_points_with_tab_width_mode_and_indent(
            text,
            viewport_width,
            tab_width,
            wrap_mode,
            wrap_indent,
        );
        let visual_line_count = wrap_points.len() + 1;

        Self {
            visual_line_count,
            wrap_points,
        }
    }
}

impl Default for VisualLineInfo {
    fn default() -> Self {
        Self::new()
    }
}

/// Calculate visual width of a character (based on UAX #11)
///
/// Return value:
/// - 1: Narrow character (ASCII, etc.)
/// - 2: Wide character (CJK, fullwidth, etc.)
/// - 0: Zero-width character (combining characters, etc.)
pub fn char_width(ch: char) -> usize {
    // Use unicode-width crate to implement UAX #11
    UnicodeWidthChar::width(ch).unwrap_or(1)
}

/// Calculate visual width (in cells) for a character at a specific cell offset within the line.
///
/// Notes:
/// - For most characters, width follows UAX #11 via [`char_width`].
/// - For `'\t'`, width advances to the next tab stop based on `tab_width`.
pub fn cell_width_at(ch: char, cell_offset_in_line: usize, tab_width: usize) -> usize {
    if ch == '\t' {
        let tab_width = tab_width.max(1);
        let rem = cell_offset_in_line % tab_width;
        tab_width - rem
    } else {
        char_width(ch)
    }
}

/// Calculate total visual width of a string
pub fn str_width(s: &str) -> usize {
    s.chars().map(char_width).sum()
}

/// Calculate total visual width of a string, interpreting `'\t'` using `tab_width`.
pub fn str_width_with_tab_width(s: &str, tab_width: usize) -> usize {
    let mut x = 0usize;
    for ch in s.chars() {
        x = x.saturating_add(cell_width_at(ch, x, tab_width));
    }
    x
}

/// Calculate the visual cell offset from the start of the line to the given character column.
///
/// - `column` is counted in `char` (not bytes).
/// - `'\t'` is expanded using `tab_width` and the current cell offset.
pub fn visual_x_for_column(line: &str, column: usize, tab_width: usize) -> usize {
    let mut x = 0usize;
    for ch in line.chars().take(column) {
        x = x.saturating_add(cell_width_at(ch, x, tab_width));
    }
    x
}

fn leading_whitespace_prefix_slice(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut end = 0usize;
    while end < bytes.len() {
        match bytes[end] {
            b' ' | b'\t' => end += 1,
            _ => break,
        }
    }
    &line[..end]
}

pub(crate) fn wrap_indent_cells_for_line_text(
    line_text: &str,
    wrap_indent: WrapIndent,
    viewport_width: usize,
    tab_width: usize,
) -> usize {
    if viewport_width <= 1 {
        return 0;
    }

    let raw = match wrap_indent {
        WrapIndent::None => 0,
        WrapIndent::FixedCells(n) => n,
        WrapIndent::SameAsLineIndent => {
            let prefix = leading_whitespace_prefix_slice(line_text);
            str_width_with_tab_width(prefix, tab_width)
        }
    };

    raw.min(viewport_width.saturating_sub(1))
}

/// Calculate wrap points for text
///
/// Given a width constraint, calculates where the text needs to wrap
pub fn calculate_wrap_points(text: &str, viewport_width: usize) -> Vec<WrapPoint> {
    calculate_wrap_points_with_tab_width(text, viewport_width, DEFAULT_TAB_WIDTH)
}

/// Calculate wrap points for text, interpreting `'\t'` using `tab_width`.
pub fn calculate_wrap_points_with_tab_width(
    text: &str,
    viewport_width: usize,
    tab_width: usize,
) -> Vec<WrapPoint> {
    calculate_wrap_points_with_tab_width_and_mode(text, viewport_width, tab_width, WrapMode::Char)
}

/// Calculate wrap points for text using a configurable [`WrapMode`].
pub fn calculate_wrap_points_with_tab_width_and_mode(
    text: &str,
    viewport_width: usize,
    tab_width: usize,
    wrap_mode: WrapMode,
) -> Vec<WrapPoint> {
    calculate_wrap_points_with_tab_width_mode_and_indent(
        text,
        viewport_width,
        tab_width,
        wrap_mode,
        WrapIndent::None,
    )
}

/// Calculate wrap points for text using a configurable [`WrapMode`] and [`WrapIndent`].
pub fn calculate_wrap_points_with_tab_width_mode_and_indent(
    text: &str,
    viewport_width: usize,
    tab_width: usize,
    wrap_mode: WrapMode,
    wrap_indent: WrapIndent,
) -> Vec<WrapPoint> {
    if viewport_width == 0 {
        return Vec::new();
    }

    match wrap_mode {
        WrapMode::None => Vec::new(),
        WrapMode::Char => {
            let indent =
                wrap_indent_cells_for_line_text(text, wrap_indent, viewport_width, tab_width);
            calculate_wrap_points_char_with_tab_width(text, viewport_width, tab_width, indent)
        }
        WrapMode::Word => {
            let indent =
                wrap_indent_cells_for_line_text(text, wrap_indent, viewport_width, tab_width);
            calculate_wrap_points_word_with_tab_width(text, viewport_width, tab_width, indent)
        }
    }
}

fn calculate_wrap_points_char_with_tab_width(
    text: &str,
    viewport_width: usize,
    tab_width: usize,
    wrap_indent_cells: usize,
) -> Vec<WrapPoint> {
    let mut wrap_points = Vec::new();
    let mut x_in_segment = 0usize;
    let mut x_in_line = 0usize;
    // Whether the current visual segment already contains at least one character. A wrap is only
    // valid once the current segment is non-empty; otherwise there is nowhere to move the character
    // to (which would spin an oversized-first-char into an empty leading segment).
    let mut segment_has_char = false;

    let mut char_index = 0;
    for (byte_offset, grapheme) in text.grapheme_indices(true) {
        let next_x = grapheme.chars().fold(x_in_line, |x, ch| {
            x.saturating_add(cell_width_at(ch, x, tab_width))
        });
        let ch_width = next_x.saturating_sub(x_in_line);

        // Keep combining, flag and emoji sequences together. An oversized
        // grapheme occupies its own segment rather than creating an empty row.
        if segment_has_char && x_in_segment + ch_width > viewport_width {
            wrap_points.push(WrapPoint {
                char_index,
                byte_offset,
            });
            x_in_segment = wrap_indent_cells;
        }

        x_in_segment = x_in_segment.saturating_add(ch_width);
        x_in_line = x_in_line.saturating_add(ch_width);
        // This character now occupies the (possibly newly started) segment.
        segment_has_char = true;
        char_index += grapheme.chars().count();
    }

    wrap_points
}

fn calculate_wrap_points_word_with_tab_width(
    text: &str,
    viewport_width: usize,
    tab_width: usize,
    wrap_indent_cells: usize,
) -> Vec<WrapPoint> {
    let mut wrap_points = Vec::new();

    let mut segment_start_char = 0usize;
    let mut segment_start_x_in_line = 0usize;
    let mut last_break: Option<(usize, usize, usize)> = None; // (char_index, byte_offset, x_in_line)

    let mut x_in_line = 0usize;

    let mut char_index = 0;
    for (byte_offset, grapheme) in text.grapheme_indices(true) {
        let next_x = grapheme.chars().fold(x_in_line, |x, ch| {
            x.saturating_add(cell_width_at(ch, x, tab_width))
        });
        let ch_width = next_x.saturating_sub(x_in_line);

        loop {
            let segment_indent = if segment_start_char == 0 {
                0
            } else {
                wrap_indent_cells
            };
            let x_in_segment = x_in_line
                .saturating_sub(segment_start_x_in_line)
                .saturating_add(segment_indent);
            if x_in_segment.saturating_add(ch_width) <= viewport_width
                || char_index == segment_start_char
            {
                break;
            }

            if let Some((break_char, break_byte, break_x)) = last_break
                && break_char > segment_start_char
            {
                wrap_points.push(WrapPoint {
                    char_index: break_char,
                    byte_offset: break_byte,
                });
                segment_start_char = break_char;
                segment_start_x_in_line = break_x;
                last_break = None;
                continue;
            }

            // Fallback: wrap before the current whole grapheme.
            wrap_points.push(WrapPoint {
                char_index,
                byte_offset,
            });
            segment_start_char = char_index;
            segment_start_x_in_line = x_in_line;
            last_break = None;
            break;
        }

        x_in_line = x_in_line.saturating_add(ch_width);

        char_index += grapheme.chars().count();
        if grapheme.chars().next().is_some_and(char::is_whitespace) {
            last_break = Some((char_index, byte_offset + grapheme.len(), x_in_line));
        }
    }

    wrap_points
}

/// Layout engine - manages visual representation of all lines
pub struct LayoutEngine {
    /// Viewport width (in character cells)
    viewport_width: usize,
    /// Tab width (in cells) for expanding `'\t'`
    tab_width: usize,
    /// Soft wrapping mode.
    wrap_mode: WrapMode,
    /// Wrapped-line indentation policy.
    wrap_indent: WrapIndent,
    /// Visual information for each logical line
    line_layouts: Vec<VisualLineInfo>,
}

impl LayoutEngine {
    /// Create a new layout engine
    pub fn new(viewport_width: usize) -> Self {
        Self {
            viewport_width,
            tab_width: DEFAULT_TAB_WIDTH,
            wrap_mode: WrapMode::Char,
            wrap_indent: WrapIndent::None,
            line_layouts: Vec::new(),
        }
    }

    /// Set viewport width.
    ///
    /// Callers must reflow existing lines after changing this option.
    pub fn set_viewport_width(&mut self, width: usize) {
        if self.viewport_width != width {
            self.viewport_width = width;
        }
    }

    /// Get viewport width
    pub fn viewport_width(&self) -> usize {
        self.viewport_width
    }

    /// Get wrap mode.
    pub fn wrap_mode(&self) -> WrapMode {
        self.wrap_mode
    }

    /// Set wrap mode.
    ///
    /// Callers must reflow existing lines after changing this option.
    pub fn set_wrap_mode(&mut self, wrap_mode: WrapMode) {
        if self.wrap_mode != wrap_mode {
            self.wrap_mode = wrap_mode;
        }
    }

    /// Get wrapped-line indentation policy.
    pub fn wrap_indent(&self) -> WrapIndent {
        self.wrap_indent
    }

    /// Set wrapped-line indentation policy.
    ///
    /// Callers must reflow existing lines after changing this option.
    pub fn set_wrap_indent(&mut self, wrap_indent: WrapIndent) {
        if self.wrap_indent != wrap_indent {
            self.wrap_indent = wrap_indent;
        }
    }

    /// Get tab width (in cells).
    pub fn tab_width(&self) -> usize {
        self.tab_width
    }

    /// Set tab width (in cells) used for expanding `'\t'`.
    ///
    /// Callers must reflow existing lines after changing this option.
    pub fn set_tab_width(&mut self, tab_width: usize) {
        let tab_width = tab_width.max(1);
        if self.tab_width != tab_width {
            self.tab_width = tab_width;
        }
    }

    /// Build layout from list of text lines
    pub fn from_lines(&mut self, lines: &[&str]) {
        self.recalculate_all_from_lines(lines.iter().copied());
    }

    /// Recalculate layout for all lines using caller-provided text.
    pub fn recalculate_all_from_lines<I, S>(&mut self, lines: I)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.line_layouts.clear();
        for line in lines {
            let line = line.as_ref();
            self.line_layouts
                .push(VisualLineInfo::from_text_with_layout_options(
                    line,
                    self.viewport_width,
                    self.tab_width,
                    self.wrap_mode,
                    self.wrap_indent,
                ));
        }
    }

    /// Add a line
    pub fn add_line(&mut self, text: &str) {
        self.line_layouts
            .push(VisualLineInfo::from_text_with_layout_options(
                text,
                self.viewport_width,
                self.tab_width,
                self.wrap_mode,
                self.wrap_indent,
            ));
    }

    /// Update a specific line
    pub fn update_line(&mut self, line_index: usize, text: &str) {
        if line_index < self.line_layouts.len() {
            self.line_layouts[line_index] = VisualLineInfo::from_text_with_layout_options(
                text,
                self.viewport_width,
                self.tab_width,
                self.wrap_mode,
                self.wrap_indent,
            );
        }
    }

    /// Insert a line
    pub fn insert_line(&mut self, line_index: usize, text: &str) {
        let pos = line_index.min(self.line_layouts.len());
        self.line_layouts.insert(
            pos,
            VisualLineInfo::from_text_with_layout_options(
                text,
                self.viewport_width,
                self.tab_width,
                self.wrap_mode,
                self.wrap_indent,
            ),
        );
    }

    /// Delete a line
    pub fn delete_line(&mut self, line_index: usize) {
        if line_index < self.line_layouts.len() {
            self.line_layouts.remove(line_index);
        }
    }

    /// Get visual information for a specific logical line
    pub fn get_line_layout(&self, line_index: usize) -> Option<&VisualLineInfo> {
        self.line_layouts.get(line_index)
    }

    /// Get total number of logical lines
    pub fn logical_line_count(&self) -> usize {
        self.line_layouts.len()
    }

    /// Get total number of visual lines
    pub fn visual_line_count(&self) -> usize {
        self.line_layouts.iter().map(|l| l.visual_line_count).sum()
    }

    /// Convert logical line number to visual line number
    ///
    /// Returns the line number of the first visual line of this logical line
    pub fn logical_to_visual_line(&self, logical_line: usize) -> usize {
        self.line_layouts
            .iter()
            .take(logical_line)
            .map(|l| l.visual_line_count)
            .sum()
    }

    /// Convert visual line number to logical line number and offset within line
    ///
    /// Returns (logical_line, visual_line_in_logical)
    pub fn visual_to_logical_line(&self, visual_line: usize) -> (usize, usize) {
        let mut cumulative_visual = 0;

        for (logical_idx, layout) in self.line_layouts.iter().enumerate() {
            if cumulative_visual + layout.visual_line_count > visual_line {
                let visual_offset = visual_line - cumulative_visual;
                return (logical_idx, visual_offset);
            }
            cumulative_visual += layout.visual_line_count;
        }

        // If out of range, return the last line
        let last_line = self.line_layouts.len().saturating_sub(1);
        let last_visual_offset = self
            .line_layouts
            .last()
            .map(|l| l.visual_line_count.saturating_sub(1))
            .unwrap_or(0);
        (last_line, last_visual_offset)
    }

    /// Clear all lines
    pub fn clear(&mut self) {
        self.line_layouts.clear();
    }

    /// Convert logical coordinates (line, column) to visual coordinates (visual row number, x cell offset within row).
    ///
    /// - `logical_line`: Logical line number (0-based)
    /// - `column`: Character column within the logical line (0-based, counted by `char`)
    /// - `line_text`: Text for `logical_line`, supplied by the caller's canonical text store.
    ///
    /// Return value:
    /// - `Some((visual_row, x))`: `visual_row` is the global visual row number, `x` is the cell offset within that visual row
    /// - `None`: Line number out of range
    pub fn logical_position_to_visual(
        &self,
        logical_line: usize,
        column: usize,
        line_text: &str,
    ) -> Option<(usize, usize)> {
        let layout = self.get_line_layout(logical_line)?;

        let line_char_len = line_text.chars().count();
        let column = column.min(line_char_len);

        // Calculate which visual line the cursor belongs to (within this logical line) and the starting character index of that visual line.
        let mut wrapped_offset = 0usize;
        let mut segment_start_col = 0usize;

        // The char_index in wrap_points indicates "where the next segment starts".
        for wrap_point in &layout.wrap_points {
            if column >= wrap_point.char_index {
                wrapped_offset += 1;
                segment_start_col = wrap_point.char_index;
            } else {
                break;
            }
        }

        // Calculate visual width from segment start to column, with tab expansion.
        let seg_start_x_in_line = visual_x_for_column(line_text, segment_start_col, self.tab_width);
        let mut x_in_line = seg_start_x_in_line;
        let mut x_in_segment = 0usize;
        for ch in line_text
            .chars()
            .skip(segment_start_col)
            .take(column.saturating_sub(segment_start_col))
        {
            let w = cell_width_at(ch, x_in_line, self.tab_width);
            x_in_line = x_in_line.saturating_add(w);
            x_in_segment = x_in_segment.saturating_add(w);
        }

        let indent = if wrapped_offset == 0 {
            0
        } else {
            wrap_indent_cells_for_line_text(
                line_text,
                self.wrap_indent,
                self.viewport_width,
                self.tab_width,
            )
        };

        let visual_row = self.logical_to_visual_line(logical_line) + wrapped_offset;
        Some((visual_row, indent.saturating_add(x_in_segment)))
    }

    /// Convert logical coordinates (line, column) to visual coordinates, allowing column to exceed line end (virtual spaces).
    ///
    /// Difference from [`logical_position_to_visual`](Self::logical_position_to_visual):
    /// - `column` is not clamped to `line_char_len`
    /// - Excess portion is treated as virtual spaces of `' '` (width=1)
    /// - `line_text` comes from the caller's canonical text store.
    pub fn logical_position_to_visual_allow_virtual(
        &self,
        logical_line: usize,
        column: usize,
        line_text: &str,
    ) -> Option<(usize, usize)> {
        let layout = self.get_line_layout(logical_line)?;

        let line_char_len = line_text.chars().count();
        let clamped_column = column.min(line_char_len);

        let mut wrapped_offset = 0usize;
        let mut segment_start_col = 0usize;
        for wrap_point in &layout.wrap_points {
            if clamped_column >= wrap_point.char_index {
                wrapped_offset += 1;
                segment_start_col = wrap_point.char_index;
            } else {
                break;
            }
        }

        let seg_start_x_in_line = visual_x_for_column(line_text, segment_start_col, self.tab_width);
        let mut x_in_line = seg_start_x_in_line;
        let mut x_in_segment = 0usize;
        for ch in line_text
            .chars()
            .skip(segment_start_col)
            .take(clamped_column.saturating_sub(segment_start_col))
        {
            let w = cell_width_at(ch, x_in_line, self.tab_width);
            x_in_line = x_in_line.saturating_add(w);
            x_in_segment = x_in_segment.saturating_add(w);
        }

        let indent = if wrapped_offset == 0 {
            0
        } else {
            wrap_indent_cells_for_line_text(
                line_text,
                self.wrap_indent,
                self.viewport_width,
                self.tab_width,
            )
        };

        let x_in_segment = x_in_segment + column.saturating_sub(line_char_len);
        let visual_row = self.logical_to_visual_line(logical_line) + wrapped_offset;
        Some((visual_row, indent.saturating_add(x_in_segment)))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn wraps_preserve_complex_graphemes_even_when_wider_than_the_viewport() {
        use super::*;
        let text = "Wi é 👩🏽‍💻 🇩🇪 한글 नमस्ते 中\tend";
        let boundaries = text
            .grapheme_indices(true)
            .map(|(byte, _)| byte)
            .collect::<Vec<_>>();
        for mode in [WrapMode::Char, WrapMode::Word] {
            for width in 1..=24 {
                let points = calculate_wrap_points_with_tab_width_mode_and_indent(
                    text,
                    width,
                    4,
                    mode,
                    WrapIndent::FixedCells(2),
                );
                for point in &points {
                    assert!(point.byte_offset > 0);
                    assert!(
                        boundaries.contains(&point.byte_offset),
                        "{mode:?} width {width}: {point:?}"
                    );
                    assert_eq!(point.char_index, text[..point.byte_offset].chars().count());
                }
                assert!(
                    points
                        .windows(2)
                        .all(|pair| pair[0].byte_offset < pair[1].byte_offset)
                );
            }
        }
    }
    use super::*;

    #[test]
    fn test_char_width() {
        // ASCII characters should have width 1
        assert_eq!(char_width('a'), 1);
        assert_eq!(char_width('A'), 1);
        assert_eq!(char_width(' '), 1);

        // CJK characters should have width 2
        assert_eq!(char_width('你'), 2);
        assert_eq!(char_width('好'), 2);
        assert_eq!(char_width('世'), 2);
        assert_eq!(char_width('界'), 2);

        // Most emojis have width 2
        assert_eq!(char_width('👋'), 2);
        assert_eq!(char_width('🌍'), 2);
        assert_eq!(char_width('🦀'), 2);
    }

    #[test]
    fn test_str_width() {
        assert_eq!(str_width("hello"), 5);
        assert_eq!(str_width("你好"), 4); // 2 CJK characters = 4 cells
        assert_eq!(str_width("hello你好"), 9); // 5 + 4
        assert_eq!(str_width("👋🌍"), 4); // 2 emojis = 4 cells
    }

    #[test]
    fn test_tab_width_expansion() {
        // tab stops every 4 cells.
        assert_eq!(cell_width_at('\t', 0, 4), 4);
        assert_eq!(cell_width_at('\t', 1, 4), 3);
        assert_eq!(cell_width_at('\t', 2, 4), 2);
        assert_eq!(cell_width_at('\t', 3, 4), 1);
        assert_eq!(cell_width_at('\t', 4, 4), 4);

        assert_eq!(str_width_with_tab_width("\t", 4), 4);
        assert_eq!(str_width_with_tab_width("a\t", 4), 4); // "a" (1) then tab to 4
        assert_eq!(str_width_with_tab_width("ab\t", 4), 4); // 2 + 2
        assert_eq!(str_width_with_tab_width("abc\t", 4), 4); // 3 + 1
        assert_eq!(str_width_with_tab_width("abcd\t", 4), 8); // 4 + 4
    }

    #[test]
    fn test_calculate_wrap_points_simple() {
        // Viewport width of 10
        let text = "hello world";
        let wraps = calculate_wrap_points(text, 10);

        // "hello world" = 11 characters, should wrap between "hello" and "world"
        // But actually wraps after the 10th character
        assert!(!wraps.is_empty());
    }

    #[test]
    fn test_calculate_wrap_points_exact_fit() {
        // Exactly 10 characters wide
        let text = "1234567890";
        let wraps = calculate_wrap_points(text, 10);

        // Exactly fills, no wrapping needed
        assert_eq!(wraps.len(), 0);
    }

    #[test]
    fn test_calculate_wrap_points_one_over() {
        // 11 characters, width of 10
        let text = "12345678901";
        let wraps = calculate_wrap_points(text, 10);

        // Should wrap after the 10th character
        assert_eq!(wraps.len(), 1);
        assert_eq!(wraps[0].char_index, 10);
    }

    #[test]
    fn test_calculate_wrap_points_cjk() {
        // 5 CJK characters = 10 cells wide
        let text = "你好世界测";
        let wraps = calculate_wrap_points(text, 10);

        // Exactly fills, no wrapping needed
        assert_eq!(wraps.len(), 0);
    }

    #[test]
    fn test_calculate_wrap_points_cjk_overflow() {
        // 6 CJK characters = 12 cells, viewport width of 10
        let text = "你好世界测试";
        let wraps = calculate_wrap_points(text, 10);

        // Should wrap after the 5th character (first 5 characters = 10 cells)
        assert_eq!(wraps.len(), 1);
        assert_eq!(wraps[0].char_index, 5);
    }

    #[test]
    fn test_wrap_mode_none_disables_wrapping() {
        let mut engine = LayoutEngine::new(5);
        engine.set_wrap_mode(WrapMode::None);
        engine.from_lines(&["abcdefghij"]);

        assert_eq!(engine.visual_line_count(), 1);
        let layout = engine.get_line_layout(0).expect("layout");
        assert_eq!(layout.visual_line_count, 1);
        assert!(layout.wrap_points.is_empty());
    }

    #[test]
    fn test_word_wrap_prefers_whitespace_when_possible() {
        // With width=7, char-wrap would wrap as "hello w" + "orld".
        // Word-wrap should prefer wrapping at the whitespace boundary ("hello " + "world").
        let text = "hello world";

        let wraps = calculate_wrap_points_with_tab_width_and_mode(
            text,
            7,
            DEFAULT_TAB_WIDTH,
            WrapMode::Word,
        );

        assert_eq!(wraps.len(), 1);
        assert_eq!(wraps[0].char_index, 6);
    }

    #[test]
    fn test_wrap_indent_same_as_line_indent_reduces_continuation_width() {
        let text = "    abcdefgh";
        let wraps = calculate_wrap_points_with_tab_width_mode_and_indent(
            text,
            6,
            DEFAULT_TAB_WIDTH,
            WrapMode::Char,
            WrapIndent::SameAsLineIndent,
        );

        let indices: Vec<usize> = wraps.iter().map(|wp| wp.char_index).collect();
        assert_eq!(indices, vec![6, 8, 10]);
    }

    #[test]
    fn test_wrap_double_width_char() {
        // Viewport has 1 cell remaining, next is a double-width character
        // "Hello" = 5 cells, "你" = 2 cells, viewport width = 6
        let text = "Hello你";
        let wraps = calculate_wrap_points(text, 6);

        // "Hello" takes 5 cells, "你" needs 2 cells but only 1 remains
        // So "你" should wrap intact to the next line
        assert_eq!(wraps.len(), 1);
        assert_eq!(wraps[0].char_index, 5); // Wrap before "你"
    }

    #[test]
    fn test_zero_width_combining_mark_stays_with_base_char() {
        // Regression (P3-18): a zero-width combining mark following the character that exactly
        // fills the viewport must not be split onto its own visual line.
        // "abcde" = 5 cells fills width 5; U+0301 (combining acute) has width 0 and must stay on
        // the same visual line, so there is no wrap point.
        let text = "abcde\u{0301}";
        let wraps = calculate_wrap_points(text, 5);
        assert_eq!(wraps.len(), 0, "combining mark must not force a wrap");

        // A following real character does wrap normally.
        let text2 = "abcde\u{0301}f";
        let wraps2 = calculate_wrap_points(text2, 5);
        assert_eq!(wraps2.len(), 1);
        assert_eq!(
            wraps2[0].char_index, 6,
            "wrap before 'f', after the combining mark"
        );
    }

    #[test]
    fn test_oversized_first_char_does_not_create_empty_leading_segment() {
        // Regression (P3-20): a character wider than the viewport must occupy the first segment
        // rather than being wrapped to a new line (which would leave an empty leading segment and
        // inflate the visual line count).
        // "你" = 2 cells, viewport width = 1.
        let single = calculate_wrap_points("你", 1);
        assert_eq!(single.len(), 0, "a lone oversized char has no wrap point");
        assert_eq!(VisualLineInfo::from_text("你", 1).visual_line_count, 1);

        // Two oversized chars: the second wraps after the first (one wrap point), no empty
        // leading segment.
        let two = calculate_wrap_points("你好", 1);
        assert_eq!(two.len(), 1);
        assert_eq!(two[0].char_index, 1);
    }

    #[test]
    fn test_exact_fit_does_not_emit_trailing_empty_wrap() {
        // Regression (P2-17 family): a line that exactly fills the viewport with no following
        // character must not emit a spurious wrap point / empty trailing visual line.
        assert_eq!(calculate_wrap_points("12345", 5).len(), 0);
        assert_eq!(VisualLineInfo::from_text("12345", 5).visual_line_count, 1);
    }

    #[test]
    fn test_visual_line_info() {
        let info = VisualLineInfo::from_text("1234567890abc", 10);
        assert_eq!(info.visual_line_count, 2); // Needs 2 visual lines
        assert_eq!(info.wrap_points.len(), 1);
    }

    #[test]
    fn test_layout_engine_basic() {
        let mut engine = LayoutEngine::new(10);
        engine.add_line("hello");
        engine.add_line("1234567890abc");

        assert_eq!(engine.logical_line_count(), 2);
        assert_eq!(engine.visual_line_count(), 3); // 1 + 2
    }

    #[test]
    fn test_layout_engine_viewport_change() {
        let mut engine = LayoutEngine::new(20);
        engine.from_lines(&["hello world", "rust programming"]);

        let initial_visual = engine.visual_line_count();
        assert_eq!(initial_visual, 2); // Both lines don't need wrapping

        // Reduce viewport width
        engine.set_viewport_width(5);
        // Note: Due to our implementation, need to reset lines
        engine.from_lines(&["hello world", "rust programming"]);

        let new_visual = engine.visual_line_count();
        assert!(new_visual > initial_visual); // Should have more visual lines
    }

    #[test]
    fn test_logical_to_visual() {
        let mut engine = LayoutEngine::new(10);
        engine.from_lines(&["12345", "1234567890abc", "hello"]);

        // Line 0 ("12345") doesn't wrap, starts at visual line 0
        assert_eq!(engine.logical_to_visual_line(0), 0);

        // Line 1 ("1234567890abc") needs wrapping, starts at visual line 1
        assert_eq!(engine.logical_to_visual_line(1), 1);

        // Line 2 ("hello") starts at visual line 3 (0 + 1 + 2)
        assert_eq!(engine.logical_to_visual_line(2), 3);
    }

    #[test]
    fn test_visual_to_logical() {
        let mut engine = LayoutEngine::new(10);
        engine.from_lines(&["12345", "1234567890abc", "hello"]);

        // Visual line 0 -> logical line 0
        assert_eq!(engine.visual_to_logical_line(0), (0, 0));

        // Visual line 1 -> logical line 1's 0th visual line
        assert_eq!(engine.visual_to_logical_line(1), (1, 0));

        // Visual line 2 -> logical line 1's 1st visual line
        assert_eq!(engine.visual_to_logical_line(2), (1, 1));

        // Visual line 3 -> logical line 2
        assert_eq!(engine.visual_to_logical_line(3), (2, 0));
    }
}
