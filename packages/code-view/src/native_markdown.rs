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
    link: Option<crate::native_link_parser::Link>,
}

fn fragment(frames: &[Frame], text: String, code: bool) -> InlineFragment {
    InlineFragment {
        text,
        math: None,
        emphasis: frames
            .iter()
            .any(|frame| frame.style == InlineStyle::Emphasis),
        strong: frames
            .iter()
            .any(|frame| frame.style == InlineStyle::Strong),
        code,
        link: frames.iter().rev().find_map(|frame| frame.link.clone()),
    }
}

fn append(fragments: &mut Vec<InlineFragment>, fragment: InlineFragment) {
    if let Some(previous) = fragments.last_mut().filter(|previous| {
        previous.math.is_none()
            && fragment.math.is_none()
            && previous.emphasis == fragment.emphasis
            && previous.strong == fragment.strong
            && previous.code == fragment.code
            && previous.link == fragment.link
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
    blocks: std::collections::HashMap<std::rc::Rc<str>, std::rc::Rc<Vec<Decoration>>>,
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
        let body_start = crate::native_properties::front_matter(source).map_or(0, |fm| fm.end);
        if body_start > 0 {
            ranges.push(0..body_start);
        }
        for (event, range) in Parser::new_ext(
            &source[body_start..],
            Options::ENABLE_TABLES | Options::ENABLE_MATH,
        )
        .into_offset_iter()
        {
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
            let (key, local) = cache
                .blocks
                .get_key_value(block)
                .or_else(|| blocks.get_key_value(block))
                .map(|(key, values)| (key.clone(), values.clone()))
                .unwrap_or_else(|| {
                    reparsed += 1;
                    (
                        std::rc::Rc::from(block),
                        std::rc::Rc::new(parse(revision, block).values),
                    )
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
            blocks.insert(key, local);
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
    let callout_values = crate::native_markdown_callouts::previews(source);
    let math_values = crate::native_math_parser::displays(source);
    let table_values = crate::native_markdown_tables::previews(source);
    let image_values = crate::native_image_parser::previews(source);
    let list_values = crate::native_markdown_lists::previews(source);
    let scalar_bytes: Vec<_> = source
        .char_indices()
        .map(|(byte, _)| byte)
        .chain(std::iter::once(source.len()))
        .collect();
    let mut frames: Vec<Frame> = Vec::new();
    let mut values = Vec::new();
    let mut blocked = 0usize;
    let mut containers = 0usize;
    let mut fenced: Option<FencedFrame> = None;
    let mut emit = |mut range: Range<usize>, label: String, mut style| {
        if !matches!(style, InlineStyle::Callout(_))
            && callout_values
                .get(callout_values.partition_point(|(item, _, _)| item.end <= range.start))
                .is_some_and(|(item, _, _)| item.start < range.end)
        {
            return;
        }
        if !matches!(style, InlineStyle::ListItem { .. })
            && list_values
                .get(list_values.partition_point(|(item, _, _)| item.end <= range.start))
                .is_some_and(|(item, _, _)| item.start < range.end)
        {
            return;
        }
        if matches!(
            style,
            InlineStyle::Heading(..)
                | InlineStyle::Fenced { .. }
                | InlineStyle::ListItem { .. }
                | InlineStyle::Table(_)
                | InlineStyle::Callout(_)
                | InlineStyle::Math(crate::native_math_parser::Math { display: true, .. })
        ) {
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
            || matches!(
                style,
                InlineStyle::Fenced { .. }
                    | InlineStyle::ListItem { .. }
                    | InlineStyle::Link(_)
                    | InlineStyle::Table(_)
                    | InlineStyle::Callout(_)
                    | InlineStyle::Math(_)
            );
        if range.is_empty()
            || label.is_empty() && !matches!(style, InlineStyle::Table(_))
            || !supported_lines
        {
            return;
        }
        let Ok(start) = scalar_bytes.binary_search(&range.start) else {
            return;
        };
        let Ok(end) = scalar_bytes.binary_search(&range.end) else {
            return;
        };
        match &mut style {
            InlineStyle::Nested(fragments) | InlineStyle::Heading(_, fragments) => {
                crate::native_link_parser::relative(fragments, source, range.start);
            }
            InlineStyle::Link(fragments) => {
                crate::native_link_parser::relative(
                    std::rc::Rc::make_mut(fragments).as_mut_slice(),
                    source,
                    range.start,
                );
            }
            _ => {}
        }
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
    // Only a conservative leading YAML block belongs to Properties. Parse
    // the body without the metadata option so later thematic breaks retain
    // CommonMark meaning, and keep all canonical offsets after the prefix.
    let body_start = crate::native_properties::front_matter(source).map_or(0, |fm| fm.end);
    let projected = crate::native_link_parser::Projected::new(&source[body_start..]);
    for (event, range) in Parser::new_ext(
        &projected.text,
        Options::ENABLE_TABLES | Options::ENABLE_MATH,
    )
    .into_offset_iter()
    {
        let range = projected.range(range);
        let range = range.start + body_start..range.end + body_start;
        match event {
            Event::Start(Tag::Emphasis | Tag::Strong | Tag::Heading { .. } | Tag::Link { .. }) => {
                let is_link = matches!(event, Event::Start(Tag::Link { .. }));
                let link = if let Event::Start(Tag::Link {
                    dest_url, title, ..
                }) = &event
                {
                    projected.link(dest_url, title, range.clone())
                } else {
                    None
                };
                if let Some(parent) = frames.last_mut() {
                    parent.nested = true;
                }
                let style = match event {
                    Event::Start(Tag::Link { .. }) => {
                        InlineStyle::Link(std::rc::Rc::new(Vec::new()))
                    }
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
                    unsupported: blocked > 0 || is_link && link.is_none(),
                    link,
                });
            }
            Event::End(TagEnd::Emphasis | TagEnd::Strong | TagEnd::Heading(_) | TagEnd::Link) => {
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
                            InlineStyle::Link(_) => {
                                InlineStyle::Link(std::rc::Rc::new(frame.fragments))
                            }
                            _ if frame.nested
                                || frame
                                    .fragments
                                    .iter()
                                    .any(|fragment| fragment.link.is_some()) =>
                            {
                                InlineStyle::Nested(frame.fragments)
                            }
                            style => style,
                        };
                        emit(frame.range, label, style);
                    }
                }
            }
            Event::Start(Tag::List(_) | Tag::BlockQuote(_)) => containers += 1,
            Event::End(TagEnd::List(_) | TagEnd::BlockQuote(_)) => {
                containers = containers.saturating_sub(1)
            }
            Event::InlineMath(tex) => {
                let math = if blocked == 0 && containers == 0 {
                    crate::native_math_parser::value(source, &range, &tex, false)
                } else {
                    None
                };
                let label = source[range.clone()].to_string();
                if !frames.is_empty() {
                    let mut fragment = fragment(&frames, label, false);
                    fragment.math = math;
                    let frame = frames.last_mut().unwrap();
                    frame.nested = true;
                    append(&mut frame.fragments, fragment);
                } else if let Some(math) = math {
                    emit(range, label, InlineStyle::Math(math));
                }
            }
            Event::DisplayMath(_) => {
                if let Some(frame) = frames.last_mut() {
                    frame.unsupported = true;
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
                Tag::Image { .. } | Tag::HtmlBlock | Tag::MetadataBlock(_) | Tag::Table(_),
            ) => {
                blocked += 1;
                if let Some(frame) = frames.last_mut() {
                    frame.unsupported = true;
                }
            }
            Event::End(
                TagEnd::Image | TagEnd::HtmlBlock | TagEnd::MetadataBlock(_) | TagEnd::Table,
            ) => {
                blocked = blocked.saturating_sub(1);
            }
            Event::SoftBreak | Event::HardBreak
                if frames.iter().any(|frame| frame.link.is_some()) =>
            {
                let text = if matches!(event, Event::HardBreak) {
                    "\n"
                } else {
                    " "
                };
                let fragment = fragment(&frames, text.into(), false);
                if let Some(frame) = frames.last_mut() {
                    append(&mut frame.fragments, fragment);
                }
            }
            Event::InlineHtml(_) | Event::Html(_) | Event::SoftBreak | Event::HardBreak => {
                if let Some(frame) = frames.last_mut() {
                    frame.unsupported = true;
                }
            }
            _ => {}
        }
    }
    for (range, label, style) in &callout_values {
        emit(range.clone(), label.clone(), style.clone());
    }
    for (range, label, style) in &list_values {
        emit(range.clone(), label.clone(), style.clone());
    }
    for (range, label, style) in table_values {
        emit(range, label, style);
    }
    for (range, math) in math_values {
        let label = source[range.clone()].to_string();
        emit(range, label, InlineStyle::Math(math));
    }
    for (range, image) in image_values {
        let start = source[..range.start].chars().count();
        let end = start + source[range].chars().count();
        values.push(Decoration {
            provider: Provider::Markdown,
            id: start,
            kind: Kind::Replace {
                range: start..end,
                widget: Some(image.label()),
                style: Some(InlineStyle::Image(image)),
            },
        });
    }
    if body_start > 0 {
        values.push(Decoration {
            provider: Provider::Markdown,
            id: 0,
            kind: Kind::Replace {
                range: 0..source[..body_start].chars().count(),
                widget: None,
                style: None,
            },
        });
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
            .filter(|value| {
                matches!(
                    &value.kind,
                    Kind::Replace {
                        widget: Some(_),
                        ..
                    }
                )
            })
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
    fn list_items_replace_inline_styles_once_and_reuse_unmodified_blocks() {
        let source = "before 😀\r\n\r\n- **parent 猫**\r\n  - [ ] child `code`\r\n- sibling\r\n\r\nafter\r\n";
        let values = previews(source);
        assert_eq!(values.len(), 3);
        assert!(values
            .iter()
            .all(|(_, _, style)| matches!(style, InlineStyle::ListItem { .. })));
        assert_eq!(values[0].0, "- **parent 猫**");
        assert_eq!(values[1].0, "  - [ ] child `code`");
        let changed = source.replace("[ ]", "[x]");
        let result = batch(DocumentRevision(4), &changed);
        assert!(result.values == parse(DocumentRevision(4), &changed).values);
        PARSED.with(|cache| assert_eq!(cache.borrow().reparsed, 1));
    }

    #[test]
    fn front_matter_hides_only_its_source_without_consuming_later_setext_headings() {
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
            math: None,
            emphasis,
            strong,
            code,
            link: None,
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
    #[test]
    fn links_preserve_nested_styles_and_original_unicode_ranges() {
        let source = "😀\n\n[**猫**](https://example.org) and [[Note#Intro|Alias <safe>]]\n\n# Title [site](https://example.org)\n\n- [[Note|item]]\n";
        let values = previews(source);
        assert!(values
            .iter()
            .any(|(raw, text, _)| raw == "[[Note#Intro|Alias <safe>]]" && text == "Alias <safe>"));
        assert!(values.iter().any(|(_, text, style)| text == "猫" && matches!(style, InlineStyle::Link(parts) if parts[0].strong && parts[0].link.is_some())));
        assert!(values.iter().any(|(_, _, style)| matches!(style, InlineStyle::Heading(_, parts) if parts.iter().any(|part| part.text == "site" && part.link.is_some()))));
        let first = batch(DocumentRevision(10), source);
        let second = batch(DocumentRevision(11), source);
        assert!(first.values == second.values);
        let prefixed = batch(DocumentRevision(12), &format!("prefix\n\n{source}"));
        assert_eq!(first.values.len(), prefixed.values.len());
        for (old, new) in first.values.iter().zip(&prefixed.values) {
            let (
                Kind::Replace {
                    range: old_range,
                    widget: old_widget,
                    style: old_style,
                },
                Kind::Replace {
                    range: new_range,
                    widget: new_widget,
                    style: new_style,
                },
            ) = (&old.kind, &new.kind)
            else {
                panic!("replacement")
            };
            assert_eq!(new_range.start, old_range.start + 8);
            assert_eq!(new_range.end, old_range.end + 8);
            assert_eq!(old_widget, new_widget);
            assert!(old_style == new_style);
        }
    }
    #[test]
    fn references_follow_definition_changes_and_unsafe_schemes_remain_source() {
        let source =
            "[Reference][id] and <https://example.org/auto>\n\n[id]: https://example.org/first\n";
        let first = previews(source);
        assert_eq!(
            first
                .iter()
                .map(|(_, text, _)| text.as_str())
                .collect::<Vec<_>>(),
            ["Reference", "https://example.org/auto"]
        );
        let changed = previews(&source.replace("/first", "/second"));
        let InlineStyle::Link(parts) = &changed[0].2 else {
            panic!("link")
        };
        assert!(
            matches!(&parts[0].link.as_ref().unwrap().target, crate::native_links::Target::External(url) if url.ends_with("/second"))
        );
        for source in [
            "[bad](javascript:alert)",
            "[bad](data:text/html,hello)",
            "[bad](file:///etc/passwd)",
            "[undefined][missing]",
        ] {
            assert!(previews(source).is_empty(), "{source}");
        }
    }
    #[test]
    fn image_references_rebase_unicode_ranges_and_refresh_destinations() {
        let source = "😀\r\n\r\n![猫][id]\r\n\r\n[id]: first.svg\r\n";
        let first = batch(DocumentRevision(20), source);
        let same = batch(DocumentRevision(21), source);
        assert!(first.values == same.values);
        let prefixed = batch(DocumentRevision(22), &format!("猫\r\n\r\n{source}"));
        let image_range = |batch: &crate::native_decorations::Batch| {
            batch
                .values
                .iter()
                .find_map(|value| match &value.kind {
                    Kind::Replace {
                        range,
                        style: Some(InlineStyle::Image(_)),
                        ..
                    } => Some(range.clone()),
                    _ => None,
                })
                .expect("image replacement")
        };
        let old = image_range(&first);
        let new = image_range(&prefixed);
        assert_eq!(new.start, old.start + 5);
        assert_eq!(new.end, old.end + 5);
        let changed = batch(
            DocumentRevision(23),
            &source.replace("first.svg", "second.svg"),
        );
        assert!(changed.values.iter().any(|value| matches!(&value.kind,
            Kind::Replace {style: Some(InlineStyle::Image(image)), ..}
                if image.destination == crate::native_image_parser::Destination::File("second.svg".into()))));
    }
    #[test]
    fn tables_own_complete_ranges_and_reuse_rebased_fragments() {
        let source = "😀\r\n\r\n| **猫** | `code` |\r\n| --- | ---: |\r\n| a | b |\r\n\r\nend\r\n";
        let first = batch(DocumentRevision(30), source);
        assert_eq!(first.values.len(), 1);
        assert!(matches!(
            &first.values[0].kind,
            Kind::Replace {
                style: Some(InlineStyle::Table(_)),
                ..
            }
        ));
        let same = batch(DocumentRevision(31), source);
        assert!(first.values == same.values);
        let prefixed = batch(DocumentRevision(32), &format!("prefix\r\n\r\n{source}"));
        let Kind::Replace { range: old, .. } = &first.values[0].kind else {
            panic!("table")
        };
        let Kind::Replace { range: new, .. } = &prefixed.values[0].kind else {
            panic!("table")
        };
        assert_eq!(new.start, old.start + 10);
        assert_eq!(new.end, old.end + 10);
        let changed = batch(DocumentRevision(33), &source.replace("a | b", "a | 中文"));
        assert!(first.values != changed.values);
        assert!(previews("| a | b |\n| --- | --- |\n| ![x](pic.png) | **source** |\n").is_empty());
    }
    #[test]
    fn math_is_source_owned_in_paragraphs_headings_lists_tables_and_links() {
        let source="😀\r\n\r\n$\\alpha$ and [$x^2$](https://example.org)\r\n\r\n# Title $x$\r\n\r\n- item $y$\r\n\r\n| Value | $z$ |\r\n| --- | --- |\r\n| $a$ | b |\r\n\r\n$$\r\n\\frac{a}{b}\r\n$$\r\n";
        let values = previews(source);
        assert!(values.iter().any(|(raw, _, style)| raw == "$\\alpha$"
            && matches!(style,InlineStyle::Math(math) if !math.display)));
        assert!(values.iter().any(|(_,_,style)|matches!(style,InlineStyle::Heading(_,parts) if parts.iter().any(|part|part.math.is_some()))));
        assert!(values.iter().any(|(_,_,style)|matches!(style,InlineStyle::ListItem{fragments,..} if fragments.iter().any(|part|part.math.is_some()))));
        assert!(values.iter().any(|(_,_,style)|matches!(style,InlineStyle::Table(table) if table.fragments.iter().filter(|part|part.math.is_some()).count()==2)));
        assert!(values.iter().any(|(_,_,style)|matches!(style,InlineStyle::Link(parts) if parts.iter().any(|part|part.math.is_some()&&part.link.is_some()))));
        assert!(values
            .iter()
            .any(|(_, _, style)| matches!(style,InlineStyle::Math(math) if math.display)));
        let first = batch(DocumentRevision(40), source);
        let same = batch(DocumentRevision(41), source);
        assert!(first.values == same.values);
        let prefixed = batch(DocumentRevision(42), &format!("猫\r\n\r\n{source}"));
        assert_eq!(first.values.len(), prefixed.values.len());
        for (old, new) in first.values.iter().zip(&prefixed.values) {
            assert_eq!(new.id, old.id + 5);
        }
    }
    #[test]
    fn math_excludes_code_metadata_currency_and_wiki_projection_inside_tex() {
        for source in [
            "`$x$`",
            "```\n$x$\n```",
            "---\ntitle: $x$\n---",
            "$5,$10",
            "escaped \\$x$",
            "unmatched $x",
            "before $$x$$ after",
        ] {
            assert!(
                !previews(source)
                    .iter()
                    .any(|(_, _, style)| matches!(style, InlineStyle::Math(_))),
                "{source}"
            );
        }
        let source = "$\\text{[[Note|Alias]]}$";
        let projected = crate::native_link_parser::Projected::new(source);
        assert_eq!(projected.text, source);
        let values = previews(source);
        assert_eq!(values.len(), 1);
        assert!(
            matches!(&values[0].2,InlineStyle::Math(math) if math.tex.contains("[[Note|Alias]]"))
        );
    }
    #[test]
    fn callouts_own_whole_ranges_and_rebase_without_nested_replacements() {
        let source="😀\r\n\r\n> [!NOTE] **猫**\r\n> Body *emphasis* and [link](https://example.org).\r\n\r\nend";
        let first = batch(DocumentRevision(50), source);
        assert_eq!(first.values.len(), 1);
        assert!(matches!(
            &first.values[0].kind,
            Kind::Replace {
                style: Some(InlineStyle::Callout(_)),
                ..
            }
        ));
        assert!(first.values == batch(DocumentRevision(51), source).values);
        let prefixed = batch(DocumentRevision(52), &format!("猫\r\n\r\n{source}"));
        let range = |b: &Batch| match &b.values[0].kind {
            Kind::Replace { range, .. } => range.clone(),
            _ => panic!("replacement"),
        };
        assert_eq!(range(&prefixed).start, range(&first).start + 5);
        assert_eq!(range(&prefixed).end, range(&first).end + 5);
        let changed = batch(
            DocumentRevision(53),
            &source.replace("example.org", "example.net"),
        );
        assert!(changed.values.iter().any(|v|matches!(&v.kind,Kind::Replace{style:Some(InlineStyle::Callout(c)),..} if c.fragments.iter().any(|f|f.link.as_ref().is_some_and(|l|l.target.description().contains("example.net"))))));
    }
}
