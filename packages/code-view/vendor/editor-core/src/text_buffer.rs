//! Internal text storage abstraction for the editor core.

use ropey::Rope;

/// Rope-backed text buffer using character offsets at its public boundary.
#[derive(Clone)]
pub(crate) struct TextBuffer {
    rope: Rope,
}

impl TextBuffer {
    /// Create an empty text buffer.
    pub(crate) fn new() -> Self {
        Self { rope: Rope::new() }
    }

    /// Build a text buffer from already-normalized internal text.
    pub(crate) fn from_text(text: &str) -> Self {
        Self {
            rope: Rope::from_str(text),
        }
    }

    /// Return the number of Unicode scalar values in the buffer.
    pub(crate) fn len_chars(&self) -> usize {
        self.rope.len_chars()
    }

    /// Return the number of UTF-8 bytes in the buffer.
    pub(crate) fn len_bytes(&self) -> usize {
        self.rope.len_bytes()
    }

    /// Return the number of logical lines in the buffer.
    pub(crate) fn line_count(&self) -> usize {
        self.rope.len_lines()
    }

    /// Insert text at a character offset, clamping offsets past EOF.
    pub(crate) fn insert(&mut self, char_offset: usize, text: &str) {
        let char_offset = char_offset.min(self.rope.len_chars());
        self.rope.insert(char_offset, text);
    }

    /// Delete up to `len_chars` characters starting at `start_char`.
    pub(crate) fn delete(&mut self, start_char: usize, len_chars: usize) {
        let start_char = start_char.min(self.rope.len_chars());
        let end_char = start_char
            .saturating_add(len_chars)
            .min(self.rope.len_chars());

        if start_char < end_char {
            self.rope.remove(start_char..end_char);
        }
    }

    /// Return the complete buffer text.
    pub(crate) fn get_text(&self) -> String {
        self.rope.to_string()
    }

    /// Return up to `len_chars` characters starting at `start_char`.
    pub(crate) fn get_range(&self, start_char: usize, len_chars: usize) -> String {
        let start_char = start_char.min(self.rope.len_chars());
        let end_char = start_char
            .saturating_add(len_chars)
            .min(self.rope.len_chars());
        self.rope.slice(start_char..end_char).to_string()
    }

    /// Return a logical line without its trailing LF, if the line exists.
    pub(crate) fn get_line_text(&self, line_number: usize) -> Option<String> {
        if line_number >= self.rope.len_lines() {
            return None;
        }

        let mut text = self.rope.line(line_number).to_string();
        if text.ends_with('\n') {
            text.pop();
        }
        Some(text)
    }

    /// Convert a logical position to a character offset, clamping to the target line.
    pub(crate) fn position_to_char_offset(&self, line: usize, column: usize) -> usize {
        if line >= self.rope.len_lines() {
            return self.rope.len_chars();
        }

        let line_start_char = self.rope.line_to_char(line);
        let line_len = if line + 1 < self.rope.len_lines() {
            self.rope.line_to_char(line + 1) - line_start_char - 1
        } else {
            self.rope.len_chars() - line_start_char
        };

        line_start_char + column.min(line_len)
    }

    /// Convert a character offset to `(line, column)` coordinates.
    pub(crate) fn char_offset_to_position(&self, char_offset: usize) -> (usize, usize) {
        let char_offset = char_offset.min(self.rope.len_chars());
        let line_idx = self.rope.char_to_line(char_offset);
        let line_start_char = self.rope.line_to_char(line_idx);
        (line_idx, char_offset - line_start_char)
    }

    /// Convert a character offset to a UTF-8 byte offset.
    pub(crate) fn char_offset_to_byte_offset(&self, char_offset: usize) -> usize {
        let char_offset = char_offset.min(self.rope.len_chars());
        self.rope.char_to_byte(char_offset)
    }

    /// Convert a UTF-8 byte offset to a character offset.
    pub(crate) fn byte_offset_to_char_offset(&self, byte_offset: usize) -> usize {
        let byte_offset = byte_offset.min(self.rope.len_bytes());
        self.rope.byte_to_char(byte_offset)
    }

    /// Return the character at the given character offset.
    pub(crate) fn char_at(&self, char_offset: usize) -> Option<char> {
        if char_offset >= self.rope.len_chars() {
            None
        } else {
            Some(self.rope.char(char_offset))
        }
    }

    /// Convert a line index to the character offset at its start.
    pub(crate) fn line_to_char(&self, line: usize) -> usize {
        self.rope.line_to_char(line)
    }

    /// Convert a line index to the byte offset at its start.
    pub(crate) fn line_to_byte(&self, line: usize) -> usize {
        self.rope.line_to_byte(line)
    }

    /// Convert a character offset to its containing line.
    pub(crate) fn char_to_line(&self, char_offset: usize) -> usize {
        self.rope.char_to_line(char_offset)
    }
}

impl Default for TextBuffer {
    fn default() -> Self {
        Self::new()
    }
}
