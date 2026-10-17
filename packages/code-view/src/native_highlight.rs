//! Rust syntax queries retain unaffected top-level spans after incremental parsing.
//! Markdown caches narrowly safe prose appends; other grammars use dioxus-code. Span precedence
//! matches dioxus-code 0.1.2 (MIT): tagged captures win, then later patterns.
use arborium_tree_sitter::{InputEdit, Parser, Point, Query, QueryCursor, StreamingIterator, Tree};
use dioxus_code::{
    advanced::{Buffer as SharedBuffer, HighlightSpan, SourceEdit},
    Language,
};
use std::{collections::HashMap, ops::Range};

pub(crate) enum Buffer {
    Shared(SharedBuffer),
    Rust(Box<RustBuffer>),
    Markdown(Box<MarkdownBuffer>),
}
impl Buffer {
    pub fn new(language: Language, source: impl ToString) -> Result<Self, ()> {
        let source = source.to_string();
        if language == Language::Markdown {
            MarkdownBuffer::new(source).map(|buffer| Self::Markdown(Box::new(buffer)))
        } else if language == Language::Rust {
            RustBuffer::new(source).map(|buffer| Self::Rust(Box::new(buffer)))
        } else {
            SharedBuffer::new(language, source)
                .map(Self::Shared)
                .map_err(|_| ())
        }
    }
    pub fn source(&self) -> &str {
        match self {
            Self::Shared(buffer) => buffer.source(),
            Self::Rust(buffer) => &buffer.source,
            Self::Markdown(buffer) => &buffer.source,
        }
    }
    pub fn spans(&self) -> &[HighlightSpan] {
        match self {
            Self::Shared(buffer) => buffer.spans(),
            Self::Rust(buffer) => &buffer.spans,
            Self::Markdown(buffer) => buffer.inner.spans(),
        }
    }
    pub fn replace(&mut self, source: impl ToString) -> Result<(), ()> {
        match self {
            Self::Shared(buffer) => buffer.replace(source).map_err(|_| ()),
            Self::Markdown(buffer) => {
                **buffer = MarkdownBuffer::new(source.to_string())?;
                Ok(())
            }
            Self::Rust(buffer) => {
                **buffer = RustBuffer::new(source.to_string())?;
                Ok(())
            }
        }
    }
    pub fn edit(&mut self, edit: SourceEdit, source: impl ToString) -> Result<(), ()> {
        match self {
            Self::Shared(buffer) => buffer.edit(edit, source).map_err(|_| ()),
            Self::Rust(buffer) => buffer.edit(edit, source.to_string()),
            Self::Markdown(buffer) => buffer.edit(edit, source.to_string()),
        }
    }
}

/// Plain ASCII word appends cannot introduce Markdown syntax. Retain exact
/// existing captures for an unhighlighted prose tail, parsing normally as soon
/// as an edit leaves that narrow case. Structural folds still parse canonical
/// source independently; this cache owns highlighting only.
pub(crate) struct MarkdownBuffer {
    inner: SharedBuffer,
    source: String,
}
impl MarkdownBuffer {
    fn new(source: String) -> Result<Self, ()> {
        let inner = SharedBuffer::new(Language::Markdown, &source).map_err(|_| ())?;
        Ok(Self { inner, source })
    }
    fn edit(&mut self, edit: SourceEdit, source: String) -> Result<(), ()> {
        let append = edit.start_byte == self.source.len()
            && edit.old_end_byte == self.source.len()
            && edit.new_end_byte == source.len()
            && source.starts_with(&self.source)
            && source.len() > self.source.len()
            && source[self.source.len()..]
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric());
        let tail = self.source.trim_end_matches('\n');
        let line_start = tail.rfind('\n').map_or(0, |index| index + 1);
        let line = &tail[line_start..];
        let plain_tail = !line.is_empty()
            && line.len() <= 4096
            && line.bytes().any(|byte| byte.is_ascii_alphanumeric())
            && line
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b" .,!?".contains(&byte))
            && self
                .inner
                .spans()
                .iter()
                .all(|span| (span.end() as usize) < line_start);
        if append && plain_tail {
            self.source = source;
            return Ok(());
        }
        if self.inner.source() == self.source {
            self.inner.edit(edit, &source).map_err(|_| ())?;
        } else {
            // Pending plain appends must be included when syntax changes.
            self.inner.replace(&source).map_err(|_| ())?;
        }
        self.source = source;
        Ok(())
    }
}

pub(crate) struct RustBuffer {
    parser: Parser,
    query: Query,
    cursor: QueryCursor,
    tree: Tree,
    source: String,
    spans: Vec<HighlightSpan>,
}
impl RustBuffer {
    fn new(source: String) -> Result<Self, ()> {
        let language = arborium::lang_rust::language().into();
        let mut parser = Parser::new();
        parser.set_language(&language).map_err(|_| ())?;
        let query = Query::new(&language, arborium::lang_rust::HIGHLIGHTS_QUERY).map_err(|_| ())?;
        let tree = parser.parse(&source, None).ok_or(())?;
        let mut buffer = Self {
            parser,
            query,
            cursor: QueryCursor::new(),
            tree,
            source,
            spans: Vec::new(),
        };
        buffer.spans = buffer.query_spans(0..buffer.source.len());
        Ok(buffer)
    }
    fn edit(&mut self, edit: SourceEdit, source: String) -> Result<(), ()> {
        let SourceEdit {
            start_byte: start,
            old_end_byte: end,
            new_end_byte: new_end,
        } = edit;
        if start > end
            || end > self.source.len()
            || new_end < start
            || new_end > source.len()
            || !self.source.is_char_boundary(start)
            || !self.source.is_char_boundary(end)
            || !source.is_char_boundary(start)
            || !source.is_char_boundary(new_end)
        {
            return Err(());
        }
        let position = |text: &str, byte: usize| {
            let prefix = &text[..byte];
            Point {
                row: prefix.bytes().filter(|byte| *byte == b'\n').count(),
                column: prefix
                    .rfind('\n')
                    .map_or(byte, |newline| byte - newline - 1),
            }
        };
        let mut edited = self.tree.clone();
        edited.edit(&InputEdit {
            start_byte: start,
            old_end_byte: end,
            new_end_byte: new_end,
            start_position: position(&self.source, start),
            old_end_position: position(&self.source, end),
            new_end_position: position(&source, new_end),
        });
        let tree = self.parser.parse(&source, Some(&edited)).ok_or(())?;
        // Include text-only changes too: tree-sitter changed_ranges can be empty
        // when identifier spelling changes a highlight query predicate.
        let mut window = start..new_end;
        for changed in edited.changed_ranges(&tree) {
            window.start = window.start.min(changed.start_byte);
            window.end = window.end.max(changed.end_byte);
        }
        // Query with the full tree as context, bounded to affected root children.
        // Adjacent children cover joins/splits and captures on syntax boundaries.
        let root = tree.root_node();
        let mut walker = root.walk();
        let children: Vec<_> = root.children(&mut walker).collect();
        if !children.is_empty() {
            let first = children
                .partition_point(|node| node.end_byte() < window.start)
                .saturating_sub(1);
            let after = children
                .partition_point(|node| node.start_byte() <= window.end)
                .saturating_add(1)
                .min(children.len());
            window.start = window
                .start
                .min(children[first.min(children.len() - 1)].start_byte());
            window.end = window.end.max(children[after.saturating_sub(1)].end_byte());
        }
        // Move retained spans into the new coordinate space. Captures touching
        // the edit are re-queried, including captures containing the insertion.
        let shift = |byte: u32| (byte as usize - end + new_end) as u32;
        let mut retained = Vec::with_capacity(self.spans.len());
        for span in &self.spans {
            if span.end() as usize <= start {
                retained.push(*span);
            } else if span.start() as usize >= end {
                retained.push(HighlightSpan::new(
                    shift(span.start())..shift(span.end()),
                    span.tag(),
                ));
            } else {
                window.start = window.start.min(span.start() as usize);
                window.end = window.end.max(if span.end() as usize > end {
                    shift(span.end()) as usize
                } else {
                    new_end
                });
            }
        }
        // A previously coalesced capture can cross a query boundary. Expand to
        // retain all of it, rather than dropping its unchanged outside portion.
        loop {
            let before = window.clone();
            for span in &retained {
                if (span.start() as usize) < window.end && span.end() as usize > window.start {
                    window.start = window.start.min(span.start() as usize);
                    window.end = window.end.max(span.end() as usize);
                }
            }
            if before == window {
                break;
            }
        }
        drop(children);
        drop(walker);
        self.tree = tree;
        self.source = source;
        retained.retain(|span| {
            if span.start() == span.end() {
                (span.start() as usize) < window.start || span.start() as usize > window.end
            } else {
                span.end() as usize <= window.start || span.start() as usize >= window.end
            }
        });
        retained.extend(self.query_spans(window));
        self.spans = coalesce(retained);
        Ok(())
    }
    fn query_spans(&mut self, window: Range<usize>) -> Vec<HighlightSpan> {
        self.cursor
            .set_byte_range(window.start..window.end.saturating_add(1));
        let names = self.query.capture_names();
        let mut raw = HashMap::new();
        let mut matches =
            self.cursor
                .matches(&self.query, self.tree.root_node(), self.source.as_bytes());
        while let Some(value) = matches.next() {
            for capture in value.captures {
                let name = names[capture.index as usize];
                let range = capture.node.byte_range();
                if name.starts_with('_')
                    || name.starts_with("injection.")
                    || (if range.is_empty() {
                        range.start < window.start || range.start > window.end
                    } else {
                        range.end <= window.start || range.start >= window.end
                    })
                {
                    continue;
                }
                let tag = arborium_theme::tag_for_capture(name);
                let current = raw
                    .entry((range.start as u32, range.end as u32))
                    .or_insert((tag, value.pattern_index));
                if (tag.is_some() && current.0.is_none())
                    || (tag.is_some() == current.0.is_some() && value.pattern_index >= current.1)
                {
                    *current = (tag, value.pattern_index);
                }
            }
        }
        coalesce(
            raw.into_iter()
                .filter_map(|((start, end), (tag, _))| Some(HighlightSpan::new(start..end, tag?)))
                .collect(),
        )
    }
}
fn coalesce(mut spans: Vec<HighlightSpan>) -> Vec<HighlightSpan> {
    spans.sort_by_key(|span| (span.start(), span.end()));
    let mut values: Vec<HighlightSpan> = Vec::with_capacity(spans.len());
    for span in spans {
        if let Some(last) = values.last_mut() {
            if last.tag() == span.tag() && span.start() <= last.end() {
                *last = HighlightSpan::new(last.start()..last.end().max(span.end()), last.tag());
                continue;
            }
        }
        values.push(span);
    }
    values
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn markdown_prose_appends_and_syntax_changes_match_fresh_captures() {
        for prefix in [
            "# Title\n\nEnd.\n",
            "before\n\nword",
            "```\nplain\n",
            "<div>\nplain\n",
            "[ref]:\n",
            "[ref]:\nurl\n",
            "> quote\nplain\n",
            "# Heading\n",
            "    code\n",
            "---\nplain\n",
        ] {
            let mut text = prefix.to_owned();
            let mut buffer = Buffer::new(Language::Markdown, &text).unwrap();
            for append in [
                "x",
                "word",
                "2",
                "\n# Heading\n",
                "[link](https://example.org)",
                "\n```\ncode\n```\n",
                "猫",
            ] {
                let start = text.len();
                text.push_str(append);
                buffer
                    .edit(
                        SourceEdit {
                            start_byte: start,
                            old_end_byte: start,
                            new_end_byte: text.len(),
                        },
                        &text,
                    )
                    .unwrap();
                let fresh = SharedBuffer::new(Language::Markdown, &text).unwrap();
                assert_eq!(buffer.source(), text);
                assert_eq!(buffer.spans(), fresh.spans(), "{text}");
            }
            // Deleting an earlier prefix must also flush any retained tail.
            let end = text.find('\n').map_or(1, |index| index + 1);
            text.replace_range(..end, "");
            buffer
                .edit(
                    SourceEdit {
                        start_byte: 0,
                        old_end_byte: end,
                        new_end_byte: 0,
                    },
                    &text,
                )
                .unwrap();
            let fresh = SharedBuffer::new(Language::Markdown, &text).unwrap();
            assert_eq!(buffer.spans(), fresh.spans());
        }
    }
    #[test]
    fn markdown_pending_prose_flushes_when_an_earlier_edit_changes_context() {
        let mut text = "before\n\nEnd.\n".to_owned();
        let mut buffer = Buffer::new(Language::Markdown, &text).unwrap();
        let start = text.len();
        text.push_str("word");
        buffer
            .edit(
                SourceEdit {
                    start_byte: start,
                    old_end_byte: start,
                    new_end_byte: text.len(),
                },
                &text,
            )
            .unwrap();
        text.insert_str(0, "```\n");
        buffer
            .edit(
                SourceEdit {
                    start_byte: 0,
                    old_end_byte: 0,
                    new_end_byte: 4,
                },
                &text,
            )
            .unwrap();
        let fresh = SharedBuffer::new(Language::Markdown, &text).unwrap();
        assert_eq!(buffer.source(), text);
        assert_eq!(buffer.spans(), fresh.spans());
    }
    fn check(buffer: &Buffer, source: &str) {
        thread_local! {
            static FRESH: std::cell::RefCell<SharedBuffer> = std::cell::RefCell::new(SharedBuffer::new(Language::Rust, "").unwrap());
        }
        FRESH.with_borrow_mut(|fresh| {
            fresh.replace(source).unwrap(); // Full parse and query; never incremental.
            assert_eq!(buffer.source(), source);
            assert_eq!(buffer.spans(), fresh.spans(), "source: {source}");
        });
    }
    #[test]
    fn incremental_rust_matches_shared_queries_through_lexical_and_root_changes() {
        let mut source = "/// docs\nfn one() { let name = \"中\"; }\n// comment\nfn two() { println!(\"hello\"); }\n".to_owned();
        let mut buffer = Buffer::new(Language::Rust, &source).unwrap();
        check(&buffer, &source);
        // Repeated text-only edits, docs ownership, raw strings, block comments,
        // multiline deletions, Unicode, joins and splits of top-level items.
        for (start, end, inserted) in [
            (0, 0, "x"),
            (0, 0, "x"),
            (0, 2, ""),
            (0, 0, "/*"),
            (0, 2, ""),
            (0, 0, "r#\""),
            (0, 3, ""),
            (5, 5, "é"),
            (5, 8, ""),
            (0, 12, ""),
            (0, 0, "/// restored\n"),
            (0, 0, "pub "),
            (0, 4, ""),
            (0, 0, "\n"),
            (0, 1, ""),
        ] {
            source.replace_range(start..end, inserted);
            buffer
                .edit(
                    SourceEdit {
                        start_byte: start,
                        old_end_byte: end,
                        new_end_byte: start + inserted.len(),
                    },
                    &source,
                )
                .unwrap();
            check(&buffer, &source);
        }
    }
    #[test]
    fn edits_at_every_scalar_boundary_match_fresh_highlighting() {
        let original = "/// A\nfn first() { let 中 = r#\"raw\"#; }\n/* block */\n#[test]\nfn second() { let x = Some(3); }\n";
        let mut buffer = Buffer::new(Language::Rust, original).unwrap();
        for (byte, _) in original
            .char_indices()
            .chain(std::iter::once((original.len(), ' ')))
        {
            for inserted in ["x", "\n", "/*", "\"", "///", "😀"] {
                let mut source = original.to_owned();
                source.insert_str(byte, inserted);
                buffer
                    .edit(
                        SourceEdit {
                            start_byte: byte,
                            old_end_byte: byte,
                            new_end_byte: byte + inserted.len(),
                        },
                        &source,
                    )
                    .unwrap();
                check(&buffer, &source);
                source.replace_range(byte..byte + inserted.len(), "");
                buffer
                    .edit(
                        SourceEdit {
                            start_byte: byte,
                            old_end_byte: byte + inserted.len(),
                            new_end_byte: byte,
                        },
                        &source,
                    )
                    .unwrap();
                check(&buffer, &source);
            }
        }
    }
    #[test]
    fn invalid_edit_leaves_source_and_spans_unchanged() {
        let mut buffer = Buffer::new(Language::Rust, "// 中\n").unwrap();
        assert!(buffer
            .edit(
                SourceEdit {
                    start_byte: 4,
                    old_end_byte: 4,
                    new_end_byte: 5
                },
                "// x中\n"
            )
            .is_err());
        check(&buffer, "// 中\n");
    }
    #[test]
    fn edits_in_separated_items_retain_the_same_global_spans() {
        let mut source = (0..40)
            .map(|n| format!("/// docs {n}\nfn sample_{n}() {{ let value = \"string\"; }}\n"))
            .collect::<String>();
        let mut buffer = Buffer::new(Language::Rust, &source).unwrap();
        for item in [0, 19, 39, 1, 38] {
            let at = source.find(&format!("sample_{item}()")).unwrap();
            for inserted in ["long_", "中", "\n/*", "\""] {
                source.insert_str(at, inserted);
                buffer
                    .edit(
                        SourceEdit {
                            start_byte: at,
                            old_end_byte: at,
                            new_end_byte: at + inserted.len(),
                        },
                        &source,
                    )
                    .unwrap();
                check(&buffer, &source);
                source.replace_range(at..at + inserted.len(), "");
                buffer
                    .edit(
                        SourceEdit {
                            start_byte: at,
                            old_end_byte: at + inserted.len(),
                            new_end_byte: at,
                        },
                        &source,
                    )
                    .unwrap();
                check(&buffer, &source);
            }
        }
    }
    #[test]
    fn bounded_edit_sequence_matches_fresh_highlighting_including_empty_source() {
        let mut source =
            "/// docs\nmod nested { fn one() { let 中 = r#\"value\"#; } }\nfn two() {}\n"
                .to_owned();
        let mut buffer = Buffer::new(Language::Rust, &source).unwrap();
        let inserts = [
            "x",
            "中",
            "//",
            "/*",
            "*/",
            "\n",
            "fn f() {}",
            "r#\"",
            "\"#",
            "/// docs\n",
            "",
            "\r\n",
            "'a'",
        ];
        let mut seed = 198_u64;
        for _ in 0..256 {
            let mut next = || {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                (seed >> 32) as usize
            };
            let points: Vec<_> = source
                .char_indices()
                .map(|(byte, _)| byte)
                .chain(std::iter::once(source.len()))
                .collect();
            let first = next() % points.len();
            let last = (first + next() % 17).min(points.len() - 1);
            let (start, end) = (points[first], points[last]);
            let inserted = inserts[next() % inserts.len()];
            source.replace_range(start..end, inserted);
            buffer
                .edit(
                    SourceEdit {
                        start_byte: start,
                        old_end_byte: end,
                        new_end_byte: start + inserted.len(),
                    },
                    &source,
                )
                .unwrap();
            check(&buffer, &source);
        }
        let end = source.len();
        buffer
            .edit(
                SourceEdit {
                    start_byte: 0,
                    old_end_byte: end,
                    new_end_byte: 0,
                },
                "",
            )
            .unwrap();
        check(&buffer, "");
    }
}
