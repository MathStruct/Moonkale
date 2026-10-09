//! Cursor, selection, and word-boundary command helpers.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TextBoundary {
    Grapheme,
    Word,
}

/// Word boundary configuration used by editor-friendly "word" operations (selection expansion,
/// double-click selection, etc.).
///
/// This intentionally follows a **code-editor** notion of "word":
/// - By default, **ASCII identifier-like runs** are treated as words.
/// - Non-ASCII characters are treated as word boundaries (so they form single-character "word units").
/// - Whitespace always separates words.
#[derive(Debug, Clone)]
pub struct WordBoundaryConfig {
    ascii_is_boundary: [bool; 128],
}

impl Default for WordBoundaryConfig {
    fn default() -> Self {
        let mut ascii_is_boundary = [true; 128];
        for b in 0u8..=127 {
            let ch = b as char;
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ascii_is_boundary[b as usize] = false;
            }
        }
        // Always treat ASCII whitespace as boundary.
        ascii_is_boundary[b' ' as usize] = true;
        ascii_is_boundary[b'\t' as usize] = true;
        ascii_is_boundary[b'\n' as usize] = true;
        ascii_is_boundary[b'\r' as usize] = true;
        Self { ascii_is_boundary }
    }
}

impl WordBoundaryConfig {
    /// Override the ASCII "word boundary" character set.
    ///
    /// - Non-ASCII characters are always treated as boundaries (and therefore form single-character word units).
    /// - ASCII whitespace is always treated as boundary.
    ///
    /// This is similar in spirit to VSCode's `wordSeparators`.
    pub fn set_ascii_boundary_chars(&mut self, boundary_chars: &str) {
        self.ascii_is_boundary = [false; 128];
        // Always treat ASCII whitespace as boundary.
        self.ascii_is_boundary[b' ' as usize] = true;
        self.ascii_is_boundary[b'\t' as usize] = true;
        self.ascii_is_boundary[b'\n' as usize] = true;
        self.ascii_is_boundary[b'\r' as usize] = true;

        for ch in boundary_chars.chars() {
            if ch.is_ascii() {
                self.ascii_is_boundary[ch as usize] = true;
            }
        }
    }

    pub(super) fn is_ascii_word_char(&self, ch: char) -> bool {
        if !ch.is_ascii() {
            return false;
        }
        !self.ascii_is_boundary[ch as usize]
    }

    pub(super) fn is_word_token_char(&self, ch: char) -> bool {
        if ch.is_whitespace() {
            return false;
        }
        if ch.is_ascii() {
            self.is_ascii_word_char(ch)
        } else {
            // Treat non-ASCII as "word units" of length 1.
            true
        }
    }
}

pub(super) fn byte_offset_for_char_column(text: &str, column: usize) -> usize {
    if column == 0 {
        return 0;
    }

    text.char_indices()
        .nth(column)
        .map(|(byte, _)| byte)
        .unwrap_or_else(|| text.len())
}

pub(super) fn char_column_for_byte_offset(text: &str, byte_offset: usize) -> usize {
    text.get(..byte_offset).unwrap_or(text).chars().count()
}

pub(super) fn leading_horizontal_whitespace(text: &str) -> (usize, usize) {
    let mut column = 0usize;
    for (byte, ch) in text.char_indices() {
        if ch != ' ' && ch != '\t' {
            return (column, byte);
        }
        column += 1;
    }

    (column, text.len())
}

pub(super) fn prev_boundary_column(text: &str, column: usize, boundary: TextBoundary) -> usize {
    let byte_pos = byte_offset_for_char_column(text, column);

    let mut prev = 0usize;
    match boundary {
        TextBoundary::Grapheme => {
            for (b, _) in text.grapheme_indices(true) {
                if b >= byte_pos {
                    break;
                }
                prev = b;
            }
        }
        TextBoundary::Word => {
            for (b, _) in text.split_word_bound_indices() {
                if b >= byte_pos {
                    break;
                }
                prev = b;
            }
        }
    }

    char_column_for_byte_offset(text, prev)
}

pub(super) fn next_boundary_column(text: &str, column: usize, boundary: TextBoundary) -> usize {
    let byte_pos = byte_offset_for_char_column(text, column);

    let mut next = text.len();
    match boundary {
        TextBoundary::Grapheme => {
            for (b, _) in text.grapheme_indices(true) {
                if b > byte_pos {
                    next = b;
                    break;
                }
            }
        }
        TextBoundary::Word => {
            for (b, _) in text.split_word_bound_indices() {
                if b > byte_pos {
                    next = b;
                    break;
                }
            }
        }
    }

    char_column_for_byte_offset(text, next)
}

impl CommandExecutor {
    pub(super) fn execute_select_line_command(&mut self) -> Result<CommandResult, CommandError> {
        let snapshot = self.snapshot_selection_set();
        let selections = snapshot.selections;
        let primary_index = snapshot.primary_index;

        let line_count = self.editor.line_index.line_count();
        if line_count == 0 {
            return Ok(CommandResult::Success);
        }

        let mut next: Vec<Selection> = Vec::with_capacity(selections.len());
        for sel in selections {
            let (min_pos, max_pos) = crate::selection_set::selection_min_max(&sel);
            let start_line = min_pos.line.min(line_count.saturating_sub(1));
            let end_line = max_pos.line.min(line_count.saturating_sub(1));

            let start = Position::new(start_line, 0);
            let end = if end_line + 1 < line_count {
                Position::new(end_line + 1, 0)
            } else {
                let line_text = self
                    .editor
                    .line_index
                    .get_line_text(end_line)
                    .unwrap_or_default();
                Position::new(end_line, line_text.chars().count())
            };

            next.push(Selection {
                start,
                end,
                direction: SelectionDirection::Forward,
            });
        }

        self.execute_cursor(CursorCommand::SetSelections {
            selections: next,
            primary_index,
        })?;
        Ok(CommandResult::Success)
    }

    pub(super) fn execute_select_word_command(&mut self) -> Result<CommandResult, CommandError> {
        let snapshot = self.snapshot_selection_set();
        let selections = snapshot.selections;
        let primary_index = snapshot.primary_index;

        let line_count = self.editor.line_index.line_count();
        if line_count == 0 {
            return Ok(CommandResult::Success);
        }

        let mut next: Vec<Selection> = Vec::with_capacity(selections.len());

        for sel in selections {
            // If already a non-empty selection, keep it.
            if sel.start != sel.end {
                next.push(sel);
                continue;
            }

            let caret = sel.end;
            let line = caret.line.min(line_count.saturating_sub(1));
            let line_text = self
                .editor
                .line_index
                .get_line_text(line)
                .unwrap_or_default();
            let col = caret.column.min(line_text.chars().count());

            let Some((start_col, end_col)) = self.word_token_range_in_line(&line_text, col) else {
                next.push(sel);
                continue;
            };

            let start = Position::new(line, start_col);
            let end = Position::new(line, end_col);

            next.push(Selection {
                start,
                end,
                direction: SelectionDirection::Forward,
            });
        }

        self.execute_cursor(CursorCommand::SetSelections {
            selections: next,
            primary_index,
        })?;
        Ok(CommandResult::Success)
    }

    pub(super) fn execute_expand_selection_command(
        &mut self,
    ) -> Result<CommandResult, CommandError> {
        // Basic expand policy:
        // - empty selection => select word
        // - non-empty selection => select line(s)
        let snapshot = self.snapshot_selection_set();
        if snapshot.selections.iter().any(|s| s.start != s.end) {
            self.execute_select_line_command()
        } else {
            self.execute_select_word_command()
        }
    }

    pub(super) fn execute_expand_selection_by_command(
        &mut self,
        unit: ExpandSelectionUnit,
        count: usize,
        direction: ExpandSelectionDirection,
    ) -> Result<CommandResult, CommandError> {
        if count == 0 {
            return Ok(CommandResult::Success);
        }

        let snapshot = self.snapshot_selection_set();
        let selections = snapshot.selections;
        let primary_index = snapshot.primary_index;

        let line_count = self.editor.line_index.line_count();
        if line_count == 0 {
            return Ok(CommandResult::Success);
        }

        let mut next: Vec<Selection> = Vec::with_capacity(selections.len());
        for sel in selections {
            let (min_pos, max_pos) = crate::selection_set::selection_min_max(&sel);
            let mut start = min_pos;
            let mut end = max_pos;

            match direction {
                ExpandSelectionDirection::Backward => {
                    start = self.expand_position_by_unit(start, unit, count, direction);
                }
                ExpandSelectionDirection::Forward => {
                    end = self.expand_position_by_unit(end, unit, count, direction);
                }
            }

            next.push(Selection {
                start,
                end,
                direction: SelectionDirection::Forward,
            });
        }

        // Delegate to SetSelections so normalization rules are shared.
        self.execute_cursor(CursorCommand::SetSelections {
            selections: next,
            primary_index,
        })?;
        Ok(CommandResult::Success)
    }

    pub(super) fn expand_position_by_unit(
        &self,
        pos: Position,
        unit: ExpandSelectionUnit,
        count: usize,
        direction: ExpandSelectionDirection,
    ) -> Position {
        match unit {
            ExpandSelectionUnit::Character => self.expand_position_by_char(pos, count, direction),
            ExpandSelectionUnit::Word => self.expand_position_by_word(pos, count, direction),
            ExpandSelectionUnit::Line => self.expand_position_by_line(pos, count, direction),
        }
    }

    pub(super) fn expand_position_by_char(
        &self,
        pos: Position,
        count: usize,
        direction: ExpandSelectionDirection,
    ) -> Position {
        let line_index = &self.editor.line_index;
        let mut offset = line_index.position_to_char_offset(pos.line, pos.column);
        let char_count = self.editor.char_count();

        offset = match direction {
            ExpandSelectionDirection::Backward => offset.saturating_sub(count),
            ExpandSelectionDirection::Forward => offset.saturating_add(count).min(char_count),
        };

        let (line, col) = line_index.char_offset_to_position(offset);
        Position::new(line, col)
    }

    pub(super) fn expand_position_by_line(
        &self,
        pos: Position,
        count: usize,
        direction: ExpandSelectionDirection,
    ) -> Position {
        let line_index = &self.editor.line_index;
        let line_count = line_index.line_count();
        if line_count == 0 {
            return Position::new(0, 0);
        }

        let mut line = pos.line.min(line_count.saturating_sub(1));
        line = match direction {
            ExpandSelectionDirection::Backward => line.saturating_sub(count),
            ExpandSelectionDirection::Forward => line.saturating_add(count),
        };

        if line >= line_count {
            let line_text = line_index
                .get_line_text(line_count.saturating_sub(1))
                .unwrap_or_default();
            return Position::new(line_count.saturating_sub(1), line_text.chars().count());
        }

        Position::new(line, 0)
    }

    pub(super) fn expand_position_by_word(
        &self,
        mut pos: Position,
        count: usize,
        direction: ExpandSelectionDirection,
    ) -> Position {
        for _ in 0..count {
            let next = match direction {
                ExpandSelectionDirection::Backward => self.prev_word_boundary_position(pos),
                ExpandSelectionDirection::Forward => self.next_word_boundary_position(pos),
            };
            if next == pos {
                break;
            }
            pos = next;
        }
        pos
    }

    pub(super) fn next_word_boundary_position(&self, pos: Position) -> Position {
        let line_index = &self.editor.line_index;
        let line_count = line_index.line_count();
        if line_count == 0 {
            return Position::new(0, 0);
        }

        let mut line = pos.line.min(line_count.saturating_sub(1));
        let mut col = pos.column;

        loop {
            let line_text = line_index.get_line_text(line).unwrap_or_default();
            let chars: Vec<char> = line_text.chars().collect();
            let len = chars.len();
            col = col.min(len);

            // Find next token start (a "word unit") in this line.
            let mut i = col;
            while i < len && !self.editor.word_boundary.is_word_token_char(chars[i]) {
                i += 1;
            }

            if i < len {
                // Consume one token.
                if chars[i].is_ascii() && self.editor.word_boundary.is_ascii_word_char(chars[i]) {
                    let mut end = i + 1;
                    while end < len
                        && chars[end].is_ascii()
                        && self.editor.word_boundary.is_ascii_word_char(chars[end])
                    {
                        end += 1;
                    }
                    return Position::new(line, end);
                }
                return Position::new(line, i + 1);
            }

            // No token on this line - move to next line.
            if line + 1 >= line_count {
                return Position::new(line, len);
            }
            line += 1;
            col = 0;
        }
    }

    pub(super) fn prev_word_boundary_position(&self, pos: Position) -> Position {
        let line_index = &self.editor.line_index;
        let line_count = line_index.line_count();
        if line_count == 0 {
            return Position::new(0, 0);
        }

        let mut line = pos.line.min(line_count.saturating_sub(1));
        let mut col = pos.column;

        loop {
            let line_text = line_index.get_line_text(line).unwrap_or_default();
            let chars: Vec<char> = line_text.chars().collect();
            let len = chars.len();
            col = col.min(len);

            if col == 0 {
                if line == 0 {
                    return Position::new(0, 0);
                }
                line -= 1;
                col = usize::MAX;
                continue;
            }

            let mut i = col.saturating_sub(1).min(len.saturating_sub(1));
            while i < len && !self.editor.word_boundary.is_word_token_char(chars[i]) {
                if i == 0 {
                    break;
                }
                i -= 1;
            }

            if self.editor.word_boundary.is_word_token_char(chars[i]) {
                // If ASCII word token, walk to token start.
                if chars[i].is_ascii() && self.editor.word_boundary.is_ascii_word_char(chars[i]) {
                    while i > 0
                        && chars[i - 1].is_ascii()
                        && self.editor.word_boundary.is_ascii_word_char(chars[i - 1])
                    {
                        i -= 1;
                    }
                    return Position::new(line, i);
                }
                return Position::new(line, i);
            }

            // No token found on this line - move to previous line.
            if line == 0 {
                return Position::new(0, 0);
            }
            line -= 1;
            col = usize::MAX;
        }
    }

    pub(super) fn execute_add_cursor_vertical_command(
        &mut self,
        above: bool,
    ) -> Result<CommandResult, CommandError> {
        let snapshot = self.snapshot_selection_set();
        let mut selections = snapshot.selections;
        let primary_index = snapshot.primary_index;

        let line_count = self.editor.line_index.line_count();
        if line_count == 0 {
            return Ok(CommandResult::Success);
        }

        let mut extra: Vec<Selection> = Vec::new();
        for sel in &selections {
            let caret = sel.end;
            let target_line = if above {
                if caret.line == 0 {
                    continue;
                }
                caret.line - 1
            } else {
                let next = caret.line + 1;
                if next >= line_count {
                    continue;
                }
                next
            };

            let col = self.clamp_column_for_line(target_line, caret.column);
            let pos = Position::new(target_line, col);
            extra.push(Selection {
                start: pos,
                end: pos,
                direction: SelectionDirection::Forward,
            });
        }

        if extra.is_empty() {
            return Ok(CommandResult::Success);
        }

        selections.extend(extra);

        self.execute_cursor(CursorCommand::SetSelections {
            selections,
            primary_index,
        })?;
        Ok(CommandResult::Success)
    }

    pub(super) fn selection_query(
        &self,
        selections: &[Selection],
        primary_index: usize,
    ) -> Option<(String, Option<SearchMatch>)> {
        let primary = selections.get(primary_index)?;
        let range = self.selection_char_range(primary);

        if range.start != range.end {
            let len = range.end - range.start;
            return Some((self.editor.text_range(range.start, len), Some(range)));
        }

        let caret = primary.end;
        let line_text = self
            .editor
            .line_index
            .get_line_text(caret.line)
            .unwrap_or_default();
        let col = caret.column.min(line_text.chars().count());
        let (start_col, end_col) = self.word_token_range_in_line(&line_text, col)?;
        if start_col == end_col {
            return None;
        }

        let start = self
            .editor
            .line_index
            .position_to_char_offset(caret.line, start_col);
        let end = self
            .editor
            .line_index
            .position_to_char_offset(caret.line, end_col);
        let range = SearchMatch {
            start,
            end: end.max(start),
        };
        Some((
            self.editor
                .text_range(range.start, range.end.saturating_sub(range.start)),
            Some(range),
        ))
    }

    pub(super) fn execute_add_next_occurrence_command(
        &mut self,
        options: SearchOptions,
    ) -> Result<CommandResult, CommandError> {
        let snapshot = self.snapshot_selection_set();
        let mut selections = snapshot.selections;
        let primary_index = snapshot.primary_index;

        let Some((query, primary_range)) = self.selection_query(&selections, primary_index) else {
            return Ok(CommandResult::Success);
        };
        if query.is_empty() {
            return Ok(CommandResult::Success);
        }

        // VSCode-like: if there is no active selection, first select the current word occurrence.
        if let Some(primary_range) = primary_range
            && primary_range.start != primary_range.end
        {
            let current = selections
                .get(primary_index)
                .map(|s| self.selection_char_range(s))
                .unwrap_or(SearchMatch { start: 0, end: 0 });
            if current.start == current.end {
                let (start_line, start_col) = self
                    .editor
                    .line_index
                    .char_offset_to_position(primary_range.start);
                let (end_line, end_col) = self
                    .editor
                    .line_index
                    .char_offset_to_position(primary_range.end);
                if let Some(sel) = selections.get_mut(primary_index) {
                    *sel = Selection {
                        start: Position::new(start_line, start_col),
                        end: Position::new(end_line, end_col),
                        direction: SelectionDirection::Forward,
                    };
                }
            }
        }

        let text = self.editor.get_text();

        let mut ranges: Vec<SearchMatch> = selections
            .iter()
            .map(|s| self.selection_char_range(s))
            .filter(|r| r.start != r.end)
            .collect();

        if let Some(primary_range) = primary_range
            && primary_range.start != primary_range.end
            && !ranges
                .iter()
                .any(|r| r.start == primary_range.start && r.end == primary_range.end)
        {
            ranges.push(primary_range);
        }

        let mut existing: Vec<(usize, usize)> = ranges
            .iter()
            .map(|r| (r.start.min(r.end), r.end.max(r.start)))
            .collect();
        existing.sort_unstable();

        let from = existing.iter().map(|(_, end)| *end).max().unwrap_or(0);

        let mut search_from = from;
        let mut wrapped = false;
        let mut found: Option<SearchMatch> = None;

        loop {
            let next = find_next(&text, &query, options, search_from)
                .map_err(|err| CommandError::Other(err.to_string()))?;

            let Some(m) = next else {
                if wrapped {
                    break;
                }
                wrapped = true;
                search_from = 0;
                continue;
            };

            let overlaps = existing.iter().any(|(s, e)| m.start < *e && m.end > *s);

            if overlaps {
                if m.end >= text.chars().count() {
                    break;
                }
                search_from = m.end + 1;
                continue;
            }

            found = Some(m);
            break;
        }

        let Some(m) = found else {
            return Ok(CommandResult::Success);
        };

        let (start_line, start_col) = self.editor.line_index.char_offset_to_position(m.start);
        let (end_line, end_col) = self.editor.line_index.char_offset_to_position(m.end);

        selections.push(Selection {
            start: Position::new(start_line, start_col),
            end: Position::new(end_line, end_col),
            direction: SelectionDirection::Forward,
        });

        let new_primary_index = selections.len().saturating_sub(1);
        self.execute_cursor(CursorCommand::SetSelections {
            selections,
            primary_index: new_primary_index,
        })?;

        Ok(CommandResult::Success)
    }

    pub(super) fn execute_add_all_occurrences_command(
        &mut self,
        options: SearchOptions,
    ) -> Result<CommandResult, CommandError> {
        let snapshot = self.snapshot_selection_set();
        let selections = snapshot.selections;
        let primary_index = snapshot.primary_index;

        let Some((query, primary_range)) = self.selection_query(&selections, primary_index) else {
            return Ok(CommandResult::Success);
        };
        if query.is_empty() {
            return Ok(CommandResult::Success);
        }

        let text = self.editor.get_text();
        let matches =
            find_all(&text, &query, options).map_err(|err| CommandError::Other(err.to_string()))?;

        if matches.is_empty() {
            return Ok(CommandResult::Success);
        }

        let mut out: Vec<Selection> = Vec::with_capacity(matches.len());
        let mut next_primary = 0usize;
        let primary_range = primary_range.filter(|r| r.start != r.end);

        for (idx, m) in matches.iter().enumerate() {
            let (start_line, start_col) = self.editor.line_index.char_offset_to_position(m.start);
            let (end_line, end_col) = self.editor.line_index.char_offset_to_position(m.end);
            out.push(Selection {
                start: Position::new(start_line, start_col),
                end: Position::new(end_line, end_col),
                direction: SelectionDirection::Forward,
            });

            if let Some(pr) = primary_range
                && pr.start == m.start
                && pr.end == m.end
            {
                next_primary = idx;
            }
        }

        self.execute_cursor(CursorCommand::SetSelections {
            selections: out,
            primary_index: next_primary,
        })?;

        Ok(CommandResult::Success)
    }
}

impl CommandExecutor {
    pub(super) fn execute_cursor(
        &mut self,
        command: CursorCommand,
    ) -> Result<CommandResult, CommandError> {
        match command {
            CursorCommand::MoveTo { line, column } => {
                if line >= self.editor.line_index.line_count() {
                    return Err(CommandError::InvalidPosition { line, column });
                }

                let clamped_column = self.clamp_column_for_line(line, column);
                self.editor.cursor_position = Position::new(line, clamped_column);
                self.preferred_x_cells = self
                    .editor
                    .logical_position_to_visual(line, clamped_column)
                    .map(|(_, x)| x);
                // VSCode-like: moving the primary caret to an absolute position collapses multi-cursor.
                self.editor.secondary_selections.clear();
                Ok(CommandResult::Success)
            }
            CursorCommand::MoveBy {
                delta_line,
                delta_column,
            } => {
                let new_line = if delta_line >= 0 {
                    self.editor.cursor_position.line + delta_line as usize
                } else {
                    self.editor
                        .cursor_position
                        .line
                        .saturating_sub((-delta_line) as usize)
                };

                let new_column = if delta_column >= 0 {
                    self.editor.cursor_position.column + delta_column as usize
                } else {
                    self.editor
                        .cursor_position
                        .column
                        .saturating_sub((-delta_column) as usize)
                };

                if new_line >= self.editor.line_index.line_count() {
                    return Err(CommandError::InvalidPosition {
                        line: new_line,
                        column: new_column,
                    });
                }

                let clamped_column = self.clamp_column_for_line(new_line, new_column);
                self.editor.cursor_position = Position::new(new_line, clamped_column);
                self.preferred_x_cells = self
                    .editor
                    .logical_position_to_visual(new_line, clamped_column)
                    .map(|(_, x)| x);
                Ok(CommandResult::Success)
            }
            CursorCommand::MoveGraphemeLeft => {
                let line_count = self.editor.line_index.line_count();
                if line_count == 0 {
                    return Ok(CommandResult::Success);
                }

                let mut line = self
                    .editor
                    .cursor_position
                    .line
                    .min(line_count.saturating_sub(1));
                let mut line_text = self
                    .editor
                    .line_index
                    .get_line_text(line)
                    .unwrap_or_default();
                let mut line_char_len = line_text.chars().count();
                let mut col = self.editor.cursor_position.column.min(line_char_len);

                if col == 0 {
                    if line == 0 {
                        return Ok(CommandResult::Success);
                    }
                    line = line.saturating_sub(1);
                    line_text = self
                        .editor
                        .line_index
                        .get_line_text(line)
                        .unwrap_or_default();
                    line_char_len = line_text.chars().count();
                    col = line_char_len;
                } else {
                    col = prev_boundary_column(&line_text, col, TextBoundary::Grapheme);
                }

                self.editor.cursor_position = Position::new(line, col);
                self.preferred_x_cells = self
                    .editor
                    .logical_position_to_visual(line, col)
                    .map(|(_, x)| x);
                Ok(CommandResult::Success)
            }
            CursorCommand::MoveGraphemeRight => {
                let line_count = self.editor.line_index.line_count();
                if line_count == 0 {
                    return Ok(CommandResult::Success);
                }

                let line = self
                    .editor
                    .cursor_position
                    .line
                    .min(line_count.saturating_sub(1));
                let line_text = self
                    .editor
                    .line_index
                    .get_line_text(line)
                    .unwrap_or_default();
                let line_char_len = line_text.chars().count();
                let col = self.editor.cursor_position.column.min(line_char_len);

                let (line, col) = if col >= line_char_len {
                    if line + 1 >= line_count {
                        return Ok(CommandResult::Success);
                    }
                    (line + 1, 0)
                } else {
                    (
                        line,
                        next_boundary_column(&line_text, col, TextBoundary::Grapheme),
                    )
                };

                self.editor.cursor_position = Position::new(line, col);
                self.preferred_x_cells = self
                    .editor
                    .logical_position_to_visual(line, col)
                    .map(|(_, x)| x);
                Ok(CommandResult::Success)
            }
            CursorCommand::MoveWordLeft => {
                let line_count = self.editor.line_index.line_count();
                if line_count == 0 {
                    return Ok(CommandResult::Success);
                }

                let mut line = self
                    .editor
                    .cursor_position
                    .line
                    .min(line_count.saturating_sub(1));
                let mut line_text = self
                    .editor
                    .line_index
                    .get_line_text(line)
                    .unwrap_or_default();
                let mut line_char_len = line_text.chars().count();
                let mut col = self.editor.cursor_position.column.min(line_char_len);

                if col == 0 {
                    if line == 0 {
                        return Ok(CommandResult::Success);
                    }
                    line = line.saturating_sub(1);
                    line_text = self
                        .editor
                        .line_index
                        .get_line_text(line)
                        .unwrap_or_default();
                    line_char_len = line_text.chars().count();
                    col = line_char_len;
                } else {
                    col = prev_boundary_column(&line_text, col, TextBoundary::Word);
                }

                self.editor.cursor_position = Position::new(line, col);
                self.preferred_x_cells = self
                    .editor
                    .logical_position_to_visual(line, col)
                    .map(|(_, x)| x);
                Ok(CommandResult::Success)
            }
            CursorCommand::MoveWordRight => {
                let line_count = self.editor.line_index.line_count();
                if line_count == 0 {
                    return Ok(CommandResult::Success);
                }

                let line = self
                    .editor
                    .cursor_position
                    .line
                    .min(line_count.saturating_sub(1));
                let line_text = self
                    .editor
                    .line_index
                    .get_line_text(line)
                    .unwrap_or_default();
                let line_char_len = line_text.chars().count();
                let col = self.editor.cursor_position.column.min(line_char_len);

                let (line, col) = if col >= line_char_len {
                    if line + 1 >= line_count {
                        return Ok(CommandResult::Success);
                    }
                    (line + 1, 0)
                } else {
                    (
                        line,
                        next_boundary_column(&line_text, col, TextBoundary::Word),
                    )
                };

                self.editor.cursor_position = Position::new(line, col);
                self.preferred_x_cells = self
                    .editor
                    .logical_position_to_visual(line, col)
                    .map(|(_, x)| x);
                Ok(CommandResult::Success)
            }
            CursorCommand::MoveVisualBy { delta_rows } => {
                let Some((current_row, current_x)) = self.editor.logical_position_to_visual(
                    self.editor.cursor_position.line,
                    self.editor.cursor_position.column,
                ) else {
                    return Ok(CommandResult::Success);
                };

                let preferred_x = self.preferred_x_cells.unwrap_or(current_x);
                self.preferred_x_cells = Some(preferred_x);

                let total_visual = self.editor.visual_line_count();
                if total_visual == 0 {
                    return Ok(CommandResult::Success);
                }

                let target_row = if delta_rows >= 0 {
                    current_row.saturating_add(delta_rows as usize)
                } else {
                    current_row.saturating_sub((-delta_rows) as usize)
                }
                .min(total_visual.saturating_sub(1));

                let Some(pos) = self
                    .editor
                    .visual_position_to_logical(target_row, preferred_x)
                else {
                    return Ok(CommandResult::Success);
                };

                self.editor.cursor_position = pos;
                Ok(CommandResult::Success)
            }
            CursorCommand::MoveToVisual { row, x_cells } => {
                let Some(pos) = self.editor.visual_position_to_logical(row, x_cells) else {
                    return Ok(CommandResult::Success);
                };

                self.editor.cursor_position = pos;
                self.preferred_x_cells = Some(x_cells);
                // Treat as an absolute move (similar to `MoveTo`).
                self.editor.secondary_selections.clear();
                Ok(CommandResult::Success)
            }
            CursorCommand::MoveToLineStart => {
                let line = self.editor.cursor_position.line;
                self.editor.cursor_position = Position::new(line, 0);
                self.preferred_x_cells = Some(0);
                self.editor.secondary_selections.clear();
                Ok(CommandResult::Success)
            }
            CursorCommand::MoveToLineEnd => {
                let line = self.editor.cursor_position.line;
                let end_col = self.clamp_column_for_line(line, usize::MAX);
                self.editor.cursor_position = Position::new(line, end_col);
                self.preferred_x_cells = self
                    .editor
                    .logical_position_to_visual(line, end_col)
                    .map(|(_, x)| x);
                self.editor.secondary_selections.clear();
                Ok(CommandResult::Success)
            }
            CursorCommand::MoveToVisualLineStart => {
                let line = self.editor.cursor_position.line;
                let Some(layout) = self.editor.layout_engine.get_line_layout(line) else {
                    return Ok(CommandResult::Success);
                };

                let line_text = self
                    .editor
                    .line_index
                    .get_line_text(line)
                    .unwrap_or_default();
                let line_char_len = line_text.chars().count();
                let column = self.editor.cursor_position.column.min(line_char_len);

                let mut seg_start = 0usize;
                for wp in &layout.wrap_points {
                    if column >= wp.char_index {
                        seg_start = wp.char_index;
                    } else {
                        break;
                    }
                }

                self.editor.cursor_position = Position::new(line, seg_start);
                self.preferred_x_cells = self
                    .editor
                    .logical_position_to_visual(line, seg_start)
                    .map(|(_, x)| x);
                self.editor.secondary_selections.clear();
                Ok(CommandResult::Success)
            }
            CursorCommand::MoveToVisualLineEnd => {
                let line = self.editor.cursor_position.line;
                let Some(layout) = self.editor.layout_engine.get_line_layout(line) else {
                    return Ok(CommandResult::Success);
                };

                let line_text = self
                    .editor
                    .line_index
                    .get_line_text(line)
                    .unwrap_or_default();
                let line_char_len = line_text.chars().count();
                let column = self.editor.cursor_position.column.min(line_char_len);

                let mut seg_end = line_char_len;
                for wp in &layout.wrap_points {
                    if column < wp.char_index {
                        seg_end = wp.char_index;
                        break;
                    }
                }

                self.editor.cursor_position = Position::new(line, seg_end);
                self.preferred_x_cells = self
                    .editor
                    .logical_position_to_visual(line, seg_end)
                    .map(|(_, x)| x);
                self.editor.secondary_selections.clear();
                Ok(CommandResult::Success)
            }
            CursorCommand::SetSelection { start, end } => {
                if start.line >= self.editor.line_index.line_count()
                    || end.line >= self.editor.line_index.line_count()
                {
                    return Err(CommandError::InvalidPosition {
                        line: start.line.max(end.line),
                        column: start.column.max(end.column),
                    });
                }

                let start = Position::new(
                    start.line,
                    self.clamp_column_for_line(start.line, start.column),
                );
                let end = Position::new(end.line, self.clamp_column_for_line(end.line, end.column));

                let direction = if start.line < end.line
                    || (start.line == end.line && start.column <= end.column)
                {
                    SelectionDirection::Forward
                } else {
                    SelectionDirection::Backward
                };

                self.editor.selection = Some(Selection {
                    start,
                    end,
                    direction,
                });
                Ok(CommandResult::Success)
            }
            CursorCommand::ExtendSelection { to } => {
                if to.line >= self.editor.line_index.line_count() {
                    return Err(CommandError::InvalidPosition {
                        line: to.line,
                        column: to.column,
                    });
                }

                let to = Position::new(to.line, self.clamp_column_for_line(to.line, to.column));

                if let Some(ref mut selection) = self.editor.selection {
                    selection.end = to;
                    selection.direction = if selection.start.line < to.line
                        || (selection.start.line == to.line && selection.start.column <= to.column)
                    {
                        SelectionDirection::Forward
                    } else {
                        SelectionDirection::Backward
                    };
                } else {
                    // If no selection, create selection from current cursor
                    self.editor.selection = Some(Selection {
                        start: self.editor.cursor_position,
                        end: to,
                        direction: if self.editor.cursor_position.line < to.line
                            || (self.editor.cursor_position.line == to.line
                                && self.editor.cursor_position.column <= to.column)
                        {
                            SelectionDirection::Forward
                        } else {
                            SelectionDirection::Backward
                        },
                    });
                }
                Ok(CommandResult::Success)
            }
            CursorCommand::ClearSelection => {
                self.editor.selection = None;
                Ok(CommandResult::Success)
            }
            CursorCommand::SetSelections {
                selections,
                primary_index,
            } => {
                let line_count = self.editor.line_index.line_count();
                if selections.is_empty() {
                    return Err(CommandError::Other(
                        "SetSelections requires a non-empty selection list".to_string(),
                    ));
                }
                if primary_index >= selections.len() {
                    return Err(CommandError::Other(format!(
                        "Invalid primary_index {} for {} selections",
                        primary_index,
                        selections.len()
                    )));
                }

                for sel in &selections {
                    if sel.start.line >= line_count || sel.end.line >= line_count {
                        return Err(CommandError::InvalidPosition {
                            line: sel.start.line.max(sel.end.line),
                            column: sel.start.column.max(sel.end.column),
                        });
                    }
                }

                let (selections, primary_index) =
                    crate::selection_set::normalize_selections(selections, primary_index);

                let primary = selections
                    .get(primary_index)
                    .cloned()
                    .ok_or_else(|| CommandError::Other("Invalid primary selection".to_string()))?;

                self.editor.cursor_position = primary.end;
                self.editor.selection = if primary.start == primary.end {
                    None
                } else {
                    Some(primary.clone())
                };

                self.editor.secondary_selections = selections
                    .into_iter()
                    .enumerate()
                    .filter_map(|(idx, sel)| {
                        if idx == primary_index {
                            None
                        } else {
                            Some(sel)
                        }
                    })
                    .collect();

                Ok(CommandResult::Success)
            }
            CursorCommand::ClearSecondarySelections => {
                self.editor.secondary_selections.clear();
                Ok(CommandResult::Success)
            }
            CursorCommand::SetRectSelection { anchor, active } => {
                let line_count = self.editor.line_index.line_count();
                if anchor.line >= line_count || active.line >= line_count {
                    return Err(CommandError::InvalidPosition {
                        line: anchor.line.max(active.line),
                        column: anchor.column.max(active.column),
                    });
                }

                let (selections, primary_index) =
                    crate::selection_set::rect_selections(anchor, active);

                // Delegate to SetSelections so normalization rules are shared.
                self.execute_cursor(CursorCommand::SetSelections {
                    selections,
                    primary_index,
                })?;
                Ok(CommandResult::Success)
            }
            CursorCommand::SelectLine => self.execute_select_line_command(),
            CursorCommand::SelectWord => self.execute_select_word_command(),
            CursorCommand::ExpandSelection => self.execute_expand_selection_command(),
            CursorCommand::ExpandSelectionBy {
                unit,
                count,
                direction,
            } => self.execute_expand_selection_by_command(unit, count, direction),
            CursorCommand::AddCursorAbove => self.execute_add_cursor_vertical_command(true),
            CursorCommand::AddCursorBelow => self.execute_add_cursor_vertical_command(false),
            CursorCommand::AddNextOccurrence { options } => {
                self.execute_add_next_occurrence_command(options)
            }
            CursorCommand::AddAllOccurrences { options } => {
                self.execute_add_all_occurrences_command(options)
            }
            CursorCommand::MoveToMatchingBracket => self.execute_move_to_matching_bracket_command(),
            CursorCommand::SnippetNextPlaceholder => self.execute_snippet_navigation_command(true),
            CursorCommand::SnippetPrevPlaceholder => self.execute_snippet_navigation_command(false),
            CursorCommand::FindNext { query, options } => {
                self.execute_find_command(query, options, true)
            }
            CursorCommand::FindPrev { query, options } => {
                self.execute_find_command(query, options, false)
            }
        }
    }

    // Private method: execute view command
}

impl CommandExecutor {
    pub(super) fn position_to_char_offset_clamped(&self, pos: Position) -> usize {
        let line_count = self.editor.line_index.line_count();
        if line_count == 0 {
            return 0;
        }

        let line = pos.line.min(line_count.saturating_sub(1));
        let line_text = self
            .editor
            .line_index
            .get_line_text(line)
            .unwrap_or_default();
        let line_char_len = line_text.chars().count();
        let column = pos.column.min(line_char_len);
        self.editor.line_index.position_to_char_offset(line, column)
    }

    pub(super) fn position_to_char_offset_and_virtual_pad(&self, pos: Position) -> (usize, usize) {
        let line_count = self.editor.line_index.line_count();
        if line_count == 0 {
            return (0, 0);
        }

        let line = pos.line.min(line_count.saturating_sub(1));
        let line_text = self
            .editor
            .line_index
            .get_line_text(line)
            .unwrap_or_default();
        let line_char_len = line_text.chars().count();
        let clamped_col = pos.column.min(line_char_len);
        let offset = self
            .editor
            .line_index
            .position_to_char_offset(line, clamped_col);
        let pad = pos.column.saturating_sub(clamped_col);
        (offset, pad)
    }

    pub(super) fn normalize_cursor_and_selection(&mut self) {
        let line_index = &self.editor.line_index;
        let line_count = line_index.line_count();
        if line_count == 0 {
            self.editor.cursor_position = Position::new(0, 0);
            self.editor.selection = None;
            self.editor.secondary_selections.clear();
            return;
        }

        self.editor.cursor_position =
            Self::clamp_position_lenient_with_index(line_index, self.editor.cursor_position);

        if let Some(ref mut selection) = self.editor.selection {
            selection.start = Self::clamp_position_lenient_with_index(line_index, selection.start);
            selection.end = Self::clamp_position_lenient_with_index(line_index, selection.end);
            selection.direction = if selection.start.line < selection.end.line
                || (selection.start.line == selection.end.line
                    && selection.start.column <= selection.end.column)
            {
                SelectionDirection::Forward
            } else {
                SelectionDirection::Backward
            };
        }

        for selection in &mut self.editor.secondary_selections {
            selection.start = Self::clamp_position_lenient_with_index(line_index, selection.start);
            selection.end = Self::clamp_position_lenient_with_index(line_index, selection.end);
            selection.direction = if selection.start.line < selection.end.line
                || (selection.start.line == selection.end.line
                    && selection.start.column <= selection.end.column)
            {
                SelectionDirection::Forward
            } else {
                SelectionDirection::Backward
            };
        }
    }

    pub(super) fn clamp_column_for_line(&self, line: usize, column: usize) -> usize {
        Self::clamp_column_for_line_with_index(&self.editor.line_index, line, column)
    }

    pub(super) fn clamp_position_lenient_with_index(
        line_index: &LineIndex,
        pos: Position,
    ) -> Position {
        let line_count = line_index.line_count();
        if line_count == 0 {
            return Position::new(0, 0);
        }

        let clamped_line = pos.line.min(line_count.saturating_sub(1));
        // Note: do NOT clamp column here. Virtual columns (box selection) are allowed.
        Position::new(clamped_line, pos.column)
    }

    pub(super) fn clamp_column_for_line_with_index(
        line_index: &LineIndex,
        line: usize,
        column: usize,
    ) -> usize {
        let line_start = line_index.position_to_char_offset(line, 0);
        let line_end = line_index.position_to_char_offset(line, usize::MAX);
        let line_len = line_end.saturating_sub(line_start);
        column.min(line_len)
    }
}
