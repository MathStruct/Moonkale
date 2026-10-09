//! Deprecated Piece Table compatibility layer.
//!
//! The main editor path uses the rope-backed `TextBuffer` behind `LineIndex`. This module remains
//! public for compatibility and standalone validation tests.

/// Buffer type identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferType {
    /// Read-only original buffer
    Original,
    /// Append-only add buffer
    Add,
}

/// Piece structure: references a fragment in a buffer
#[derive(Debug, Clone)]
pub struct Piece {
    /// Buffer type
    pub buffer_type: BufferType,
    /// Start position in the corresponding buffer (byte offset)
    pub start: usize,
    /// Byte length of the fragment
    pub byte_length: usize,
    /// Character count of the fragment (handles UTF-8 multi-byte characters)
    pub char_count: usize,
}

impl Piece {
    /// Create a new Piece
    pub fn new(
        buffer_type: BufferType,
        start: usize,
        byte_length: usize,
        char_count: usize,
    ) -> Self {
        Self {
            buffer_type,
            start,
            byte_length,
            char_count,
        }
    }
}

/// Piece Table - main storage structure
pub struct PieceTable {
    /// Read-only original buffer
    original_buffer: Vec<u8>,
    /// Append-only add buffer
    add_buffer: Vec<u8>,
    /// List of pieces
    pieces: Vec<Piece>,
    /// Operation counter (for triggering GC)
    operation_count: usize,
    /// GC threshold (trigger GC after every N operations)
    gc_threshold: usize,
}

/// Error returned by fallible [`PieceTable`] compatibility APIs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PieceTableError {
    /// A piece points outside of its backing buffer.
    InvalidPieceRange {
        /// Backing buffer selected by the piece.
        buffer_type: BufferType,
        /// Byte start recorded in the piece.
        start: usize,
        /// Byte length recorded in the piece.
        byte_length: usize,
        /// Current byte length of the selected backing buffer.
        buffer_len: usize,
    },
    /// A piece range is not valid UTF-8.
    InvalidPieceUtf8 {
        /// Backing buffer selected by the piece.
        buffer_type: BufferType,
        /// Byte start recorded in the piece.
        start: usize,
        /// Byte length recorded in the piece.
        byte_length: usize,
    },
}

impl std::fmt::Display for PieceTableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPieceRange {
                buffer_type,
                start,
                byte_length,
                buffer_len,
            } => write!(
                f,
                "invalid {:?} piece range {}..{} for buffer length {}",
                buffer_type,
                start,
                start.saturating_add(*byte_length),
                buffer_len
            ),
            Self::InvalidPieceUtf8 {
                buffer_type,
                start,
                byte_length,
            } => write!(
                f,
                "invalid UTF-8 in {:?} piece range {}..{}",
                buffer_type,
                start,
                start.saturating_add(*byte_length)
            ),
        }
    }
}

impl std::error::Error for PieceTableError {}

fn piece_table_invariant_error<T>(error: PieceTableError, fallback: T) -> T {
    let _ = &fallback;

    #[cfg(debug_assertions)]
    panic!("PieceTable invariant violated: {error}");

    #[cfg(not(debug_assertions))]
    {
        let _ = error;
        fallback
    }
}

impl PieceTable {
    /// Create a new Piece Table from original text
    pub fn new(text: &str) -> Self {
        let bytes = text.as_bytes().to_vec();
        let char_count = text.chars().count();
        let byte_length = bytes.len();

        let pieces = if byte_length > 0 {
            vec![Piece::new(BufferType::Original, 0, byte_length, char_count)]
        } else {
            Vec::new()
        };

        Self {
            original_buffer: bytes,
            add_buffer: Vec::new(),
            pieces,
            operation_count: 0,
            gc_threshold: 1000, // Trigger GC after every 1000 operations
        }
    }

    /// Create an empty Piece Table
    pub fn empty() -> Self {
        Self {
            original_buffer: Vec::new(),
            add_buffer: Vec::new(),
            pieces: Vec::new(),
            operation_count: 0,
            gc_threshold: 1000,
        }
    }

    /// Insert text at the specified character offset
    pub fn insert(&mut self, offset: usize, text: &str) {
        if let Err(error) = self.try_insert(offset, text) {
            piece_table_invariant_error(error, ());
        }
    }

    /// Fallible variant of [`PieceTable::insert`] that reports piece-table invariant failures.
    pub fn try_insert(&mut self, offset: usize, text: &str) -> Result<(), PieceTableError> {
        if text.is_empty() {
            return Ok(());
        }

        let text_bytes = text.as_bytes();
        let text_char_count = text.chars().count();
        let text_byte_length = text_bytes.len();
        let add_start = self.add_buffer.len();

        // Find the piece at the insertion position
        let (piece_index, char_offset_in_piece) = self.find_piece_at_offset(offset);

        enum InsertPlan {
            Insert { index: usize, piece: Piece },
            Splice { index: usize, pieces: Vec<Piece> },
            Push(Piece),
        }

        let new_piece = Piece::new(
            BufferType::Add,
            add_start,
            text_byte_length,
            text_char_count,
        );

        let plan = if let Some(piece_idx) = piece_index {
            let piece = self.pieces[piece_idx].clone();

            if char_offset_in_piece == 0 {
                // Insert at the beginning of the piece
                InsertPlan::Insert {
                    index: piece_idx,
                    piece: new_piece,
                }
            } else if char_offset_in_piece == piece.char_count {
                // Insert at the end of the piece
                InsertPlan::Insert {
                    index: piece_idx + 1,
                    piece: new_piece,
                }
            } else {
                // Insert in the middle of the piece, need to split
                let (left_piece, right_piece) = self.split_piece(&piece, char_offset_in_piece)?;
                InsertPlan::Splice {
                    index: piece_idx,
                    pieces: vec![left_piece, new_piece, right_piece],
                }
            }
        } else {
            // Empty document or insert at the end
            InsertPlan::Push(new_piece)
        };

        self.add_buffer.extend_from_slice(text_bytes);
        match plan {
            InsertPlan::Insert { index, piece } => self.pieces.insert(index, piece),
            InsertPlan::Splice { index, pieces } => {
                self.pieces.splice(index..=index, pieces);
            }
            InsertPlan::Push(piece) => self.pieces.push(piece),
        }

        // Try to merge adjacent pieces
        self.try_merge_adjacent_pieces();

        // Check if GC needs to be triggered
        self.try_check_gc()?;
        Ok(())
    }

    /// Delete characters in the specified range
    pub fn delete(&mut self, start_offset: usize, length: usize) {
        if let Err(error) = self.try_delete(start_offset, length) {
            piece_table_invariant_error(error, ());
        }
    }

    /// Fallible variant of [`PieceTable::delete`] that reports piece-table invariant failures.
    pub fn try_delete(
        &mut self,
        start_offset: usize,
        length: usize,
    ) -> Result<(), PieceTableError> {
        if length == 0 {
            return Ok(());
        }

        let end_offset = start_offset.saturating_add(length);

        // Find the pieces at the start and end positions
        let (start_piece_idx, start_char_offset) = self.find_piece_at_offset(start_offset);
        let (end_piece_idx, end_char_offset) = self.find_piece_at_offset(end_offset);

        match (start_piece_idx, end_piece_idx) {
            (Some(start_idx), Some(end_idx)) if start_idx == end_idx => {
                // Delete range is within the same piece
                let piece = self.pieces[start_idx].clone();

                if start_char_offset == 0 && end_char_offset == piece.char_count {
                    // Delete the entire piece
                    self.pieces.remove(start_idx);
                } else if start_char_offset == 0 {
                    // Delete from the beginning
                    let (_, right) = self.split_piece(&piece, end_char_offset)?;
                    self.pieces[start_idx] = right;
                } else if end_char_offset == piece.char_count {
                    // Delete to the end
                    let (left, _) = self.split_piece(&piece, start_char_offset)?;
                    self.pieces[start_idx] = left;
                } else {
                    // Delete in the middle
                    let (left, temp) = self.split_piece(&piece, start_char_offset)?;
                    let (_, right) =
                        self.split_piece(&temp, end_char_offset - start_char_offset)?;
                    self.pieces.splice(start_idx..=start_idx, vec![left, right]);
                }
            }
            (Some(start_idx), Some(end_idx)) => {
                // Delete range spans multiple pieces
                let start_piece = self.pieces[start_idx].clone();
                let end_piece = self.pieces[end_idx].clone();

                let mut new_pieces = Vec::new();

                // Handle the starting piece
                if start_char_offset > 0 {
                    let (left, _) = self.split_piece(&start_piece, start_char_offset)?;
                    new_pieces.push(left);
                }

                // Handle the ending piece
                if end_char_offset < end_piece.char_count {
                    let (_, right) = self.split_piece(&end_piece, end_char_offset)?;
                    new_pieces.push(right);
                }

                // Replace all pieces in the range
                self.pieces.splice(start_idx..=end_idx, new_pieces);
            }
            (None, None) => {
                // Empty document, no need to delete
            }
            _ => {
                // Only one position is valid, handle edge cases
                if let Some(start_idx) = start_piece_idx {
                    // Delete from start_idx to the end
                    let start_piece = self.pieces[start_idx].clone();
                    if start_char_offset == 0 {
                        self.pieces.truncate(start_idx);
                    } else {
                        let (left, _) = self.split_piece(&start_piece, start_char_offset)?;
                        self.pieces.truncate(start_idx);
                        self.pieces.push(left);
                    }
                }
            }
        }

        // Check if GC needs to be triggered
        self.try_check_gc()?;
        Ok(())
    }

    /// Get the entire document content
    pub fn get_text(&self) -> String {
        match self.try_get_text() {
            Ok(text) => text,
            Err(error) => piece_table_invariant_error(error, String::new()),
        }
    }

    /// Fallible variant of [`PieceTable::get_text`] that reports invalid piece ranges or UTF-8.
    pub fn try_get_text(&self) -> Result<String, PieceTableError> {
        let mut result = String::new();
        for piece in &self.pieces {
            result.push_str(self.piece_text(piece)?);
        }
        Ok(result)
    }

    /// Get text in the specified range
    pub fn get_range(&self, start_offset: usize, length: usize) -> String {
        match self.try_get_range(start_offset, length) {
            Ok(text) => text,
            Err(error) => piece_table_invariant_error(error, String::new()),
        }
    }

    /// Fallible variant of [`PieceTable::get_range`] that reports invalid piece ranges or UTF-8.
    pub fn try_get_range(
        &self,
        start_offset: usize,
        length: usize,
    ) -> Result<String, PieceTableError> {
        let mut result = String::new();
        let mut current_offset: usize = 0;
        let end_offset = start_offset.saturating_add(length);

        for piece in &self.pieces {
            let piece_end = current_offset.saturating_add(piece.char_count);

            if current_offset >= end_offset {
                break;
            }

            if piece_end > start_offset {
                let piece_text = self.piece_text(piece)?;

                let skip_chars = start_offset.saturating_sub(current_offset);

                let take_chars = if piece_end > end_offset {
                    end_offset - current_offset.max(start_offset)
                } else {
                    piece.char_count - skip_chars
                };

                result.push_str(
                    &piece_text
                        .chars()
                        .skip(skip_chars)
                        .take(take_chars)
                        .collect::<String>(),
                );
            }

            current_offset = piece_end;
        }

        Ok(result)
    }

    /// Get the total character count of the document
    pub fn char_count(&self) -> usize {
        self.pieces.iter().map(|p| p.char_count).sum()
    }

    /// Get the total byte count of the document
    pub fn byte_count(&self) -> usize {
        self.pieces.iter().map(|p| p.byte_length).sum()
    }

    /// Get the size of add_buffer (for memory testing)
    pub fn add_buffer_size(&self) -> usize {
        self.add_buffer.len()
    }

    fn buffer_for_piece(&self, piece: &Piece) -> &[u8] {
        match piece.buffer_type {
            BufferType::Original => &self.original_buffer,
            BufferType::Add => &self.add_buffer,
        }
    }

    fn piece_bytes(&self, piece: &Piece) -> Result<&[u8], PieceTableError> {
        let buffer = self.buffer_for_piece(piece);
        let end = piece.start.checked_add(piece.byte_length).ok_or(
            PieceTableError::InvalidPieceRange {
                buffer_type: piece.buffer_type,
                start: piece.start,
                byte_length: piece.byte_length,
                buffer_len: buffer.len(),
            },
        )?;

        buffer
            .get(piece.start..end)
            .ok_or(PieceTableError::InvalidPieceRange {
                buffer_type: piece.buffer_type,
                start: piece.start,
                byte_length: piece.byte_length,
                buffer_len: buffer.len(),
            })
    }

    fn piece_text(&self, piece: &Piece) -> Result<&str, PieceTableError> {
        std::str::from_utf8(self.piece_bytes(piece)?).map_err(|_| {
            PieceTableError::InvalidPieceUtf8 {
                buffer_type: piece.buffer_type,
                start: piece.start,
                byte_length: piece.byte_length,
            }
        })
    }

    /// Find the piece at the specified character offset and the offset within that piece
    /// Returns (piece_index, char_offset_in_piece)
    fn find_piece_at_offset(&self, offset: usize) -> (Option<usize>, usize) {
        let mut current_offset: usize = 0;

        for (idx, piece) in self.pieces.iter().enumerate() {
            let next_offset = current_offset.saturating_add(piece.char_count);
            if offset <= next_offset {
                return (Some(idx), offset - current_offset);
            }
            current_offset = next_offset;
        }

        match self.pieces.last() {
            Some(piece) => (Some(self.pieces.len() - 1), piece.char_count),
            None => (None, 0),
        }
    }

    /// Split a piece at the specified character position
    /// Returns (left_piece, right_piece)
    fn split_piece(
        &self,
        piece: &Piece,
        char_offset: usize,
    ) -> Result<(Piece, Piece), PieceTableError> {
        let piece_text = self.piece_text(piece)?;
        let char_offset = char_offset.min(piece.char_count);

        // Calculate byte offset (O(n))
        // `char_offset` is the character offset within this piece; convert it to UTF-8 byte offset to complete the split.
        let byte_offset = piece_text
            .char_indices()
            .nth(char_offset)
            .map(|(i, _)| i)
            .unwrap_or(piece.byte_length);

        let left = Piece::new(piece.buffer_type, piece.start, byte_offset, char_offset);

        let right = Piece::new(
            piece.buffer_type,
            piece.start + byte_offset,
            piece.byte_length - byte_offset,
            piece.char_count - char_offset,
        );

        Ok((left, right))
    }

    /// Check if two pieces can be merged (must be from the same buffer and adjacent)
    fn can_merge(&self, p1: &Piece, p2: &Piece) -> bool {
        p1.buffer_type == p2.buffer_type &&
        p1.buffer_type == BufferType::Add && // Only merge pieces in AddBuffer
        p1.start.saturating_add(p1.byte_length) == p2.start
    }

    /// Merge two adjacent pieces
    fn merge_pieces(&self, p1: &Piece, p2: &Piece) -> Piece {
        Piece::new(
            p1.buffer_type,
            p1.start,
            p1.byte_length.saturating_add(p2.byte_length),
            p1.char_count.saturating_add(p2.char_count),
        )
    }

    /// Try to merge adjacent pieces after insertion
    fn try_merge_adjacent_pieces(&mut self) {
        let mut i = 0;
        while i + 1 < self.pieces.len() {
            if self.can_merge(&self.pieces[i], &self.pieces[i + 1]) {
                let merged = self.merge_pieces(&self.pieces[i], &self.pieces[i + 1]);
                self.pieces.splice(i..=i + 1, vec![merged]);
            } else {
                i += 1;
            }
        }
    }

    /// Garbage collection: compact add_buffer, remove unreferenced data
    pub fn gc(&mut self) {
        if let Err(error) = self.try_gc() {
            piece_table_invariant_error(error, ());
        }
    }

    /// Fallible variant of [`PieceTable::gc`] that reports invalid add-buffer piece ranges.
    pub fn try_gc(&mut self) -> Result<(), PieceTableError> {
        // Collect all referenced fragments in AddBuffer
        let mut referenced_ranges: Vec<(usize, usize)> = Vec::new();
        for piece in self
            .pieces
            .iter()
            .filter(|p| p.buffer_type == BufferType::Add)
        {
            let end = piece.start.checked_add(piece.byte_length).ok_or(
                PieceTableError::InvalidPieceRange {
                    buffer_type: piece.buffer_type,
                    start: piece.start,
                    byte_length: piece.byte_length,
                    buffer_len: self.add_buffer.len(),
                },
            )?;
            if self.add_buffer.get(piece.start..end).is_none() {
                return Err(PieceTableError::InvalidPieceRange {
                    buffer_type: piece.buffer_type,
                    start: piece.start,
                    byte_length: piece.byte_length,
                    buffer_len: self.add_buffer.len(),
                });
            }
            referenced_ranges.push((piece.start, end));
        }

        if referenced_ranges.is_empty() {
            // No references, clear add_buffer
            self.add_buffer.clear();
            return Ok(());
        }

        // Sort by start position
        referenced_ranges.sort_by_key(|r| r.0);

        // Merge overlapping ranges
        let mut merged_ranges = vec![referenced_ranges[0]];
        for range in referenced_ranges.iter().skip(1) {
            let last_idx = merged_ranges.len() - 1;
            if range.0 <= merged_ranges[last_idx].1 {
                // Overlapping or adjacent, merge
                merged_ranges[last_idx].1 = merged_ranges[last_idx].1.max(range.1);
            } else {
                merged_ranges.push(*range);
            }
        }

        // Build new add_buffer and update piece references
        let mut new_add_buffer = Vec::new();
        let mut mappings: Vec<(usize, usize, usize)> = Vec::new(); // (old_start, old_end, new_start)

        for (old_start, old_end) in merged_ranges {
            let new_start = new_add_buffer.len();
            let slice = self.add_buffer.get(old_start..old_end).ok_or(
                PieceTableError::InvalidPieceRange {
                    buffer_type: BufferType::Add,
                    start: old_start,
                    byte_length: old_end.saturating_sub(old_start),
                    buffer_len: self.add_buffer.len(),
                },
            )?;
            new_add_buffer.extend_from_slice(slice);
            mappings.push((old_start, old_end, new_start));
        }

        // Update offsets of all AddBuffer pieces (allow piece.start to fall within merged ranges)
        for piece in &mut self.pieces {
            if piece.buffer_type != BufferType::Add {
                continue;
            }

            // Binary search: find the last mapping where old_start <= piece.start
            let idx = match mappings.binary_search_by_key(&piece.start, |(s, _, _)| *s) {
                Ok(exact) => exact,
                Err(insert_pos) => insert_pos.saturating_sub(1),
            };

            if let Some((old_start, old_end, new_start)) = mappings.get(idx).copied()
                && piece.start < old_end
            {
                piece.start = new_start + (piece.start - old_start);
            }
        }

        self.add_buffer = new_add_buffer;
        self.operation_count = 0; // Reset counter
        Ok(())
    }

    fn try_check_gc(&mut self) -> Result<(), PieceTableError> {
        self.operation_count += 1;
        if self.operation_count >= self.gc_threshold {
            self.try_gc()?;
        }
        Ok(())
    }

    /// Set GC threshold
    pub fn set_gc_threshold(&mut self, threshold: usize) {
        self.gc_threshold = threshold;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_piece_table() {
        let pt = PieceTable::new("Hello, World!");
        assert_eq!(pt.get_text(), "Hello, World!");
        assert_eq!(pt.char_count(), 13);
    }

    #[test]
    fn test_empty_piece_table() {
        let pt = PieceTable::empty();
        assert_eq!(pt.get_text(), "");
        assert_eq!(pt.char_count(), 0);
    }

    #[test]
    fn test_insert_at_start() {
        let mut pt = PieceTable::new("World");
        pt.insert(0, "Hello, ");
        assert_eq!(pt.get_text(), "Hello, World");
    }

    #[test]
    fn test_insert_at_end() {
        let mut pt = PieceTable::new("Hello");
        pt.insert(5, ", World");
        assert_eq!(pt.get_text(), "Hello, World");
    }

    #[test]
    fn test_insert_in_middle() {
        let mut pt = PieceTable::new("Hlo");
        pt.insert(1, "el");
        assert_eq!(pt.get_text(), "Hello");
    }

    #[test]
    fn test_delete_at_start() {
        let mut pt = PieceTable::new("Hello, World");
        pt.delete(0, 7);
        assert_eq!(pt.get_text(), "World");
    }

    #[test]
    fn test_delete_at_end() {
        let mut pt = PieceTable::new("Hello, World");
        pt.delete(5, 7);
        assert_eq!(pt.get_text(), "Hello");
    }

    #[test]
    fn test_delete_in_middle() {
        let mut pt = PieceTable::new("Hello, World");
        pt.delete(5, 2);
        assert_eq!(pt.get_text(), "HelloWorld");
    }

    #[test]
    fn test_multiple_operations() {
        let mut pt = PieceTable::new("Hello");
        pt.insert(5, " World");
        pt.insert(5, ",");
        pt.delete(0, 7);
        pt.insert(0, "Hi, ");
        assert_eq!(pt.get_text(), "Hi, World");
    }

    #[test]
    fn test_utf8_chinese() {
        let mut pt = PieceTable::new("你好");
        assert_eq!(pt.char_count(), 2);
        assert_eq!(pt.byte_count(), 6);

        pt.insert(1, "们");
        assert_eq!(pt.get_text(), "你们好");
        assert_eq!(pt.char_count(), 3);
    }

    #[test]
    fn test_utf8_emoji() {
        let mut pt = PieceTable::new("Hello 👋");
        pt.insert(6, "World ");
        assert_eq!(pt.get_text(), "Hello World 👋");
    }

    #[test]
    fn test_get_range() {
        let pt = PieceTable::new("Hello, World!");
        assert_eq!(pt.get_range(0, 5), "Hello");
        assert_eq!(pt.get_range(7, 5), "World");
        assert_eq!(pt.get_range(0, 13), "Hello, World!");
    }

    #[test]
    fn test_try_get_text_reports_invalid_piece_range_without_panic() {
        let mut pt = PieceTable::new("abc");
        pt.pieces[0].byte_length = usize::MAX;

        let result = std::panic::catch_unwind(|| pt.try_get_text());
        assert!(matches!(
            result,
            Ok(Err(PieceTableError::InvalidPieceRange { .. }))
        ));
    }

    #[test]
    fn test_try_get_text_reports_invalid_utf8_without_panic() {
        let mut pt = PieceTable::empty();
        pt.add_buffer.push(0xff);
        pt.pieces.push(Piece::new(BufferType::Add, 0, 1, 1));

        let result = std::panic::catch_unwind(|| pt.try_get_text());
        assert!(matches!(
            result,
            Ok(Err(PieceTableError::InvalidPieceUtf8 { .. }))
        ));
    }

    #[test]
    fn test_try_insert_reports_split_piece_invariant_failure() {
        let mut pt = PieceTable::empty();
        pt.add_buffer.push(0xff);
        pt.pieces.push(Piece::new(BufferType::Add, 0, 1, 2));

        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| pt.try_insert(1, "x")));
        assert!(matches!(
            result,
            Ok(Err(PieceTableError::InvalidPieceUtf8 { .. }))
        ));
        assert_eq!(pt.add_buffer, vec![0xff]);
    }

    #[test]
    fn test_piece_merging() {
        let mut pt = PieceTable::new("Hello");

        // 连续插入相邻文本应该被合并
        let initial_pieces = pt.pieces.len();
        pt.insert(5, " ");
        pt.insert(6, "World");

        // 由于合并，pieces 数量应该较少
        assert_eq!(pt.get_text(), "Hello World");
        // 验证合并发生了（应该有2个pieces：原始的 "Hello" 和合并后的 " World"）
        assert!(pt.pieces.len() <= initial_pieces + 1);
    }

    #[test]
    fn test_gc_basic() {
        let mut pt = PieceTable::new("Hello");

        // 插入一些文本
        pt.insert(5, " World");
        pt.insert(11, "!");

        let add_buffer_size_before = pt.add_buffer.len();

        // 删除一些文本，产生未引用的片段
        pt.delete(5, 6); // 删除 " World"

        // 手动触发 GC
        pt.gc();

        // 验证内容不变
        assert_eq!(pt.get_text(), "Hello!");

        // 验证 add_buffer 被压缩了
        assert!(pt.add_buffer.len() < add_buffer_size_before);
    }

    #[test]
    fn test_gc_multiple_references() {
        let mut pt = PieceTable::new("ABC");

        // 创建多个引用到 add_buffer 的 pieces
        pt.insert(1, "1");
        pt.insert(3, "2");
        pt.insert(5, "3");

        assert_eq!(pt.get_text(), "A1B2C3");

        // GC 不应该删除被引用的数据
        pt.gc();

        // 内容应该保持不变
        assert_eq!(pt.get_text(), "A1B2C3");

        // add_buffer 仍然包含所有被引用的数据
        assert!(!pt.add_buffer.is_empty());
    }

    #[test]
    fn test_auto_gc_trigger() {
        let mut pt = PieceTable::new("Test");
        pt.set_gc_threshold(5); // 设置低阈值便于测试

        // 执行多次操作
        for i in 0..6 {
            pt.insert(4 + i, "x");
        }

        // 应该触发了自动 GC（计数器被重置）
        assert!(pt.operation_count < 6);
    }
}
