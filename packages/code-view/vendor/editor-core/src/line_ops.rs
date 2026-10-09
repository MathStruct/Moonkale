//! Line-oriented editing and comment toggling helpers.

use super::*;

impl CommandExecutor {
    pub(super) fn selection_char_range(&self, selection: &Selection) -> SearchMatch {
        let (min_pos, max_pos) = crate::selection_set::selection_min_max(selection);
        let start = self.position_to_char_offset_clamped(min_pos);
        let end = self.position_to_char_offset_clamped(max_pos);
        SearchMatch {
            start: start.min(end),
            end: start.max(end),
        }
    }

    pub(super) fn selected_line_blocks(selections: &[Selection]) -> Vec<(usize, usize)> {
        let mut lines: Vec<usize> = Vec::new();
        for sel in selections {
            let (min_pos, max_pos) = crate::selection_set::selection_min_max(sel);
            for line in min_pos.line..=max_pos.line {
                lines.push(line);
            }
        }

        lines.sort_unstable();
        lines.dedup();

        let mut blocks: Vec<(usize, usize)> = Vec::new();
        for line in lines {
            if let Some((_, end)) = blocks.last_mut()
                && *end + 1 == line
            {
                *end = line;
                continue;
            }
            blocks.push((line, line));
        }
        blocks
    }

    pub(super) fn slice_text_for_lines(&self, start_line: usize, end_line: usize) -> String {
        let line_count = self.editor.line_index.line_count();
        if line_count == 0 || start_line >= line_count || start_line > end_line {
            return String::new();
        }

        let end_line = end_line.min(line_count - 1);
        let start_offset = self
            .editor
            .line_index
            .position_to_char_offset(start_line, 0);
        let end_offset = if end_line + 1 < line_count {
            self.editor
                .line_index
                .position_to_char_offset(end_line + 1, 0)
        } else {
            self.editor.char_count()
        };

        if end_offset <= start_offset {
            return String::new();
        }

        self.editor
            .text_range(start_offset, end_offset - start_offset)
    }

    pub(super) fn execute_duplicate_lines_command(
        &mut self,
    ) -> Result<CommandResult, CommandError> {
        self.undo_redo.end_group();

        let before_char_count = self.editor.char_count();
        let before_selection = self.snapshot_selection_set();
        let selections = before_selection.selections.clone();
        let primary_index = before_selection.primary_index;

        let line_count = self.editor.line_index.line_count();
        if line_count == 0 {
            return Ok(CommandResult::Success);
        }

        let blocks = Self::selected_line_blocks(&selections);
        if blocks.is_empty() {
            return Ok(CommandResult::Success);
        }

        let doc_ends_with_newline = before_char_count > 0
            && self
                .editor
                .line_index
                .char_at(before_char_count - 1)
                .is_some_and(|ch| ch == '\n');

        struct Op {
            start_before: usize,
            start_after: usize,
            deleted_text: String,
            inserted_text: String,
            inserted_len: usize,
        }

        let mut ops: Vec<Op> = Vec::new();

        for (start_line, end_line) in blocks {
            if start_line >= line_count {
                continue;
            }
            let end_line = end_line.min(line_count - 1);

            let insertion_offset = if end_line + 1 < line_count {
                self.editor
                    .line_index
                    .position_to_char_offset(end_line + 1, 0)
            } else {
                before_char_count
            };

            let block_text = self.slice_text_for_lines(start_line, end_line);
            if block_text.is_empty() && before_char_count == 0 {
                continue;
            }

            let mut inserted_text = block_text;
            if insertion_offset == before_char_count
                && !doc_ends_with_newline
                && before_char_count > 0
            {
                inserted_text.insert(0, '\n');
            }

            let inserted_len = inserted_text.chars().count();
            if inserted_len == 0 {
                continue;
            }

            ops.push(Op {
                start_before: insertion_offset,
                start_after: insertion_offset,
                deleted_text: String::new(),
                inserted_text,
                inserted_len,
            });
        }

        if ops.is_empty() {
            return Ok(CommandResult::Success);
        }

        // Compute start_after using ascending order and delta accumulation.
        let mut asc_indices: Vec<usize> = (0..ops.len()).collect();
        asc_indices.sort_by_key(|&idx| ops[idx].start_before);

        let mut delta: i64 = 0;
        for &idx in &asc_indices {
            let op = &mut ops[idx];
            let effective_start = op.start_before as i64 + delta;
            if effective_start < 0 {
                return Err(CommandError::Other(
                    "DuplicateLines produced an invalid intermediate offset".to_string(),
                ));
            }
            op.start_after = effective_start as usize;
            delta += op.inserted_len as i64;
        }

        let apply_ops: Vec<(usize, usize, &str)> = ops
            .iter()
            .map(|op| (op.start_before, 0usize, op.inserted_text.as_str()))
            .collect();
        self.apply_text_ops(apply_ops)?;

        // Move selections/carets to the duplicated lines, VSCode-style.
        let mut mapped: Vec<Selection> = Vec::with_capacity(selections.len());

        // Precompute per-block cumulative shift (in lines) for blocks above.
        let mut block_info: Vec<(usize, usize, usize, usize)> = Vec::new(); // (start,end,size,shift_before)
        let mut cumulative = 0usize;
        let mut blocks = Self::selected_line_blocks(&selections);
        blocks.sort_by_key(|(s, _)| *s);
        for (s, e) in blocks {
            let size = e.saturating_sub(s) + 1;
            block_info.push((s, e, size, cumulative));
            cumulative = cumulative.saturating_add(size);
        }

        let line_index = &self.editor.line_index;
        for sel in selections {
            let mut start = sel.start;
            let mut end = sel.end;

            let map_line = |line: usize, info: &[(usize, usize, usize, usize)]| -> usize {
                // If inside a duplicated block, map to the duplicate copy (shift by block_size).
                for (s, e, size, shift_before) in info {
                    if line >= *s && line <= *e {
                        return line + *shift_before + *size;
                    }
                    if line < *s {
                        break;
                    }
                }

                // Otherwise, shift down by the number of duplicated lines above this line.
                let mut shift = 0usize;
                for (s, e, size, shift_before) in info {
                    let _ = shift_before;
                    if *e < line {
                        shift = shift.saturating_add(*size);
                    } else if line < *s {
                        break;
                    }
                }
                line + shift
            };

            start.line = map_line(start.line, &block_info);
            end.line = map_line(end.line, &block_info);

            start.column =
                Self::clamp_column_for_line_with_index(line_index, start.line, start.column);
            end.column = Self::clamp_column_for_line_with_index(line_index, end.line, end.column);

            mapped.push(Selection {
                start,
                end,
                direction: crate::selection_set::selection_direction(start, end),
            });
        }

        let (mapped, mapped_primary) =
            crate::selection_set::normalize_selections(mapped, primary_index);
        self.execute_cursor(CursorCommand::SetSelections {
            selections: mapped,
            primary_index: mapped_primary,
        })?;

        let after_selection = self.snapshot_selection_set();

        let edits: Vec<TextEdit> = ops
            .into_iter()
            .map(|op| TextEdit {
                start_before: op.start_before,
                start_after: op.start_after,
                deleted_text: op.deleted_text,
                inserted_text: op.inserted_text,
            })
            .collect();

        let mut delta_edits: Vec<TextDeltaEdit> = edits
            .iter()
            .map(|e| TextDeltaEdit {
                start: e.start_before,
                deleted_text: e.deleted_text.clone(),
                inserted_text: e.inserted_text.clone(),
            })
            .collect();
        delta_edits.sort_by_key(|e| std::cmp::Reverse(e.start));

        let step = UndoStep {
            group_id: 0,
            edits,
            before_selection,
            after_selection,
        };
        let group_id = self.undo_redo.push_step(step, false);

        self.last_text_delta = Some(TextDelta {
            before_char_count,
            after_char_count: self.editor.char_count(),
            edits: delta_edits,
            undo_group_id: Some(group_id),
        });

        Ok(CommandResult::Success)
    }

    pub(super) fn execute_delete_lines_command(&mut self) -> Result<CommandResult, CommandError> {
        self.undo_redo.end_group();

        let before_char_count = self.editor.char_count();
        let before_selection = self.snapshot_selection_set();
        let selections = before_selection.selections.clone();
        let primary_selection = selections
            .get(before_selection.primary_index)
            .cloned()
            .unwrap_or_else(|| selections[0].clone());

        let line_count = self.editor.line_index.line_count();
        if line_count == 0 {
            return Ok(CommandResult::Success);
        }

        let blocks = Self::selected_line_blocks(&selections);
        if blocks.is_empty() {
            return Ok(CommandResult::Success);
        }

        struct Op {
            start_before: usize,
            start_after: usize,
            delete_len: usize,
            deleted_text: String,
        }

        let mut ops: Vec<Op> = Vec::new();
        let mut primary_op_index = 0usize;

        for (idx, (start_line, end_line)) in blocks.into_iter().enumerate() {
            if start_line >= line_count {
                continue;
            }

            let end_line = end_line.min(line_count - 1);
            let mut start_offset = self
                .editor
                .line_index
                .position_to_char_offset(start_line, 0);
            let end_offset = if end_line + 1 < line_count {
                self.editor
                    .line_index
                    .position_to_char_offset(end_line + 1, 0)
            } else {
                before_char_count
            };

            if end_line + 1 >= line_count && start_offset > 0 {
                // Deleting the last line: also remove the newline before it, if any.
                start_offset = start_offset.saturating_sub(1);
            }

            if end_offset <= start_offset {
                continue;
            }

            let delete_len = end_offset - start_offset;
            let deleted_text = self.editor.text_range(start_offset, delete_len);

            if crate::selection_set::selection_contains_position_inclusive(
                &primary_selection,
                Position::new(start_line, 0),
            ) {
                primary_op_index = idx;
            }

            ops.push(Op {
                start_before: start_offset,
                start_after: start_offset,
                delete_len,
                deleted_text,
            });
        }

        if ops.is_empty() {
            return Ok(CommandResult::Success);
        }

        // Compute start_after using ascending order and delta accumulation.
        let mut asc_indices: Vec<usize> = (0..ops.len()).collect();
        asc_indices.sort_by_key(|&idx| ops[idx].start_before);

        let mut delta: i64 = 0;
        for &idx in &asc_indices {
            let op = &mut ops[idx];
            let effective_start = op.start_before as i64 + delta;
            if effective_start < 0 {
                return Err(CommandError::Other(
                    "DeleteLines produced an invalid intermediate offset".to_string(),
                ));
            }
            op.start_after = effective_start as usize;
            delta -= op.delete_len as i64;
        }

        let apply_ops: Vec<(usize, usize, &str)> = ops
            .iter()
            .map(|op| (op.start_before, op.delete_len, ""))
            .collect();
        self.apply_text_ops(apply_ops)?;

        // Collapse selection state to carets at the start of each deleted block.
        let mut new_carets: Vec<Selection> = Vec::with_capacity(ops.len());
        for op in &ops {
            let (line, column) = self
                .editor
                .line_index
                .char_offset_to_position(op.start_after);
            let pos = Position::new(line, column);
            new_carets.push(Selection {
                start: pos,
                end: pos,
                direction: SelectionDirection::Forward,
            });
        }

        let primary_index = primary_op_index.min(new_carets.len().saturating_sub(1));
        self.execute_cursor(CursorCommand::SetSelections {
            selections: new_carets,
            primary_index,
        })?;

        let after_selection = self.snapshot_selection_set();

        let edits: Vec<TextEdit> = ops
            .into_iter()
            .map(|op| TextEdit {
                start_before: op.start_before,
                start_after: op.start_after,
                deleted_text: op.deleted_text,
                inserted_text: String::new(),
            })
            .collect();

        let mut delta_edits: Vec<TextDeltaEdit> = edits
            .iter()
            .map(|e| TextDeltaEdit {
                start: e.start_before,
                deleted_text: e.deleted_text.clone(),
                inserted_text: String::new(),
            })
            .collect();
        delta_edits.sort_by_key(|e| std::cmp::Reverse(e.start));

        let step = UndoStep {
            group_id: 0,
            edits,
            before_selection,
            after_selection,
        };
        let group_id = self.undo_redo.push_step(step, false);

        self.last_text_delta = Some(TextDelta {
            before_char_count,
            after_char_count: self.editor.char_count(),
            edits: delta_edits,
            undo_group_id: Some(group_id),
        });

        Ok(CommandResult::Success)
    }

    pub(super) fn execute_move_lines_command(
        &mut self,
        up: bool,
    ) -> Result<CommandResult, CommandError> {
        self.undo_redo.end_group();

        let before_char_count = self.editor.char_count();
        let before_selection = self.snapshot_selection_set();
        let selections = before_selection.selections.clone();
        let primary_index = before_selection.primary_index;

        let line_count = self.editor.line_index.line_count();
        if line_count <= 1 {
            return Ok(CommandResult::Success);
        }

        let blocks = Self::selected_line_blocks(&selections);
        if blocks.is_empty() {
            return Ok(CommandResult::Success);
        }

        #[derive(Debug, Clone, Copy)]
        struct Block {
            start: usize,
            end: usize,
        }

        let mut moved_blocks: Vec<Block> = Vec::new();
        for (start, end) in blocks {
            let start = start.min(line_count - 1);
            let end = end.min(line_count - 1);
            if up {
                if start == 0 {
                    continue;
                }
            } else if end + 1 >= line_count {
                continue;
            }
            moved_blocks.push(Block { start, end });
        }

        if moved_blocks.is_empty() {
            return Ok(CommandResult::Success);
        }

        struct Op {
            start_before: usize,
            start_after: usize,
            delete_len: usize,
            deleted_text: String,
            inserted_text: String,
        }

        let mut ops: Vec<Op> = Vec::with_capacity(moved_blocks.len());

        for block in &moved_blocks {
            let (range_start_line, range_end_line) = if up {
                (block.start - 1, block.end)
            } else {
                (block.start, block.end + 1)
            };

            let start_offset = self
                .editor
                .line_index
                .position_to_char_offset(range_start_line, 0);
            let end_offset = if range_end_line + 1 < line_count {
                self.editor
                    .line_index
                    .position_to_char_offset(range_end_line + 1, 0)
            } else {
                before_char_count
            };

            if end_offset <= start_offset {
                continue;
            }

            let deleted_text = self
                .editor
                .text_range(start_offset, end_offset - start_offset);

            let block_text = self.slice_text_for_lines(block.start, block.end);

            let inserted_text = if up {
                let above_text = self.slice_text_for_lines(block.start - 1, block.start - 1);
                format!("{}{}", block_text, above_text)
            } else {
                let below_text = self.slice_text_for_lines(block.end + 1, block.end + 1);
                format!("{}{}", below_text, block_text)
            };

            ops.push(Op {
                start_before: start_offset,
                start_after: start_offset,
                delete_len: end_offset - start_offset,
                deleted_text,
                inserted_text,
            });
        }

        if ops.is_empty() {
            return Ok(CommandResult::Success);
        }

        // start_after is stable here (equal-length replacements), but compute for consistency.
        let mut asc_indices: Vec<usize> = (0..ops.len()).collect();
        asc_indices.sort_by_key(|&idx| ops[idx].start_before);

        let mut delta: i64 = 0;
        for &idx in &asc_indices {
            let op = &mut ops[idx];
            let effective_start = op.start_before as i64 + delta;
            if effective_start < 0 {
                return Err(CommandError::Other(
                    "MoveLines produced an invalid intermediate offset".to_string(),
                ));
            }
            op.start_after = effective_start as usize;
            let inserted_len = op.inserted_text.chars().count() as i64;
            delta += inserted_len - op.delete_len as i64;
        }

        let apply_ops: Vec<(usize, usize, &str)> = ops
            .iter()
            .map(|op| (op.start_before, op.delete_len, op.inserted_text.as_str()))
            .collect();
        self.apply_text_ops(apply_ops)?;

        // Move selections with their line blocks (and adjust displaced neighbor line).
        let line_index = &self.editor.line_index;
        let mut mapped: Vec<Selection> = Vec::with_capacity(selections.len());

        for sel in selections {
            let mut start = sel.start;
            let mut end = sel.end;

            let map_line = |line: usize, moved_blocks: &[Block], up: bool| -> usize {
                for block in moved_blocks {
                    let size = block.end.saturating_sub(block.start) + 1;
                    if line >= block.start && line <= block.end {
                        return if up { line - 1 } else { line + 1 };
                    }
                    if up && line == block.start - 1 {
                        return line + size;
                    }
                    if !up && line == block.end + 1 {
                        return line.saturating_sub(size);
                    }
                }
                line
            };

            start.line = map_line(start.line, &moved_blocks, up);
            end.line = map_line(end.line, &moved_blocks, up);

            start.column =
                Self::clamp_column_for_line_with_index(line_index, start.line, start.column);
            end.column = Self::clamp_column_for_line_with_index(line_index, end.line, end.column);

            mapped.push(Selection {
                start,
                end,
                direction: crate::selection_set::selection_direction(start, end),
            });
        }

        let (mapped, mapped_primary) =
            crate::selection_set::normalize_selections(mapped, primary_index);
        self.execute_cursor(CursorCommand::SetSelections {
            selections: mapped,
            primary_index: mapped_primary,
        })?;

        let after_selection = self.snapshot_selection_set();

        let edits: Vec<TextEdit> = ops
            .into_iter()
            .map(|op| TextEdit {
                start_before: op.start_before,
                start_after: op.start_after,
                deleted_text: op.deleted_text,
                inserted_text: op.inserted_text,
            })
            .collect();

        let mut delta_edits: Vec<TextDeltaEdit> = edits
            .iter()
            .map(|e| TextDeltaEdit {
                start: e.start_before,
                deleted_text: e.deleted_text.clone(),
                inserted_text: e.inserted_text.clone(),
            })
            .collect();
        delta_edits.sort_by_key(|e| std::cmp::Reverse(e.start));

        let step = UndoStep {
            group_id: 0,
            edits,
            before_selection,
            after_selection,
        };
        let group_id = self.undo_redo.push_step(step, false);

        self.last_text_delta = Some(TextDelta {
            before_char_count,
            after_char_count: self.editor.char_count(),
            edits: delta_edits,
            undo_group_id: Some(group_id),
        });

        Ok(CommandResult::Success)
    }

    pub(super) fn execute_join_lines_command(&mut self) -> Result<CommandResult, CommandError> {
        self.undo_redo.end_group();

        let before_char_count = self.editor.char_count();
        let before_selection = self.snapshot_selection_set();
        let selections = before_selection.selections.clone();

        let line_count = self.editor.line_index.line_count();
        if line_count <= 1 {
            return Ok(CommandResult::Success);
        }

        let mut join_lines: Vec<usize> = Vec::new();
        for sel in &selections {
            let (min_pos, max_pos) = crate::selection_set::selection_min_max(sel);
            if min_pos.line >= line_count {
                continue;
            }
            let last = max_pos.line.min(line_count - 1);
            if min_pos.line == last {
                join_lines.push(last);
            } else {
                for line in min_pos.line..last {
                    join_lines.push(line);
                }
            }
        }

        join_lines.sort_unstable();
        join_lines.dedup();
        join_lines.retain(|l| *l + 1 < line_count);

        if join_lines.is_empty() {
            return Ok(CommandResult::Success);
        }

        struct Op {
            start_before: usize,
            start_after: usize,
            delete_len: usize,
            deleted_text: String,
            inserted_text: String,
            inserted_len: usize,
        }

        let mut ops: Vec<Op> = Vec::with_capacity(join_lines.len());

        // Process from bottom to top to keep (line->offset) stable in the pre-edit document.
        join_lines.sort_by_key(|l| std::cmp::Reverse(*l));

        for line in join_lines {
            let line_text = self
                .editor
                .line_index
                .get_line_text(line)
                .unwrap_or_default();
            let next_text = self
                .editor
                .line_index
                .get_line_text(line + 1)
                .unwrap_or_default();

            let line_len = line_text.chars().count();
            let join_offset = self
                .editor
                .line_index
                .position_to_char_offset(line, line_len);
            let leading_ws = next_text
                .chars()
                .take_while(|c| *c == ' ' || *c == '\t')
                .count();
            let end_offset = self
                .editor
                .line_index
                .position_to_char_offset(line + 1, leading_ws);

            if end_offset <= join_offset {
                continue;
            }

            let left_ends_with_ws = line_text
                .chars()
                .last()
                .is_some_and(|c| c == ' ' || c == '\t');
            let right_trimmed_empty = next_text.chars().nth(leading_ws).is_none();
            let insert_space = !left_ends_with_ws && !line_text.is_empty() && !right_trimmed_empty;

            let inserted_text = if insert_space {
                " ".to_string()
            } else {
                String::new()
            };
            let inserted_len = inserted_text.chars().count();
            let delete_len = end_offset - join_offset;
            let deleted_text = self.editor.text_range(join_offset, delete_len);

            ops.push(Op {
                start_before: join_offset,
                start_after: join_offset,
                delete_len,
                deleted_text,
                inserted_text,
                inserted_len,
            });
        }

        if ops.is_empty() {
            return Ok(CommandResult::Success);
        }

        ops.sort_by_key(|op| op.start_before);

        let mut delta: i64 = 0;
        for op in &mut ops {
            let effective_start = op.start_before as i64 + delta;
            if effective_start < 0 {
                return Err(CommandError::Other(
                    "JoinLines produced an invalid intermediate offset".to_string(),
                ));
            }
            op.start_after = effective_start as usize;
            delta += op.inserted_len as i64 - op.delete_len as i64;
        }

        let apply_ops: Vec<(usize, usize, &str)> = ops
            .iter()
            .map(|op| (op.start_before, op.delete_len, op.inserted_text.as_str()))
            .collect();
        self.apply_text_ops(apply_ops)?;

        // Collapse selection state to carets at each join point.
        let mut new_carets: Vec<Selection> = Vec::with_capacity(ops.len());
        for op in &ops {
            let caret_offset = op.start_after + op.inserted_len;
            let (line, column) = self.editor.line_index.char_offset_to_position(caret_offset);
            let pos = Position::new(line, column);
            new_carets.push(Selection {
                start: pos,
                end: pos,
                direction: SelectionDirection::Forward,
            });
        }

        let (new_carets, primary_index) = crate::selection_set::normalize_selections(new_carets, 0);
        self.execute_cursor(CursorCommand::SetSelections {
            selections: new_carets,
            primary_index,
        })?;

        let after_selection = self.snapshot_selection_set();

        let edits: Vec<TextEdit> = ops
            .into_iter()
            .map(|op| TextEdit {
                start_before: op.start_before,
                start_after: op.start_after,
                deleted_text: op.deleted_text,
                inserted_text: op.inserted_text,
            })
            .collect();

        let mut delta_edits: Vec<TextDeltaEdit> = edits
            .iter()
            .map(|e| TextDeltaEdit {
                start: e.start_before,
                deleted_text: e.deleted_text.clone(),
                inserted_text: e.inserted_text.clone(),
            })
            .collect();
        delta_edits.sort_by_key(|e| std::cmp::Reverse(e.start));

        let step = UndoStep {
            group_id: 0,
            edits,
            before_selection,
            after_selection,
        };
        let group_id = self.undo_redo.push_step(step, false);

        self.last_text_delta = Some(TextDelta {
            before_char_count,
            after_char_count: self.editor.char_count(),
            edits: delta_edits,
            undo_group_id: Some(group_id),
        });

        Ok(CommandResult::Success)
    }

    pub(super) fn execute_toggle_comment_command(
        &mut self,
        config: CommentConfig,
    ) -> Result<CommandResult, CommandError> {
        if !config.has_line() && !config.has_block() {
            return Err(CommandError::Other(
                "ToggleComment requires at least one comment token".to_string(),
            ));
        }

        self.undo_redo.end_group();

        let before_char_count = self.editor.char_count();
        let before_selection = self.snapshot_selection_set();
        let selections = before_selection.selections.clone();
        let primary_index = before_selection.primary_index;

        let line_count = self.editor.line_index.line_count();
        if line_count == 0 {
            return Ok(CommandResult::Success);
        }

        let all_single_line_selections = selections.iter().all(|sel| {
            let (min_pos, max_pos) = crate::selection_set::selection_min_max(sel);
            min_pos.line == max_pos.line && min_pos != max_pos
        });

        if config.has_block()
            && all_single_line_selections
            && let (Some(block_start), Some(block_end)) =
                (config.block_start.as_deref(), config.block_end.as_deref())
        {
            return self.execute_toggle_block_comment_inline(
                block_start,
                block_end,
                before_char_count,
                before_selection,
                selections,
                primary_index,
            );
        }

        if config.has_line()
            && let Some(token) = config.line.as_deref()
        {
            return self.execute_toggle_line_comment(
                token,
                before_char_count,
                before_selection,
                selections,
                primary_index,
            );
        }

        if config.has_block()
            && let (Some(block_start), Some(block_end)) =
                (config.block_start.as_deref(), config.block_end.as_deref())
        {
            return self.execute_toggle_block_comment_lines(
                block_start,
                block_end,
                before_char_count,
                before_selection,
                selections,
                primary_index,
            );
        }

        Ok(CommandResult::Success)
    }

    pub(super) fn execute_toggle_line_comment(
        &mut self,
        token: &str,
        before_char_count: usize,
        before_selection: SelectionSetSnapshot,
        selections: Vec<Selection>,
        _primary_index: usize,
    ) -> Result<CommandResult, CommandError> {
        let token = token.trim_end();
        if token.is_empty() {
            return Ok(CommandResult::Success);
        }

        let token_len = token.chars().count();
        let insert_text = format!("{} ", token);
        let insert_len = insert_text.chars().count();

        // Collect unique target lines.
        let mut lines: Vec<usize> = Vec::new();
        for sel in &selections {
            let (min_pos, max_pos) = crate::selection_set::selection_min_max(sel);
            // A half-open selection ending at the next line's start does not
            // include that line (the same boundary used by indentation).
            let end_line = if min_pos.line < max_pos.line && max_pos.column == 0 {
                max_pos.line - 1
            } else {
                max_pos.line
            };
            for line in min_pos.line..=end_line {
                lines.push(line);
            }
        }
        lines.sort_unstable();
        lines.dedup();
        lines.retain(|l| *l < self.editor.line_index.line_count());

        if lines.is_empty() {
            return Ok(CommandResult::Success);
        }

        struct LineCommentTarget {
            line: usize,
            text: String,
            indent_col: usize,
            indent_byte: usize,
        }

        // Decide whether to comment or uncomment while scanning each line once.
        let mut targets: Vec<LineCommentTarget> = Vec::with_capacity(lines.len());
        let mut non_empty = 0usize;
        let mut all_commented = true;
        for line in lines {
            let line_text = self
                .editor
                .line_index
                .get_line_text(line)
                .unwrap_or_default();
            let (indent_col, indent_byte) = leading_horizontal_whitespace(&line_text);
            let rest = line_text.get(indent_byte..).unwrap_or("");
            if rest.is_empty() {
                targets.push(LineCommentTarget {
                    line,
                    text: line_text,
                    indent_col,
                    indent_byte,
                });
                continue;
            }
            non_empty += 1;
            if !rest.starts_with(token) {
                all_commented = false;
            }
            targets.push(LineCommentTarget {
                line,
                text: line_text,
                indent_col,
                indent_byte,
            });
        }

        let should_uncomment = non_empty > 0 && all_commented;

        struct Op {
            start_before: usize,
            start_after: usize,
            delete_len: usize,
            deleted_text: String,
            inserted_text: String,
            inserted_len: usize,
            line: usize,
            indent_col: usize,
            col_delta: isize,
        }

        let mut ops: Vec<Op> = Vec::new();

        for target in targets {
            let rest = target.text.get(target.indent_byte..).unwrap_or("");

            let start_offset = self
                .editor
                .line_index
                .position_to_char_offset(target.line, target.indent_col);

            if should_uncomment {
                if rest.is_empty() || !rest.starts_with(token) {
                    continue;
                }

                let mut remove_len = token_len;
                if let Some(ch) = rest
                    .get(token.len()..)
                    .and_then(|suffix| suffix.chars().next())
                    && ch == ' '
                {
                    remove_len += 1;
                }

                if remove_len == 0 {
                    continue;
                }

                let deleted_text = self.editor.text_range(start_offset, remove_len);
                ops.push(Op {
                    start_before: start_offset,
                    start_after: start_offset,
                    delete_len: remove_len,
                    deleted_text,
                    inserted_text: String::new(),
                    inserted_len: 0,
                    line: target.line,
                    indent_col: target.indent_col,
                    col_delta: -(remove_len as isize),
                });
            } else {
                ops.push(Op {
                    start_before: start_offset,
                    start_after: start_offset,
                    delete_len: 0,
                    deleted_text: String::new(),
                    inserted_text: insert_text.clone(),
                    inserted_len: insert_len,
                    line: target.line,
                    indent_col: target.indent_col,
                    col_delta: insert_len as isize,
                });
            }
        }

        if ops.is_empty() {
            return Ok(CommandResult::Success);
        }

        // Compute start_after.
        let mut asc_indices: Vec<usize> = (0..ops.len()).collect();
        asc_indices.sort_by_key(|&idx| ops[idx].start_before);

        let mut delta: i64 = 0;
        for &idx in &asc_indices {
            let op = &mut ops[idx];
            let effective_start = op.start_before as i64 + delta;
            if effective_start < 0 {
                return Err(CommandError::Other(
                    "ToggleComment produced an invalid intermediate offset".to_string(),
                ));
            }
            op.start_after = effective_start as usize;
            delta += op.inserted_len as i64 - op.delete_len as i64;
        }

        let apply_ops: Vec<(usize, usize, &str)> = ops
            .iter()
            .map(|op| (op.start_before, op.delete_len, op.inserted_text.as_str()))
            .collect();
        self.apply_text_ops(apply_ops)?;

        // Shift cursor/selections for touched lines, but only for columns at/after the insertion point.
        use std::collections::HashMap;
        let mut line_deltas: HashMap<usize, (usize, isize)> = HashMap::new();
        for op in &ops {
            line_deltas.insert(op.line, (op.indent_col, op.col_delta));
        }

        let line_index = &self.editor.line_index;
        let apply_delta = |pos: &mut Position, deltas: &HashMap<usize, (usize, isize)>| {
            let Some((indent_col, delta)) = deltas.get(&pos.line) else {
                return;
            };
            if pos.column < *indent_col {
                return;
            }

            let new_col = if *delta >= 0 {
                pos.column.saturating_add(*delta as usize)
            } else {
                pos.column.saturating_sub((-*delta) as usize)
            };

            pos.column = Self::clamp_column_for_line_with_index(line_index, pos.line, new_col);
        };

        apply_delta(&mut self.editor.cursor_position, &line_deltas);
        if let Some(sel) = &mut self.editor.selection {
            apply_delta(&mut sel.start, &line_deltas);
            apply_delta(&mut sel.end, &line_deltas);
        }
        for sel in &mut self.editor.secondary_selections {
            apply_delta(&mut sel.start, &line_deltas);
            apply_delta(&mut sel.end, &line_deltas);
        }

        self.normalize_cursor_and_selection();
        let after_selection = self.snapshot_selection_set();

        let edits: Vec<TextEdit> = ops
            .into_iter()
            .map(|op| TextEdit {
                start_before: op.start_before,
                start_after: op.start_after,
                deleted_text: op.deleted_text,
                inserted_text: op.inserted_text,
            })
            .collect();

        let mut delta_edits: Vec<TextDeltaEdit> = edits
            .iter()
            .map(|e| TextDeltaEdit {
                start: e.start_before,
                deleted_text: e.deleted_text.clone(),
                inserted_text: e.inserted_text.clone(),
            })
            .collect();
        delta_edits.sort_by_key(|e| std::cmp::Reverse(e.start));

        let step = UndoStep {
            group_id: 0,
            edits,
            before_selection,
            after_selection,
        };
        let group_id = self.undo_redo.push_step(step, false);

        self.last_text_delta = Some(TextDelta {
            before_char_count,
            after_char_count: self.editor.char_count(),
            edits: delta_edits,
            undo_group_id: Some(group_id),
        });

        Ok(CommandResult::Success)
    }

    pub(super) fn execute_toggle_block_comment_inline(
        &mut self,
        block_start: &str,
        block_end: &str,
        before_char_count: usize,
        before_selection: SelectionSetSnapshot,
        selections: Vec<Selection>,
        primary_index: usize,
    ) -> Result<CommandResult, CommandError> {
        let start_len = block_start.chars().count();
        let end_len = block_end.chars().count();
        if start_len == 0 || end_len == 0 {
            return Ok(CommandResult::Success);
        }

        let mut selection_ranges: Vec<SearchMatch> = selections
            .iter()
            .map(|s| self.selection_char_range(s))
            .filter(|r| r.start < r.end)
            .collect();

        if selection_ranges.is_empty() {
            return Ok(CommandResult::Success);
        }

        selection_ranges.sort_by_key(|r| (r.start, r.end));

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        enum TokenOpKind {
            Start,
            End,
        }

        // Decide per-selection whether it is already wrapped; then build ops.
        struct Op {
            start_before: usize,
            start_after: usize,
            delete_len: usize,
            deleted_text: String,
            inserted_text: String,
            inserted_len: usize,
            sel_id: usize,
            kind: TokenOpKind,
        }

        let mut ops: Vec<Op> = Vec::new();

        for (sel_id, range) in selection_ranges.iter().enumerate() {
            let start = range.start;
            let end = range.end;

            let already_wrapped = start >= start_len
                && end + end_len <= before_char_count
                && self.editor.text_range(start - start_len, start_len) == block_start
                && self.editor.text_range(end, end_len) == block_end;

            if already_wrapped {
                // Delete end token first (higher offset), then start token.
                let deleted_end = self.editor.text_range(end, end_len);
                ops.push(Op {
                    start_before: end,
                    start_after: end,
                    delete_len: end_len,
                    deleted_text: deleted_end,
                    inserted_text: String::new(),
                    inserted_len: 0,
                    sel_id,
                    kind: TokenOpKind::End,
                });

                let start_token_offset = start - start_len;
                let deleted_start = self.editor.text_range(start_token_offset, start_len);
                ops.push(Op {
                    start_before: start_token_offset,
                    start_after: start_token_offset,
                    delete_len: start_len,
                    deleted_text: deleted_start,
                    inserted_text: String::new(),
                    inserted_len: 0,
                    sel_id,
                    kind: TokenOpKind::Start,
                });
            } else {
                // Insert end token first (higher offset), then start token.
                ops.push(Op {
                    start_before: end,
                    start_after: end,
                    delete_len: 0,
                    deleted_text: String::new(),
                    inserted_text: block_end.to_string(),
                    inserted_len: end_len,
                    sel_id,
                    kind: TokenOpKind::End,
                });
                ops.push(Op {
                    start_before: start,
                    start_after: start,
                    delete_len: 0,
                    deleted_text: String::new(),
                    inserted_text: block_start.to_string(),
                    inserted_len: start_len,
                    sel_id,
                    kind: TokenOpKind::Start,
                });
            }
        }

        if ops.is_empty() {
            return Ok(CommandResult::Success);
        }

        ops.sort_by_key(|op| op.start_before);

        let mut delta: i64 = 0;
        for op in &mut ops {
            let effective_start = op.start_before as i64 + delta;
            if effective_start < 0 {
                return Err(CommandError::Other(
                    "ToggleComment produced an invalid intermediate offset".to_string(),
                ));
            }
            op.start_after = effective_start as usize;
            delta += op.inserted_len as i64 - op.delete_len as i64;
        }

        let apply_ops: Vec<(usize, usize, &str)> = ops
            .iter()
            .map(|op| (op.start_before, op.delete_len, op.inserted_text.as_str()))
            .collect();
        self.apply_text_ops(apply_ops)?;

        // Keep selections around the inner text between tokens so toggling is repeatable.
        let mut new_starts: Vec<usize> = vec![0; selection_ranges.len()];
        let mut new_ends: Vec<usize> = vec![0; selection_ranges.len()];

        for op in &ops {
            match op.kind {
                TokenOpKind::Start => {
                    new_starts[op.sel_id] = if op.inserted_len > 0 {
                        op.start_after + start_len
                    } else {
                        op.start_after
                    };
                }
                TokenOpKind::End => {
                    new_ends[op.sel_id] = op.start_after;
                }
            }
        }

        let mut next_selections: Vec<Selection> = Vec::with_capacity(selection_ranges.len());
        for i in 0..selection_ranges.len() {
            let start = new_starts[i].min(new_ends[i]);
            let end = new_starts[i].max(new_ends[i]);
            let (start_line, start_col) = self.editor.line_index.char_offset_to_position(start);
            let (end_line, end_col) = self.editor.line_index.char_offset_to_position(end);
            next_selections.push(Selection {
                start: Position::new(start_line, start_col),
                end: Position::new(end_line, end_col),
                direction: SelectionDirection::Forward,
            });
        }

        self.execute_cursor(CursorCommand::SetSelections {
            selections: next_selections,
            primary_index: primary_index.min(selection_ranges.len().saturating_sub(1)),
        })?;

        let after_selection = self.snapshot_selection_set();

        let edits: Vec<TextEdit> = ops
            .into_iter()
            .map(|op| TextEdit {
                start_before: op.start_before,
                start_after: op.start_after,
                deleted_text: op.deleted_text,
                inserted_text: op.inserted_text,
            })
            .collect();

        let mut delta_edits: Vec<TextDeltaEdit> = edits
            .iter()
            .map(|e| TextDeltaEdit {
                start: e.start_before,
                deleted_text: e.deleted_text.clone(),
                inserted_text: e.inserted_text.clone(),
            })
            .collect();
        delta_edits.sort_by_key(|e| std::cmp::Reverse(e.start));

        let step = UndoStep {
            group_id: 0,
            edits,
            before_selection,
            after_selection,
        };
        let group_id = self.undo_redo.push_step(step, false);

        self.last_text_delta = Some(TextDelta {
            before_char_count,
            after_char_count: self.editor.char_count(),
            edits: delta_edits,
            undo_group_id: Some(group_id),
        });

        Ok(CommandResult::Success)
    }

    pub(super) fn execute_toggle_block_comment_lines(
        &mut self,
        block_start: &str,
        block_end: &str,
        before_char_count: usize,
        before_selection: SelectionSetSnapshot,
        selections: Vec<Selection>,
        primary_index: usize,
    ) -> Result<CommandResult, CommandError> {
        let start_len = block_start.chars().count();
        let end_len = block_end.chars().count();
        if start_len == 0 || end_len == 0 {
            return Ok(CommandResult::Success);
        }

        let mut ranges: Vec<(usize, usize)> = Vec::new();
        for sel in &selections {
            let (min_pos, max_pos) = crate::selection_set::selection_min_max(sel);
            let start_line = min_pos.line.min(self.editor.line_index.line_count() - 1);
            let end_line = max_pos.line.min(self.editor.line_index.line_count() - 1);

            let start = self
                .editor
                .line_index
                .position_to_char_offset(start_line, 0);
            let end_line_text = self
                .editor
                .line_index
                .get_line_text(end_line)
                .unwrap_or_default();
            let end = self
                .editor
                .line_index
                .position_to_char_offset(end_line, end_line_text.chars().count());
            if start < end {
                ranges.push((start, end));
            }
        }

        ranges.sort_unstable();
        ranges.dedup();

        if ranges.is_empty() {
            return Ok(CommandResult::Success);
        }

        // Unwrap if every range already starts/ends with the tokens.
        let mut all_wrapped = true;
        for (start, end) in &ranges {
            if *end < *start + start_len + end_len {
                all_wrapped = false;
                break;
            }
            let text = self.editor.text_range(*start, end - start);
            if !text.starts_with(block_start) || !text.ends_with(block_end) {
                all_wrapped = false;
                break;
            }
        }

        struct Op {
            start_before: usize,
            start_after: usize,
            delete_len: usize,
            deleted_text: String,
            inserted_text: String,
            inserted_len: usize,
        }

        let mut ops: Vec<Op> = Vec::new();

        for (start, end) in &ranges {
            if all_wrapped {
                // Remove end token (at end-end_len) and start token (at start).
                let end_token_start = end.saturating_sub(end_len);
                let deleted_end = self.editor.text_range(end_token_start, end_len);
                ops.push(Op {
                    start_before: end_token_start,
                    start_after: end_token_start,
                    delete_len: end_len,
                    deleted_text: deleted_end,
                    inserted_text: String::new(),
                    inserted_len: 0,
                });

                let deleted_start = self.editor.text_range(*start, start_len);
                ops.push(Op {
                    start_before: *start,
                    start_after: *start,
                    delete_len: start_len,
                    deleted_text: deleted_start,
                    inserted_text: String::new(),
                    inserted_len: 0,
                });
            } else {
                // Insert end token then start token.
                ops.push(Op {
                    start_before: *end,
                    start_after: *end,
                    delete_len: 0,
                    deleted_text: String::new(),
                    inserted_text: block_end.to_string(),
                    inserted_len: end_len,
                });
                ops.push(Op {
                    start_before: *start,
                    start_after: *start,
                    delete_len: 0,
                    deleted_text: String::new(),
                    inserted_text: block_start.to_string(),
                    inserted_len: start_len,
                });
            }
        }

        ops.sort_by_key(|op| op.start_before);
        let mut delta: i64 = 0;
        for op in &mut ops {
            let effective_start = op.start_before as i64 + delta;
            if effective_start < 0 {
                return Err(CommandError::Other(
                    "ToggleComment produced an invalid intermediate offset".to_string(),
                ));
            }
            op.start_after = effective_start as usize;
            delta += op.inserted_len as i64 - op.delete_len as i64;
        }

        let apply_ops: Vec<(usize, usize, &str)> = ops
            .iter()
            .map(|op| (op.start_before, op.delete_len, op.inserted_text.as_str()))
            .collect();
        self.apply_text_ops(apply_ops)?;

        // Keep a single caret at the end of the primary range.
        let (primary_start, primary_end) = ranges
            .get(primary_index.min(ranges.len().saturating_sub(1)))
            .copied()
            .unwrap_or((0, 0));
        let caret_offset = primary_end.max(primary_start);
        let (line, column) = self.editor.line_index.char_offset_to_position(caret_offset);
        let pos = Position::new(line, column);
        self.execute_cursor(CursorCommand::SetSelections {
            selections: vec![Selection {
                start: pos,
                end: pos,
                direction: SelectionDirection::Forward,
            }],
            primary_index: 0,
        })?;

        let after_selection = self.snapshot_selection_set();

        let edits: Vec<TextEdit> = ops
            .into_iter()
            .map(|op| TextEdit {
                start_before: op.start_before,
                start_after: op.start_after,
                deleted_text: op.deleted_text,
                inserted_text: op.inserted_text,
            })
            .collect();

        let mut delta_edits: Vec<TextDeltaEdit> = edits
            .iter()
            .map(|e| TextDeltaEdit {
                start: e.start_before,
                deleted_text: e.deleted_text.clone(),
                inserted_text: e.inserted_text.clone(),
            })
            .collect();
        delta_edits.sort_by_key(|e| std::cmp::Reverse(e.start));

        let step = UndoStep {
            group_id: 0,
            edits,
            before_selection,
            after_selection,
        };
        let group_id = self.undo_redo.push_step(step, false);

        self.last_text_delta = Some(TextDelta {
            before_char_count,
            after_char_count: self.editor.char_count(),
            edits: delta_edits,
            undo_group_id: Some(group_id),
        });

        Ok(CommandResult::Success)
    }
}
