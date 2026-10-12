//! Viewport and render-grid snapshot helpers.

use super::*;

impl EditorCore {
    /// Get styled headless grid snapshot (by visual line).
    ///
    /// - Supportsoft wrapping (based `layout_engine`)
    /// - `Cell.styles` will `interval_tree` + `style_layers` merged from
    /// - Supportcode folding (based `folding_manager`)
    ///
    /// Note: This API is not responsible for mapping `StyleId` to specific colors.
    pub fn get_headless_grid_styled(&self, start_visual_row: usize, count: usize) -> HeadlessGrid {
        self.get_headless_grid_styled_window(start_visual_row, count, None)
    }

    /// Render a bounded horizontal cell window for unwrapped lines. Hosts use
    /// this when wrap mode is None. Source offsets describe only
    /// the returned cells; `segment_x_start_cells` is their document x position.
    pub fn get_headless_grid_styled_window(
        &self,
        start_visual_row: usize,
        count: usize,
        window: Option<(usize, usize)>,
    ) -> HeadlessGrid {
        self.with_visual_row_index(|index| {
            let mut grid = HeadlessGrid::new(start_visual_row, count);
            if count == 0 {
                return grid;
            }

            let total_visual = index.total_visual_lines();
            if start_visual_row >= total_visual {
                return grid;
            }

            let tab_width = self.layout_engine.tab_width();
            let end_visual = start_visual_row.saturating_add(count).min(total_visual);
            let regions = self.folding_manager.regions();

            let Some((mut span, mut visual_in_line)) = index.span_for_visual_row(start_visual_row)
            else {
                return grid;
            };
            let mut current_visual = start_visual_row;

            while current_visual < end_visual {
                let logical_line = span.logical_line;

                let Some(layout) = self.layout_engine.get_line_layout(logical_line) else {
                    let remaining_in_span = span.visual_line_count.saturating_sub(visual_in_line);
                    current_visual = current_visual.saturating_add(remaining_in_span);
                    let Some(next_span) = index.next_span_after_logical_line(logical_line) else {
                        break;
                    };
                    span = next_span;
                    visual_in_line = 0;
                    continue;
                };

                let line_text = self
                    .line_index
                    .get_line_text(logical_line)
                    .unwrap_or_default();
                let line_char_len = line_text.chars().count();
                let line_start_offset = self.line_index.position_to_char_offset(logical_line, 0);

                let segment_start_col = if visual_in_line == 0 {
                    0
                } else {
                    layout
                        .wrap_points
                        .get(visual_in_line - 1)
                        .map(|wp| wp.char_index)
                        .unwrap_or(0)
                        .min(line_char_len)
                };

                let segment_end_col = if visual_in_line < layout.wrap_points.len() {
                    layout.wrap_points[visual_in_line]
                        .char_index
                        .min(line_char_len)
                } else {
                    line_char_len
                };

                let mut headless_line = HeadlessLine::new(logical_line, visual_in_line > 0);
                let mut segment_x_start_cells = 0usize;
                if visual_in_line > 0 {
                    let indent_cells = wrap_indent_cells_for_line_text(
                        &line_text,
                        self.layout_engine.wrap_indent(),
                        self.viewport_width,
                        tab_width,
                    );
                    segment_x_start_cells = indent_cells;
                    for _ in 0..indent_cells {
                        headless_line.add_cell(Cell::new(' ', 1));
                    }
                }
                let mut x_in_line = visual_x_for_column(&line_text, segment_start_col, tab_width);

                let mut rendered_start = segment_start_col;
                let mut rendered_end = segment_start_col;
                for (col, ch) in line_text
                    .chars()
                    .enumerate()
                    .skip(segment_start_col)
                    .take(segment_end_col.saturating_sub(segment_start_col))
                {
                    let w = cell_width_at(ch, x_in_line, tab_width);
                    if let Some((left, width)) = window {
                        if x_in_line.saturating_add(w) <= left {
                            x_in_line = x_in_line.saturating_add(w);
                            rendered_start = col + 1;
                            rendered_end = col + 1;
                            segment_x_start_cells = x_in_line;
                            continue;
                        }
                        if x_in_line >= left.saturating_add(width) {
                            break;
                        }
                    }
                    let offset = line_start_offset + col;
                    let styles = self.styles_at_offset(offset);
                    rendered_end = col + 1;
                    x_in_line = x_in_line.saturating_add(w);
                    headless_line.add_cell(Cell::with_styles(ch, w, styles));
                }

                headless_line.set_visual_metadata(
                    visual_in_line,
                    line_start_offset.saturating_add(rendered_start),
                    line_start_offset.saturating_add(rendered_end),
                    segment_x_start_cells,
                );
                headless_line.set_fold_placeholder_appended(false);

                // For collapsed folding start line, append placeholder to the last segment.
                if rendered_end == segment_end_col
                    && visual_in_line + 1 == layout.visual_line_count
                    && let Some(region) = Self::collapsed_region_starting_at(regions, logical_line)
                    && !region.placeholder.is_empty()
                {
                    if !headless_line.cells.is_empty() {
                        x_in_line = x_in_line.saturating_add(char_width(' '));
                        headless_line.add_cell(Cell::with_styles(
                            ' ',
                            char_width(' '),
                            vec![FOLD_PLACEHOLDER_STYLE_ID],
                        ));
                    }
                    for ch in region.placeholder.chars() {
                        let w = cell_width_at(ch, x_in_line, tab_width);
                        x_in_line = x_in_line.saturating_add(w);
                        headless_line.add_cell(Cell::with_styles(
                            ch,
                            w,
                            vec![FOLD_PLACEHOLDER_STYLE_ID],
                        ));
                    }
                    if let Some(right_bracket) = self.fold_right_boundary_bracket_char(region) {
                        x_in_line = x_in_line.saturating_add(char_width(' '));
                        headless_line.add_cell(Cell::with_styles(
                            ' ',
                            char_width(' '),
                            vec![FOLD_PLACEHOLDER_STYLE_ID],
                        ));

                        let w = cell_width_at(right_bracket, x_in_line, tab_width);
                        x_in_line = x_in_line.saturating_add(w);
                        headless_line.add_cell(Cell::with_styles(
                            right_bracket,
                            w,
                            vec![FOLD_PLACEHOLDER_STYLE_ID],
                        ));
                    }
                    headless_line.set_fold_placeholder_appended(true);
                }

                grid.add_line(headless_line);
                current_visual = current_visual.saturating_add(1);
                visual_in_line = visual_in_line.saturating_add(1);
                if visual_in_line >= span.visual_line_count {
                    let Some(next_span) = index.next_span_after_logical_line(logical_line) else {
                        break;
                    };
                    span = next_span;
                    visual_in_line = 0;
                }
            }

            grid
        })
    }

    /// Get a lightweight minimap snapshot (by visual line).
    ///
    /// Compared with [`Self::get_headless_grid_styled`], this API returns aggregated per-line
    /// summaries instead of per-character cells, which is intended for minimap-like overviews.
    pub fn get_minimap_grid(&self, start_visual_row: usize, count: usize) -> MinimapGrid {
        self.with_visual_row_index(|index| {
            let mut grid = MinimapGrid::new(start_visual_row, count);
            if count == 0 {
                return grid;
            }

            let total_visual = index.total_visual_lines();
            if start_visual_row >= total_visual {
                return grid;
            }

            let tab_width = self.layout_engine.tab_width();
            let end_visual = start_visual_row.saturating_add(count).min(total_visual);
            let regions = self.folding_manager.regions();

            let Some((mut span, mut visual_in_line)) = index.span_for_visual_row(start_visual_row)
            else {
                return grid;
            };
            let mut current_visual = start_visual_row;

            while current_visual < end_visual {
                let logical_line = span.logical_line;

                let Some(layout) = self.layout_engine.get_line_layout(logical_line) else {
                    let remaining_in_span = span.visual_line_count.saturating_sub(visual_in_line);
                    current_visual = current_visual.saturating_add(remaining_in_span);
                    let Some(next_span) = index.next_span_after_logical_line(logical_line) else {
                        break;
                    };
                    span = next_span;
                    visual_in_line = 0;
                    continue;
                };

                let line_text = self
                    .line_index
                    .get_line_text(logical_line)
                    .unwrap_or_default();
                let line_char_len = line_text.chars().count();
                let line_start_offset = self.line_index.position_to_char_offset(logical_line, 0);

                let segment_start_col = if visual_in_line == 0 {
                    0
                } else {
                    layout
                        .wrap_points
                        .get(visual_in_line - 1)
                        .map(|wp| wp.char_index)
                        .unwrap_or(0)
                        .min(line_char_len)
                };

                let segment_end_col = if visual_in_line < layout.wrap_points.len() {
                    layout.wrap_points[visual_in_line]
                        .char_index
                        .min(line_char_len)
                } else {
                    line_char_len
                };

                let mut total_cells = 0usize;
                let mut non_whitespace_cells = 0usize;
                let mut dominant_style_counts: HashMap<StyleId, usize> = HashMap::new();
                if visual_in_line > 0 {
                    let indent_cells = wrap_indent_cells_for_line_text(
                        &line_text,
                        self.layout_engine.wrap_indent(),
                        self.viewport_width,
                        tab_width,
                    );
                    total_cells = total_cells.saturating_add(indent_cells);
                }
                let mut x_in_line = visual_x_for_column(&line_text, segment_start_col, tab_width);

                for (col, ch) in line_text
                    .chars()
                    .enumerate()
                    .skip(segment_start_col)
                    .take(segment_end_col.saturating_sub(segment_start_col))
                {
                    let offset = line_start_offset + col;
                    let styles = self.styles_at_offset(offset);
                    let w = cell_width_at(ch, x_in_line, tab_width);
                    x_in_line = x_in_line.saturating_add(w);
                    total_cells = total_cells.saturating_add(w);
                    if !ch.is_whitespace() {
                        non_whitespace_cells = non_whitespace_cells.saturating_add(w);
                    }
                    if let Some(style) = styles.first().copied() {
                        let entry = dominant_style_counts.entry(style).or_insert(0);
                        *entry = entry.saturating_add(w);
                    }
                }

                let mut placeholder_appended = false;
                if visual_in_line + 1 == layout.visual_line_count
                    && let Some(region) = Self::collapsed_region_starting_at(regions, logical_line)
                    && !region.placeholder.is_empty()
                {
                    placeholder_appended = true;
                    if total_cells > 0 {
                        x_in_line = x_in_line.saturating_add(char_width(' '));
                        total_cells = total_cells.saturating_add(char_width(' '));
                    }
                    for ch in region.placeholder.chars() {
                        let w = cell_width_at(ch, x_in_line, tab_width);
                        x_in_line = x_in_line.saturating_add(w);
                        total_cells = total_cells.saturating_add(w);
                        if !ch.is_whitespace() {
                            non_whitespace_cells = non_whitespace_cells.saturating_add(w);
                        }
                        let entry = dominant_style_counts
                            .entry(FOLD_PLACEHOLDER_STYLE_ID)
                            .or_insert(0);
                        *entry = entry.saturating_add(w);
                    }
                    if let Some(right_bracket) = self.fold_right_boundary_bracket_char(region) {
                        x_in_line = x_in_line.saturating_add(char_width(' '));
                        total_cells = total_cells.saturating_add(char_width(' '));
                        let entry = dominant_style_counts
                            .entry(FOLD_PLACEHOLDER_STYLE_ID)
                            .or_insert(0);
                        *entry = entry.saturating_add(char_width(' '));

                        let w = cell_width_at(right_bracket, x_in_line, tab_width);
                        x_in_line = x_in_line.saturating_add(w);
                        total_cells = total_cells.saturating_add(w);
                        non_whitespace_cells = non_whitespace_cells.saturating_add(w);
                        let entry = dominant_style_counts
                            .entry(FOLD_PLACEHOLDER_STYLE_ID)
                            .or_insert(0);
                        *entry = entry.saturating_add(w);
                    }
                }

                let dominant_style = dominant_style_counts
                    .into_iter()
                    .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0)))
                    .map(|(style, _)| style);

                grid.lines.push(MinimapLine {
                    logical_line_index: logical_line,
                    visual_in_logical: visual_in_line,
                    char_offset_start: line_start_offset.saturating_add(segment_start_col),
                    char_offset_end: line_start_offset.saturating_add(segment_end_col),
                    total_cells,
                    non_whitespace_cells,
                    dominant_style,
                    is_fold_placeholder_appended: placeholder_appended,
                });

                current_visual = current_visual.saturating_add(1);
                visual_in_line = visual_in_line.saturating_add(1);
                if visual_in_line >= span.visual_line_count {
                    let Some(next_span) = index.next_span_after_logical_line(logical_line) else {
                        break;
                    };
                    span = next_span;
                    visual_in_line = 0;
                }
            }

            grid
        })
    }

    /// Get a decoration-aware composed grid snapshot (by composed visual line).
    ///
    /// This is an **optional** snapshot path that injects:
    /// - inline virtual text (`DecorationPlacement::{Before,After}`), e.g. inlay hints
    /// - above-line virtual text (`DecorationPlacement::AboveLine`), e.g. code lens
    ///
    /// Notes:
    /// - Wrapping is still computed from the underlying document text only.
    /// - Virtual text can therefore extend past the viewport width; hosts may clip.
    /// - Each [`ComposedCell`] carries its origin (`Document` vs `Virtual`) so hosts can map
    ///   interactions back to document offsets without re-implementing layout.
    pub fn get_headless_grid_composed(
        &self,
        start_visual_row: usize,
        count: usize,
    ) -> ComposedGrid {
        let mut grid = ComposedGrid::new(start_visual_row, count);
        if count == 0 {
            return grid;
        }

        #[derive(Debug, Clone)]
        struct VirtualText {
            anchor: usize,
            text: String,
            styles: Vec<StyleId>,
        }

        // Collect virtual text decorations from all layers.
        let mut inline_before: HashMap<usize, Vec<VirtualText>> = HashMap::new();
        let mut inline_after: HashMap<usize, Vec<VirtualText>> = HashMap::new();
        let mut above_by_line: BTreeMap<usize, Vec<VirtualText>> = BTreeMap::new();

        for decorations in self.decorations.values() {
            for deco in decorations {
                let Some(text) = deco.text.as_ref() else {
                    continue;
                };
                if text.is_empty() {
                    continue;
                }

                let anchor = match deco.placement {
                    DecorationPlacement::After => deco.range.end,
                    DecorationPlacement::Before | DecorationPlacement::AboveLine => {
                        deco.range.start
                    }
                };
                let vt = VirtualText {
                    anchor,
                    text: text.clone(),
                    styles: deco.styles.clone(),
                };

                match deco.placement {
                    DecorationPlacement::Before => {
                        inline_before.entry(anchor).or_default().push(vt);
                    }
                    DecorationPlacement::After => {
                        inline_after.entry(anchor).or_default().push(vt);
                    }
                    DecorationPlacement::AboveLine => {
                        let line = self.line_index.char_offset_to_position(anchor).0;
                        above_by_line.entry(line).or_default().push(vt);
                    }
                }
            }
        }

        let regions = self.folding_manager.regions();
        let line_count = self.layout_engine.logical_line_count();
        let mut above_lines = Vec::new();
        let mut above_prefix_counts = vec![0usize];
        let mut total_above = 0usize;
        for (&line, above) in &above_by_line {
            if line >= line_count || Self::is_logical_line_hidden(regions, line) {
                continue;
            }
            above_lines.push(line);
            total_above = total_above.saturating_add(above.len());
            above_prefix_counts.push(total_above);
        }

        let above_count_before_line = |line: usize| -> usize {
            let idx = above_lines.partition_point(|candidate| *candidate < line);
            above_prefix_counts[idx]
        };

        let (total_composed, start_logical_line, mut current_visual) =
            self.with_visual_row_index(|index| {
                let total_composed = index.total_visual_lines().saturating_add(total_above);
                if line_count == 0 || start_visual_row >= total_composed {
                    return (total_composed, line_count, total_composed);
                }

                let composed_rows_before_line = |line: usize| -> usize {
                    index
                        .visual_rows_before_logical_line(line)
                        .saturating_add(above_count_before_line(line))
                };

                let mut low = 0usize;
                let mut high = line_count;
                while low < high {
                    let mid = (low + high).div_ceil(2);
                    if composed_rows_before_line(mid) <= start_visual_row {
                        low = mid;
                    } else {
                        high = mid.saturating_sub(1);
                    }
                }

                let mut start_line = low.min(line_count.saturating_sub(1));
                if index.span_for_logical_line(start_line).is_none() {
                    let doc_visual = index.visual_rows_before_logical_line(start_line);
                    let Some((span, _)) = index.span_for_visual_row(doc_visual) else {
                        return (total_composed, line_count, total_composed);
                    };
                    start_line = span.logical_line;
                }

                (
                    total_composed,
                    start_line,
                    composed_rows_before_line(start_line),
                )
            });

        if start_visual_row >= total_composed {
            return grid;
        }

        let end_visual = start_visual_row.saturating_add(count).min(total_composed);
        let tab_width = self.layout_engine.tab_width();

        for logical_line in start_logical_line..line_count {
            if Self::is_logical_line_hidden(regions, logical_line) {
                continue;
            }

            // Above-line virtual text (e.g. code lens).
            if let Some(above) = above_by_line.get(&logical_line) {
                for vt in above {
                    if current_visual >= end_visual {
                        return grid;
                    }

                    if current_visual >= start_visual_row {
                        let mut x_render = 0usize;
                        let mut cells: Vec<ComposedCell> = Vec::new();
                        for ch in vt.text.chars() {
                            let w = cell_width_at(ch, x_render, tab_width);
                            x_render = x_render.saturating_add(w);
                            cells.push(ComposedCell {
                                ch,
                                width: w,
                                styles: vt.styles.clone(),
                                source: ComposedCellSource::Virtual {
                                    anchor_offset: vt.anchor,
                                },
                            });
                        }

                        grid.lines.push(ComposedLine {
                            kind: ComposedLineKind::VirtualAboveLine { logical_line },
                            char_offset_start: vt.anchor,
                            char_offset_end: vt.anchor,
                            cells,
                        });
                    }

                    current_visual = current_visual.saturating_add(1);
                }
            }

            let Some(layout) = self.layout_engine.get_line_layout(logical_line) else {
                continue;
            };

            let line_text = self
                .line_index
                .get_line_text(logical_line)
                .unwrap_or_default();
            let line_char_len = line_text.chars().count();
            let line_start_offset = self.line_index.position_to_char_offset(logical_line, 0);

            for visual_in_line in 0..layout.visual_line_count {
                if current_visual >= end_visual {
                    return grid;
                }

                if current_visual < start_visual_row {
                    current_visual = current_visual.saturating_add(1);
                    continue;
                }

                let segment_start_col = if visual_in_line == 0 {
                    0
                } else {
                    layout
                        .wrap_points
                        .get(visual_in_line - 1)
                        .map(|wp| wp.char_index)
                        .unwrap_or(0)
                        .min(line_char_len)
                };

                let segment_end_col = if visual_in_line < layout.wrap_points.len() {
                    layout.wrap_points[visual_in_line]
                        .char_index
                        .min(line_char_len)
                } else {
                    line_char_len
                };

                let segment_start_offset = line_start_offset + segment_start_col;

                let mut cells: Vec<ComposedCell> = Vec::new();

                let mut x_render = 0usize;
                if visual_in_line > 0 {
                    let indent_cells = wrap_indent_cells_for_line_text(
                        &line_text,
                        self.layout_engine.wrap_indent(),
                        self.viewport_width,
                        tab_width,
                    );
                    x_render = x_render.saturating_add(indent_cells);
                    for _ in 0..indent_cells {
                        cells.push(ComposedCell {
                            ch: ' ',
                            width: 1,
                            styles: Vec::new(),
                            source: ComposedCellSource::Virtual {
                                anchor_offset: segment_start_offset,
                            },
                        });
                    }
                }

                let mut x_in_line = visual_x_for_column(&line_text, segment_start_col, tab_width);

                let push_virtual = |anchor: usize,
                                    list: &[VirtualText],
                                    cells: &mut Vec<ComposedCell>,
                                    x_render: &mut usize| {
                    for vt in list {
                        for ch in vt.text.chars() {
                            let w = cell_width_at(ch, *x_render, tab_width);
                            *x_render = x_render.saturating_add(w);
                            cells.push(ComposedCell {
                                ch,
                                width: w,
                                styles: vt.styles.clone(),
                                source: ComposedCellSource::Virtual {
                                    anchor_offset: anchor,
                                },
                            });
                        }
                    }
                };

                for (col, ch) in line_text
                    .chars()
                    .enumerate()
                    .skip(segment_start_col)
                    .take(segment_end_col.saturating_sub(segment_start_col))
                {
                    let offset = line_start_offset + col;

                    if let Some(list) = inline_before.get(&offset) {
                        push_virtual(offset, list, &mut cells, &mut x_render);
                    }
                    if let Some(list) = inline_after.get(&offset) {
                        push_virtual(offset, list, &mut cells, &mut x_render);
                    }

                    let styles = self.styles_at_offset(offset);
                    let w = cell_width_at(ch, x_in_line, tab_width);
                    x_in_line = x_in_line.saturating_add(w);
                    x_render = x_render.saturating_add(w);
                    cells.push(ComposedCell {
                        ch,
                        width: w,
                        styles,
                        source: ComposedCellSource::Document { offset },
                    });
                }

                // End-of-line inline virtual text (only on the last visual segment).
                if visual_in_line + 1 == layout.visual_line_count {
                    let eol_offset = line_start_offset + line_char_len;
                    if let Some(list) = inline_before.get(&eol_offset) {
                        push_virtual(eol_offset, list, &mut cells, &mut x_render);
                    }
                    if let Some(list) = inline_after.get(&eol_offset) {
                        push_virtual(eol_offset, list, &mut cells, &mut x_render);
                    }

                    // For collapsed folding start line, append placeholder to the last segment.
                    if let Some(region) = Self::collapsed_region_starting_at(regions, logical_line)
                        && !region.placeholder.is_empty()
                    {
                        if !cells.is_empty() {
                            x_render = x_render.saturating_add(char_width(' '));
                            cells.push(ComposedCell {
                                ch: ' ',
                                width: char_width(' '),
                                styles: vec![FOLD_PLACEHOLDER_STYLE_ID],
                                source: ComposedCellSource::Virtual {
                                    anchor_offset: eol_offset,
                                },
                            });
                        }
                        for ch in region.placeholder.chars() {
                            let w = cell_width_at(ch, x_render, tab_width);
                            x_render = x_render.saturating_add(w);
                            cells.push(ComposedCell {
                                ch,
                                width: w,
                                styles: vec![FOLD_PLACEHOLDER_STYLE_ID],
                                source: ComposedCellSource::Virtual {
                                    anchor_offset: eol_offset,
                                },
                            });
                        }
                        if let Some(right_bracket) = self.fold_right_boundary_bracket_char(region) {
                            x_render = x_render.saturating_add(char_width(' '));
                            cells.push(ComposedCell {
                                ch: ' ',
                                width: char_width(' '),
                                styles: vec![FOLD_PLACEHOLDER_STYLE_ID],
                                source: ComposedCellSource::Virtual {
                                    anchor_offset: eol_offset,
                                },
                            });

                            let w = cell_width_at(right_bracket, x_render, tab_width);
                            cells.push(ComposedCell {
                                ch: right_bracket,
                                width: w,
                                styles: vec![FOLD_PLACEHOLDER_STYLE_ID],
                                source: ComposedCellSource::Virtual {
                                    anchor_offset: eol_offset,
                                },
                            });
                        }
                    }
                }

                grid.lines.push(ComposedLine {
                    kind: ComposedLineKind::Document {
                        logical_line,
                        visual_in_logical: visual_in_line,
                    },
                    char_offset_start: segment_start_offset,
                    char_offset_end: line_start_offset + segment_end_col,
                    cells,
                });

                current_visual = current_visual.saturating_add(1);
            }
        }

        grid
    }
}
