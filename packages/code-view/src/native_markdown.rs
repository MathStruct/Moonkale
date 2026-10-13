//! Conservative CommonMark presentation with unchanged block reuse.
use crate::native_decorations::{
    Batch, CodeFragment, Decoration, InlineFragment, InlineStyle, Kind, Provider,
};
use moonkale_ext_api::editor::DocumentRevision;
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use std::ops::Range;

struct Frame {
    range: Range<usize>,
    style: InlineStyle,
    fragments: Vec<InlineFragment>,
    nested: bool,
    unsupported: bool,
}

fn fragment(frames: &[Frame], text: String, code: bool) -> InlineFragment {
    InlineFragment {
        text,
        emphasis: frames
            .iter()
            .any(|frame| frame.style == InlineStyle::Emphasis),
        strong: frames
            .iter()
            .any(|frame| frame.style == InlineStyle::Strong),
        code,
    }
}

fn append(fragments: &mut Vec<InlineFragment>, fragment: InlineFragment) {
    if let Some(previous) = fragments.last_mut().filter(|previous| {
        previous.emphasis == fragment.emphasis
            && previous.strong == fragment.strong
            && previous.code == fragment.code
    }) {
        previous.text.push_str(&fragment.text);
    } else if !fragment.text.is_empty() {
        fragments.push(fragment);
    }
}

struct FencedFrame {
    range: Range<usize>,
    info: String,
    text: String,
}

fn closed_fence(source: &str, range: &Range<usize>) -> bool {
    // The first slice supports root-level closed fences, without container prefixes.
    if range.start > 0 && source.as_bytes()[range.start - 1] != b'\n' {
        return false;
    }
    let block = source[range.clone()].trim_end_matches(['\r', '\n']);
    let Some((opening, _)) = block.split_once('\n') else {
        return false;
    };
    let Some(marker) = opening.as_bytes().first().copied() else {
        return false;
    };
    if !matches!(marker, b'`' | b'~') {
        return false;
    }
    let count = opening.bytes().take_while(|byte| *byte == marker).count();
    let closing_line = block.rsplit_once('\n').unwrap().1;
    let indent = closing_line
        .bytes()
        .take_while(|byte| *byte == b' ')
        .count();
    if indent > 3 {
        return false;
    }
    let closing = &closing_line[indent..];
    let close_count = closing.bytes().take_while(|byte| *byte == marker).count();
    count >= 3
        && close_count >= count
        && closing[close_count..]
            .bytes()
            .all(|byte| matches!(byte, b' ' | b'\t'))
}

fn highlighted_code(text: &str, info: &str) -> InlineStyle {
    let hint = info
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    let slug = match hint.as_str() {
        "rs" => "rust",
        "py" => "python",
        "js" => "javascript",
        "ts" => "typescript",
        "sh" => "bash",
        "c++" => "cpp",
        other => other,
    };
    let buffer = crate::native_language::language_for_hint(Some(slug))
        .and_then(|language| dioxus_code::advanced::Buffer::new(language, text).ok());
    let mut fragments: Vec<CodeFragment> = Vec::new();
    for (byte, ch) in text.char_indices() {
        let class = buffer
            .as_ref()
            .and_then(|buffer| {
                let spans = buffer.spans();
                let index = spans.partition_point(|span| span.end() as usize <= byte);
                spans
                    .get(index)
                    .filter(|span| span.start() as usize <= byte)
                    .map(|span| format!("a-{}", span.tag()))
            })
            .unwrap_or_default();
        if let Some(previous) = fragments
            .last_mut()
            .filter(|fragment| fragment.class == class)
        {
            previous.text.push(ch);
        } else {
            fragments.push(CodeFragment {
                text: ch.to_string(),
                class,
            });
        }
    }
    InlineStyle::Fenced {
        language: slug.to_string(),
        fragments,
    }
}

#[derive(Default)]
struct Parsed {
    source: String,
    values: Vec<Decoration>,
    blocks: std::collections::HashMap<String, Vec<Decoration>>,
    #[cfg_attr(not(test), allow(dead_code))]
    reparsed: usize,
}
thread_local! {
    static PARSED: std::cell::RefCell<Parsed> = std::cell::RefCell::new(Parsed::default());
}

pub(crate) fn batch(revision: DocumentRevision, source: &str) -> Batch {
    PARSED.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.source == source {
            return Batch {
                revision,
                values: cache.values.clone(),
            };
        }
        // CommonMark's block pass keeps container/fence/setext context correct.
        // Unchanged top-level blocks reuse their semantic decorations and code
        // highlighting; only changed blocks enter the presentation parser.
        // References have document-wide semantics, so conservatively parse them
        // together until the link provider owns reference-definition invalidation.
        let mut ranges = Vec::new();
        let (mut depth, mut start) = (0usize, 0usize);
        let body_start = Parser::new_ext(source, Options::ENABLE_YAML_STYLE_METADATA_BLOCKS)
            .into_offset_iter()
            .next()
            .filter(|(event, range)| {
                range.start == 0 && matches!(event, Event::Start(Tag::MetadataBlock(_)))
            })
            .map_or(0, |(_, range)| range.end);
        for (event, range) in Parser::new(&source[body_start..]).into_offset_iter() {
            let range = range.start + body_start..range.end + body_start;
            match event {
                Event::Start(_) => {
                    if depth == 0 {
                        start = source[..range.start].rfind('\n').map_or(0, |byte| byte + 1);
                    }
                    depth += 1;
                }
                Event::End(_) => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        ranges.push(start..range.end);
                    }
                }
                _ => {}
            }
        }
        if source.contains("]:") {
            ranges.clear();
            ranges.push(0..source.len());
        }
        let mut blocks = std::collections::HashMap::new();
        let mut values = Vec::new();
        let (mut previous, mut scalar, mut reparsed) = (0, 0, 0);
        for range in ranges {
            scalar += source[previous..range.start].chars().count();
            let block = &source[range.clone()];
            let local = cache
                .blocks
                .get(block)
                .or_else(|| blocks.get(block))
                .cloned()
                .unwrap_or_else(|| {
                    reparsed += 1;
                    parse(revision, block).values
                });
            for mut value in local.iter().cloned() {
                value.id += scalar;
                if let Kind::Replace { range, .. } = &mut value.kind {
                    range.start += scalar;
                    range.end += scalar;
                }
                values.push(value);
            }
            scalar += block.chars().count();
            previous = range.end;
            blocks.insert(block.to_string(), local);
        }
        *cache = Parsed {
            source: source.to_string(),
            values,
            blocks,
            reparsed,
        };
        Batch {
            revision,
            values: cache.values.clone(),
        }
    })
}

fn parse(revision: DocumentRevision, source: &str) -> Batch {
    let mut frames: Vec<Frame> = Vec::new();
    let mut values = Vec::new();
    let mut blocked = 0usize;
    let mut fenced: Option<FencedFrame> = None;
    let mut emit = |mut range: Range<usize>, label: String, style| {
        if matches!(style, InlineStyle::Heading(..) | InlineStyle::Fenced { .. }) {
            while range.end > range.start
                && matches!(source.as_bytes()[range.end - 1], b'\n' | b'\r')
            {
                range.end -= 1;
            }
        }
        // Setext headings and closed fenced blocks own multiple source lines.
        // Other multiline constructs and paragraph-spanning headings keep source.
        let newlines = source[range.clone()]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count();
        let supported_lines = newlines == 0
            || (matches!(style, InlineStyle::Heading(..)) && newlines == 1)
            || matches!(style, InlineStyle::Fenced { .. });
        if range.is_empty() || label.is_empty() || !supported_lines {
            return;
        }
        let start = source[..range.start].chars().count();
        let end = start + source[range].chars().count();
        values.push(Decoration {
            provider: Provider::Markdown,
            id: start,
            kind: Kind::Replace {
                range: start..end,
                widget: Some(label),
                style: Some(style),
            },
        });
    };
    // Metadata is front matter only at the beginning of the document. The
    // parser option also accepts later fenced paragraphs, so parse the body
    // without it and retain canonical offsets after the leading source block.
    let body_start = Parser::new_ext(source, Options::ENABLE_YAML_STYLE_METADATA_BLOCKS)
        .into_offset_iter()
        .next()
        .filter(|(event, range)| {
            range.start == 0 && matches!(event, Event::Start(Tag::MetadataBlock(_)))
        })
        .map_or(0, |(_, range)| range.end);
    for (event, range) in Parser::new(&source[body_start..]).into_offset_iter() {
        let range = range.start + body_start..range.end + body_start;
        match event {
            Event::Start(Tag::Emphasis | Tag::Strong | Tag::Heading { .. }) => {
                if let Some(parent) = frames.last_mut() {
                    parent.nested = true;
                }
                let style = match event {
                    Event::Start(Tag::Strong) => InlineStyle::Strong,
                    Event::Start(Tag::Heading { level, .. }) => {
                        InlineStyle::Heading(level as u8, Vec::new())
                    }
                    _ => InlineStyle::Emphasis,
                };
                frames.push(Frame {
                    range,
                    style,
                    fragments: Vec::new(),
                    nested: false,
                    unsupported: blocked > 0,
                });
            }
            Event::End(TagEnd::Emphasis | TagEnd::Strong | TagEnd::Heading(_)) => {
                if let Some(frame) = frames.pop() {
                    if let Some(parent) = frames.last_mut() {
                        parent.unsupported |= frame.unsupported;
                        for fragment in frame.fragments {
                            append(&mut parent.fragments, fragment);
                        }
                    } else if !frame.unsupported {
                        let label = frame
                            .fragments
                            .iter()
                            .map(|fragment| fragment.text.as_str())
                            .collect();
                        let style = match frame.style {
                            InlineStyle::Heading(level, _) => {
                                InlineStyle::Heading(level, frame.fragments)
                            }
                            _ if frame.nested => InlineStyle::Nested(frame.fragments),
                            style => style,
                        };
                        emit(frame.range, label, style);
                    }
                }
            }
            Event::Code(text) => {
                if !frames.is_empty() {
                    let fragment = fragment(&frames, text.into_string(), true);
                    let frame = frames.last_mut().unwrap();
                    frame.nested = true;
                    frame.unsupported |= source[range].contains('\n');
                    append(&mut frame.fragments, fragment);
                } else if blocked == 0 {
                    emit(range, text.into_string(), InlineStyle::Code);
                }
            }
            Event::Text(text) if fenced.is_some() => {
                fenced.as_mut().unwrap().text.push_str(&text);
            }
            Event::Text(text) => {
                let fragment = fragment(&frames, text.into_string(), false);
                if let Some(frame) = frames.last_mut() {
                    append(&mut frame.fragments, fragment);
                }
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                if blocked == 0 && frames.is_empty() {
                    if let CodeBlockKind::Fenced(info) = kind {
                        if closed_fence(source, &range) {
                            fenced = Some(FencedFrame {
                                range,
                                info: info.into_string(),
                                text: String::new(),
                            });
                        }
                    }
                }
                blocked += 1;
                if let Some(frame) = frames.last_mut() {
                    frame.unsupported = true;
                }
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some(frame) = fenced.take() {
                    let style = highlighted_code(&frame.text, &frame.info);
                    emit(frame.range, frame.text, style);
                }
                blocked = blocked.saturating_sub(1);
            }
            Event::Start(
                Tag::Link { .. } | Tag::Image { .. } | Tag::HtmlBlock | Tag::MetadataBlock(_),
            ) => {
                blocked += 1;
                if let Some(frame) = frames.last_mut() {
                    frame.unsupported = true;
                }
            }
            Event::End(
                TagEnd::Link | TagEnd::Image | TagEnd::HtmlBlock | TagEnd::MetadataBlock(_),
            ) => {
                blocked = blocked.saturating_sub(1);
            }
            Event::InlineHtml(_) | Event::Html(_) | Event::SoftBreak | Event::HardBreak => {
                if let Some(frame) = frames.last_mut() {
                    frame.unsupported = true;
                }
            }
            _ => {}
        }
    }
    values.sort_by_key(|value| value.id);
    Batch { revision, values }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn previews(source: &str) -> Vec<(String, String, InlineStyle)> {
        let chars: Vec<_> = source.chars().collect();
        batch(DocumentRevision(3), source)
            .values
            .into_iter()
            .map(|value| {
                let Kind::Replace {
                    range,
                    widget,
                    style,
                } = value.kind
                else {
                    unreachable!()
                };
                (
                    chars[range].iter().collect(),
                    widget.unwrap(),
                    style.unwrap(),
                )
            })
            .collect()
    }
    #[test]
    fn long_document_reuses_unchanged_blocks_and_rebases_offsets() {
        let source = (0..240)
            .map(|n| {
                format!(
                    "# Section {n} 猫\n\n{} **bold** and `code`.\n\n",
                    "plain words ".repeat(75)
                )
            })
            .collect::<String>();
        let original = batch(DocumentRevision(1), &source);
        assert_eq!(original.values.len(), 720);
        let changed = source.replacen("Section 200", "Updated section 200", 1);
        let result = batch(DocumentRevision(2), &changed);
        assert_eq!(result.revision, DocumentRevision(2));
        PARSED.with(|cache| assert_eq!(cache.borrow().reparsed, 1));
        assert!(result.values == parse(DocumentRevision(2), &changed).values);
        let prefixed = format!("new paragraph\n\n{changed}");
        let result = batch(DocumentRevision(3), &prefixed);
        PARSED.with(|cache| assert_eq!(cache.borrow().reparsed, 1));
        assert!(result.values == parse(DocumentRevision(3), &prefixed).values);
        for source in [
            "before\n\n```rust\n*x*\n\n# later\n",
            "before\n\n```rust\n*x*\n```\n\n# later\n",
            "*a [link][ref]*\n\n[ref]: https://example.org",
        ] {
            assert!(
                batch(DocumentRevision(4), source).values
                    == parse(DocumentRevision(4), source).values
            );
        }
    }

    #[test]
    fn front_matter_keeps_source_without_consuming_later_setext_headings() {
        for metadata in ["title: Notes", "title: Notes\ntags: [test]"] {
            let source = format!("---\n{metadata}\n---\n\nBody\n---\n");
            let values = previews(&source);
            assert_eq!(values.len(), 1);
            assert_eq!(values[0].0, "Body\n---");
            assert_eq!(values[0].1, "Body");
            assert!(matches!(values[0].2, InlineStyle::Heading(2, _)));
        }
        assert!(previews("---\ntitle: Notes\n---").is_empty());
        let values = previews("Before\n\n---\ntitle: Notes\n---");
        assert_eq!(values.len(), 1);
        assert_eq!(values[0].0, "title: Notes\n---");
        assert_eq!(values[0].1, "title: Notes");
    }

    #[test]
    fn commonmark_ranges_preserve_unicode_escapes_entities_and_code_delimiters() {
        assert_eq!(
            previews("😀 *猫* **bold** _italic_ __strong__ `x < y` `` a ` b ``"),
            vec![
                ("*猫*".into(), "猫".into(), InlineStyle::Emphasis),
                ("**bold**".into(), "bold".into(), InlineStyle::Strong),
                ("_italic_".into(), "italic".into(), InlineStyle::Emphasis),
                ("__strong__".into(), "strong".into(), InlineStyle::Strong),
                ("`x < y`".into(), "x < y".into(), InlineStyle::Code),
                ("`` a ` b ``".into(), "a ` b".into(), InlineStyle::Code),
            ]
        );
        assert_eq!(
            previews("*a &amp; b* **a\\*b**"),
            vec![
                ("*a &amp; b*".into(), "a & b".into(), InlineStyle::Emphasis),
                ("**a\\*b**".into(), "a*b".into(), InlineStyle::Strong),
            ]
        );
    }
    #[test]
    fn incomplete_multiline_links_html_and_fenced_code_keep_source() {
        for source in [
            "*unfinished",
            "a_b_c",
            "\\*literal*",
            "*a\nb*",
            "`a\nb`",
            "[**link**](url)",
            "![*image*](url)",
            "*a <b>b</b>*",
            "```\n*code* `raw`\n",
            "    *code* `raw`",
        ] {
            assert!(previews(source).is_empty(), "{source}");
        }
    }
    #[test]
    fn nested_styles_keep_one_outer_range_and_combine_fragment_styles() {
        let fragment = |text: &str, emphasis, strong, code| InlineFragment {
            text: text.into(),
            emphasis,
            strong,
            code,
        };
        assert_eq!(
            previews("😀 ***猫 &amp; dog***"),
            vec![(
                "***猫 &amp; dog***".into(),
                "猫 & dog".into(),
                InlineStyle::Nested(vec![fragment("猫 & dog", true, true, false)]),
            )]
        );
        assert_eq!(
            previews("**bold *italic* and `x * y` tail**"),
            vec![(
                "**bold *italic* and `x * y` tail**".into(),
                "bold italic and x * y tail".into(),
                InlineStyle::Nested(vec![
                    fragment("bold ", false, true, false),
                    fragment("italic", true, true, false),
                    fragment(" and ", false, true, false),
                    fragment("x * y", false, true, true),
                    fragment(" tail", false, true, false),
                ]),
            )]
        );
        assert_eq!(previews("*plain **strong** then `code` end*").len(), 1);
        for source in [
            "*before **a <b>b</b>** after*",
            "**before *a\nb* after**",
            "*before [**link**](url) after*",
            "*before `a\nb` after*",
        ] {
            assert!(previews(source).is_empty(), "{source}");
        }
        let result = batch(DocumentRevision(9), &"***x*** ".repeat(100));
        assert_eq!(
            result.values.len(),
            100,
            "Nested children retain one outer owner"
        );
    }
    #[test]
    fn reveal_budget_is_deterministic_and_revision_bound() {
        let source = "*x* ".repeat(100);
        let result = batch(DocumentRevision(8), &source);
        assert_eq!(result.revision, DocumentRevision(8));
        assert_eq!(result.values.len(), 100);
        assert_eq!(result.values.last().unwrap().id, 396);
    }

    #[test]
    fn atx_headings_own_markers_but_not_newlines_and_keep_inline_styles() {
        for level in 1..=6 {
            let token = format!("{} Title **猫** and `x` ###", "#".repeat(level));
            let result = previews(&format!("{token}\nnext"));
            assert_eq!(result.len(), 1);
            assert_eq!(result[0].0, token);
            assert_eq!(result[0].1, "Title 猫 and x");
            let InlineStyle::Heading(actual, fragments) = &result[0].2 else {
                panic!("heading style");
            };
            assert_eq!(*actual, level as u8);
            assert!(fragments
                .iter()
                .any(|fragment| fragment.text == "猫" && fragment.strong));
            assert!(fragments
                .iter()
                .any(|fragment| fragment.text == "x" && fragment.code));
        }
        for source in [
            "Title *source*\ncontinued\n---\n",
            "#No heading",
            "####### Too many",
            "# [**link**](url)",
            "# a <b>b</b>",
            "    # literal",
            "#",
        ] {
            assert!(previews(source).is_empty(), "{source}");
        }
    }

    #[test]
    fn setext_headings_own_content_separator_and_underline() {
        for (underline, level) in [("===", 1), ("---", 2)] {
            let token = format!("Title **猫** &amp; `x`\n{underline}");
            let result = previews(&format!("before\n\n{token}\n\nafter"));
            assert_eq!(result.len(), 1);
            assert_eq!(result[0].0, token);
            assert_eq!(result[0].1, "Title 猫 & x");
            let InlineStyle::Heading(actual, fragments) = &result[0].2 else {
                panic!("heading");
            };
            assert_eq!(*actual, level);
            assert!(fragments
                .iter()
                .any(|fragment| fragment.text == "猫" && fragment.strong));
            assert!(fragments.iter().any(|fragment| fragment.code));
        }
        for source in [
            "first\nsecond\n===",
            "Title [**link**](url)\n---",
            "Title <b>html</b>\n===",
            "    Title\n    ===",
            "---\n",
            "Title\n\n---",
        ] {
            assert!(previews(source).is_empty(), "{source}");
        }
    }

    #[test]
    fn closed_fenced_blocks_keep_literal_code_and_use_grammar_highlighting() {
        let source = "```rs title=demo\nfn main() { let 猫 = \"<safe>\"; }\n```\n";
        let values = previews(source);
        assert_eq!(values.len(), 1);
        assert_eq!(values[0].0, source.trim_end_matches('\n'));
        assert_eq!(values[0].1, "fn main() { let 猫 = \"<safe>\"; }\n");
        let InlineStyle::Fenced {
            language,
            fragments,
        } = &values[0].2
        else {
            panic!("code block");
        };
        assert_eq!(language, "rust");
        assert_eq!(
            fragments
                .iter()
                .map(|fragment| fragment.text.as_str())
                .collect::<String>(),
            values[0].1
        );
        assert!(fragments
            .iter()
            .any(|fragment| fragment.class.starts_with("a-") && fragment.text.contains("fn")));
        let values = previews("~~~~unknown\n*literal* &amp; <b>\n\n~~~~~\n");
        assert_eq!(values.len(), 1);
        assert_eq!(values[0].1, "*literal* &amp; <b>\n\n");
        let InlineStyle::Fenced { fragments, .. } = &values[0].2 else {
            panic!("plain code");
        };
        assert!(fragments.iter().all(|fragment| fragment.class.is_empty()));
        for source in [
            "```rust\nfn main() {}\n",
            "````\n```\n",
            "```\nbody\n    ```",
            "```\nbody\n```\u{a0}",
            "```\n```",
            "> ```\n> body\n> ```",
            "    body\n",
        ] {
            assert!(previews(source).is_empty(), "{source}");
        }
    }

    #[test]
    fn reused_code_decorations_follow_live_revisions_and_changed_fence_language() {
        let source = "```rust\nfn main() {}\n```\n";
        let first = batch(DocumentRevision(1), source);
        let next = batch(DocumentRevision(2), source);
        assert_eq!(next.revision, DocumentRevision(2));
        assert!(first.values == next.values);
        let changed = batch(DocumentRevision(3), &source.replace("rust", "unknown"));
        let Kind::Replace {
            style:
                Some(InlineStyle::Fenced {
                    language,
                    fragments,
                }),
            ..
        } = &changed.values[0].kind
        else {
            panic!("changed code");
        };
        assert_eq!(language, "unknown");
        assert!(fragments.iter().all(|fragment| fragment.class.is_empty()));
        assert!(batch(DocumentRevision(4), "```rust\nfn main() {}\n")
            .values
            .is_empty());
    }
}
