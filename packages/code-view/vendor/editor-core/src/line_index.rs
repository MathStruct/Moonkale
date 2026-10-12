//! Stage 2: Logical Line Index
//!
//! Provides efficient line indexing using Rope data structure, supporting O(log N) access and editing.

use crate::text_buffer::TextBuffer;

/// Metadata for a logical line.
#[derive(Debug, Clone)]
pub struct LineMetadata {
    /// Fast path flag: whether this is pure ASCII.
    pub is_pure_ascii: bool,
    /// Byte length of this line.
    pub byte_length: usize,
    /// Character count of this line.
    pub char_count: usize,
}

impl LineMetadata {
    /// Create an empty line metadata record.
    pub fn new() -> Self {
        Self {
            is_pure_ascii: true,
            byte_length: 0,
            char_count: 0,
        }
    }

    /// Build line metadata for a single logical line (no trailing `\n`).
    pub fn from_text(text: &str) -> Self {
        let is_pure_ascii = text.is_ascii();
        Self {
            is_pure_ascii,
            byte_length: text.len(),
            char_count: text.chars().count(),
        }
    }
}

impl Default for LineMetadata {
    fn default() -> Self {
        Self::new()
    }
}

/// Logical line index - implemented using Rope data structure
///
/// Rope provides O(log N) line access, insertion, and deletion performance, suitable for large file editing
#[derive(Clone)]
pub struct LineIndex {
    /// Rope-backed text buffer that also provides line indexing primitives.
    text_buffer: TextBuffer,
}

impl LineIndex {
    /// Create a new line index
    pub fn new() -> Self {
        Self {
            text_buffer: TextBuffer::new(),
        }
    }

    /// Build line index from already-normalized internal text.
    ///
    /// This low-level constructor does not normalize line endings. Higher-level editor entry points
    /// normalize CRLF and lone CR to LF before constructing `LineIndex`; direct CRLF input here keeps
    /// `\r` as ordinary line content.
    pub fn from_text(text: &str) -> Self {
        Self {
            text_buffer: TextBuffer::from_text(text),
        }
    }

    /// Return the internal text buffer used as the canonical line-index backing store.
    pub(crate) fn text_buffer(&self) -> &TextBuffer {
        &self.text_buffer
    }

    /// Delete the specified line
    pub fn delete_line(&mut self, line_number: usize) {
        if line_number >= self.text_buffer.line_count() {
            return;
        }

        let start_char = self.text_buffer.line_to_char(line_number);
        let end_char = if line_number + 1 < self.text_buffer.line_count() {
            self.text_buffer.line_to_char(line_number + 1)
        } else {
            self.text_buffer.len_chars()
        };

        self.text_buffer.delete(start_char, end_char - start_char);
    }

    /// Get metadata for the specified line number (simulated)
    pub fn get_line(&self, line_number: usize) -> Option<LineMetadata> {
        let text = self.text_buffer.get_line_text(line_number)?;
        Some(LineMetadata::from_text(&text))
    }

    fn legacy_line_to_content_byte_offset(&self, line_number: usize) -> usize {
        if line_number == 0 {
            return 0;
        }

        if line_number >= self.text_buffer.line_count() {
            // Return total bytes minus newline count
            let newline_count = self.text_buffer.line_count().saturating_sub(1);
            return self.text_buffer.len_bytes().saturating_sub(newline_count);
        }

        // Rope's line_to_byte includes all newlines from previous lines
        // Subtract line_number newlines to match old behavior
        self.text_buffer
            .line_to_byte(line_number)
            .saturating_sub(line_number)
    }

    /// Legacy byte offset for a line start, excluding previous LF separator bytes.
    ///
    /// Prefer [`LineIndex::position_to_char_offset`] plus
    /// [`LineIndex::char_offset_to_byte_offset`] for offsets in the canonical document text. This
    /// compatibility method preserves the older byte-offset convention where previous `\n` bytes are
    /// not counted. If this index was built directly from CRLF text, `\r` remains line content and is
    /// counted.
    #[deprecated(
        note = "legacy byte offset excludes previous LF separators; use position_to_char_offset plus char_offset_to_byte_offset"
    )]
    pub fn line_to_offset(&self, line_number: usize) -> usize {
        self.legacy_line_to_content_byte_offset(line_number)
    }

    /// Legacy line lookup from a byte offset that excludes previous LF separator bytes.
    ///
    /// Prefer [`LineIndex::byte_offset_to_char_offset`] plus [`LineIndex::char_offset_to_position`]
    /// for offsets in the canonical document text. This compatibility method uses the same legacy
    /// convention as [`LineIndex::line_to_offset`].
    #[deprecated(
        note = "legacy byte offset excludes previous LF separators; use byte_offset_to_char_offset plus char_offset_to_position"
    )]
    pub fn offset_to_line(&self, offset: usize) -> usize {
        if offset == 0 {
            return 0;
        }

        // Need to add back newline count to get actual Rope byte offset
        // Binary search to find the correct line
        let mut low = 0;
        let mut high = self.text_buffer.line_count();

        while low < high {
            let mid = (low + high) / 2;
            let mid_offset = self.legacy_line_to_content_byte_offset(mid);

            if mid_offset < offset {
                low = mid + 1;
            } else if mid_offset > offset {
                high = mid;
            } else {
                return mid;
            }
        }

        low.saturating_sub(1)
            .min(self.text_buffer.line_count().saturating_sub(1))
    }

    /// Get line number and offset within line from character offset
    pub fn char_offset_to_position(&self, char_offset: usize) -> (usize, usize) {
        self.text_buffer.char_offset_to_position(char_offset)
    }

    /// Get character offset from line number and column number
    pub fn position_to_char_offset(&self, line: usize, column: usize) -> usize {
        self.text_buffer.position_to_char_offset(line, column)
    }

    /// Get total line count
    pub fn line_count(&self) -> usize {
        self.text_buffer.line_count()
    }

    /// Get total byte count
    pub fn byte_count(&self) -> usize {
        self.text_buffer.len_bytes()
    }

    /// Get total character count
    pub fn char_count(&self) -> usize {
        self.text_buffer.len_chars()
    }

    /// Get the character at the specified character offset (Unicode scalar index).
    ///
    /// Returns `None` if `char_offset` is out of bounds.
    pub fn char_at(&self, char_offset: usize) -> Option<char> {
        self.text_buffer.char_at(char_offset)
    }

    /// Convert a character offset (Unicode scalar values) to a UTF-8 byte offset.
    ///
    /// The returned byte offset is clamped to the document length.
    pub fn char_offset_to_byte_offset(&self, char_offset: usize) -> usize {
        self.text_buffer.char_offset_to_byte_offset(char_offset)
    }

    /// Convert a UTF-8 byte offset to a character offset (Unicode scalar values).
    ///
    /// The returned character offset is clamped to the document length.
    pub fn byte_offset_to_char_offset(&self, byte_offset: usize) -> usize {
        self.text_buffer.byte_offset_to_char_offset(byte_offset)
    }

    /// Convert a character offset to `(line, byte_column)` where `byte_column` is measured in UTF-8 bytes.
    pub fn char_offset_to_line_byte_column(&self, char_offset: usize) -> (usize, usize) {
        let char_offset = char_offset.min(self.text_buffer.len_chars());
        let line = self.text_buffer.char_to_line(char_offset);
        let line_start_char = self.text_buffer.line_to_char(line);

        let line_start_byte = self.text_buffer.char_offset_to_byte_offset(line_start_char);
        let byte_offset = self.text_buffer.char_offset_to_byte_offset(char_offset);
        (line, byte_offset.saturating_sub(line_start_byte))
    }

    /// Insert text (at specified character offset)
    pub fn insert(&mut self, char_offset: usize, text: &str) {
        self.text_buffer.insert(char_offset, text);
    }

    /// Delete text range (character offset)
    pub fn delete(&mut self, start_char: usize, len_chars: usize) {
        self.text_buffer.delete(start_char, len_chars);
    }

    /// Get complete text
    pub fn get_text(&self) -> String {
        self.text_buffer.get_text()
    }

    /// Get text in the specified character range.
    pub fn get_range(&self, start_char: usize, len_chars: usize) -> String {
        self.text_buffer.get_range(start_char, len_chars)
    }

    /// Get text of the specified line (excluding newline)
    pub fn get_line_text(&self, line_number: usize) -> Option<String> {
        self.text_buffer.get_line_text(line_number)
    }
}

impl Default for LineIndex {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_line_index() {
        let index = LineIndex::new();
        assert_eq!(index.line_count(), 1); // Rope empty document has 1 line
        assert_eq!(index.byte_count(), 0);
        assert_eq!(index.char_count(), 0);
    }

    #[test]
    fn test_from_text() {
        let text = "Line 1\nLine 2\nLine 3";
        let index = LineIndex::from_text(text);

        assert_eq!(index.line_count(), 3);
        assert_eq!(index.byte_count(), text.len());
        assert_eq!(index.char_count(), text.chars().count());
    }

    #[test]
    #[allow(deprecated)]
    fn test_line_to_offset() {
        let text = "First line\nSecond line\nThird line";
        let index = LineIndex::from_text(text);

        assert_eq!(index.line_to_offset(0), 0);
        assert_eq!(index.line_to_offset(1), 10); // "First line" (excluding \n)
        assert_eq!(index.line_to_offset(2), 21); // "First line" (10) + "Second line" (11) = 21
    }

    #[test]
    #[allow(deprecated)]
    fn test_offset_to_line() {
        let text = "First line\nSecond line\nThird line";
        let index = LineIndex::from_text(text);

        assert_eq!(index.offset_to_line(0), 0);
        assert_eq!(index.offset_to_line(5), 0);
        assert_eq!(index.offset_to_line(11), 1);
        assert_eq!(index.offset_to_line(23), 2);
    }

    #[test]
    fn test_char_offset_to_position() {
        let text = "ABC\nDEF\nGHI";
        let index = LineIndex::from_text(text);

        assert_eq!(index.char_offset_to_position(0), (0, 0)); // A
        assert_eq!(index.char_offset_to_position(2), (0, 2)); // C
        assert_eq!(index.char_offset_to_position(4), (1, 0)); // D
        assert_eq!(index.char_offset_to_position(8), (2, 0)); // G
    }

    #[test]
    fn test_position_to_char_offset() {
        let text = "ABC\nDEF\nGHI";
        let index = LineIndex::from_text(text);

        assert_eq!(index.position_to_char_offset(0, 0), 0); // A
        assert_eq!(index.position_to_char_offset(0, 2), 2); // C
        assert_eq!(index.position_to_char_offset(1, 0), 4); // D
        assert_eq!(index.position_to_char_offset(2, 0), 8); // G
    }

    #[test]
    fn test_utf8_cjk() {
        let text = "你好\n世界";
        let index = LineIndex::from_text(text);

        assert_eq!(index.line_count(), 2);
        assert_eq!(index.byte_count(), text.len());
        assert_eq!(index.char_count(), 5); // 5 characters (你好\n世界)

        // First line: "你好"
        assert_eq!(index.char_offset_to_position(0), (0, 0));
        assert_eq!(index.char_offset_to_position(1), (0, 1));
        // Second line: "世界" (newline at character offset 2)
        assert_eq!(index.char_offset_to_position(3), (1, 0));
    }

    #[test]
    fn test_get_line() {
        let text = "Line 1\nLine 2\nLine 3";
        let index = LineIndex::from_text(text);

        let line0 = index.get_line(0);
        assert!(line0.is_some());
        let meta = line0.unwrap();
        assert!(meta.is_pure_ascii);

        let line_none = index.get_line(10);
        assert!(line_none.is_none());
    }

    #[test]
    fn test_insert_delete_lines() {
        let mut index = LineIndex::from_text("Line 1\nLine 2");
        assert_eq!(index.line_count(), 2);

        index.delete_line(0);
        assert_eq!(index.line_count(), 1);
    }

    #[test]
    fn test_mixed_ascii_cjk() {
        let text = "Hello 你好\nWorld 世界";
        let index = LineIndex::from_text(text);

        assert_eq!(index.line_count(), 2);
        assert!(index.byte_count() > index.char_count());
    }

    #[test]
    fn test_large_document() {
        let mut lines = Vec::new();
        for i in 0..10000 {
            lines.push(format!("Line {}", i));
        }
        let text = lines.join("\n");

        let index = LineIndex::from_text(&text);
        assert_eq!(index.line_count(), 10000);

        // Test accessing middle line
        let line_5000 = index.get_line(5000);
        assert!(line_5000.is_some());
    }

    #[test]
    fn test_insert_text() {
        let mut index = LineIndex::from_text("Hello World");

        index.insert(6, "Beautiful ");
        assert_eq!(index.get_text(), "Hello Beautiful World");
    }

    #[test]
    fn test_delete_text() {
        let mut index = LineIndex::from_text("Hello Beautiful World");

        index.delete(6, 10); // Delete "Beautiful "
        assert_eq!(index.get_text(), "Hello World");
    }

    #[test]
    fn test_char_byte_offset_roundtrip() {
        let text = "a你好\n🌍b";
        let index = LineIndex::from_text(text);

        for char_offset in 0..=index.char_count() {
            let byte_offset = index.char_offset_to_byte_offset(char_offset);
            let recovered = index.byte_offset_to_char_offset(byte_offset);
            assert_eq!(recovered, char_offset);

            let (line, byte_col) = index.char_offset_to_line_byte_column(char_offset);
            let line_start_char = index.position_to_char_offset(line, 0);
            let line_start_byte = index.char_offset_to_byte_offset(line_start_char);
            assert_eq!(line_start_byte + byte_col, byte_offset);
        }
    }
}
