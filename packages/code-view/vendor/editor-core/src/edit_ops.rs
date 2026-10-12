//! Text editing command helpers.

use super::*;

impl CommandExecutor {
    pub(super) fn execute_edit(
        &mut self,
        command: EditCommand,
    ) -> Result<CommandResult, CommandError> {
        match command {
            EditCommand::Undo => self.execute_undo_command(),
            EditCommand::Redo => self.execute_redo_command(),
            EditCommand::EndUndoGroup => {
                self.undo_redo.end_group();
                Ok(CommandResult::Success)
            }
            EditCommand::ReplaceCurrent {
                query,
                replacement,
                options,
            } => self.execute_replace_current_command(query, replacement, options),
            EditCommand::ReplaceAll {
                query,
                replacement,
                options,
            } => self.execute_replace_all_command(query, replacement, options),
            EditCommand::DeleteToPrevTabStop => self.execute_delete_to_prev_tab_stop_command(),
            EditCommand::DeleteGraphemeBack => {
                self.execute_delete_by_boundary_command(false, TextBoundary::Grapheme)
            }
            EditCommand::DeleteGraphemeForward => {
                self.execute_delete_by_boundary_command(true, TextBoundary::Grapheme)
            }
            EditCommand::DeleteWordBack => {
                self.execute_delete_by_boundary_command(false, TextBoundary::Word)
            }
            EditCommand::DeleteWordForward => {
                self.execute_delete_by_boundary_command(true, TextBoundary::Word)
            }
            EditCommand::Backspace => self.execute_backspace_command(),
            EditCommand::DeleteForward => self.execute_delete_forward_command(),
            EditCommand::InsertText { text } => self.execute_insert_text_command(text),
            EditCommand::TypeChar { ch } => self.execute_type_char_command(ch),
            EditCommand::InsertTab => self.execute_insert_tab_command(),
            EditCommand::InsertNewline { auto_indent } => {
                self.execute_insert_newline_command(auto_indent)
            }
            EditCommand::Indent => self.execute_indent_command(false),
            EditCommand::Outdent => self.execute_indent_command(true),
            EditCommand::DuplicateLines => self.execute_duplicate_lines_command(),
            EditCommand::DeleteLines => self.execute_delete_lines_command(),
            EditCommand::MoveLinesUp => self.execute_move_lines_command(true),
            EditCommand::MoveLinesDown => self.execute_move_lines_command(false),
            EditCommand::JoinLines => self.execute_join_lines_command(),
            EditCommand::SplitLine => self.execute_insert_newline_command(false),
            EditCommand::ToggleComment { config } => self.execute_toggle_comment_command(config),
            EditCommand::ApplyTextEdits { edits } => self.execute_apply_text_edits_command(edits),
            EditCommand::ApplySnippet {
                start,
                end,
                snippet,
                additional_edits,
            } => self.execute_apply_snippet_command(start, end, snippet, additional_edits),
            EditCommand::Insert { offset, text } => self.execute_insert_command(offset, text),
            EditCommand::Delete { start, length } => self.execute_delete_command(start, length),
            EditCommand::Replace {
                start,
                length,
                text,
            } => self.execute_replace_command(start, length, text, false, None),
            EditCommand::ReplaceCoalescingUndo {
                start,
                length,
                text,
            } => self.execute_replace_command(start, length, text, true, None),
            EditCommand::ReplaceCoalescingUndoWithSelection {
                start,
                length,
                text,
                selection_start,
                selection_end,
            } => self.execute_replace_command(
                start,
                length,
                text,
                true,
                Some((selection_start, selection_end)),
            ),
        }
    }

    pub(super) fn execute_undo_command(&mut self) -> Result<CommandResult, CommandError> {
        self.undo_redo.end_group();
        let before_char_count = self.editor.char_count();
        let steps = self
            .undo_redo
            .pop_undo_group()?
            .ok_or_else(|| CommandError::Other("Nothing to undo".to_string()))?;

        let undo_group_id = steps.first().map(|s| s.group_id);
        let mut delta_edits: Vec<TextDeltaEdit> = Vec::new();

        for step in &steps {
            let mut step_edits: Vec<TextDeltaEdit> = step
                .edits
                .iter()
                .map(|edit| TextDeltaEdit {
                    start: edit.start_after,
                    deleted_text: edit.inserted_text.clone(),
                    inserted_text: edit.deleted_text.clone(),
                })
                .collect();
            step_edits.sort_by_key(|e| std::cmp::Reverse(e.start));
            delta_edits.extend(step_edits);

            self.apply_undo_edits(&step.edits)?;
            self.restore_selection_set(step.before_selection.clone());
        }

        self.last_text_delta = Some(TextDelta {
            before_char_count,
            after_char_count: self.editor.char_count(),
            edits: delta_edits,
            undo_group_id,
        });

        Ok(CommandResult::Success)
    }

    pub(super) fn execute_redo_command(&mut self) -> Result<CommandResult, CommandError> {
        self.undo_redo.end_group();
        let before_char_count = self.editor.char_count();
        let steps = self
            .undo_redo
            .pop_redo_group()?
            .ok_or_else(|| CommandError::Other("Nothing to redo".to_string()))?;

        let undo_group_id = steps.first().map(|s| s.group_id);
        let mut delta_edits: Vec<TextDeltaEdit> = Vec::new();

        for step in &steps {
            let mut step_edits: Vec<TextDeltaEdit> = step
                .edits
                .iter()
                .map(|edit| TextDeltaEdit {
                    start: edit.start_before,
                    deleted_text: edit.deleted_text.clone(),
                    inserted_text: edit.inserted_text.clone(),
                })
                .collect();
            step_edits.sort_by_key(|e| std::cmp::Reverse(e.start));
            delta_edits.extend(step_edits);

            self.apply_redo_edits(&step.edits)?;
            self.restore_selection_set(step.after_selection.clone());
        }

        self.last_text_delta = Some(TextDelta {
            before_char_count,
            after_char_count: self.editor.char_count(),
            edits: delta_edits,
            undo_group_id,
        });

        Ok(CommandResult::Success)
    }

    pub(super) fn execute_insert_text_command(
        &mut self,
        text: String,
    ) -> Result<CommandResult, CommandError> {
        // `InsertText` 的语义是“对每个 caret/selection 执行一次 replace(selection, text)”。
        //
        // 以前我们把空字符串当成完全 no-op（直接返回），但这会让“用空字符串替换选区”
        // 这种合法操作无法表达，也导致 UI 层（cut）不得不走自定义的 ApplyTextEdits 路径，
        // 并且很难保证 undo/redo 的 selection 快照一致。
        //
        // 这里保留优化：如果 text 为空，且所有 selection 都为空（仅 caret），才视为 no-op。
        if text.is_empty() {
            let primary_non_empty = self
                .editor
                .selection
                .as_ref()
                .is_some_and(|s| s.start != s.end);
            let secondary_non_empty = self
                .editor
                .secondary_selections
                .iter()
                .any(|s| s.start != s.end);
            if !primary_non_empty && !secondary_non_empty {
                return Ok(CommandResult::Success);
            }
        }

        let text = crate::text::normalize_crlf_to_lf_string(text);
        let before_char_count = self.editor.char_count();
        let before_selection = self.snapshot_selection_set();

        // Build canonical selection set (primary + secondary), VSCode-like: edits are applied
        // "simultaneously" by computing ranges in the original document and mutating in
        // descending offset order.
        let mut selections: Vec<Selection> =
            Vec::with_capacity(1 + self.editor.secondary_selections.len());
        let primary_selection = self.editor.selection.clone().unwrap_or(Selection {
            start: self.editor.cursor_position,
            end: self.editor.cursor_position,
            direction: SelectionDirection::Forward,
        });
        selections.push(primary_selection);
        selections.extend(self.editor.secondary_selections.iter().cloned());

        let (selections, primary_index) = crate::selection_set::normalize_selections(selections, 0);

        let text_char_len = text.chars().count();

        struct Op {
            selection_index: usize,
            start_offset: usize,
            start_after: usize,
            delete_len: usize,
            deleted_text: String,
            insert_text: String,
            insert_char_len: usize,
        }

        let mut ops: Vec<Op> = Vec::with_capacity(selections.len());

        for (selection_index, selection) in selections.iter().enumerate() {
            let (range_start_pos, range_end_pos) = if selection.start <= selection.end {
                (selection.start, selection.end)
            } else {
                (selection.end, selection.start)
            };

            let (start_offset, start_pad) =
                self.position_to_char_offset_and_virtual_pad(range_start_pos);
            let end_offset = self.position_to_char_offset_clamped(range_end_pos);

            let delete_len = end_offset.saturating_sub(start_offset);
            let insert_char_len = start_pad + text_char_len;

            let deleted_text = if delete_len == 0 {
                String::new()
            } else {
                self.editor.text_range(start_offset, delete_len)
            };

            let mut insert_text = String::with_capacity(text.len() + start_pad);
            for _ in 0..start_pad {
                insert_text.push(' ');
            }
            insert_text.push_str(&text);

            ops.push(Op {
                selection_index,
                start_offset,
                start_after: start_offset,
                delete_len,
                deleted_text,
                insert_text,
                insert_char_len,
            });
        }

        // Compute final caret offsets in the post-edit document (ascending order with delta),
        // while also recording each operation's start offset in the post-edit document.
        let mut asc_indices: Vec<usize> = (0..ops.len()).collect();
        asc_indices.sort_by_key(|&idx| ops[idx].start_offset);

        let mut caret_offsets: Vec<usize> = vec![0; ops.len()];
        let mut delta: i64 = 0;
        for &idx in &asc_indices {
            let op = &mut ops[idx];
            let effective_start = (op.start_offset as i64 + delta) as usize;
            op.start_after = effective_start;
            caret_offsets[op.selection_index] = effective_start + op.insert_char_len;
            delta += op.insert_char_len as i64 - op.delete_len as i64;
        }

        // Apply edits safely (descending offsets).
        let mut desc_indices = asc_indices;
        desc_indices.sort_by_key(|&idx| std::cmp::Reverse(ops[idx].start_offset));
        let interval_edits: Vec<IntervalTextEdit> = desc_indices
            .iter()
            .map(|&idx| {
                let op = &ops[idx];
                IntervalTextEdit::new(op.start_offset, op.delete_len, op.insert_char_len)
            })
            .collect();
        self.update_interval_trees_for_text_edits(&interval_edits);
        let mut folding_line_changed = false;

        for &idx in &desc_indices {
            let op = &ops[idx];

            folding_line_changed |= self.apply_text_change_to_line_index_and_layout(
                op.start_offset,
                &op.deleted_text,
                &op.insert_text,
            ) != 0;
        }

        if folding_line_changed {
            self.editor
                .folding_manager
                .clamp_to_line_count(self.editor.line_index.line_count());
        }

        // Update selection state: collapse to carets after typing.
        let mut new_carets: Vec<Selection> = Vec::with_capacity(caret_offsets.len());
        for offset in &caret_offsets {
            let (line, column) = self.editor.line_index.char_offset_to_position(*offset);
            let pos = Position::new(line, column);
            new_carets.push(Selection {
                start: pos,
                end: pos,
                direction: SelectionDirection::Forward,
            });
        }

        let (new_carets, new_primary_index) =
            crate::selection_set::normalize_selections(new_carets, primary_index);
        let primary = new_carets
            .get(new_primary_index)
            .cloned()
            .ok_or_else(|| CommandError::Other("Invalid primary caret".to_string()))?;

        self.editor.cursor_position = primary.end;
        self.editor.selection = None;
        self.editor.secondary_selections = new_carets
            .into_iter()
            .enumerate()
            .filter_map(|(idx, sel)| {
                if idx == new_primary_index {
                    None
                } else {
                    Some(sel)
                }
            })
            .collect();

        let after_selection = self.snapshot_selection_set();

        let edits: Vec<TextEdit> = ops
            .into_iter()
            .map(|op| TextEdit {
                start_before: op.start_offset,
                start_after: op.start_after,
                deleted_text: op.deleted_text,
                inserted_text: op.insert_text,
            })
            .collect();

        let is_pure_insert = edits.iter().all(|e| e.deleted_text.is_empty());
        let coalescible_insert = is_pure_insert && !text.contains('\n');

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
        let group_id = self.undo_redo.push_step(step, coalescible_insert);

        self.last_text_delta = Some(TextDelta {
            before_char_count,
            after_char_count: self.editor.char_count(),
            edits: delta_edits,
            undo_group_id: Some(group_id),
        });

        Ok(CommandResult::Success)
    }

    pub(super) fn execute_type_char_command(
        &mut self,
        ch: char,
    ) -> Result<CommandResult, CommandError> {
        if !self.auto_pairs.enabled {
            return self.execute_insert_text_command(ch.to_string());
        }

        // Treat CR as LF (internal model uses LF).
        let ch = if ch == '\r' { '\n' } else { ch };

        // Most typing paths won't call TypeChar for newline/tab, but handle it gracefully.
        if ch == '\n' {
            return self.execute_insert_newline_command(true);
        }
        if ch == '\t' {
            return self.execute_insert_tab_command();
        }

        let before_char_count = self.editor.char_count();
        let before_selection = self.snapshot_selection_set();
        let selections = before_selection.selections.clone();
        let primary_index = before_selection.primary_index;

        #[derive(Debug, Clone)]
        enum AfterSel {
            Caret {
                delta_from_start: usize,
            },
            Range {
                start_delta: usize,
                end_delta: usize,
            },
        }

        #[derive(Debug)]
        struct Op {
            selection_index: usize,
            start_offset: usize,
            start_after: usize,
            delete_len: usize,
            deleted_text: String,
            insert_text: String,
            insert_char_len: usize,
            apply: bool,
            after: AfterSel,
        }

        let mut ops: Vec<Op> = Vec::with_capacity(selections.len());

        let doc_char_count = self.editor.char_count();

        let next_char_matches = |this: &Self, offset: usize, ch: char| -> bool {
            this.editor.line_index.char_at(offset) == Some(ch)
        };

        for (selection_index, selection) in selections.iter().enumerate() {
            let (range_start_pos, range_end_pos) = if selection.start <= selection.end {
                (selection.start, selection.end)
            } else {
                (selection.end, selection.start)
            };

            let (start_offset, start_pad) =
                self.position_to_char_offset_and_virtual_pad(range_start_pos);
            let end_offset = self.position_to_char_offset_clamped(range_end_pos);

            let delete_len = end_offset.saturating_sub(start_offset);
            let deleted_text = if delete_len == 0 {
                String::new()
            } else {
                self.editor.text_range(start_offset, delete_len)
            };

            // Skip-over closing delimiter: only for empty selections/carets.
            if self.auto_pairs.skip_over_closing
                && delete_len == 0
                && start_pad == 0
                && self.auto_pairs.open_for_close(ch).is_some()
                && next_char_matches(self, start_offset, ch)
            {
                ops.push(Op {
                    selection_index,
                    start_offset,
                    start_after: start_offset,
                    delete_len: 0,
                    deleted_text: String::new(),
                    insert_text: String::new(),
                    insert_char_len: 0,
                    apply: false,
                    after: AfterSel::Caret {
                        delta_from_start: 1,
                    },
                });
                continue;
            }

            let mut insert_text = String::new();
            for _ in 0..start_pad {
                insert_text.push(' ');
            }

            let mut apply = true;
            let after = if let Some(close) = self.auto_pairs.close_for_open(ch) {
                if delete_len > 0 && self.auto_pairs.wrap_selection {
                    // Wrap selection: open + (selected) + close; keep selection on inner text.
                    insert_text.push(ch);
                    insert_text.push_str(&deleted_text);
                    insert_text.push(close);
                    AfterSel::Range {
                        start_delta: start_pad + 1,
                        end_delta: start_pad + 1 + delete_len,
                    }
                } else if delete_len == 0 {
                    // Auto-close pair and place caret between.
                    insert_text.push(ch);
                    insert_text.push(close);
                    AfterSel::Caret {
                        delta_from_start: start_pad + 1,
                    }
                } else {
                    // Replace selection with the typed opening delimiter.
                    insert_text.push(ch);
                    AfterSel::Caret {
                        delta_from_start: start_pad + 1,
                    }
                }
            } else {
                // Normal typing.
                insert_text.push(ch);
                AfterSel::Caret {
                    delta_from_start: start_pad + 1,
                }
            };

            let insert_char_len = insert_text.chars().count();
            if insert_char_len == 0 && delete_len == 0 {
                apply = false;
            }

            ops.push(Op {
                selection_index,
                start_offset,
                start_after: start_offset,
                delete_len,
                deleted_text,
                insert_text,
                insert_char_len,
                apply,
                after,
            });
        }

        // If no text edits will be applied, we still need to update the cursor/selection state
        // (e.g. skip-over closing delimiters).
        if !ops
            .iter()
            .any(|op| op.apply && (op.delete_len > 0 || !op.insert_text.is_empty()))
        {
            let mut new_selections: Vec<Selection> = Vec::with_capacity(ops.len());
            for op in &ops {
                let offset = match op.after {
                    AfterSel::Caret { delta_from_start } => op.start_offset + delta_from_start,
                    AfterSel::Range { .. } => op.start_offset,
                }
                .min(doc_char_count);

                let (line, column) = self.editor.line_index.char_offset_to_position(offset);
                let pos = Position::new(line, column);
                new_selections.push(Selection {
                    start: pos,
                    end: pos,
                    direction: SelectionDirection::Forward,
                });
            }

            let (new_selections, new_primary_index) =
                crate::selection_set::normalize_selections(new_selections, primary_index);
            let primary = new_selections
                .get(new_primary_index)
                .cloned()
                .ok_or_else(|| CommandError::Other("Invalid primary caret".to_string()))?;

            self.editor.cursor_position = primary.end;
            self.editor.selection = None;
            self.editor.secondary_selections = new_selections
                .into_iter()
                .enumerate()
                .filter_map(|(idx, sel)| (idx != new_primary_index).then_some(sel))
                .collect();

            return Ok(CommandResult::Success);
        }

        // Compute per-op post-edit start offsets (ascending by start_offset with delta).
        let mut asc_indices: Vec<usize> = (0..ops.len()).collect();
        asc_indices.sort_by_key(|&idx| ops[idx].start_offset);

        let mut delta: i64 = 0;
        for &idx in &asc_indices {
            let op = &mut ops[idx];
            let effective_start = (op.start_offset as i64 + delta) as usize;
            op.start_after = effective_start;

            if op.apply {
                delta += op.insert_char_len as i64 - op.delete_len as i64;
            }
        }

        // Apply edits safely (descending offsets).
        let mut desc_indices = asc_indices;
        desc_indices.sort_by_key(|&idx| std::cmp::Reverse(ops[idx].start_offset));
        let interval_edits: Vec<IntervalTextEdit> = desc_indices
            .iter()
            .filter_map(|&idx| {
                let op = &ops[idx];
                op.apply.then_some(IntervalTextEdit::new(
                    op.start_offset,
                    op.delete_len,
                    op.insert_char_len,
                ))
            })
            .collect();
        self.update_interval_trees_for_text_edits(&interval_edits);
        let mut folding_line_changed = false;

        for &idx in &desc_indices {
            let op = &ops[idx];
            if !op.apply {
                continue;
            }

            folding_line_changed |= self.apply_text_change_to_line_index_and_layout(
                op.start_offset,
                &op.deleted_text,
                &op.insert_text,
            ) != 0;
        }

        if folding_line_changed {
            self.editor
                .folding_manager
                .clamp_to_line_count(self.editor.line_index.line_count());
        }

        // Build post-edit selection set (one entry per original selection index).
        let mut new_selections: Vec<Selection> = vec![
            Selection {
                start: Position::new(0, 0),
                end: Position::new(0, 0),
                direction: SelectionDirection::Forward,
            };
            ops.len()
        ];

        for op in &ops {
            let base = op.start_after;
            let (start_offset, end_offset, is_range) = match op.after {
                AfterSel::Caret { delta_from_start } => {
                    let caret = base.saturating_add(delta_from_start);
                    (caret, caret, false)
                }
                AfterSel::Range {
                    start_delta,
                    end_delta,
                } => (
                    base.saturating_add(start_delta),
                    base.saturating_add(end_delta),
                    true,
                ),
            };

            let (start_line, start_col) = self
                .editor
                .line_index
                .char_offset_to_position(start_offset.min(self.editor.char_count()));
            let (end_line, end_col) = self
                .editor
                .line_index
                .char_offset_to_position(end_offset.min(self.editor.char_count()));

            let start_pos = Position::new(start_line, start_col);
            let end_pos = Position::new(end_line, end_col);

            new_selections[op.selection_index] = Selection {
                start: start_pos,
                end: end_pos,
                direction: if is_range && start_pos > end_pos {
                    SelectionDirection::Backward
                } else {
                    SelectionDirection::Forward
                },
            };
        }

        let (new_selections, new_primary_index) =
            crate::selection_set::normalize_selections(new_selections, primary_index);
        let primary = new_selections
            .get(new_primary_index)
            .cloned()
            .ok_or_else(|| CommandError::Other("Invalid primary selection".to_string()))?;

        self.editor.cursor_position = primary.end;
        self.editor.selection = if primary.start == primary.end {
            None
        } else {
            Some(primary.clone())
        };
        self.editor.secondary_selections = new_selections
            .into_iter()
            .enumerate()
            .filter_map(|(idx, sel)| (idx != new_primary_index).then_some(sel))
            .collect();

        let after_selection = self.snapshot_selection_set();

        let edits: Vec<TextEdit> = ops
            .into_iter()
            .filter(|op| op.apply)
            .map(|op| TextEdit {
                start_before: op.start_offset,
                start_after: op.start_after,
                deleted_text: op.deleted_text,
                inserted_text: op.insert_text,
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

        let is_pure_insert = edits.iter().all(|e| e.deleted_text.is_empty());
        let coalescible_insert = is_pure_insert && ch != '\n';

        let step = UndoStep {
            group_id: 0,
            edits,
            before_selection,
            after_selection,
        };
        let group_id = self.undo_redo.push_step(step, coalescible_insert);

        self.last_text_delta = Some(TextDelta {
            before_char_count,
            after_char_count: self.editor.char_count(),
            edits: delta_edits,
            undo_group_id: Some(group_id),
        });

        Ok(CommandResult::Success)
    }

    pub(super) fn bracket_pair_for_char(bracket: char) -> Option<(char, char, bool)> {
        match bracket {
            '(' => Some(('(', ')', true)),
            ')' => Some(('(', ')', false)),
            '[' => Some(('[', ']', true)),
            ']' => Some(('[', ']', false)),
            '{' => Some(('{', '}', true)),
            '}' => Some(('{', '}', false)),
            _ => None,
        }
    }

    /// Returns `(bracket_offset, open, close, is_open)` for a bracket adjacent to the caret.
    ///
    /// This follows a common editor convention:
    /// - prefer the character *before* the caret (caret is "after" a bracket)
    /// - otherwise fall back to the character *at* the caret
    pub(super) fn bracket_at_caret(
        &self,
        caret_offset: usize,
    ) -> Option<(usize, char, char, bool)> {
        let doc_char_count = self.editor.line_index.char_count();

        if caret_offset > 0
            && let Some(ch) = self.editor.line_index.char_at(caret_offset - 1)
            && let Some((open, close, is_open)) = Self::bracket_pair_for_char(ch)
        {
            return Some((caret_offset - 1, open, close, is_open));
        }

        if caret_offset < doc_char_count
            && let Some(ch) = self.editor.line_index.char_at(caret_offset)
            && let Some((open, close, is_open)) = Self::bracket_pair_for_char(ch)
        {
            return Some((caret_offset, open, close, is_open));
        }

        None
    }

    pub(super) fn matching_bracket_offset(
        &self,
        bracket_offset: usize,
        open: char,
        close: char,
        is_open: bool,
    ) -> Option<usize> {
        let doc_char_count = self.editor.line_index.char_count();
        if doc_char_count == 0 {
            return None;
        }

        if is_open {
            // Scan forward.
            let mut nesting = 0usize;
            let mut offset = bracket_offset.saturating_add(1);
            while offset < doc_char_count {
                let ch = self.editor.line_index.char_at(offset)?;
                if ch == open {
                    nesting = nesting.saturating_add(1);
                } else if ch == close {
                    if nesting == 0 {
                        return Some(offset);
                    }
                    nesting = nesting.saturating_sub(1);
                }
                offset = offset.saturating_add(1);
            }
            None
        } else {
            // Scan backward.
            if bracket_offset == 0 {
                return None;
            }

            let mut nesting = 0usize;
            let mut offset = bracket_offset;
            while offset > 0 {
                offset = offset.saturating_sub(1);
                let ch = self.editor.line_index.char_at(offset)?;
                if ch == close {
                    nesting = nesting.saturating_add(1);
                } else if ch == open {
                    if nesting == 0 {
                        return Some(offset);
                    }
                    nesting = nesting.saturating_sub(1);
                }
            }
            None
        }
    }

    pub(super) fn execute_move_to_matching_bracket_command(
        &mut self,
    ) -> Result<CommandResult, CommandError> {
        let before_selection = self.snapshot_selection_set();
        let selections = before_selection.selections.clone();
        let primary_index = before_selection.primary_index;
        let doc_char_count = self.editor.char_count();

        let mut new_carets: Vec<Selection> = Vec::with_capacity(selections.len());
        for selection in &selections {
            let caret_offset = self.position_to_char_offset_clamped(selection.end);
            let target_offset = self
                .bracket_at_caret(caret_offset)
                .and_then(|(bracket_offset, open, close, is_open)| {
                    self.matching_bracket_offset(bracket_offset, open, close, is_open)
                })
                .unwrap_or(caret_offset)
                .min(doc_char_count);

            let (line, column) = self
                .editor
                .line_index
                .char_offset_to_position(target_offset);
            let pos = Position::new(line, column);
            new_carets.push(Selection {
                start: pos,
                end: pos,
                direction: SelectionDirection::Forward,
            });
        }

        let (new_carets, new_primary_index) =
            crate::selection_set::normalize_selections(new_carets, primary_index);
        let primary = new_carets
            .get(new_primary_index)
            .cloned()
            .ok_or_else(|| CommandError::Other("Invalid primary caret".to_string()))?;

        self.editor.cursor_position = primary.end;
        self.preferred_x_cells = self
            .editor
            .logical_position_to_visual(primary.end.line, primary.end.column)
            .map(|(_, x)| x);
        self.editor.selection = None;
        self.editor.secondary_selections = new_carets
            .into_iter()
            .enumerate()
            .filter_map(|(idx, sel)| (idx != new_primary_index).then_some(sel))
            .collect();

        Ok(CommandResult::Success)
    }

    pub(super) fn execute_snippet_navigation_command(
        &mut self,
        forward: bool,
    ) -> Result<CommandResult, CommandError> {
        let Some(session) = self.snippet_session.as_mut() else {
            return Ok(CommandResult::Success);
        };

        let action = if forward {
            session.next()
        } else {
            session.prev()
        };
        match action {
            SnippetNavigation::Noop => Ok(CommandResult::Success),
            SnippetNavigation::Finish(offset) => {
                let doc_char_count = self.editor.char_count();
                let target = offset.min(doc_char_count);
                let (line, column) = self.editor.line_index.char_offset_to_position(target);
                let pos = Position::new(line, column);

                self.editor.cursor_position = pos;
                self.editor.selection = None;
                self.editor.secondary_selections.clear();
                self.preferred_x_cells = self
                    .editor
                    .logical_position_to_visual(pos.line, pos.column)
                    .map(|(_, x)| x);

                self.snippet_session = None;
                Ok(CommandResult::Success)
            }
            SnippetNavigation::SelectRanges(ranges) => {
                if ranges.is_empty() {
                    return Ok(CommandResult::Success);
                }

                let doc_char_count = self.editor.char_count();
                let mut selections: Vec<Selection> = Vec::with_capacity(ranges.len());
                for (start, end) in ranges {
                    let a = start.min(doc_char_count);
                    let b = end.min(doc_char_count);
                    let (s_line, s_col) = self.editor.line_index.char_offset_to_position(a);
                    let (e_line, e_col) = self.editor.line_index.char_offset_to_position(b);
                    selections.push(Selection {
                        start: Position::new(s_line, s_col),
                        end: Position::new(e_line, e_col),
                        direction: SelectionDirection::Forward,
                    });
                }

                let (selections, primary_index) =
                    crate::selection_set::normalize_selections(selections, 0);
                self.restore_selection_set(SelectionSetSnapshot {
                    selections,
                    primary_index,
                });

                let pos = self.editor.cursor_position;
                self.preferred_x_cells = self
                    .editor
                    .logical_position_to_visual(pos.line, pos.column)
                    .map(|(_, x)| x);

                Ok(CommandResult::Success)
            }
        }
    }

    pub(super) fn execute_update_bracket_match_highlights_command(
        &mut self,
    ) -> Result<CommandResult, CommandError> {
        let snapshot = self.snapshot_selection_set();
        let selections = snapshot.selections;

        let mut intervals: Vec<(usize, usize, StyleId)> =
            Vec::with_capacity(selections.len().saturating_mul(2));

        for selection in &selections {
            let caret_offset = self.position_to_char_offset_clamped(selection.end);
            let Some((bracket_offset, open, close, is_open)) = self.bracket_at_caret(caret_offset)
            else {
                continue;
            };
            let Some(other_offset) =
                self.matching_bracket_offset(bracket_offset, open, close, is_open)
            else {
                continue;
            };

            intervals.push((
                bracket_offset,
                bracket_offset.saturating_add(1),
                crate::MATCH_HIGHLIGHT_STYLE_ID,
            ));
            intervals.push((
                other_offset,
                other_offset.saturating_add(1),
                crate::MATCH_HIGHLIGHT_STYLE_ID,
            ));
        }

        intervals.sort_unstable();
        intervals.dedup();

        if intervals.is_empty() {
            self.editor
                .style_layers
                .remove(&StyleLayerId::BRACKET_MATCHES);
            return Ok(CommandResult::Success);
        }

        let tree = self
            .editor
            .style_layers
            .entry(StyleLayerId::BRACKET_MATCHES)
            .or_default();
        tree.clear();
        for (start, end, style_id) in intervals {
            if start < end {
                tree.insert(crate::intervals::Interval::new(start, end, style_id));
            }
        }

        Ok(CommandResult::Success)
    }

    pub(super) fn execute_clear_bracket_match_highlights_command(
        &mut self,
    ) -> Result<CommandResult, CommandError> {
        self.editor
            .style_layers
            .remove(&StyleLayerId::BRACKET_MATCHES);
        Ok(CommandResult::Success)
    }

    pub(super) fn execute_insert_tab_command(&mut self) -> Result<CommandResult, CommandError> {
        let before_char_count = self.editor.char_count();
        let before_selection = self.snapshot_selection_set();

        let mut selections: Vec<Selection> =
            Vec::with_capacity(1 + self.editor.secondary_selections.len());
        let primary_selection = self.editor.selection.clone().unwrap_or(Selection {
            start: self.editor.cursor_position,
            end: self.editor.cursor_position,
            direction: SelectionDirection::Forward,
        });
        selections.push(primary_selection);
        selections.extend(self.editor.secondary_selections.iter().cloned());

        let (selections, primary_index) = crate::selection_set::normalize_selections(selections, 0);

        // If the selection spans multiple lines (including "Select Line" style selections) or
        // covers a whole logical line, Tab should indent the selected lines.
        //
        // This mirrors common code editor behavior (VS Code, etc) and avoids the surprising
        // "replace selection with whitespace" behavior for multi-line selections.
        let should_indent_lines = selections.iter().any(|selection| {
            if selection.start == selection.end {
                return false;
            }

            let (min_pos, max_pos) = crate::selection_set::selection_min_max(selection);
            if min_pos.line != max_pos.line {
                return true;
            }

            if min_pos.column != 0 {
                return false;
            }

            let line_text = self
                .editor
                .line_index
                .get_line_text(min_pos.line)
                .unwrap_or_default();
            let line_len = line_text.chars().count();
            max_pos.column >= line_len
        });

        if should_indent_lines {
            return self.execute_indent_command(false);
        }

        let tab_width = self.editor.layout_engine.tab_width();

        struct Op {
            selection_index: usize,
            start_offset: usize,
            start_after: usize,
            delete_len: usize,
            deleted_text: String,
            insert_text: String,
            insert_char_len: usize,
        }

        let mut ops: Vec<Op> = Vec::with_capacity(selections.len());

        for (selection_index, selection) in selections.iter().enumerate() {
            let (range_start_pos, range_end_pos) = if selection.start <= selection.end {
                (selection.start, selection.end)
            } else {
                (selection.end, selection.start)
            };

            let (start_offset, start_pad) =
                self.position_to_char_offset_and_virtual_pad(range_start_pos);
            let end_offset = self.position_to_char_offset_clamped(range_end_pos);

            let delete_len = end_offset.saturating_sub(start_offset);

            let deleted_text = if delete_len == 0 {
                String::new()
            } else {
                self.editor.text_range(start_offset, delete_len)
            };

            // Compute cell X within the logical line at the insertion position (including virtual pad).
            let line_text = self
                .editor
                .line_index
                .get_line_text(range_start_pos.line)
                .unwrap_or_default();
            let line_char_len = line_text.chars().count();
            let clamped_col = range_start_pos.column.min(line_char_len);
            let x_in_line =
                visual_x_for_column(&line_text, clamped_col, tab_width).saturating_add(start_pad);

            let mut insert_text = String::new();
            for _ in 0..start_pad {
                insert_text.push(' ');
            }

            match self.tab_key_behavior {
                TabKeyBehavior::Tab => {
                    insert_text.push('\t');
                    ops.push(Op {
                        selection_index,
                        start_offset,
                        start_after: start_offset,
                        delete_len,
                        deleted_text,
                        insert_text,
                        insert_char_len: start_pad + 1,
                    });
                }
                TabKeyBehavior::Spaces => {
                    let tab_width = tab_width.max(1);
                    let rem = x_in_line % tab_width;
                    let spaces = tab_width - rem;
                    for _ in 0..spaces {
                        insert_text.push(' ');
                    }

                    ops.push(Op {
                        selection_index,
                        start_offset,
                        start_after: start_offset,
                        delete_len,
                        deleted_text,
                        insert_text,
                        insert_char_len: start_pad + spaces,
                    });
                }
            }
        }

        // Compute final caret offsets in the post-edit document (ascending order with delta),
        // while also recording each operation's start offset in the post-edit document.
        let mut asc_indices: Vec<usize> = (0..ops.len()).collect();
        asc_indices.sort_by_key(|&idx| ops[idx].start_offset);

        let mut caret_offsets: Vec<usize> = vec![0; ops.len()];
        let mut delta: i64 = 0;
        for &idx in &asc_indices {
            let op = &mut ops[idx];
            let effective_start = (op.start_offset as i64 + delta) as usize;
            op.start_after = effective_start;
            caret_offsets[op.selection_index] = effective_start + op.insert_char_len;
            delta += op.insert_char_len as i64 - op.delete_len as i64;
        }

        // Apply edits safely (descending offsets).
        let mut desc_indices = asc_indices;
        desc_indices.sort_by_key(|&idx| std::cmp::Reverse(ops[idx].start_offset));
        let interval_edits: Vec<IntervalTextEdit> = desc_indices
            .iter()
            .map(|&idx| {
                let op = &ops[idx];
                IntervalTextEdit::new(op.start_offset, op.delete_len, op.insert_char_len)
            })
            .collect();
        self.update_interval_trees_for_text_edits(&interval_edits);
        let mut folding_line_changed = false;

        for &idx in &desc_indices {
            let op = &ops[idx];

            folding_line_changed |= self.apply_text_change_to_line_index_and_layout(
                op.start_offset,
                &op.deleted_text,
                &op.insert_text,
            ) != 0;
        }

        if folding_line_changed {
            self.editor
                .folding_manager
                .clamp_to_line_count(self.editor.line_index.line_count());
        }

        // Update selection state: collapse to carets after insertion.
        let mut new_carets: Vec<Selection> = Vec::with_capacity(caret_offsets.len());
        for offset in &caret_offsets {
            let (line, column) = self.editor.line_index.char_offset_to_position(*offset);
            let pos = Position::new(line, column);
            new_carets.push(Selection {
                start: pos,
                end: pos,
                direction: SelectionDirection::Forward,
            });
        }

        let (new_carets, new_primary_index) =
            crate::selection_set::normalize_selections(new_carets, primary_index);
        let primary = new_carets
            .get(new_primary_index)
            .cloned()
            .ok_or_else(|| CommandError::Other("Invalid primary caret".to_string()))?;

        self.editor.cursor_position = primary.end;
        self.editor.selection = None;
        self.editor.secondary_selections = new_carets
            .into_iter()
            .enumerate()
            .filter_map(|(idx, sel)| {
                if idx == new_primary_index {
                    None
                } else {
                    Some(sel)
                }
            })
            .collect();

        let after_selection = self.snapshot_selection_set();

        let edits: Vec<TextEdit> = ops
            .into_iter()
            .map(|op| TextEdit {
                start_before: op.start_offset,
                start_after: op.start_after,
                deleted_text: op.deleted_text,
                inserted_text: op.insert_text,
            })
            .collect();

        let is_pure_insert = edits.iter().all(|e| e.deleted_text.is_empty());
        let coalescible_insert = is_pure_insert;

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
        let group_id = self.undo_redo.push_step(step, coalescible_insert);

        self.last_text_delta = Some(TextDelta {
            before_char_count,
            after_char_count: self.editor.char_count(),
            edits: delta_edits,
            undo_group_id: Some(group_id),
        });

        Ok(CommandResult::Success)
    }

    pub(super) fn leading_whitespace_prefix(line_text: &str) -> String {
        line_text
            .chars()
            .take_while(|ch| *ch == ' ' || *ch == '\t')
            .collect()
    }

    pub(super) fn split_at_char(s: &str, char_idx: usize) -> (&str, &str) {
        let byte_idx = s
            .char_indices()
            .nth(char_idx)
            .map(|(b, _)| b)
            .unwrap_or_else(|| s.len());
        s.split_at(byte_idx)
    }

    pub(super) fn last_non_space_or_tab(s: &str) -> Option<char> {
        s.chars().rev().find(|ch| *ch != ' ' && *ch != '\t')
    }

    pub(super) fn first_non_space_or_tab(s: &str) -> Option<char> {
        s.chars().find(|ch| *ch != ' ' && *ch != '\t')
    }

    pub(super) fn matching_closer_for(open: char) -> Option<char> {
        match open {
            '{' => Some('}'),
            '[' => Some(']'),
            '(' => Some(')'),
            _ => None,
        }
    }

    pub(super) fn indent_unit_for_newline(&self) -> String {
        match self.indentation_config.style {
            IndentStyle::Tabs => "\t".to_string(),
            IndentStyle::Spaces(width) => " ".repeat(width.max(1) as usize),
        }
    }

    pub(super) fn indent_unit(&self) -> String {
        match self.tab_key_behavior {
            TabKeyBehavior::Tab => "\t".to_string(),
            TabKeyBehavior::Spaces => " ".repeat(self.editor.layout_engine.tab_width().max(1)),
        }
    }

    pub(super) fn execute_insert_newline_command(
        &mut self,
        auto_indent: bool,
    ) -> Result<CommandResult, CommandError> {
        // Newline insertion should not coalesce into a typing group.
        self.undo_redo.end_group();

        let before_char_count = self.editor.char_count();
        let before_selection = self.snapshot_selection_set();

        // Canonical selection set (primary + secondary).
        let mut selections: Vec<Selection> =
            Vec::with_capacity(1 + self.editor.secondary_selections.len());
        let primary_selection = self.editor.selection.clone().unwrap_or(Selection {
            start: self.editor.cursor_position,
            end: self.editor.cursor_position,
            direction: SelectionDirection::Forward,
        });
        selections.push(primary_selection);
        selections.extend(self.editor.secondary_selections.iter().cloned());

        let (selections, primary_index) = crate::selection_set::normalize_selections(selections, 0);

        struct Op {
            selection_index: usize,
            start_offset: usize,
            start_after: usize,
            delete_len: usize,
            deleted_text: String,
            insert_text: String,
            insert_char_len: usize,
            caret_offset_in_insert: usize,
        }

        let mut ops: Vec<Op> = Vec::with_capacity(selections.len());

        for (selection_index, selection) in selections.iter().enumerate() {
            let (range_start_pos, range_end_pos) =
                crate::selection_set::selection_min_max(selection);

            let start_offset = self.position_to_char_offset_clamped(range_start_pos);
            let end_offset = self.position_to_char_offset_clamped(range_end_pos);

            let delete_len = end_offset.saturating_sub(start_offset);
            let deleted_text = if delete_len == 0 {
                String::new()
            } else {
                self.editor.text_range(start_offset, delete_len)
            };

            let (insert_text, caret_offset_in_insert) = if auto_indent {
                let line_text = self
                    .editor
                    .line_index
                    .get_line_text(range_start_pos.line)
                    .unwrap_or_default();
                let base_indent = Self::leading_whitespace_prefix(&line_text);
                let indent_unit = self.indent_unit_for_newline();

                let (before_cursor, after_cursor) =
                    Self::split_at_char(&line_text, range_start_pos.column);
                let before_last = Self::last_non_space_or_tab(before_cursor);
                let after_first = Self::first_non_space_or_tab(after_cursor);

                // Special-case: between a matching pair like `{|}` → insert a blank indented line
                // and keep the closing delimiter aligned.
                if let Some(open) = before_last
                    && self.indentation_config.indent_triggers.contains(&open)
                    && Self::matching_closer_for(open)
                        .is_some_and(|close| after_first == Some(close))
                {
                    let inner_indent = format!("{base_indent}{indent_unit}");
                    let insert = format!("\n{inner_indent}\n{base_indent}");
                    let caret = 1 + inner_indent.chars().count();
                    (insert, caret)
                } else {
                    let mut indent = base_indent;
                    if let Some(last) = before_last
                        && self.indentation_config.indent_triggers.contains(&last)
                    {
                        indent.push_str(&indent_unit);
                    }

                    let insert = format!("\n{indent}");
                    let caret = insert.chars().count();
                    (insert, caret)
                }
            } else {
                ("\n".to_string(), 1)
            };

            let insert_char_len = insert_text.chars().count();

            ops.push(Op {
                selection_index,
                start_offset,
                start_after: start_offset,
                delete_len,
                deleted_text,
                insert_text,
                insert_char_len,
                caret_offset_in_insert,
            });
        }

        // Compute final caret offsets in the post-edit document (ascending order with delta),
        // while also recording each operation's start offset in the post-edit document.
        let mut asc_indices: Vec<usize> = (0..ops.len()).collect();
        asc_indices.sort_by_key(|&idx| ops[idx].start_offset);

        let mut caret_offsets: Vec<usize> = vec![0; ops.len()];
        let mut delta: i64 = 0;
        for &idx in &asc_indices {
            let op = &mut ops[idx];
            let effective_start = (op.start_offset as i64 + delta) as usize;
            op.start_after = effective_start;
            caret_offsets[op.selection_index] = effective_start + op.caret_offset_in_insert;
            delta += op.insert_char_len as i64 - op.delete_len as i64;
        }

        // Apply edits safely (descending offsets).
        let mut desc_indices = asc_indices;
        desc_indices.sort_by_key(|&idx| std::cmp::Reverse(ops[idx].start_offset));
        let interval_edits: Vec<IntervalTextEdit> = desc_indices
            .iter()
            .map(|&idx| {
                let op = &ops[idx];
                IntervalTextEdit::new(op.start_offset, op.delete_len, op.insert_char_len)
            })
            .collect();
        self.update_interval_trees_for_text_edits(&interval_edits);
        let mut folding_line_changed = false;

        for &idx in &desc_indices {
            let op = &ops[idx];

            folding_line_changed |= self.apply_text_change_to_line_index_and_layout(
                op.start_offset,
                &op.deleted_text,
                &op.insert_text,
            ) != 0;
        }

        if folding_line_changed {
            self.editor
                .folding_manager
                .clamp_to_line_count(self.editor.line_index.line_count());
        }

        // Update selection state: collapse to carets after insertion.
        let mut new_carets: Vec<Selection> = Vec::with_capacity(caret_offsets.len());
        for offset in &caret_offsets {
            let (line, column) = self.editor.line_index.char_offset_to_position(*offset);
            let pos = Position::new(line, column);
            new_carets.push(Selection {
                start: pos,
                end: pos,
                direction: SelectionDirection::Forward,
            });
        }

        let (new_carets, new_primary_index) =
            crate::selection_set::normalize_selections(new_carets, primary_index);
        let primary = new_carets
            .get(new_primary_index)
            .cloned()
            .ok_or_else(|| CommandError::Other("Invalid primary caret".to_string()))?;

        self.editor.cursor_position = primary.end;
        self.editor.selection = None;
        self.editor.secondary_selections = new_carets
            .into_iter()
            .enumerate()
            .filter_map(|(idx, sel)| {
                if idx == new_primary_index {
                    None
                } else {
                    Some(sel)
                }
            })
            .collect();

        let after_selection = self.snapshot_selection_set();

        let edits: Vec<TextEdit> = ops
            .into_iter()
            .map(|op| TextEdit {
                start_before: op.start_offset,
                start_after: op.start_after,
                deleted_text: op.deleted_text,
                inserted_text: op.insert_text,
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

    pub(super) fn execute_indent_command(
        &mut self,
        outdent: bool,
    ) -> Result<CommandResult, CommandError> {
        self.undo_redo.end_group();

        let before_char_count = self.editor.char_count();
        let before_selection = self.snapshot_selection_set();
        let selections = before_selection.selections.clone();

        let mut lines: Vec<usize> = Vec::new();
        for sel in &selections {
            let (min_pos, max_pos) = crate::selection_set::selection_min_max(sel);
            // Selections are treated as half-open ranges at the text-edit boundary (`end` is
            // exclusive). For line-based operations like indent/outdent, do not include the
            // trailing line when the selection ends at column 0 of the next line (common for
            // "Select Line" style selections).
            let end_line = if min_pos.line < max_pos.line && max_pos.column == 0 {
                max_pos.line.saturating_sub(1)
            } else {
                max_pos.line
            };

            for line in min_pos.line..=end_line {
                lines.push(line);
            }
        }
        lines.sort_unstable();
        lines.dedup();

        if lines.is_empty() {
            return Ok(CommandResult::Success);
        }

        let tab_width = self.editor.layout_engine.tab_width().max(1);
        let indent_unit = self.indent_unit();
        let indent_chars = indent_unit.chars().count();

        #[derive(Debug)]
        struct Op {
            start_offset: usize,
            start_after: usize,
            delete_len: usize,
            deleted_text: String,
            insert_text: String,
            insert_len: usize,
        }

        let mut ops: Vec<Op> = Vec::new();
        let mut line_deltas: std::collections::HashMap<usize, isize> =
            std::collections::HashMap::new();

        for line in lines {
            if line >= self.editor.line_index.line_count() {
                continue;
            }

            let start_offset = self.editor.line_index.position_to_char_offset(line, 0);
            let line_text = self
                .editor
                .line_index
                .get_line_text(line)
                .unwrap_or_default();

            if outdent {
                let mut remove_len = 0usize;
                if let Some(first) = line_text.chars().next() {
                    if first == '\t' {
                        remove_len = 1;
                    } else if first == ' ' {
                        let leading_spaces = line_text.chars().take_while(|c| *c == ' ').count();
                        remove_len = leading_spaces.min(tab_width);
                    }
                }

                if remove_len == 0 {
                    continue;
                }

                let deleted_text = self.editor.text_range(start_offset, remove_len);
                ops.push(Op {
                    start_offset,
                    start_after: start_offset,
                    delete_len: remove_len,
                    deleted_text,
                    insert_text: String::new(),
                    insert_len: 0,
                });
                line_deltas.insert(line, -(remove_len as isize));
            } else {
                if indent_chars == 0 {
                    continue;
                }

                ops.push(Op {
                    start_offset,
                    start_after: start_offset,
                    delete_len: 0,
                    deleted_text: String::new(),
                    insert_text: indent_unit.clone(),
                    insert_len: indent_chars,
                });
                line_deltas.insert(line, indent_chars as isize);
            }
        }

        if ops.is_empty() {
            return Ok(CommandResult::Success);
        }

        // Compute start_after using ascending order and delta accumulation.
        let mut asc_indices: Vec<usize> = (0..ops.len()).collect();
        asc_indices.sort_by_key(|&idx| ops[idx].start_offset);

        let mut delta: i64 = 0;
        for &idx in &asc_indices {
            let op = &mut ops[idx];
            let effective_start = (op.start_offset as i64 + delta) as usize;
            op.start_after = effective_start;
            delta += op.insert_len as i64 - op.delete_len as i64;
        }

        // Apply ops descending so offsets remain valid.
        let mut desc_indices = asc_indices;
        desc_indices.sort_by_key(|&idx| std::cmp::Reverse(ops[idx].start_offset));
        let interval_edits: Vec<IntervalTextEdit> = desc_indices
            .iter()
            .map(|&idx| {
                let op = &ops[idx];
                IntervalTextEdit::new(op.start_offset, op.delete_len, op.insert_len)
            })
            .collect();
        self.update_interval_trees_for_text_edits(&interval_edits);

        for &idx in &desc_indices {
            let op = &ops[idx];

            self.apply_text_change_to_line_index_and_layout(
                op.start_offset,
                &op.deleted_text,
                &op.insert_text,
            );
        }

        // Shift cursor/selections for touched lines.
        let line_index = &self.editor.line_index;
        let apply_delta = |pos: &mut Position, deltas: &std::collections::HashMap<usize, isize>| {
            let Some(delta) = deltas.get(&pos.line) else {
                return;
            };

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
        self.preferred_x_cells = self
            .editor
            .logical_position_to_visual(
                self.editor.cursor_position.line,
                self.editor.cursor_position.column,
            )
            .map(|(_, x)| x);

        let after_selection = self.snapshot_selection_set();

        let edits: Vec<TextEdit> = ops
            .into_iter()
            .map(|op| TextEdit {
                start_before: op.start_offset,
                start_after: op.start_after,
                deleted_text: op.deleted_text,
                inserted_text: op.insert_text,
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

    pub(super) fn execute_apply_text_edits_command(
        &mut self,
        mut edits: Vec<TextEditSpec>,
    ) -> Result<CommandResult, CommandError> {
        self.undo_redo.end_group();

        if edits.is_empty() {
            return Ok(CommandResult::Success);
        }

        let before_char_count = self.editor.char_count();
        let before_selection = self.snapshot_selection_set();

        let max_offset = before_char_count;

        for edit in &mut edits {
            if edit.start > edit.end {
                return Err(CommandError::InvalidRange {
                    start: edit.start,
                    end: edit.end,
                });
            }
            if edit.end > max_offset {
                return Err(CommandError::InvalidRange {
                    start: edit.start,
                    end: edit.end,
                });
            }
            edit.text = crate::text::normalize_crlf_to_lf_string(edit.text.clone());
        }

        edits.sort_by_key(|e| (e.start, e.end));

        // Validate non-overlap (pre-edit coordinates).
        let mut prev_end = 0usize;
        for (idx, edit) in edits.iter().enumerate() {
            if idx > 0 && edit.start < prev_end {
                return Err(CommandError::Other(
                    "ApplyTextEdits requires non-overlapping edits".to_string(),
                ));
            }
            prev_end = prev_end.max(edit.end);
        }

        struct Op {
            start_before: usize,
            start_after: usize,
            delete_len: usize,
            deleted_text: String,
            inserted_text: String,
            inserted_len: usize,
        }

        let mut ops: Vec<Op> = Vec::with_capacity(edits.len());
        for edit in edits {
            let delete_len = edit.end.saturating_sub(edit.start);
            let deleted_text = if delete_len == 0 {
                String::new()
            } else {
                self.editor.text_range(edit.start, delete_len)
            };

            let inserted_text = edit.text;
            let inserted_len = inserted_text.chars().count();

            ops.push(Op {
                start_before: edit.start,
                start_after: edit.start,
                delete_len,
                deleted_text,
                inserted_text,
                inserted_len,
            });
        }

        // Compute start_after using ascending order and delta accumulation.
        let mut delta: i64 = 0;
        for op in &mut ops {
            let effective_start = op.start_before as i64 + delta;
            if effective_start < 0 {
                return Err(CommandError::Other(
                    "ApplyTextEdits produced an invalid intermediate offset".to_string(),
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

    pub(super) fn execute_apply_snippet_command(
        &mut self,
        start: usize,
        end: usize,
        snippet: String,
        additional_edits: Vec<TextEditSpec>,
    ) -> Result<CommandResult, CommandError> {
        self.undo_redo.end_group();

        // A new snippet insert replaces any existing snippet session.
        self.snippet_session = None;

        let before_char_count = self.editor.char_count();
        let before_selection = self.snapshot_selection_set();

        if start > end {
            return Err(CommandError::InvalidRange { start, end });
        }
        if end > before_char_count {
            return Err(CommandError::InvalidRange { start, end });
        }

        let template = parse_snippet(&snippet);

        let mut edits: Vec<TextEditSpec> = Vec::with_capacity(1 + additional_edits.len());
        edits.push(TextEditSpec {
            start,
            end,
            text: template.text.clone(),
        });
        edits.extend(additional_edits);

        let max_offset = before_char_count;
        for edit in &mut edits {
            if edit.start > edit.end {
                return Err(CommandError::InvalidRange {
                    start: edit.start,
                    end: edit.end,
                });
            }
            if edit.end > max_offset {
                return Err(CommandError::InvalidRange {
                    start: edit.start,
                    end: edit.end,
                });
            }
            edit.text = crate::text::normalize_crlf_to_lf_string(edit.text.clone());
        }

        edits.sort_by_key(|e| (e.start, e.end));

        // Validate non-overlap (pre-edit coordinates).
        let mut prev_end = 0usize;
        for (idx, edit) in edits.iter().enumerate() {
            if idx > 0 && edit.start < prev_end {
                return Err(CommandError::Other(
                    "ApplySnippet requires non-overlapping edits".to_string(),
                ));
            }
            prev_end = prev_end.max(edit.end);
        }

        struct Op {
            start_before: usize,
            start_after: usize,
            delete_len: usize,
            deleted_text: String,
            inserted_text: String,
            inserted_len: usize,
            is_main: bool,
        }

        let mut ops: Vec<Op> = Vec::with_capacity(edits.len());
        for edit in edits {
            let delete_len = edit.end.saturating_sub(edit.start);
            let deleted_text = if delete_len == 0 {
                String::new()
            } else {
                self.editor.text_range(edit.start, delete_len)
            };

            let inserted_text = edit.text;
            let inserted_len = inserted_text.chars().count();

            ops.push(Op {
                start_before: edit.start,
                start_after: edit.start,
                delete_len,
                deleted_text,
                inserted_text,
                inserted_len,
                is_main: edit.start == start && edit.end == end,
            });
        }

        // Compute start_after using ascending order and delta accumulation.
        let mut delta: i64 = 0;
        for op in &mut ops {
            let effective_start = op.start_before as i64 + delta;
            if effective_start < 0 {
                return Err(CommandError::Other(
                    "ApplySnippet produced an invalid intermediate offset".to_string(),
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

        // Figure out where the expanded snippet text begins in the post-edit document.
        let main_start_after = ops
            .iter()
            .find(|op| op.is_main)
            .map(|op| op.start_after)
            .unwrap_or(start);

        // Activate snippet session + select first tabstop (or jump to `$0` when there are none).
        if let Some(session) = SnippetSession::new(main_start_after, &template) {
            let ranges = session.current_ranges();
            if ranges.is_empty() {
                // Defensive fallback: treat as "no tabstops".
                self.snippet_session = None;
            } else {
                let doc_char_count = self.editor.char_count();
                let mut selections: Vec<Selection> = Vec::with_capacity(ranges.len());
                for (a, b) in ranges {
                    let start = a.min(doc_char_count);
                    let end = b.min(doc_char_count);
                    let (s_line, s_col) = self.editor.line_index.char_offset_to_position(start);
                    let (e_line, e_col) = self.editor.line_index.char_offset_to_position(end);
                    selections.push(Selection {
                        start: Position::new(s_line, s_col),
                        end: Position::new(e_line, e_col),
                        direction: SelectionDirection::Forward,
                    });
                }

                let (selections, primary_index) =
                    crate::selection_set::normalize_selections(selections, 0);
                self.restore_selection_set(SelectionSetSnapshot {
                    selections,
                    primary_index,
                });

                let pos = self.editor.cursor_position;
                self.preferred_x_cells = self
                    .editor
                    .logical_position_to_visual(pos.line, pos.column)
                    .map(|(_, x)| x);

                self.snippet_session = Some(session);
            }
        } else {
            let doc_char_count = self.editor.char_count();
            let target = main_start_after
                .saturating_add(template.final_offset)
                .min(doc_char_count);
            let (line, column) = self.editor.line_index.char_offset_to_position(target);
            let pos = Position::new(line, column);
            self.editor.cursor_position = pos;
            self.editor.selection = None;
            self.editor.secondary_selections.clear();
            self.preferred_x_cells = self
                .editor
                .logical_position_to_visual(pos.line, pos.column)
                .map(|(_, x)| x);
        }

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

    pub(super) fn word_token_range_in_line(
        &self,
        line_text: &str,
        column: usize,
    ) -> Option<(usize, usize)> {
        if line_text.is_empty() {
            return None;
        }

        let chars: Vec<char> = line_text.chars().collect();
        if chars.is_empty() {
            return None;
        }

        let len = chars.len();
        let col = column.min(len);
        let idx = if col < len {
            col
        } else {
            len.saturating_sub(1)
        };

        let classify = |ch: char| self.editor.word_boundary.is_word_token_char(ch);

        let token_span_at = |i: usize| -> (usize, usize) {
            let is_token = classify(chars[i]);
            if !is_token {
                return (i, i + 1);
            }
            if chars[i].is_ascii() && self.editor.word_boundary.is_ascii_word_char(chars[i]) {
                let mut start = i;
                while start > 0
                    && chars[start - 1].is_ascii()
                    && self
                        .editor
                        .word_boundary
                        .is_ascii_word_char(chars[start - 1])
                {
                    start -= 1;
                }
                let mut end = i + 1;
                while end < len
                    && chars[end].is_ascii()
                    && self.editor.word_boundary.is_ascii_word_char(chars[end])
                {
                    end += 1;
                }
                (start, end)
            } else {
                (i, i + 1)
            }
        };

        // Prefer the token under the caret (or the last char if at EOL).
        if classify(chars[idx]) {
            return Some(token_span_at(idx));
        }

        // Search to the right.
        #[allow(clippy::needless_range_loop)]
        for i in (idx + 1)..len {
            if classify(chars[i]) {
                return Some(token_span_at(i));
            }
        }

        // Search to the left.
        let mut i = idx;
        while i > 0 {
            i -= 1;
            if classify(chars[i]) {
                return Some(token_span_at(i));
            }
        }

        None
    }

    pub(super) fn execute_insert_command(
        &mut self,
        offset: usize,
        text: String,
    ) -> Result<CommandResult, CommandError> {
        if text.is_empty() {
            return Err(CommandError::EmptyText);
        }

        let text = crate::text::normalize_crlf_to_lf_string(text);
        let max_offset = self.editor.char_count();
        if offset > max_offset {
            return Err(CommandError::InvalidOffset(offset));
        }

        let before_char_count = self.editor.char_count();
        let before_selection = self.snapshot_selection_set();

        // Update line index + layout engine incrementally.
        let line_delta = self.apply_text_change_to_line_index_and_layout(offset, "", &text);

        if line_delta != 0 {
            self.editor
                .folding_manager
                .clamp_to_line_count(self.editor.line_index.line_count());
        }

        let inserted_len = text.chars().count();

        // Update interval tree offsets
        self.update_interval_trees_for_text_edits(&[IntervalTextEdit::new(
            offset,
            0,
            inserted_len,
        )]);

        // Ensure cursor/selection still within valid range
        self.normalize_cursor_and_selection();

        let after_selection = self.snapshot_selection_set();

        let step = UndoStep {
            group_id: 0,
            edits: vec![TextEdit {
                start_before: offset,
                start_after: offset,
                deleted_text: String::new(),
                inserted_text: text.clone(),
            }],
            before_selection,
            after_selection,
        };

        let coalescible_insert = !text.contains('\n');
        let group_id = self.undo_redo.push_step(step, coalescible_insert);

        self.last_text_delta = Some(TextDelta {
            before_char_count,
            after_char_count: self.editor.char_count(),
            edits: vec![TextDeltaEdit {
                start: offset,
                deleted_text: String::new(),
                inserted_text: text,
            }],
            undo_group_id: Some(group_id),
        });

        Ok(CommandResult::Success)
    }

    pub(super) fn execute_delete_command(
        &mut self,
        start: usize,
        length: usize,
    ) -> Result<CommandResult, CommandError> {
        if length == 0 {
            return Ok(CommandResult::Success);
        }

        let before_char_count = self.editor.char_count();
        let max_offset = self.editor.char_count();
        if start > max_offset {
            return Err(CommandError::InvalidOffset(start));
        }
        let end = start
            .checked_add(length)
            .ok_or(CommandError::InvalidRange {
                start,
                end: usize::MAX,
            })?;
        if end > max_offset {
            return Err(CommandError::InvalidRange { start, end });
        }

        let before_selection = self.snapshot_selection_set();

        let deleted_text = self.editor.text_range(start, length);
        let delta_deleted_text = deleted_text.clone();
        // Update line index + layout engine incrementally.
        let line_delta =
            self.apply_text_change_to_line_index_and_layout(start, &delta_deleted_text, "");

        if line_delta != 0 {
            self.editor
                .folding_manager
                .clamp_to_line_count(self.editor.line_index.line_count());
        }

        // Update interval tree offsets
        self.update_interval_trees_for_text_edits(&[IntervalTextEdit::new(start, length, 0)]);

        // Ensure cursor/selection still within valid range
        self.normalize_cursor_and_selection();

        let after_selection = self.snapshot_selection_set();

        let step = UndoStep {
            group_id: 0,
            edits: vec![TextEdit {
                start_before: start,
                start_after: start,
                deleted_text,
                inserted_text: String::new(),
            }],
            before_selection,
            after_selection,
        };
        let group_id = self.undo_redo.push_step(step, false);

        self.last_text_delta = Some(TextDelta {
            before_char_count,
            after_char_count: self.editor.char_count(),
            edits: vec![TextDeltaEdit {
                start,
                deleted_text: delta_deleted_text,
                inserted_text: String::new(),
            }],
            undo_group_id: Some(group_id),
        });

        Ok(CommandResult::Success)
    }

    pub(super) fn execute_replace_command(
        &mut self,
        start: usize,
        length: usize,
        text: String,
        coalesce_undo: bool,
        selection_after: Option<(usize, usize)>,
    ) -> Result<CommandResult, CommandError> {
        let before_char_count = self.editor.char_count();
        let max_offset = self.editor.char_count();
        if start > max_offset {
            return Err(CommandError::InvalidOffset(start));
        }
        let end = start
            .checked_add(length)
            .ok_or(CommandError::InvalidRange {
                start,
                end: usize::MAX,
            })?;
        if end > max_offset {
            return Err(CommandError::InvalidRange { start, end });
        }

        if length == 0 && text.is_empty() {
            return Ok(CommandResult::Success);
        }

        let text = crate::text::normalize_crlf_to_lf_string(text);
        let before_selection = self.snapshot_selection_set();

        let deleted_text = if length == 0 {
            String::new()
        } else {
            self.editor.text_range(start, length)
        };
        let delta_deleted_text = deleted_text.clone();
        let delta_inserted_text = text.clone();

        let inserted_len = text.chars().count();
        // Apply as a single operation (delete then insert at the same offset).
        self.update_interval_trees_for_text_edits(&[IntervalTextEdit::new(
            start,
            length,
            inserted_len,
        )]);

        // Update line index + layout engine incrementally.
        let line_delta =
            self.apply_text_change_to_line_index_and_layout(start, &deleted_text, &text);

        if line_delta != 0 {
            self.editor
                .folding_manager
                .clamp_to_line_count(self.editor.line_index.line_count());
        }

        // Ensure cursor/selection still valid.
        self.normalize_cursor_and_selection();

        // Optional: set selection/caret as part of this edit command (so hosts can update IME
        // preedit selection without breaking the undo group via a separate cursor command).
        if let Some((mut sel_start_off, mut sel_end_off)) = selection_after {
            let doc_char_count = self.editor.char_count();
            sel_start_off = sel_start_off.min(doc_char_count);
            sel_end_off = sel_end_off.min(doc_char_count);

            let (a_line, a_col) = self
                .editor
                .line_index
                .char_offset_to_position(sel_start_off);
            let (b_line, b_col) = self.editor.line_index.char_offset_to_position(sel_end_off);

            let start_pos = Position::new(a_line, a_col);
            let end_pos = Position::new(b_line, b_col);

            self.editor.cursor_position = end_pos;
            self.editor.secondary_selections.clear();
            if sel_start_off == sel_end_off {
                self.editor.selection = None;
            } else {
                self.editor.selection = Some(Selection {
                    start: start_pos,
                    end: end_pos,
                    direction: crate::selection_set::selection_direction(start_pos, end_pos),
                });
            }

            self.preferred_x_cells = self
                .editor
                .logical_position_to_visual(end_pos.line, end_pos.column)
                .map(|(_, x)| x);
        }

        let after_selection = self.snapshot_selection_set();

        let step = UndoStep {
            group_id: 0,
            edits: vec![TextEdit {
                start_before: start,
                start_after: start,
                deleted_text,
                inserted_text: text,
            }],
            before_selection,
            after_selection,
        };
        let group_id = if coalesce_undo {
            self.undo_redo.push_explicit_coalescing_step(step)
        } else {
            self.undo_redo.push_step(step, false)
        };

        self.last_text_delta = Some(TextDelta {
            before_char_count,
            after_char_count: self.editor.char_count(),
            edits: vec![TextDeltaEdit {
                start,
                deleted_text: delta_deleted_text,
                inserted_text: delta_inserted_text,
            }],
            undo_group_id: Some(group_id),
        });

        Ok(CommandResult::Success)
    }

    pub(super) fn cursor_char_offset(&self) -> usize {
        self.position_to_char_offset_clamped(self.editor.cursor_position)
    }

    pub(super) fn primary_selection_char_range(&self) -> Option<SearchMatch> {
        let selection = self.editor.selection.as_ref()?;
        let (min_pos, max_pos) = crate::selection_set::selection_min_max(selection);
        let start = self.position_to_char_offset_clamped(min_pos);
        let end = self.position_to_char_offset_clamped(max_pos);
        if start == end {
            None
        } else {
            Some(SearchMatch { start, end })
        }
    }

    pub(super) fn set_primary_selection_by_char_range(&mut self, range: SearchMatch) {
        let (start_line, start_col) = self.editor.line_index.char_offset_to_position(range.start);
        let (end_line, end_col) = self.editor.line_index.char_offset_to_position(range.end);

        self.editor.cursor_position = Position::new(end_line, end_col);
        self.editor.secondary_selections.clear();

        if range.start == range.end {
            self.editor.selection = None;
        } else {
            self.editor.selection = Some(Selection {
                start: Position::new(start_line, start_col),
                end: Position::new(end_line, end_col),
                direction: SelectionDirection::Forward,
            });
        }
    }

    pub(super) fn execute_find_command(
        &mut self,
        query: String,
        options: SearchOptions,
        forward: bool,
    ) -> Result<CommandResult, CommandError> {
        if query.is_empty() {
            return Ok(CommandResult::SearchNotFound);
        }

        let text = self.editor.get_text();
        let from = if let Some(selection) = self.primary_selection_char_range() {
            if forward {
                selection.end
            } else {
                selection.start
            }
        } else {
            self.cursor_char_offset()
        };

        let found = if forward {
            find_next(&text, &query, options, from)
        } else {
            find_prev(&text, &query, options, from)
        }
        .map_err(|err| CommandError::Other(err.to_string()))?;

        let Some(m) = found else {
            return Ok(CommandResult::SearchNotFound);
        };

        self.set_primary_selection_by_char_range(m);

        Ok(CommandResult::SearchMatch {
            start: m.start,
            end: m.end,
        })
    }

    pub(super) fn compile_user_regex(
        query: &str,
        options: SearchOptions,
    ) -> Result<regex::Regex, CommandError> {
        RegexBuilder::new(query)
            .case_insensitive(!options.case_sensitive)
            .multi_line(true)
            .build()
            .map_err(|err| CommandError::Other(format!("Invalid regex: {}", err)))
    }

    pub(super) fn regex_expand_replacement(
        re: &regex::Regex,
        text: &str,
        index: &CharIndex,
        range: SearchMatch,
        replacement: &str,
    ) -> Result<String, CommandError> {
        let start_byte = index.char_to_byte(range.start);
        let end_byte = index.char_to_byte(range.end);

        let caps = re
            .captures_at(text, start_byte)
            .ok_or_else(|| CommandError::Other("Regex match not found".to_string()))?;
        let whole = caps
            .get(0)
            .ok_or_else(|| CommandError::Other("Regex match missing capture 0".to_string()))?;
        if whole.start() != start_byte || whole.end() != end_byte {
            return Err(CommandError::Other(
                "Regex match did not align with the selected range".to_string(),
            ));
        }

        let mut expanded = String::new();
        caps.expand(replacement, &mut expanded);
        Ok(expanded)
    }

    pub(super) fn execute_replace_current_command(
        &mut self,
        query: String,
        replacement: String,
        options: SearchOptions,
    ) -> Result<CommandResult, CommandError> {
        if query.is_empty() {
            return Err(CommandError::Other("Search query is empty".to_string()));
        }

        let text = self.editor.get_text();
        let selection_range = self.primary_selection_char_range();

        let mut target = None::<SearchMatch>;
        if let Some(range) = selection_range {
            let is_match = crate::search::is_match_exact(&text, &query, options, range)
                .map_err(|err| CommandError::Other(err.to_string()))?;
            if is_match {
                target = Some(range);
            }
        }

        if target.is_none() {
            let from = self.cursor_char_offset();
            target = find_next(&text, &query, options, from)
                .map_err(|err| CommandError::Other(err.to_string()))?;
        }

        let Some(target) = target else {
            return Err(CommandError::Other("No match found".to_string()));
        };

        let index = CharIndex::new(&text);
        let inserted_text = if options.regex {
            let re = Self::compile_user_regex(&query, options)?;
            Self::regex_expand_replacement(&re, &text, &index, target, &replacement)?
        } else {
            replacement
        };
        let inserted_text = crate::text::normalize_crlf_to_lf_string(inserted_text);

        let deleted_text = self.editor.text_range(target.start, target.len());
        let before_char_count = self.editor.char_count();
        let delta_deleted_text = deleted_text.clone();

        let before_selection = self.snapshot_selection_set();
        self.apply_text_ops(vec![(target.start, target.len(), inserted_text.as_str())])?;

        let inserted_len = inserted_text.chars().count();
        let new_range = SearchMatch {
            start: target.start,
            end: target.start + inserted_len,
        };
        self.set_primary_selection_by_char_range(new_range);
        let after_selection = self.snapshot_selection_set();

        let step = UndoStep {
            group_id: 0,
            edits: vec![TextEdit {
                start_before: target.start,
                start_after: target.start,
                deleted_text,
                inserted_text: inserted_text.clone(),
            }],
            before_selection,
            after_selection,
        };
        let group_id = self.undo_redo.push_step(step, false);

        self.last_text_delta = Some(TextDelta {
            before_char_count,
            after_char_count: self.editor.char_count(),
            edits: vec![TextDeltaEdit {
                start: target.start,
                deleted_text: delta_deleted_text,
                inserted_text,
            }],
            undo_group_id: Some(group_id),
        });

        Ok(CommandResult::ReplaceResult { replaced: 1 })
    }

    pub(super) fn execute_replace_all_command(
        &mut self,
        query: String,
        replacement: String,
        options: SearchOptions,
    ) -> Result<CommandResult, CommandError> {
        if query.is_empty() {
            return Err(CommandError::Other("Search query is empty".to_string()));
        }

        let replacement = crate::text::normalize_crlf_to_lf_string(replacement);
        let text = self.editor.get_text();
        let matches =
            find_all(&text, &query, options).map_err(|err| CommandError::Other(err.to_string()))?;
        if matches.is_empty() {
            return Err(CommandError::Other("No match found".to_string()));
        }
        let match_count = matches.len();

        let index = CharIndex::new(&text);

        struct Op {
            start_before: usize,
            start_after: usize,
            delete_len: usize,
            deleted_text: String,
            inserted_text: String,
            inserted_len: usize,
        }

        let mut ops: Vec<Op> = Vec::with_capacity(match_count);
        if options.regex {
            let re = Self::compile_user_regex(&query, options)?;
            for m in matches {
                let deleted_text = {
                    let start_byte = index.char_to_byte(m.start);
                    let end_byte = index.char_to_byte(m.end);
                    text.get(start_byte..end_byte)
                        .unwrap_or_default()
                        .to_string()
                };
                let inserted_text =
                    Self::regex_expand_replacement(&re, &text, &index, m, &replacement)?;
                let inserted_text = crate::text::normalize_crlf_to_lf_string(inserted_text);
                let inserted_len = inserted_text.chars().count();
                ops.push(Op {
                    start_before: m.start,
                    start_after: m.start,
                    delete_len: m.len(),
                    deleted_text,
                    inserted_text,
                    inserted_len,
                });
            }
        } else {
            let inserted_len = replacement.chars().count();
            for m in matches {
                let deleted_text = {
                    let start_byte = index.char_to_byte(m.start);
                    let end_byte = index.char_to_byte(m.end);
                    text.get(start_byte..end_byte)
                        .unwrap_or_default()
                        .to_string()
                };
                ops.push(Op {
                    start_before: m.start,
                    start_after: m.start,
                    delete_len: m.len(),
                    deleted_text,
                    inserted_text: replacement.clone(),
                    inserted_len,
                });
            }
        }

        ops.sort_by_key(|op| op.start_before);

        let mut delta: i64 = 0;
        for op in &mut ops {
            let effective_start = op.start_before as i64 + delta;
            if effective_start < 0 {
                return Err(CommandError::Other(
                    "ReplaceAll produced an invalid intermediate offset".to_string(),
                ));
            }
            op.start_after = effective_start as usize;
            delta += op.inserted_len as i64 - op.delete_len as i64;
        }

        let before_char_count = self.editor.char_count();
        let before_selection = self.snapshot_selection_set();
        let apply_ops: Vec<(usize, usize, &str)> = ops
            .iter()
            .map(|op| (op.start_before, op.delete_len, op.inserted_text.as_str()))
            .collect();
        self.apply_text_ops(apply_ops)?;

        if let Some(first) = ops.first() {
            let caret_end = first.start_after + first.inserted_len;
            let select_end = if first.inserted_len == 0 {
                first.start_after
            } else {
                caret_end
            };
            self.set_primary_selection_by_char_range(SearchMatch {
                start: first.start_after,
                end: select_end,
            });
        } else {
            self.editor.selection = None;
            self.editor.secondary_selections.clear();
        }

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

        Ok(CommandResult::ReplaceResult {
            replaced: match_count,
        })
    }

    pub(super) fn execute_backspace_command(&mut self) -> Result<CommandResult, CommandError> {
        self.execute_delete_like_command(false)
    }

    pub(super) fn execute_delete_forward_command(&mut self) -> Result<CommandResult, CommandError> {
        self.execute_delete_like_command(true)
    }

    pub(super) fn execute_delete_to_prev_tab_stop_command(
        &mut self,
    ) -> Result<CommandResult, CommandError> {
        // Treat like a delete-like action: end any open insert coalescing group, even if it turns out
        // to be a no-op.
        self.undo_redo.end_group();

        let before_selection = self.snapshot_selection_set();
        let selections = before_selection.selections.clone();
        let primary_index = before_selection.primary_index;

        let tab_width = self.editor.layout_engine.tab_width().max(1);

        #[derive(Debug)]
        struct Op {
            selection_index: usize,
            start_offset: usize,
            delete_len: usize,
            deleted_text: String,
            start_after: usize,
        }

        let mut ops: Vec<Op> = Vec::with_capacity(selections.len());

        for (selection_index, selection) in selections.iter().enumerate() {
            let (range_start_pos, range_end_pos) = if selection.start <= selection.end {
                (selection.start, selection.end)
            } else {
                (selection.end, selection.start)
            };

            let (start_offset, end_offset) = if range_start_pos != range_end_pos {
                let start_offset = self.position_to_char_offset_clamped(range_start_pos);
                let end_offset = self.position_to_char_offset_clamped(range_end_pos);
                if start_offset <= end_offset {
                    (start_offset, end_offset)
                } else {
                    (end_offset, start_offset)
                }
            } else {
                let caret = selection.end;
                let caret_offset = self.position_to_char_offset_clamped(caret);
                if caret_offset == 0 {
                    (0, 0)
                } else {
                    let line_text = self
                        .editor
                        .line_index
                        .get_line_text(caret.line)
                        .unwrap_or_default();
                    let line_char_len = line_text.chars().count();
                    let col = caret.column.min(line_char_len);

                    let in_leading_whitespace = line_text
                        .chars()
                        .take(col)
                        .all(|ch| ch == ' ' || ch == '\t');

                    if !in_leading_whitespace {
                        (caret_offset - 1, caret_offset)
                    } else {
                        let x_in_line = visual_x_for_column(&line_text, col, tab_width);
                        let back = if x_in_line == 0 {
                            0
                        } else {
                            let rem = x_in_line % tab_width;
                            if rem == 0 { tab_width } else { rem }
                        };
                        let target_x = x_in_line.saturating_sub(back);

                        let mut target_col = col;
                        while target_col > 0 {
                            let prev_col = target_col - 1;
                            let prev_x = visual_x_for_column(&line_text, prev_col, tab_width);
                            if prev_x < target_x {
                                break;
                            }
                            target_col = prev_col;
                            if prev_x == target_x {
                                break;
                            }
                        }

                        let target_offset = self
                            .editor
                            .line_index
                            .position_to_char_offset(caret.line, target_col);
                        (target_offset, caret_offset)
                    }
                }
            };

            let delete_len = end_offset.saturating_sub(start_offset);
            let deleted_text = if delete_len == 0 {
                String::new()
            } else {
                self.editor.text_range(start_offset, delete_len)
            };

            ops.push(Op {
                selection_index,
                start_offset,
                delete_len,
                deleted_text,
                start_after: start_offset,
            });
        }

        if !ops.iter().any(|op| op.delete_len > 0) {
            return Ok(CommandResult::Success);
        }

        let before_char_count = self.editor.char_count();

        // Compute caret offsets in the post-delete document (ascending order with delta).
        let mut asc_indices: Vec<usize> = (0..ops.len()).collect();
        asc_indices.sort_by_key(|&idx| ops[idx].start_offset);

        let mut caret_offsets: Vec<usize> = vec![0; ops.len()];
        let mut delta: i64 = 0;
        for &idx in &asc_indices {
            let op = &mut ops[idx];
            let effective_start = (op.start_offset as i64 + delta) as usize;
            op.start_after = effective_start;
            caret_offsets[op.selection_index] = effective_start;
            delta -= op.delete_len as i64;
        }

        // Apply deletes descending to keep offsets valid.
        let mut desc_indices = asc_indices;
        desc_indices.sort_by_key(|&idx| std::cmp::Reverse(ops[idx].start_offset));
        let interval_edits: Vec<IntervalTextEdit> = desc_indices
            .iter()
            .map(|&idx| {
                let op = &ops[idx];
                IntervalTextEdit::new(op.start_offset, op.delete_len, 0)
            })
            .collect();
        self.update_interval_trees_for_text_edits(&interval_edits);
        let mut folding_line_changed = false;

        for &idx in &desc_indices {
            let op = &ops[idx];
            if op.delete_len == 0 {
                continue;
            }

            folding_line_changed |= self.apply_text_change_to_line_index_and_layout(
                op.start_offset,
                &op.deleted_text,
                "",
            ) != 0;
        }

        if folding_line_changed {
            self.editor
                .folding_manager
                .clamp_to_line_count(self.editor.line_index.line_count());
        }

        // Collapse selection state to carets at the start of deleted ranges.
        let mut new_carets: Vec<Selection> = Vec::with_capacity(caret_offsets.len());
        for offset in &caret_offsets {
            let (line, column) = self.editor.line_index.char_offset_to_position(*offset);
            let pos = Position::new(line, column);
            new_carets.push(Selection {
                start: pos,
                end: pos,
                direction: SelectionDirection::Forward,
            });
        }

        let (new_carets, new_primary_index) =
            crate::selection_set::normalize_selections(new_carets, primary_index);
        let primary = new_carets
            .get(new_primary_index)
            .cloned()
            .ok_or_else(|| CommandError::Other("Invalid primary caret".to_string()))?;

        self.editor.cursor_position = primary.end;
        self.editor.selection = None;
        self.editor.secondary_selections = new_carets
            .into_iter()
            .enumerate()
            .filter_map(|(idx, sel)| {
                if idx == new_primary_index {
                    None
                } else {
                    Some(sel)
                }
            })
            .collect();

        let after_selection = self.snapshot_selection_set();

        let edits: Vec<TextEdit> = ops
            .into_iter()
            .map(|op| TextEdit {
                start_before: op.start_offset,
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

    pub(super) fn execute_delete_by_boundary_command(
        &mut self,
        forward: bool,
        boundary: TextBoundary,
    ) -> Result<CommandResult, CommandError> {
        // Any delete-like action should end an open insert coalescing group, even if it turns out
        // to be a no-op.
        self.undo_redo.end_group();

        let before_selection = self.snapshot_selection_set();
        let selections = before_selection.selections.clone();
        let primary_index = before_selection.primary_index;

        let doc_char_count = self.editor.char_count();

        #[derive(Debug)]
        struct Op {
            selection_index: usize,
            start_offset: usize,
            delete_len: usize,
            deleted_text: String,
            start_after: usize,
        }

        let mut ops: Vec<Op> = Vec::with_capacity(selections.len());

        for (selection_index, selection) in selections.iter().enumerate() {
            let (range_start_pos, range_end_pos) = if selection.start <= selection.end {
                (selection.start, selection.end)
            } else {
                (selection.end, selection.start)
            };

            let (start_offset, end_offset) = if range_start_pos != range_end_pos {
                let start_offset = self.position_to_char_offset_clamped(range_start_pos);
                let end_offset = self.position_to_char_offset_clamped(range_end_pos);
                if start_offset <= end_offset {
                    (start_offset, end_offset)
                } else {
                    (end_offset, start_offset)
                }
            } else {
                let caret = selection.end;
                let caret_offset = self.position_to_char_offset_clamped(caret);
                let line_count = self.editor.line_index.line_count();
                let line = caret.line.min(line_count.saturating_sub(1));
                let line_text = self
                    .editor
                    .line_index
                    .get_line_text(line)
                    .unwrap_or_default();
                let line_char_len = line_text.chars().count();
                let col = caret.column.min(line_char_len);

                if forward {
                    if caret_offset >= doc_char_count {
                        (caret_offset, caret_offset)
                    } else if col >= line_char_len {
                        (caret_offset, (caret_offset + 1).min(doc_char_count))
                    } else {
                        let next_col = next_boundary_column(&line_text, col, boundary);
                        let start_offset =
                            self.editor.line_index.position_to_char_offset(line, col);
                        let end_offset = self
                            .editor
                            .line_index
                            .position_to_char_offset(line, next_col);
                        (start_offset, end_offset)
                    }
                } else if caret_offset == 0 {
                    (0, 0)
                } else if col == 0 {
                    (caret_offset - 1, caret_offset)
                } else {
                    let prev_col = prev_boundary_column(&line_text, col, boundary);
                    let start_offset = self
                        .editor
                        .line_index
                        .position_to_char_offset(line, prev_col);
                    let end_offset = self.editor.line_index.position_to_char_offset(line, col);
                    (start_offset, end_offset)
                }
            };

            let delete_len = end_offset.saturating_sub(start_offset);
            let deleted_text = if delete_len == 0 {
                String::new()
            } else {
                self.editor.text_range(start_offset, delete_len)
            };

            ops.push(Op {
                selection_index,
                start_offset,
                delete_len,
                deleted_text,
                start_after: start_offset,
            });
        }

        if !ops.iter().any(|op| op.delete_len > 0) {
            return Ok(CommandResult::Success);
        }

        let before_char_count = self.editor.char_count();

        // Compute caret offsets in the post-delete document (ascending order with delta).
        let mut asc_indices: Vec<usize> = (0..ops.len()).collect();
        asc_indices.sort_by_key(|&idx| ops[idx].start_offset);

        let mut caret_offsets: Vec<usize> = vec![0; ops.len()];
        let mut delta: i64 = 0;
        for &idx in &asc_indices {
            let op = &mut ops[idx];
            let effective_start = (op.start_offset as i64 + delta) as usize;
            op.start_after = effective_start;
            caret_offsets[op.selection_index] = effective_start;
            delta -= op.delete_len as i64;
        }

        // Apply deletes descending to keep offsets valid.
        let mut desc_indices = asc_indices;
        desc_indices.sort_by_key(|&idx| std::cmp::Reverse(ops[idx].start_offset));
        let interval_edits: Vec<IntervalTextEdit> = desc_indices
            .iter()
            .map(|&idx| {
                let op = &ops[idx];
                IntervalTextEdit::new(op.start_offset, op.delete_len, 0)
            })
            .collect();
        self.update_interval_trees_for_text_edits(&interval_edits);
        let mut folding_line_changed = false;

        for &idx in &desc_indices {
            let op = &ops[idx];
            if op.delete_len == 0 {
                continue;
            }

            folding_line_changed |= self.apply_text_change_to_line_index_and_layout(
                op.start_offset,
                &op.deleted_text,
                "",
            ) != 0;
        }

        if folding_line_changed {
            self.editor
                .folding_manager
                .clamp_to_line_count(self.editor.line_index.line_count());
        }

        // Collapse selection state to carets at the start of deleted ranges.
        let mut new_carets: Vec<Selection> = Vec::with_capacity(caret_offsets.len());
        for offset in &caret_offsets {
            let (line, column) = self.editor.line_index.char_offset_to_position(*offset);
            let pos = Position::new(line, column);
            new_carets.push(Selection {
                start: pos,
                end: pos,
                direction: SelectionDirection::Forward,
            });
        }

        let (new_carets, new_primary_index) =
            crate::selection_set::normalize_selections(new_carets, primary_index);
        let primary = new_carets
            .get(new_primary_index)
            .cloned()
            .ok_or_else(|| CommandError::Other("Invalid primary caret".to_string()))?;

        self.editor.cursor_position = primary.end;
        self.editor.selection = None;
        self.editor.secondary_selections = new_carets
            .into_iter()
            .enumerate()
            .filter_map(|(idx, sel)| {
                if idx == new_primary_index {
                    None
                } else {
                    Some(sel)
                }
            })
            .collect();

        let after_selection = self.snapshot_selection_set();

        let edits: Vec<TextEdit> = ops
            .into_iter()
            .map(|op| TextEdit {
                start_before: op.start_offset,
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

    pub(super) fn execute_delete_like_command(
        &mut self,
        forward: bool,
    ) -> Result<CommandResult, CommandError> {
        // Any delete-like action should end an open insert coalescing group, even if it turns out
        // to be a no-op (e.g. backspace at the beginning of the document).
        self.undo_redo.end_group();

        let before_selection = self.snapshot_selection_set();
        let selections = before_selection.selections.clone();
        let primary_index = before_selection.primary_index;

        let doc_char_count = self.editor.char_count();

        #[derive(Debug)]
        struct Op {
            selection_index: usize,
            start_offset: usize,
            delete_len: usize,
            deleted_text: String,
            start_after: usize,
        }

        let mut ops: Vec<Op> = Vec::with_capacity(selections.len());

        for (selection_index, selection) in selections.iter().enumerate() {
            let (range_start_pos, range_end_pos) = if selection.start <= selection.end {
                (selection.start, selection.end)
            } else {
                (selection.end, selection.start)
            };

            let (start_offset, end_offset) = if range_start_pos != range_end_pos {
                let start_offset = self.position_to_char_offset_clamped(range_start_pos);
                let end_offset = self.position_to_char_offset_clamped(range_end_pos);
                if start_offset <= end_offset {
                    (start_offset, end_offset)
                } else {
                    (end_offset, start_offset)
                }
            } else {
                let caret_offset = self.position_to_char_offset_clamped(selection.end);
                let maybe_pair_delete = (self.auto_pairs.enabled && self.auto_pairs.delete_pair)
                    .then(|| {
                        if caret_offset == 0 || caret_offset >= doc_char_count {
                            return None;
                        }

                        let open = self.editor.line_index.char_at(caret_offset - 1)?;
                        let close = self.editor.line_index.char_at(caret_offset)?;
                        self.auto_pairs
                            .is_matching_pair(open, close)
                            .then_some((caret_offset - 1, (caret_offset + 1).min(doc_char_count)))
                    })
                    .flatten();

                if let Some(range) = maybe_pair_delete {
                    range
                } else if forward {
                    if caret_offset >= doc_char_count {
                        (caret_offset, caret_offset)
                    } else {
                        (caret_offset, (caret_offset + 1).min(doc_char_count))
                    }
                } else if caret_offset == 0 {
                    (0, 0)
                } else {
                    (caret_offset - 1, caret_offset)
                }
            };

            let delete_len = end_offset.saturating_sub(start_offset);
            let deleted_text = if delete_len == 0 {
                String::new()
            } else {
                self.editor.text_range(start_offset, delete_len)
            };

            ops.push(Op {
                selection_index,
                start_offset,
                delete_len,
                deleted_text,
                start_after: start_offset,
            });
        }

        if !ops.iter().any(|op| op.delete_len > 0) {
            return Ok(CommandResult::Success);
        }

        let before_char_count = self.editor.char_count();

        // Compute caret offsets in the post-delete document (ascending order with delta).
        let mut asc_indices: Vec<usize> = (0..ops.len()).collect();
        asc_indices.sort_by_key(|&idx| ops[idx].start_offset);

        let mut caret_offsets: Vec<usize> = vec![0; ops.len()];
        let mut delta: i64 = 0;
        for &idx in &asc_indices {
            let op = &mut ops[idx];
            let effective_start = (op.start_offset as i64 + delta) as usize;
            op.start_after = effective_start;
            caret_offsets[op.selection_index] = effective_start;
            delta -= op.delete_len as i64;
        }

        // Apply deletes descending to keep offsets valid.
        let mut desc_indices = asc_indices;
        desc_indices.sort_by_key(|&idx| std::cmp::Reverse(ops[idx].start_offset));
        let interval_edits: Vec<IntervalTextEdit> = desc_indices
            .iter()
            .map(|&idx| {
                let op = &ops[idx];
                IntervalTextEdit::new(op.start_offset, op.delete_len, 0)
            })
            .collect();
        self.update_interval_trees_for_text_edits(&interval_edits);
        let mut folding_line_changed = false;

        for &idx in &desc_indices {
            let op = &ops[idx];
            if op.delete_len == 0 {
                continue;
            }

            folding_line_changed |= self.apply_text_change_to_line_index_and_layout(
                op.start_offset,
                &op.deleted_text,
                "",
            ) != 0;
        }

        if folding_line_changed {
            self.editor
                .folding_manager
                .clamp_to_line_count(self.editor.line_index.line_count());
        }

        // Collapse selection state to carets at the start of deleted ranges.
        let mut new_carets: Vec<Selection> = Vec::with_capacity(caret_offsets.len());
        for offset in &caret_offsets {
            let (line, column) = self.editor.line_index.char_offset_to_position(*offset);
            let pos = Position::new(line, column);
            new_carets.push(Selection {
                start: pos,
                end: pos,
                direction: SelectionDirection::Forward,
            });
        }

        let (new_carets, new_primary_index) =
            crate::selection_set::normalize_selections(new_carets, primary_index);
        let primary = new_carets
            .get(new_primary_index)
            .cloned()
            .ok_or_else(|| CommandError::Other("Invalid primary caret".to_string()))?;

        self.editor.cursor_position = primary.end;
        self.editor.selection = None;
        self.editor.secondary_selections = new_carets
            .into_iter()
            .enumerate()
            .filter_map(|(idx, sel)| {
                if idx == new_primary_index {
                    None
                } else {
                    Some(sel)
                }
            })
            .collect();

        let after_selection = self.snapshot_selection_set();

        let edits: Vec<TextEdit> = ops
            .into_iter()
            .map(|op| TextEdit {
                start_before: op.start_offset,
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

    pub(super) fn snapshot_selection_set(&self) -> SelectionSetSnapshot {
        let mut selections: Vec<Selection> =
            Vec::with_capacity(1 + self.editor.secondary_selections.len());

        let primary = self.editor.selection.clone().unwrap_or(Selection {
            start: self.editor.cursor_position,
            end: self.editor.cursor_position,
            direction: SelectionDirection::Forward,
        });
        selections.push(primary);
        selections.extend(self.editor.secondary_selections.iter().cloned());

        let (selections, primary_index) = crate::selection_set::normalize_selections(selections, 0);
        SelectionSetSnapshot {
            selections,
            primary_index,
        }
    }

    pub(super) fn restore_selection_set(&mut self, snapshot: SelectionSetSnapshot) {
        if snapshot.selections.is_empty() {
            self.editor.cursor_position = Position::new(0, 0);
            self.editor.selection = None;
            self.editor.secondary_selections.clear();
            return;
        }

        let primary = snapshot
            .selections
            .get(snapshot.primary_index)
            .cloned()
            .unwrap_or_else(|| snapshot.selections[0].clone());

        self.editor.cursor_position = primary.end;
        self.editor.selection = if primary.start == primary.end {
            None
        } else {
            Some(primary.clone())
        };

        self.editor.secondary_selections = snapshot
            .selections
            .into_iter()
            .enumerate()
            .filter_map(|(idx, sel)| {
                if idx == snapshot.primary_index {
                    None
                } else {
                    Some(sel)
                }
            })
            .collect();

        self.normalize_cursor_and_selection();
    }

    pub(super) fn apply_undo_edits(&mut self, edits: &[TextEdit]) -> Result<(), CommandError> {
        // Apply inverse: delete inserted text, then reinsert deleted text.
        let mut ops: Vec<(usize, usize, &str)> = Vec::with_capacity(edits.len());
        for edit in edits {
            let start = edit.start_after;
            let delete_len = edit.inserted_len();
            let insert_text = edit.deleted_text.as_str();
            ops.push((start, delete_len, insert_text));
        }
        self.apply_text_ops(ops)
    }

    pub(super) fn apply_redo_edits(&mut self, edits: &[TextEdit]) -> Result<(), CommandError> {
        let mut ops: Vec<(usize, usize, &str)> = Vec::with_capacity(edits.len());
        for edit in edits {
            let start = edit.start_before;
            let delete_len = edit.deleted_len();
            let insert_text = edit.inserted_text.as_str();
            ops.push((start, delete_len, insert_text));
        }
        self.apply_text_ops(ops)
    }

    pub(super) fn apply_text_ops(
        &mut self,
        mut ops: Vec<(usize, usize, &str)>,
    ) -> Result<(), CommandError> {
        // Sort descending by start offset to make offsets stable while mutating.
        ops.sort_by_key(|(start, _, _)| std::cmp::Reverse(*start));

        struct PreparedTextOp<'a> {
            start: usize,
            delete_len: usize,
            deleted_text: String,
            insert_text: &'a str,
            insert_len: usize,
        }

        let max_offset = self.editor.char_count();
        let mut prepared_ops = Vec::with_capacity(ops.len());
        for (start, delete_len, insert_text) in ops {
            let end = start
                .checked_add(delete_len)
                .ok_or(CommandError::InvalidRange {
                    start,
                    end: usize::MAX,
                })?;
            if start > max_offset {
                return Err(CommandError::InvalidOffset(start));
            }
            if end > max_offset {
                return Err(CommandError::InvalidRange { start, end });
            }

            let deleted_text = if delete_len > 0 {
                self.editor.text_range(start, delete_len)
            } else {
                String::new()
            };
            let insert_len = insert_text.chars().count();

            prepared_ops.push(PreparedTextOp {
                start,
                delete_len,
                deleted_text,
                insert_text,
                insert_len,
            });
        }

        let interval_edits: Vec<IntervalTextEdit> = prepared_ops
            .iter()
            .map(|op| IntervalTextEdit::new(op.start, op.delete_len, op.insert_len))
            .collect();
        self.update_interval_trees_for_text_edits(&interval_edits);

        let mut folding_line_changed = false;

        for op in prepared_ops {
            folding_line_changed |= self.apply_text_change_to_line_index_and_layout(
                op.start,
                &op.deleted_text,
                op.insert_text,
            ) != 0;
        }

        if folding_line_changed {
            self.editor
                .folding_manager
                .clamp_to_line_count(self.editor.line_index.line_count());
        }
        self.normalize_cursor_and_selection();

        Ok(())
    }

    // Private method: execute cursor command

    pub(super) fn apply_text_change_to_line_index_and_layout(
        &mut self,
        start_offset: usize,
        deleted_text: &str,
        inserted_text: &str,
    ) -> isize {
        let start_line = self
            .editor
            .line_index
            .char_offset_to_position(start_offset)
            .0;

        let deleted_newlines = deleted_text
            .as_bytes()
            .iter()
            .filter(|b| **b == b'\n')
            .count();
        let inserted_newlines = inserted_text
            .as_bytes()
            .iter()
            .filter(|b| **b == b'\n')
            .count();
        let line_delta = inserted_newlines as isize - deleted_newlines as isize;
        if line_delta != 0 {
            self.editor
                .folding_manager
                .apply_line_delta(start_line, line_delta);
        }

        let deleted_chars = deleted_text.chars().count();
        if deleted_chars > 0 {
            self.editor.line_index.delete(start_offset, deleted_chars);
        }
        if !inserted_text.is_empty() {
            self.editor.line_index.insert(start_offset, inserted_text);
        }

        #[cfg(debug_assertions)]
        {
            if deleted_chars > 0 {
                self.editor
                    .piece_table_shadow
                    .delete(start_offset, deleted_chars);
            }
            if !inserted_text.is_empty() {
                self.editor
                    .piece_table_shadow
                    .insert(start_offset, inserted_text);
            }
        }

        self.assert_piece_table_text_buffer_consistent();

        if line_delta > 0 {
            for i in 0..(line_delta as usize) {
                let line = start_line.saturating_add(1).saturating_add(i);
                let line_text = self
                    .editor
                    .line_index
                    .get_line_text(line)
                    .unwrap_or_default();
                self.editor.layout_engine.insert_line(line, &line_text);
            }
        } else if line_delta < 0 {
            for _ in 0..((-line_delta) as usize) {
                self.editor
                    .layout_engine
                    .delete_line(start_line.saturating_add(1));
            }
        }

        // Update a small window around the edit site. This keeps the layout engine incremental
        // while still handling multi-line inserts/deletes deterministically.
        let touch_lines = deleted_newlines.max(inserted_newlines).saturating_add(1);
        let end_line = start_line.saturating_add(touch_lines);

        let line_count = self.editor.line_index.line_count();
        if line_count == 0 {
            return line_delta;
        }
        let last_line = line_count.saturating_sub(1);
        for line in start_line..=end_line.min(last_line) {
            let line_text = self
                .editor
                .line_index
                .get_line_text(line)
                .unwrap_or_default();
            self.editor.layout_engine.update_line(line, &line_text);
        }

        self.editor.sync_visual_row_index_after_text_change(
            start_line,
            deleted_newlines,
            inserted_newlines,
        );

        line_delta
    }

    #[cfg(debug_assertions)]
    pub(super) fn assert_piece_table_text_buffer_consistent(&self) {
        debug_assert_eq!(
            self.editor.piece_table_shadow.get_text(),
            self.editor.line_index.text_buffer().get_text(),
            "PieceTable shadow text diverged from TextBuffer"
        );
    }

    #[cfg(not(debug_assertions))]
    pub(super) fn assert_piece_table_text_buffer_consistent(&self) {}
}
