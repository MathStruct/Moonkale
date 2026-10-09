//! Snippet parsing + placeholder navigation primitives.
//!
//! This module implements a **pragmatic subset** of TextMate / VS Code snippet syntax that is
//! sufficient for kernel features like:
//!
//! - LSP completion items with `insertTextFormat == 2` (snippets)
//! - placeholder (`$1`, `${1:default}`) navigation (tab / shift-tab)
//! - best-effort handling of choices (`${1|a,b,c|}`) and variables (`${TM_FILENAME:foo.rs}`)
//!
//! Goals:
//! - Keep the kernel UI-agnostic: no rendering, no popup UI for choices.
//! - Prefer "do something reasonable" over rejecting malformed snippet text.
//! - Express all ranges in **character offsets** (Rust `char` indices), consistent with the rest
//!   of `editor-core`.

use std::collections::{BTreeMap, HashMap};

use crate::{AnchorBias, TextAnchor, TextDelta};

/// A half-open character range (start..end) within a snippet's expanded text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SnippetRange {
    /// Inclusive start character offset.
    pub start: usize,
    /// Exclusive end character offset.
    pub end: usize,
}

impl SnippetRange {
    fn offset_by(&self, base: usize) -> Self {
        Self {
            start: self.start.saturating_add(base),
            end: self.end.saturating_add(base),
        }
    }
}

/// A numbered tabstop within a snippet.
///
/// Notes:
/// - Tabstop indices start at `1`. `$0` is the "final caret position" and is represented as
///   [`SnippetTemplate::final_offset`], not as a tabstop.
/// - A tabstop may have multiple ranges (mirrored placeholders).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnippetTabstop {
    /// Tabstop index (1..).
    pub index: u32,
    /// All placeholder ranges for this tabstop, in character offsets within the expanded text.
    pub ranges: Vec<SnippetRange>,
}

/// An expanded snippet template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnippetTemplate {
    /// Expanded text to insert into the document.
    pub text: String,
    /// Ordered tabstops (ascending index, excluding `$0`).
    pub tabstops: Vec<SnippetTabstop>,
    /// Final caret position (character offset) within `text`.
    pub final_offset: usize,
}

#[derive(Debug, Default, Clone)]
struct SnippetParts {
    text: String,
    text_char_len: usize,
    tabstops: BTreeMap<u32, Vec<SnippetRange>>,
    /// `Some(offset)` only when `$0` / `${0}` appears. We intentionally keep this optional so we
    /// don't accidentally treat "default to end" as an explicit `$0` when nesting.
    final_offset: Option<usize>,
}

impl SnippetParts {
    fn push_char(&mut self, ch: char) {
        self.text.push(ch);
        self.text_char_len = self.text_char_len.saturating_add(1);
    }

    fn push_str(&mut self, s: &str) {
        self.text.push_str(s);
        self.text_char_len = self.text_char_len.saturating_add(s.chars().count());
    }

    fn record_tabstop_range(&mut self, idx: u32, range: SnippetRange) {
        if idx == 0 {
            return;
        }
        self.tabstops.entry(idx).or_default().push(range);
    }

    fn merge_with_offset(&mut self, other: SnippetParts, base: usize) {
        self.push_str(&other.text);

        for (idx, mut ranges) in other.tabstops {
            for r in &mut ranges {
                *r = r.offset_by(base);
            }
            self.tabstops.entry(idx).or_default().extend(ranges);
        }

        if self.final_offset.is_none()
            && let Some(off) = other.final_offset
        {
            self.final_offset = Some(base.saturating_add(off));
        }
    }

    fn finish(mut self) -> SnippetTemplate {
        let mut tabstops: Vec<SnippetTabstop> = self
            .tabstops
            .into_iter()
            .filter(|(idx, _)| *idx != 0)
            .map(|(index, mut ranges)| {
                ranges.sort_by_key(|r| (r.start, r.end));
                SnippetTabstop { index, ranges }
            })
            .collect();
        tabstops.sort_by_key(|t| t.index);

        let final_offset = self.final_offset.unwrap_or(self.text_char_len);
        if final_offset > self.text_char_len {
            self.final_offset = Some(self.text_char_len);
        }

        SnippetTemplate {
            text: self.text,
            tabstops,
            final_offset: final_offset.min(self.text_char_len),
        }
    }
}

const MAX_SNIPPET_PARSE_DEPTH: usize = 32;

/// Parse a TextMate/VS Code-style snippet string into an expanded template.
///
/// Supported constructs (best-effort):
/// - `$1`, `$2`, ... tabstops
/// - `${1}`, `${1:default}`
/// - `${1|a,b,c|}` (inserts the first option; selects as placeholder)
/// - `$0` / `${0}` final caret position
/// - `${VAR}` / `${VAR:default}` variables (unresolved; default is used when provided)
///
/// Unsupported constructs are ignored or treated as literal text where reasonable.
pub fn parse_snippet(snippet: &str) -> SnippetTemplate {
    let mut chars = snippet.chars().peekable();
    let mut ctx = ParseCtx::default();
    parse_until(&mut chars, None, 0, &mut ctx).finish()
}

fn parse_until<I>(
    chars: &mut std::iter::Peekable<I>,
    terminator: Option<char>,
    depth: usize,
    ctx: &mut ParseCtx,
) -> SnippetParts
where
    I: Iterator<Item = char>,
{
    if depth > MAX_SNIPPET_PARSE_DEPTH {
        // Best-effort: treat remaining content as literal text.
        let mut parts = SnippetParts::default();
        for ch in chars.by_ref() {
            if terminator == Some(ch) {
                break;
            }
            parts.push_char(ch);
        }
        return parts;
    }

    let mut parts = SnippetParts::default();

    while let Some(ch) = chars.next() {
        if terminator == Some(ch) {
            break;
        }

        match ch {
            '\\' => {
                // Escape next character (if any).
                if let Some(next) = chars.next() {
                    parts.push_char(next);
                }
            }
            '$' => match chars.peek().copied() {
                Some('{') => {
                    chars.next(); // consume '{'
                    parse_braced_expression(&mut parts, chars, depth + 1, ctx);
                }
                Some(d) if d.is_ascii_digit() => {
                    let idx = parse_number(chars);
                    insert_tabstop_reference(&mut parts, ctx, idx);
                }
                Some(c) if c == '_' || c.is_ascii_alphabetic() => {
                    // `$VAR` - unresolved variable; consume its name and insert nothing.
                    parse_identifier(chars);
                }
                _ => parts.push_char('$'),
            },
            other => parts.push_char(other),
        }
    }

    parts
}

#[derive(Debug, Default)]
struct ParseCtx {
    tabstop_defaults: HashMap<u32, String>,
}

fn parse_number<I>(chars: &mut std::iter::Peekable<I>) -> u32
where
    I: Iterator<Item = char>,
{
    let mut num: u32 = 0;
    let mut saw_any = false;
    while let Some(ch) = chars.peek().copied() {
        if !ch.is_ascii_digit() {
            break;
        }
        saw_any = true;
        chars.next();
        num = num
            .saturating_mul(10)
            .saturating_add((ch as u8 - b'0') as u32);
    }
    if saw_any { num } else { 0 }
}

fn parse_identifier<I>(chars: &mut std::iter::Peekable<I>) -> String
where
    I: Iterator<Item = char>,
{
    let mut out = String::new();
    while let Some(ch) = chars.peek().copied() {
        if ch == '_' || ch.is_ascii_alphanumeric() {
            chars.next();
            out.push(ch);
        } else {
            break;
        }
    }
    out
}

fn parse_braced_expression<I>(
    parts: &mut SnippetParts,
    chars: &mut std::iter::Peekable<I>,
    depth: usize,
    ctx: &mut ParseCtx,
) where
    I: Iterator<Item = char>,
{
    // `${...}` form: either a numeric tabstop (`${1:foo}`) or a variable (`${TM_FILENAME:foo.rs}`).
    let ident_first = chars.peek().copied();
    let is_number = matches!(ident_first, Some(c) if c.is_ascii_digit());

    if is_number {
        let idx = parse_number(chars);
        match chars.peek().copied() {
            Some('}') => {
                chars.next();
                insert_tabstop_reference(parts, ctx, idx);
            }
            Some(':') => {
                chars.next();
                parse_tabstop_default(parts, chars, depth, idx, ctx);
            }
            Some('|') => {
                chars.next();
                parse_tabstop_choice(parts, chars, idx, ctx);
            }
            _ => {
                // Unknown / malformed; consume until matching `}`.
                consume_until_closing_brace(chars);
            }
        }
        return;
    }

    let name = parse_identifier(chars);
    match chars.peek().copied() {
        Some('}') => {
            chars.next();
            // Unresolved variable: insert nothing.
        }
        Some(':') => {
            chars.next();
            let start = parts.text_char_len;
            let inner = parse_until(chars, Some('}'), depth, ctx);
            parts.merge_with_offset(inner, start);
        }
        _ => {
            // Unhandled variable transform `${VAR/...}` or malformed: consume.
            consume_until_closing_brace(chars);
        }
    }

    let _ = name; // keep `name` for potential future variable resolution hooks.
}

fn parse_tabstop_default<I>(
    parts: &mut SnippetParts,
    chars: &mut std::iter::Peekable<I>,
    depth: usize,
    idx: u32,
    ctx: &mut ParseCtx,
) where
    I: Iterator<Item = char>,
{
    // `${1:default}`: insert default (snippet-expanded) and mark it as a placeholder range.
    // `$0` / `${0:...}`: we treat as final caret only (no textual output), consistent with the
    // previous downgrade behavior.
    let start = parts.text_char_len;
    let inner = parse_until(chars, Some('}'), depth, ctx);

    if idx == 0 {
        // Final cursor position, no output.
        if parts.final_offset.is_none() {
            parts.final_offset = Some(start);
        }
        return;
    }

    let placeholder_start = parts.text_char_len;
    if let std::collections::hash_map::Entry::Vacant(e) = ctx.tabstop_defaults.entry(idx) {
        e.insert(inner.text.clone());
    }
    parts.merge_with_offset(inner, placeholder_start);
    let placeholder_end = parts.text_char_len;
    parts.record_tabstop_range(
        idx,
        SnippetRange {
            start: placeholder_start,
            end: placeholder_end,
        },
    );
}

fn parse_tabstop_choice<I>(
    parts: &mut SnippetParts,
    chars: &mut std::iter::Peekable<I>,
    idx: u32,
    ctx: &mut ParseCtx,
) where
    I: Iterator<Item = char>,
{
    // `${1|a,b,c|}`: pick the first option as inserted text.
    let mut options: Vec<String> = Vec::new();
    let mut current = String::new();

    while let Some(ch) = chars.next() {
        match ch {
            '\\' => {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            ',' => {
                options.push(current);
                current = String::new();
            }
            '|' => {
                if chars.peek().copied() == Some('}') {
                    chars.next(); // consume '}'
                    options.push(current);
                    break;
                }
                current.push('|');
            }
            other => current.push(other),
        }
    }

    if idx == 0 {
        // `$0` is not a placeholder; don't insert choice text.
        if parts.final_offset.is_none() {
            parts.final_offset = Some(parts.text_char_len);
        }
        return;
    }

    let insert_text = options.first().map(String::as_str).unwrap_or("");
    ctx.tabstop_defaults
        .entry(idx)
        .or_insert_with(|| insert_text.to_string());
    let start = parts.text_char_len;
    parts.push_str(insert_text);
    let end = parts.text_char_len;
    parts.record_tabstop_range(idx, SnippetRange { start, end });
}

fn insert_tabstop_reference(parts: &mut SnippetParts, ctx: &mut ParseCtx, idx: u32) {
    if idx == 0 {
        if parts.final_offset.is_none() {
            parts.final_offset = Some(parts.text_char_len);
        }
        return;
    }

    if let Some(default) = ctx.tabstop_defaults.get(&idx) {
        let start = parts.text_char_len;
        parts.push_str(default);
        let end = parts.text_char_len;
        parts.record_tabstop_range(idx, SnippetRange { start, end });
    } else {
        let at = parts.text_char_len;
        parts.record_tabstop_range(idx, SnippetRange { start: at, end: at });
    }
}

fn consume_until_closing_brace<I>(chars: &mut std::iter::Peekable<I>)
where
    I: Iterator<Item = char>,
{
    while let Some(ch) = chars.next() {
        match ch {
            '\\' => {
                let _ = chars.next();
            }
            '}' => break,
            _ => {}
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct AnchoredRange {
    start: TextAnchor,
    end: TextAnchor,
}

impl AnchoredRange {
    fn from_offsets(start: usize, end: usize) -> Self {
        Self {
            start: TextAnchor::new(start, AnchorBias::Left),
            end: TextAnchor::new(end, AnchorBias::Right),
        }
    }

    fn offsets(&self) -> (usize, usize) {
        let a = self.start.offset;
        let b = self.end.offset;
        (a.min(b), a.max(b))
    }

    fn apply_delta(&mut self, delta: &TextDelta) {
        self.start.apply_delta(delta);
        self.end.apply_delta(delta);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AnchoredTabstop {
    index: u32,
    ranges: Vec<AnchoredRange>,
}

/// An active snippet session tracked in document coordinates.
///
/// This is a **view-local** editing state: it is expected to be stored alongside cursor/selection
/// state (e.g. per `ViewId` in a `Workspace`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnippetSession {
    tabstops: Vec<AnchoredTabstop>,
    final_anchor: TextAnchor,
    active_tabstop_index: usize,
}

/// Result of a snippet navigation operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SnippetNavigation {
    /// Select the given ranges (in document char offsets) for the next/prev tabstop.
    SelectRanges(Vec<(usize, usize)>),
    /// Finish the snippet session by moving the caret to the final offset.
    Finish(usize),
    /// No active snippet / no movement possible.
    Noop,
}

impl SnippetSession {
    /// Create a snippet session for an inserted template.
    ///
    /// - `insert_start` is the character offset where `template.text` begins in the document.
    /// - Returns `None` when `template` contains no tabstops (nothing to navigate).
    pub fn new(insert_start: usize, template: &SnippetTemplate) -> Option<Self> {
        if template.tabstops.is_empty() {
            return None;
        }

        let mut tabstops: Vec<AnchoredTabstop> = Vec::with_capacity(template.tabstops.len());
        for t in &template.tabstops {
            let mut ranges: Vec<AnchoredRange> = Vec::with_capacity(t.ranges.len());
            for r in &t.ranges {
                let start = insert_start.saturating_add(r.start);
                let end = insert_start.saturating_add(r.end);
                ranges.push(AnchoredRange::from_offsets(start, end));
            }

            tabstops.push(AnchoredTabstop {
                index: t.index,
                ranges,
            });
        }

        Some(Self {
            tabstops,
            final_anchor: TextAnchor::new(
                insert_start.saturating_add(template.final_offset),
                AnchorBias::Right,
            ),
            active_tabstop_index: 0,
        })
    }

    /// Return `true` if this session has at least one tabstop.
    pub fn is_active(&self) -> bool {
        !self.tabstops.is_empty()
    }

    /// Shift all tracked placeholder/final offsets through a document text delta.
    pub fn apply_delta(&mut self, delta: &TextDelta) {
        for t in &mut self.tabstops {
            for r in &mut t.ranges {
                r.apply_delta(delta);
            }
        }
        self.final_anchor.apply_delta(delta);
    }

    pub(crate) fn current_ranges(&self) -> Vec<(usize, usize)> {
        self.tabstops
            .get(self.active_tabstop_index)
            .map(|t| {
                let mut ranges: Vec<(usize, usize)> =
                    t.ranges.iter().map(|r| r.offsets()).collect();
                ranges.sort_by_key(|r| (r.0, r.1));
                ranges
            })
            .unwrap_or_default()
    }

    pub(crate) fn next(&mut self) -> SnippetNavigation {
        if self.tabstops.is_empty() {
            return SnippetNavigation::Noop;
        }

        let next_index = self.active_tabstop_index.saturating_add(1);
        if next_index >= self.tabstops.len() {
            return SnippetNavigation::Finish(self.final_anchor.offset);
        }

        self.active_tabstop_index = next_index;
        SnippetNavigation::SelectRanges(self.current_ranges())
    }

    pub(crate) fn prev(&mut self) -> SnippetNavigation {
        if self.tabstops.is_empty() {
            return SnippetNavigation::Noop;
        }

        if self.active_tabstop_index == 0 {
            return SnippetNavigation::SelectRanges(self.current_ranges());
        }

        self.active_tabstop_index = self.active_tabstop_index.saturating_sub(1);
        SnippetNavigation::SelectRanges(self.current_ranges())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_placeholders_and_final_offset() {
        let t = parse_snippet("println!(${1:msg})$0");
        assert_eq!(t.text, "println!(msg)");
        assert_eq!(t.final_offset, "println!(msg)".chars().count());
        assert_eq!(t.tabstops.len(), 1);
        assert_eq!(t.tabstops[0].index, 1);
        assert_eq!(
            t.tabstops[0].ranges,
            vec![SnippetRange {
                start: "println!(".chars().count(),
                end: "println!(msg".chars().count()
            }]
        );
    }

    #[test]
    fn parses_choice_placeholder_picks_first() {
        let t = parse_snippet("${1|a,b,c|} $0");
        assert_eq!(t.text, "a ");
        assert_eq!(t.tabstops.len(), 1);
        assert_eq!(t.tabstops[0].ranges[0], SnippetRange { start: 0, end: 1 });
        assert_eq!(t.final_offset, 2);
    }

    #[test]
    fn parses_variable_with_default_inserts_default() {
        let t = parse_snippet("${TM_FILENAME:main.rs} $0");
        assert_eq!(t.text, "main.rs ");
        assert!(t.tabstops.is_empty());
        assert_eq!(t.final_offset, "main.rs ".chars().count());
    }

    #[test]
    fn mirrored_tabstops_insert_default_text() {
        let t = parse_snippet("${1:foo} = $1; $0");
        assert_eq!(t.text, "foo = foo; ");
        assert_eq!(t.tabstops.len(), 1);
        assert_eq!(t.tabstops[0].index, 1);
        assert_eq!(t.tabstops[0].ranges.len(), 2);
    }
}
