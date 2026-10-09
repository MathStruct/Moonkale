//! Rust model and localized delta adapter; independent of the Dioxus view.
use dioxus_code::{
    advanced::{Buffer, SourceEdit},
    Language,
};
use editor_core::{EditorStateManager, Position, TextDelta};
use moonkale_ext_api::editor::{
    DocumentRevision, EditBatch, EditorSnapshot, TextChange, Utf16Selection,
};

#[derive(Clone, Copy, PartialEq)]
pub(crate) struct Preferences {
    pub wrap: bool,
    pub insert_spaces: Option<bool>,
    pub indent_width: Option<u8>,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            wrap: true,
            insert_spaces: None,
            indent_width: None,
        }
    }
}
#[derive(Clone)]
struct LineMetrics {
    byte: usize,
    utf16: u32,
    bytes: usize,
    units: u32,
    chars: usize,
    crlf: bool,
    width: usize,
}

pub(crate) struct NativeModel {
    pub engine: EditorStateManager,
    pub revision: DocumentRevision,
    pub highlight: Option<Buffer>,
    pub language: Option<Language>,
    pub structure: crate::native_structure::Structure,
    pub structure_dirty: bool,
    pub preferences: Preferences,
    pub max_columns: usize,
    lines: Vec<LineMetrics>,
    widths: std::collections::BTreeMap<usize, usize>,
    pub normalized_chars: usize,
    crlf_lines: usize,
}
impl NativeModel {
    pub fn new(snapshot: &EditorSnapshot, language: Option<Language>) -> Self {
        let mut engine = EditorStateManager::new(&snapshot.text, 100);
        crate::native_language::configure(&mut engine, language);
        let mut model = Self {
            engine,
            revision: snapshot.revision,
            highlight: language.and_then(|language| {
                Buffer::new(language, snapshot.text.replace("\r\n", "\n")).ok()
            }),
            language,
            structure: Default::default(),
            structure_dirty: false,
            preferences: Default::default(),
            max_columns: 0,
            lines: Vec::new(),
            widths: Default::default(),
            normalized_chars: 0,
            crlf_lines: 0,
        };
        model.refresh_structure(false);
        model.rebuild_metrics(&snapshot.text);
        model
    }
    pub fn replace(&mut self, snapshot: &EditorSnapshot) {
        let width = self.engine.editor().viewport_width();
        let previous = self.engine.get_folding_state().regions;
        let preferences = self.preferences;
        *self = Self::new(snapshot, self.language);
        self.set_preferences(preferences);
        let _ = self.engine.execute(editor_core::Command::View(
            editor_core::ViewCommand::SetViewportWidth { width },
        ));
        let mut folds = self.structure.folds.clone();
        for fold in &mut folds {
            fold.is_collapsed = previous.iter().any(|old| {
                old.is_collapsed
                    && old.start_line == fold.start_line
                    && old.end_line == fold.end_line
            });
        }
        self.engine.replace_folding_regions(folds, false);
        if let Some(selection) = snapshot.selection {
            let anchor = utf16_position(&snapshot.text, selection.anchor);
            let head = utf16_position(&snapshot.text, selection.head);
            let _ = self.engine.execute(editor_core::Command::Cursor(
                editor_core::CursorCommand::MoveTo {
                    line: head.line,
                    column: head.column,
                },
            ));
            if anchor != head {
                let _ = self.engine.execute(editor_core::Command::Cursor(
                    editor_core::CursorCommand::SetSelection {
                        start: anchor,
                        end: head,
                    },
                ));
            }
        }
        self.reveal_cursor();
    }
    #[cfg(test)]
    pub fn update_highlight(&mut self, snapshot: &EditorSnapshot, batch: &EditBatch) {
        self.update_highlight_deferred(snapshot, batch);
        self.flush_structure();
    }
    pub fn update_highlight_deferred(&mut self, snapshot: &EditorSnapshot, batch: &EditBatch) {
        self.revision = snapshot.revision;
        // SourceEdit uses bytes of normalized text, while Workspace preserves CRLF.
        // CRLF replacements use a reparse; ordinary edits update the existing tree.
        if let Some(buffer) = &mut self.highlight {
            let result = if !snapshot.text.contains("\r\n") && batch.changes.len() == 1 {
                let change = &batch.changes[0];
                buffer.edit(
                    SourceEdit {
                        start_byte: change.range.start,
                        old_end_byte: change.range.end,
                        new_end_byte: change.range.start + change.inserted_text.len(),
                    },
                    &snapshot.text,
                )
            } else {
                buffer.replace(snapshot.text.replace("\r\n", "\n"))
            };
            if result.is_err() {
                self.highlight = None;
            }
        }
        // Parser failures are transient while typing. Retry the current source
        // instead of leaving syntax-dependent editing disabled until remount.
        if self.highlight.is_none() {
            self.highlight = self.language.and_then(|language| {
                Buffer::new(language, snapshot.text.replace("\r\n", "\n")).ok()
            });
        }
        self.structure_dirty = true;
        self.structure.pairs.clear(); // Old byte offsets must never highlight unrelated brackets.
        self.update_metrics(&snapshot.text, batch);
    }
    pub fn set_preferences(&mut self, preferences: Preferences) {
        if self.preferences == preferences {
            return;
        }
        self.preferences = preferences;
        crate::native_language::configure_preferences(
            &mut self.engine,
            self.language,
            preferences.insert_spaces,
            preferences.indent_width,
        );
        let _ = self.engine.execute(editor_core::Command::View(
            editor_core::ViewCommand::SetWrapMode {
                mode: if preferences.wrap {
                    editor_core::WrapMode::Char
                } else {
                    editor_core::WrapMode::None
                },
            },
        ));
        let tab = self.engine.editor().layout_engine().tab_width();
        let text = self.engine.editor().get_text();
        let normalized = Self::metrics(&text, tab, true);
        self.widths.clear();
        for (line, normalized) in self.lines.iter_mut().zip(normalized) {
            line.width = normalized.width;
            *self.widths.entry(line.width).or_default() += 1;
        }
        self.index_metrics();
    }
    fn metrics(text: &str, tab: usize, terminal: bool) -> Vec<LineMetrics> {
        let mut lines: Vec<_> = text
            .split_inclusive('\n')
            .map(|line| LineMetrics {
                byte: 0,
                utf16: 0,
                bytes: line.len(),
                units: line.encode_utf16().count() as u32,
                chars: line.chars().count() - usize::from(line.ends_with("\r\n")),
                crlf: line.ends_with("\r\n"),
                width: editor_core::str_width_with_tab_width(
                    line.trim_end_matches(['\r', '\n']),
                    tab,
                ),
            })
            .collect();
        if terminal && (text.is_empty() || text.ends_with('\n')) {
            lines.push(LineMetrics {
                byte: 0,
                utf16: 0,
                bytes: 0,
                units: 0,
                chars: 0,
                crlf: false,
                width: 0,
            });
        }
        lines
    }
    fn index_metrics(&mut self) {
        let (mut byte, mut units, mut chars, mut crlf) = (0, 0, 0, 0);
        for line in &mut self.lines {
            line.byte = byte;
            line.utf16 = units;
            byte += line.bytes;
            units += line.units;
            chars += line.chars;
            crlf += usize::from(line.crlf);
        }
        self.normalized_chars = chars;
        self.crlf_lines = crlf;
        self.max_columns = self
            .widths
            .last_key_value()
            .map(|(width, _)| *width)
            .unwrap_or(0);
    }
    fn rebuild_metrics(&mut self, text: &str) {
        self.lines = Self::metrics(text, self.engine.editor().layout_engine().tab_width(), true);
        self.widths.clear();
        for line in &self.lines {
            *self.widths.entry(line.width).or_default() += 1;
        }
        self.index_metrics();
    }
    fn update_metrics(&mut self, text: &str, batch: &EditBatch) {
        let Some(first) = batch.changes.iter().map(|change| change.range.start).min() else {
            return;
        };
        let last = batch
            .changes
            .iter()
            .map(|change| change.range.end)
            .max()
            .unwrap();
        let start_line = self
            .lines
            .partition_point(|line| line.byte <= first)
            .saturating_sub(1);
        let end_line = self
            .lines
            .partition_point(|line| line.byte <= last)
            .saturating_sub(1);
        let start = self.lines[start_line].byte;
        let old_end = self.lines[end_line].byte + self.lines[end_line].bytes;
        let shift = batch
            .changes
            .iter()
            .map(|change| change.inserted_text.len() as isize - change.removed_text.len() as isize)
            .sum();
        let end = old_end.checked_add_signed(shift).unwrap();
        let terminal = end_line + 1 == self.lines.len();
        let lines = Self::metrics(
            &text[start..end],
            self.engine.editor().layout_engine().tab_width(),
            terminal,
        );
        for line in &self.lines[start_line..=end_line] {
            let count = self.widths.get_mut(&line.width).unwrap();
            *count -= 1;
            if *count == 0 {
                self.widths.remove(&line.width);
            }
        }
        for line in &lines {
            *self.widths.entry(line.width).or_default() += 1;
        }
        self.lines.splice(start_line..=end_line, lines);
        self.index_metrics();
    }
    pub fn cursor_line_col(&self, canonical: &str) -> (u32, u32) {
        let position = self.engine.get_cursor_state().position;
        let column = self
            .lines
            .get(position.line)
            .map(|line| {
                canonical[line.byte..line.byte + line.bytes]
                    .trim_end_matches(['\r', '\n'])
                    .chars()
                    .take(position.column)
                    .map(|ch| ch.len_utf16() as u32)
                    .sum()
            })
            .unwrap_or(0);
        (position.line as u32, column)
    }
    fn position_offset(&self, canonical: &str, position: Position) -> u32 {
        let Some(line) = self.lines.get(position.line) else {
            return self
                .lines
                .last()
                .map(|line| line.utf16 + line.units)
                .unwrap_or(0);
        };
        line.utf16
            + canonical[line.byte..line.byte + line.bytes]
                .trim_end_matches(['\r', '\n'])
                .chars()
                .take(position.column)
                .map(|ch| ch.len_utf16() as u32)
                .sum::<u32>()
    }
    pub fn flush_structure(&mut self) {
        if self.structure_dirty {
            self.refresh_structure(true);
        }
    }
    fn refresh_structure(&mut self, preserve: bool) {
        self.structure_dirty = false;
        self.structure =
            crate::native_structure::Structure::new(self.highlight.as_ref(), self.language);
        self.engine
            .replace_folding_regions(self.structure.folds.clone(), preserve);
    }
    /// Expand ancestors before navigation/editing so a hidden caret is never stranded.
    pub fn reveal_cursor(&mut self) {
        let cursor = self.engine.get_cursor_state();
        let line = cursor.position.line;
        let anchor = cursor
            .selection
            .as_ref()
            .map(|selection| selection.start.line)
            .unwrap_or(line);
        let mut folds = self.engine.get_folding_state().regions;
        let mut changed = false;
        for fold in &mut folds {
            if fold.is_collapsed
                && [line, anchor]
                    .iter()
                    .any(|line| *line > fold.start_line && *line <= fold.end_line)
            {
                fold.is_collapsed = false;
                changed = true;
            }
        }
        if changed {
            self.engine.replace_folding_regions(folds, false);
        }
    }
    pub fn set_fold(&mut self, line: Option<usize>, collapsed: bool) {
        self.flush_structure();
        let mut folds = self.engine.get_folding_state().regions;
        for fold in &mut folds {
            if line.is_none_or(|line| fold.start_line == line) {
                fold.is_collapsed = collapsed;
            }
        }
        let cursor = self.engine.get_cursor_state().position;
        if let Some(header) = folds
            .iter()
            .filter(|fold| {
                fold.is_collapsed && cursor.line > fold.start_line && cursor.line <= fold.end_line
            })
            .map(|fold| fold.start_line)
            .min()
        {
            crate::editor_core_spike::place_caret_at(&mut self.engine, Position::new(header, 0));
        }
        self.engine.replace_folding_regions(folds, false);
    }
    pub fn selection(&self, canonical: &str) -> Utf16Selection {
        let cursor = self.engine.get_cursor_state();
        let (anchor, head) = cursor
            .selection
            .map(|selection| (selection.start, selection.end))
            .unwrap_or((cursor.position, cursor.position));
        Utf16Selection {
            anchor: self.position_offset(canonical, anchor),
            head: self.position_offset(canonical, head),
        }
    }
}

pub(crate) fn utf16_position(text: &str, offset: u32) -> Position {
    let mut units = 0;
    let mut line = 0;
    let mut column = 0;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if units + ch.len_utf16() as u32 > offset {
            break;
        }
        units += ch.len_utf16() as u32;
        if ch == '\n' {
            line += 1;
            column = 0;
        } else if ch != '\r' || chars.peek() != Some(&'\n') {
            column += 1;
        }
    }
    Position::new(line, column)
}
fn byte_at_char(text: &str, offset: usize) -> Option<usize> {
    text.char_indices()
        .map(|(byte, _)| byte)
        .chain(std::iter::once(text.len()))
        .nth(offset)
}
fn canonical_byte(text: &str, normalized_byte: usize) -> usize {
    let mut normalized = 0;
    for (byte, ch) in text.char_indices() {
        if normalized == normalized_byte {
            return byte;
        }
        if ch != '\r' || !text[byte..].starts_with("\r\n") {
            normalized += ch.len_utf8();
        }
    }
    text.len()
}

/// Reduce sequential engine edits to one localized replacement, without
/// computing a full-document prefix/suffix diff. All engine offsets refer to
/// the intermediate document; the resulting range refers to the old snapshot.
#[cfg(test)]
pub(crate) fn delta_batch(
    snapshot: &EditorSnapshot,
    delta: &TextDelta,
    selection: Utf16Selection,
) -> Result<EditBatch, &'static str> {
    delta_batch_with_length(
        snapshot,
        delta,
        selection,
        snapshot.text.chars().count() - snapshot.text.matches("\r\n").count(),
    )
}

pub(crate) fn delta_batch_with_length(
    snapshot: &EditorSnapshot,
    delta: &TextDelta,
    selection: Utf16Selection,
    normalized_chars: usize,
) -> Result<EditBatch, &'static str> {
    if normalized_chars != delta.before_char_count {
        return Err("delta base length mismatch");
    }
    if let [edit] = delta.edits.as_slice() {
        let deleted_chars = edit.deleted_text.chars().count();
        if normalized_chars
            .checked_sub(deleted_chars)
            .and_then(|count| count.checked_add(edit.inserted_text.chars().count()))
            != Some(delta.after_char_count)
        {
            return Err("delta result length mismatch");
        }
        let canonical_offset = |offset: usize| {
            let mut chars = snapshot.text.char_indices().peekable();
            let mut normalized = 0;
            while let Some((byte, ch)) = chars.next() {
                if normalized == offset {
                    return Some(byte);
                }
                if ch == '\r' && chars.peek().is_some_and(|(_, next)| *next == '\n') {
                    chars.next();
                }
                normalized += 1;
            }
            (normalized == offset).then_some(snapshot.text.len())
        };
        let start = canonical_offset(edit.start).ok_or("delta start out of range")?;
        let end = canonical_offset(edit.end()).ok_or("delta end out of range")?;
        let removed = &snapshot.text[start..end];
        if removed.replace("\r\n", "\n") != edit.deleted_text {
            return Err("delta removed text mismatch");
        }
        let inserted = if snapshot.text.contains("\r\n") {
            edit.inserted_text.replace('\n', "\r\n")
        } else {
            edit.inserted_text.clone()
        };
        return Ok(EditBatch {
            node: snapshot.node,
            base_revision: snapshot.revision,
            changes: vec![TextChange {
                range: start..end,
                removed_text: removed.into(),
                inserted_text: inserted,
            }],
            selection: Some(selection),
        });
    }
    let original = snapshot.text.replace("\r\n", "\n");
    if original.chars().count() != delta.before_char_count {
        return Err("delta base length mismatch");
    }
    let mut working = original.clone();
    let mut envelope: Option<(usize, usize)> = None;
    let mut shift = 0isize;
    for edit in &delta.edits {
        let start = byte_at_char(&working, edit.start).ok_or("delta start out of range")?;
        let end = byte_at_char(&working, edit.end()).ok_or("delta end out of range")?;
        if working.get(start..end) != Some(edit.deleted_text.as_str()) {
            return Err("delta removed text mismatch");
        }
        envelope = Some(match envelope {
            None => (start, end),
            Some((base_start, base_end)) => {
                let current_end = base_end
                    .checked_add_signed(shift)
                    .ok_or("delta range overflow")?;
                (
                    base_start.min(start),
                    if end > current_end {
                        end.checked_add_signed(-shift)
                            .ok_or("delta range overflow")?
                    } else {
                        base_end
                    },
                )
            }
        });
        working.replace_range(start..end, &edit.inserted_text);
        shift += edit.inserted_text.len() as isize - (end - start) as isize;
    }
    if working.chars().count() != delta.after_char_count {
        return Err("delta result length mismatch");
    }
    let changes = if let Some((start, end)) = envelope {
        let new_end = end
            .checked_add_signed(shift)
            .ok_or("delta range overflow")?;
        let range = canonical_byte(&snapshot.text, start)..canonical_byte(&snapshot.text, end);
        let mut inserted_text = working
            .get(start..new_end)
            .ok_or("delta replacement out of range")?
            .to_owned();
        if snapshot.text.contains("\r\n") {
            inserted_text = inserted_text.replace('\n', "\r\n");
        }
        vec![TextChange {
            removed_text: snapshot.text[range.clone()].into(),
            range,
            inserted_text,
        }]
    } else {
        vec![]
    };
    Ok(EditBatch {
        node: snapshot.node,
        base_revision: snapshot.revision,
        changes,
        selection: Some(selection),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use editor_core::{Command, CursorCommand, EditCommand, TextDeltaEdit};
    use moonkale_core::NodeId;
    use moonkale_ext_api::editor::RevisionedDocument;
    fn snapshot(text: &str) -> EditorSnapshot {
        RevisionedDocument::new(NodeId::fresh("native-delta"), text.into())
            .snapshot()
            .clone()
    }
    fn verify(base: &str, edits: Vec<TextDeltaEdit>, expected: &str) {
        let snapshot = snapshot(base);
        let delta = TextDelta {
            before_char_count: base.replace("\r\n", "\n").chars().count(),
            after_char_count: expected.replace("\r\n", "\n").chars().count(),
            edits,
            undo_group_id: None,
        };
        let batch = delta_batch(&snapshot, &delta, Utf16Selection { anchor: 0, head: 0 }).unwrap();
        let mut document = RevisionedDocument::new(snapshot.node, snapshot.text);
        assert_eq!(document.apply(&batch).unwrap().text, expected);
        assert_eq!(document.undo().unwrap().unwrap().text, base);
    }
    fn edit(start: usize, removed: &str, inserted: &str) -> TextDeltaEdit {
        TextDeltaEdit {
            start,
            deleted_text: removed.into(),
            inserted_text: inserted.into(),
        }
    }
    #[test]
    fn localized_metrics_follow_multiline_crlf_edits_and_selection() {
        let mut doc = RevisionedDocument::new(
            NodeId::fresh("cached-lines"),
            "wide界😀\r\nshort\r\nlast".into(),
        );
        let mut model = NativeModel::new(doc.snapshot(), None);
        let changes = vec![TextChange {
            range: 0..11,
            removed_text: "wide界😀".into(),
            inserted_text: "x\r\n界".into(),
        }];
        let batch = EditBatch {
            node: doc.snapshot().node,
            base_revision: doc.snapshot().revision,
            changes,
            selection: None,
        };
        let next = doc.apply(&batch).unwrap();
        model.engine = EditorStateManager::new(&next.text, 100);
        model.update_highlight(&next, &batch);
        assert_eq!(model.lines.len(), 4);
        assert_eq!(
            model.normalized_chars,
            next.text.replace("\r\n", "\n").chars().count()
        );
        assert_eq!(model.position_offset(&next.text, Position::new(2, 3)), 9);
        assert_eq!(model.max_columns, 5);
        let batch = EditBatch {
            node: next.node,
            base_revision: next.revision,
            changes: vec![TextChange {
                range: 3..next.text.len(),
                removed_text: next.text[3..].into(),
                inserted_text: String::new(),
            }],
            selection: None,
        };
        let next = doc.apply(&batch).unwrap();
        model.engine = EditorStateManager::new(&next.text, 100);
        model.update_highlight(&next, &batch);
        assert_eq!(model.lines.len(), 2);
        assert_eq!(model.position_offset(&next.text, Position::new(1, 0)), 3);
        assert_eq!(model.max_columns, 1);
    }

    #[test]
    fn native_deltas_handle_sequential_unicode_replacement_and_disjoint_edits() {
        verify("a😀中z", vec![edit(1, "😀中", ""), edit(1, "", "λ")], "aλz");
        verify(
            "abcdef",
            vec![edit(4, "e", "😀"), edit(0, "a", "中")],
            "中bcd😀f",
        );
        verify(
            "abcdef",
            vec![edit(1, "b", "😀😀"), edit(6, "f", "!")],
            "a😀😀cde!",
        );
        verify(
            "abcdef",
            vec![edit(2, "cd", "XY"), edit(1, "bXYe", "λ")],
            "aλf",
        );
        verify("abc", vec![edit(1, "", "XY"), edit(2, "Y", "😀")], "aX😀bc");
    }
    #[test]
    fn native_deltas_preserve_crlf_in_workspace() {
        verify("a\r\nb\r\n", vec![edit(2, "b", "😀")], "a\r\n😀\r\n");
        verify("a\r\nb", vec![edit(1, "\n", "")], "ab");
        verify("a\r\nb", vec![edit(3, "", "\n中")], "a\r\nb\r\n中");
    }
    #[test]
    fn engine_input_and_selection_feed_the_contract_without_dom_selection() {
        let base = snapshot("a😀中z");
        let mut model = NativeModel::new(&base, Some(Language::Rust));
        model
            .engine
            .execute(Command::Cursor(CursorCommand::SetSelection {
                start: Position::new(0, 3),
                end: Position::new(0, 1),
            }))
            .unwrap();
        assert_eq!(
            model.selection(&base.text),
            Utf16Selection { anchor: 4, head: 1 }
        );
        model
            .engine
            .execute(Command::Edit(EditCommand::InsertText { text: "λ".into() }))
            .unwrap();
        let delta = model.engine.take_last_text_delta().unwrap();
        let batch = delta_batch(&base, &delta, Utf16Selection { anchor: 2, head: 2 }).unwrap();
        let mut doc = RevisionedDocument::new(base.node, base.text.clone());
        let next = doc.apply(&batch).unwrap();
        assert_eq!(next.text, "aλz");
        model.update_highlight(&next, &batch);
        assert_eq!(
            model.selection(&next.text),
            Utf16Selection { anchor: 2, head: 2 }
        );
        model.replace(&doc.undo().unwrap().unwrap());
        assert_eq!(model.engine.editor().get_text(), base.text);
    }
    #[test]
    fn restored_utf16_selection_handles_emoji_crlf_and_reversed_ranges() {
        let mut base = snapshot("😀x\r\n中y");
        base.selection = Some(Utf16Selection { anchor: 6, head: 2 });
        let mut model = NativeModel::new(&base, None);
        model.replace(&base);
        assert_eq!(model.selection(&base.text), base.selection.unwrap());
        assert_eq!(utf16_position(&base.text, 1), Position::new(0, 0));
        assert_eq!(utf16_position(&base.text, 5), Position::new(1, 0));
    }
    #[test]
    fn localized_change_does_not_include_an_unchanged_large_document() {
        let base = snapshot(&"x".repeat(3_000_000));
        let delta = TextDelta {
            before_char_count: 3_000_000,
            after_char_count: 3_000_001,
            edits: vec![edit(0, "", "😀")],
            undo_group_id: None,
        };
        let batch = delta_batch(&base, &delta, Utf16Selection { anchor: 2, head: 2 }).unwrap();
        assert_eq!(batch.changes[0].range, 0..0);
        assert_eq!(batch.changes[0].inserted_text, "😀");
        assert!(batch.changes[0].removed_text.is_empty());
    }
    #[test]
    fn restored_caret_moves_to_document_start() {
        let mut base = snapshot("external λ\n中 update\n");
        base.selection = Some(Utf16Selection {
            anchor: 10,
            head: 10,
        });
        let mut model = NativeModel::new(&base, None);
        model.replace(&base);
        crate::editor_core_spike::move_cursor(
            &mut model.engine,
            CursorCommand::MoveTo { line: 0, column: 0 },
            false,
        );
        assert_eq!(model.engine.get_cursor_state().offset, 0);
    }
    #[test]
    fn invalid_native_deltas_are_rejected() {
        let base = snapshot("abc");
        for (before, after, edits) in [
            (4, 4, vec![edit(0, "", "x")]),
            (3, 3, vec![edit(1, "X", "Y")]),
            (3, 4, vec![edit(9, "", "x")]),
            (3, 99, vec![edit(0, "", "x")]),
        ] {
            let delta = TextDelta {
                before_char_count: before,
                after_char_count: after,
                edits,
                undo_group_id: None,
            };
            assert!(delta_batch(&base, &delta, Utf16Selection { anchor: 0, head: 0 }).is_err());
        }
    }
    #[test]
    fn folds_are_view_state_and_reveal_expands_nested_ancestors() {
        let base = snapshot("fn main() {\r\n    if true {\r\n        work();\r\n    }\r\n}\r\n");
        let mut model = NativeModel::new(&base, Some(Language::Rust));
        crate::editor_core_spike::place_caret_at(&mut model.engine, Position::new(2, 8));
        model.set_fold(None, true);
        assert_eq!(model.engine.get_cursor_state().position.line, 0);
        assert_eq!(model.engine.get_folding_state().collapsed_line_count, 4);
        assert!(model.engine.take_last_text_delta().is_none());
        let visible = model.engine.get_viewport_content_styled(0, 20);
        assert_eq!(
            visible
                .lines
                .iter()
                .map(|line| line.logical_line_index)
                .collect::<Vec<_>>(),
            vec![0, 5]
        );
        assert!(visible.lines[0].is_fold_placeholder_appended);
        assert_eq!(
            model.engine.editor().get_text(),
            base.text.replace("\r\n", "\n")
        );
        model.replace(&base);
        assert_eq!(model.engine.get_folding_state().collapsed_line_count, 4);
        crate::editor_core_spike::place_caret_at(&mut model.engine, Position::new(2, 8));
        model.reveal_cursor();
        assert_eq!(model.engine.get_folding_state().collapsed_line_count, 0);
        assert!(model.structure.at_caret(11).is_some());
        model.set_fold(None, true);
        model
            .engine
            .execute(Command::Cursor(CursorCommand::SetSelection {
                start: Position::new(2, 0),
                end: Position::new(5, 0),
            }))
            .unwrap();
        model.reveal_cursor();
        assert_eq!(model.engine.get_folding_state().collapsed_line_count, 0);
    }
    #[test]
    fn preferences_preserve_source_selection_and_survive_model_replacement() {
        let mut base = snapshot("fn main() {}\r\n");
        base.selection = Some(Utf16Selection {
            anchor: 11,
            head: 11,
        });
        let mut model = NativeModel::new(&base, Some(Language::Rust));
        model.replace(&base);
        let preferences = Preferences {
            wrap: false,
            insert_spaces: Some(true),
            indent_width: Some(2),
        };
        model.set_preferences(preferences);
        assert_eq!(model.selection(&base.text), base.selection.unwrap());
        assert_eq!(
            model.engine.editor().get_text(),
            base.text.replace("\r\n", "\n")
        );
        assert!(model.engine.take_last_text_delta().is_none());
        model.replace(&base);
        assert!(model.preferences == preferences);
        model
            .engine
            .execute(Command::Edit(EditCommand::InsertNewline {
                auto_indent: true,
            }))
            .unwrap();
        assert_eq!(model.engine.editor().get_text(), "fn main() {\n  \n}\n");
        model.replace(&base);
        model.set_preferences(Preferences {
            insert_spaces: Some(false),
            indent_width: Some(3),
            ..preferences
        });
        model
            .engine
            .execute(Command::Edit(EditCommand::InsertTab))
            .unwrap();
        assert_eq!(model.engine.editor().get_text(), "fn main() {\t}\n");
    }
    #[test]
    fn unwrapped_large_line_has_bounded_horizontal_windows_and_unicode_offsets() {
        let base = snapshot(&format!("a\t😀中{}", "x".repeat(3_000_000)));
        let mut model = NativeModel::new(&base, None);
        model.set_preferences(Preferences {
            wrap: false,
            insert_spaces: None,
            indent_width: Some(4),
        });
        let grid = model.engine.get_viewport_content_window(0, 20, 4, 5);
        assert_eq!(grid.lines.len(), 1);
        assert_eq!(grid.lines[0].char_offset_start, 2);
        assert_eq!(grid.lines[0].segment_x_start_cells, 4);
        assert_eq!(
            grid.lines[0]
                .cells
                .iter()
                .map(|cell| cell.ch)
                .collect::<String>(),
            "😀中x"
        );
        let tail = model
            .engine
            .get_viewport_content_window(0, 20, 3_000_000, 50);
        assert!(tail.lines[0].cells.len() <= 50);
        assert!(tail.lines[0].char_offset_start > 2_999_000);
        assert_eq!(model.engine.total_visual_lines(), 1);
        assert!(model.max_columns > 3_000_000);
    }
}
