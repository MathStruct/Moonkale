//! Text search helpers.
//!
//! This module provides simple search APIs over a UTF-8 `&str`, using **character offsets**
//! (not byte offsets) for all public inputs/outputs. It supports:
//!
//! - plain substring search (escaped and compiled into a regex)
//! - regex search
//! - optional whole-word matching

use regex::{Regex, RegexBuilder};

/// Options that control how search is performed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchOptions {
    /// If `true`, performs a case-sensitive search.
    pub case_sensitive: bool,
    /// If `true`, matches only whole words (ASCII-alphanumeric and `_`).
    pub whole_word: bool,
    /// If `true`, treats the query as a regex pattern.
    pub regex: bool,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            case_sensitive: true,
            whole_word: false,
            regex: false,
        }
    }
}

/// A match returned by the search APIs, expressed as a half-open character range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchMatch {
    /// Inclusive start character offset.
    pub start: usize,
    /// Exclusive end character offset.
    pub end: usize,
}

impl SearchMatch {
    /// Returns the length of the match in characters.
    pub fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }

    /// Returns `true` if the match is empty.
    pub fn is_empty(&self) -> bool {
        self.start >= self.end
    }
}

/// Search errors.
#[derive(Debug)]
pub enum SearchError {
    /// The provided regex pattern failed to compile.
    InvalidRegex(regex::Error),
}

impl std::fmt::Display for SearchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRegex(err) => write!(f, "Invalid regex: {}", err),
        }
    }
}

impl std::error::Error for SearchError {}

#[derive(Debug)]
pub(crate) struct CharIndex {
    char_to_byte: Vec<usize>,
    text_len: usize,
}

impl CharIndex {
    pub(crate) fn new(text: &str) -> Self {
        let mut char_to_byte: Vec<usize> = text.char_indices().map(|(b, _)| b).collect();
        char_to_byte.push(text.len());
        Self {
            char_to_byte,
            text_len: text.len(),
        }
    }

    pub(crate) fn char_count(&self) -> usize {
        self.char_to_byte.len().saturating_sub(1)
    }

    pub(crate) fn char_to_byte(&self, char_offset: usize) -> usize {
        let clamped = char_offset.min(self.char_count());
        self.char_to_byte
            .get(clamped)
            .cloned()
            .unwrap_or(self.text_len)
    }

    pub(crate) fn byte_to_char(&self, byte_offset: usize) -> usize {
        let clamped = byte_offset.min(self.text_len);
        match self.char_to_byte.binary_search(&clamped) {
            Ok(idx) => idx,
            Err(idx) => idx,
        }
    }

    pub(crate) fn char_at(&self, text: &str, char_offset: usize) -> Option<char> {
        if char_offset >= self.char_count() {
            return None;
        }
        let start = self.char_to_byte[char_offset];
        let end = self.char_to_byte[char_offset + 1];
        text.get(start..end)?.chars().next()
    }
}

fn compile_search_regex(query: &str, options: SearchOptions) -> Result<Regex, SearchError> {
    let pattern = if options.regex {
        query.to_string()
    } else {
        regex::escape(query)
    };

    RegexBuilder::new(&pattern)
        .case_insensitive(!options.case_sensitive)
        .multi_line(true)
        .build()
        .map_err(SearchError::InvalidRegex)
}

/// A search query whose regex is compiled once and can be reused across many texts.
///
/// The one-shot free functions ([`find_all`], [`find_next`], …) recompile the regex on every
/// call, which is wasteful when scanning many small texts (e.g. searching a file line-by-line, or
/// many files). Compile once with [`CompiledSearch::new`] and call [`CompiledSearch::find_all`]
/// repeatedly instead.
#[derive(Debug, Clone)]
pub struct CompiledSearch {
    regex: Regex,
    whole_word: bool,
}

impl CompiledSearch {
    /// Compile `query` under `options` for repeated use.
    ///
    /// Returns [`SearchError::InvalidRegex`] if the query is an invalid regex (only possible when
    /// `options.regex` is set).
    pub fn new(query: &str, options: SearchOptions) -> Result<Self, SearchError> {
        Ok(Self {
            regex: compile_search_regex(query, options)?,
            whole_word: options.whole_word,
        })
    }

    /// Find all non-empty matches in `text` (character offsets, half-open ranges).
    ///
    /// Semantics match [`find_all`]; only the regex compilation is amortized.
    pub fn find_all(&self, text: &str) -> Vec<SearchMatch> {
        let index = CharIndex::new(text);
        let mut matches: Vec<SearchMatch> = Vec::new();
        for m in self.regex.find_iter(text) {
            let start = index.byte_to_char(m.start());
            let end = index.byte_to_char(m.end());
            let candidate = SearchMatch { start, end };

            if candidate.is_empty() {
                continue;
            }
            if self.whole_word && !is_whole_word(text, &index, candidate) {
                continue;
            }

            matches.push(candidate);
        }

        matches
    }
}

fn is_word_char(ch: char) -> bool {
    ch == '_' || ch.is_alphanumeric()
}

fn is_whole_word(text: &str, index: &CharIndex, m: SearchMatch) -> bool {
    if m.is_empty() {
        return false;
    }

    let before = if m.start == 0 {
        None
    } else {
        index.char_at(text, m.start.saturating_sub(1))
    };
    let after = index.char_at(text, m.end);

    !before.is_some_and(is_word_char) && !after.is_some_and(is_word_char)
}

/// Find the next occurrence of `query` in `text`, searching forward from `from_char`.
///
/// - Returns `Ok(None)` if no match is found (or if `query` is empty).
/// - Match ranges are character offsets and are half-open (`[start, end)`).
pub fn find_next(
    text: &str,
    query: &str,
    options: SearchOptions,
    from_char: usize,
) -> Result<Option<SearchMatch>, SearchError> {
    if query.is_empty() {
        return Ok(None);
    }

    let re = compile_search_regex(query, options)?;
    let index = CharIndex::new(text);

    let mut start_char = from_char.min(index.char_count());
    loop {
        let start_byte = index.char_to_byte(start_char);
        let Some(m) = re.find_at(text, start_byte) else {
            return Ok(None);
        };

        let start = index.byte_to_char(m.start());
        let end = index.byte_to_char(m.end());
        let candidate = SearchMatch { start, end };

        if candidate.is_empty() {
            if end >= index.char_count() {
                return Ok(None);
            }
            start_char = end + 1;
            continue;
        }

        if options.whole_word && !is_whole_word(text, &index, candidate) {
            start_char = candidate.end;
            continue;
        }

        return Ok(Some(candidate));
    }
}

/// Find the previous occurrence of `query` in `text`, searching backward from `from_char`.
///
/// - Returns `Ok(None)` if no match is found (or if `query` is empty).
/// - Match ranges are character offsets and are half-open (`[start, end)`).
pub fn find_prev(
    text: &str,
    query: &str,
    options: SearchOptions,
    from_char: usize,
) -> Result<Option<SearchMatch>, SearchError> {
    if query.is_empty() {
        return Ok(None);
    }

    let re = compile_search_regex(query, options)?;
    let index = CharIndex::new(text);

    let limit_char = from_char.min(index.char_count());
    let limit_byte = index.char_to_byte(limit_char);

    // Iterate over the *full* text (not a truncated slice) so anchors (`$`, `\b`, `\z`) and greedy
    // quantifiers see the real trailing context, then keep the last match that ends at or before
    // the boundary. Searching a `&text[..limit_byte]` slice would let those constructs match at the
    // artificial slice end, yielding matches that do not exist in the full document.
    let mut last: Option<SearchMatch> = None;
    for m in re.find_iter(text) {
        // Matches are yielded left-to-right; once one starts at/after the boundary, no later match
        // can end before it either.
        if m.start() >= limit_byte {
            break;
        }
        // A match that crosses the boundary is not "before" `from_char`.
        if m.end() > limit_byte {
            continue;
        }

        let start = index.byte_to_char(m.start());
        let end = index.byte_to_char(m.end());
        let candidate = SearchMatch { start, end };

        if candidate.is_empty() {
            continue;
        }
        if options.whole_word && !is_whole_word(text, &index, candidate) {
            continue;
        }

        last = Some(candidate);
    }

    Ok(last)
}

/// Find all occurrences of `query` in `text`.
///
/// - Returns an empty list if `query` is empty.
/// - Match ranges are character offsets and are half-open (`[start, end)`).
pub fn find_all(
    text: &str,
    query: &str,
    options: SearchOptions,
) -> Result<Vec<SearchMatch>, SearchError> {
    if query.is_empty() {
        return Ok(Vec::new());
    }

    Ok(CompiledSearch::new(query, options)?.find_all(text))
}

/// Returns `true` if `range` exactly matches an occurrence of `query` in `text`.
///
/// This is useful for checking whether a current selection/caret range corresponds to the
/// "current match" when implementing find/replace flows.
pub fn is_match_exact(
    text: &str,
    query: &str,
    options: SearchOptions,
    range: SearchMatch,
) -> Result<bool, SearchError> {
    if range.is_empty() {
        return Ok(false);
    }

    let Some(next) = find_next(text, query, options, range.start)? else {
        return Ok(false);
    };

    Ok(next.start == range.start && next.end == range.end)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn regex_opts() -> SearchOptions {
        SearchOptions {
            case_sensitive: true,
            whole_word: false,
            regex: true,
        }
    }

    #[test]
    fn find_prev_does_not_fabricate_end_anchor_match_at_slice_boundary() {
        // Regression (P1-4): searching a `&text[..limit_byte]` slice let `$` match at the
        // artificial slice end. In "foo\nbar", `o$` only matches the last `o` of "foo" (offset
        // 2..3, end of line 0). With from_char=2 the slice would be "fo", where the truncated
        // "o" sits at the slice end and `$` would falsely match [1,2). There is no match ending
        // at/before offset 2, so the correct answer is None.
        let m = find_prev("foo\nbar", "o$", regex_opts(), 2).unwrap();
        assert_eq!(m, None);
    }

    #[test]
    fn find_prev_finds_real_end_anchor_match_using_full_context() {
        // The real `o$` match is [2,3) (the "o" at end of line 0). Searching from the end of the
        // document must find it, proving `$` is evaluated against the true line boundary.
        let text = "foo\nbar";
        let m = find_prev(text, "o$", regex_opts(), text.chars().count())
            .unwrap()
            .expect("should find the end-of-line match");
        assert_eq!((m.start, m.end), (2, 3));
    }

    #[test]
    fn find_prev_does_not_truncate_greedy_match_crossing_boundary() {
        // Greedy `a+` over "aaaa": the only match is [0,4). With from_char=2, a slice "aa" would
        // wrongly yield [0,2). Since the real match crosses the boundary, the answer is None.
        assert_eq!(find_prev("aaaa", "a+", regex_opts(), 2).unwrap(), None);
        // From the end, the full greedy match is returned intact.
        let m = find_prev("aaaa", "a+", regex_opts(), 4).unwrap().unwrap();
        assert_eq!((m.start, m.end), (0, 4));
    }

    #[test]
    fn find_prev_returns_last_match_before_boundary() {
        // Plain (non-regex) search: "abcabcabc", query "abc". From offset 9 the last match is
        // [6,9); from offset 6 it is [3,6).
        let opts = SearchOptions::default();
        let m = find_prev("abcabcabc", "abc", opts, 9).unwrap().unwrap();
        assert_eq!((m.start, m.end), (6, 9));
        let m = find_prev("abcabcabc", "abc", opts, 6).unwrap().unwrap();
        assert_eq!((m.start, m.end), (3, 6));
    }
}
