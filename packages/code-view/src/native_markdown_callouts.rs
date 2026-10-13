//! Root callouts own their complete quote; unsupported bodies stay canonical source.
use crate::native_decorations::{InlineFragment, InlineStyle};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::{ops::Range, rc::Rc};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Row {
    pub fragments: Range<usize>,
    pub quote_depth: usize,
    pub heading: u8,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Callout {
    pub kind: String,
    /// None: always open; Some(false): '-' initially folded; Some(true): '+'.
    pub fold: Option<bool>,
    pub title: Range<usize>,
    pub rows: Vec<Row>,
    pub fragments: Vec<InlineFragment>,
}

fn header(source: &str, start: usize) -> Option<(String, Option<bool>, Range<usize>)> {
    let line_end = source[start..]
        .find('\n')
        .map_or(source.len(), |n| start + n);
    let line = source[start..line_end].trim_end_matches('\r');
    let indent = line.bytes().take_while(|b| *b == b' ').count();
    if indent > 3 {
        return None;
    }
    let after = line[indent..].strip_prefix('>')?;
    let after = after.strip_prefix(' ').unwrap_or(after);
    let marker = after.strip_prefix("[!")?;
    let end = marker.find(']')?;
    let kind = &marker[..end];
    if kind.is_empty()
        || !kind
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return None;
    }
    let rest = &marker[end + 1..];
    let (fold, rest) = match rest.as_bytes().first() {
        Some(b'+') => (Some(true), &rest[1..]),
        Some(b'-') => (Some(false), &rest[1..]),
        _ => (None, rest),
    };
    if !rest.is_empty() && !rest.starts_with([' ', '\t']) {
        return None;
    }
    let title = rest.trim_start_matches([' ', '\t']);
    let title_start = start + line.len() - title.len();
    Some((
        kind.to_ascii_lowercase(),
        fold,
        title_start..start + line.len(),
    ))
}

pub(crate) fn previews(source: &str) -> Vec<(Range<usize>, String, InlineStyle)> {
    let body_start = Parser::new_ext(source, Options::ENABLE_YAML_STYLE_METADATA_BLOCKS)
        .into_offset_iter()
        .next()
        .filter(|(event, range)| {
            range.start == 0 && matches!(event, Event::Start(Tag::MetadataBlock(_)))
        })
        .map_or(0, |(_, range)| range.end);
    let projected = crate::native_link_parser::Projected::new(&source[body_start..]);
    let mut depth = 0usize;
    let mut current = None::<(Range<usize>, Range<usize>, Callout)>;
    let (mut unsupported, mut quote_depth, mut emphasis, mut strong) =
        (false, 0usize, 0usize, 0usize);
    let mut link = None;
    let mut lists = Vec::<Option<u64>>::new();
    let mut fresh_item = false;
    let mut values = Vec::new();
    for (event, range) in Parser::new_ext(
        &projected.text,
        Options::ENABLE_TASKLISTS | Options::ENABLE_MATH | Options::ENABLE_TABLES,
    )
    .into_offset_iter()
    {
        let range = projected.range(range);
        let range = range.start + body_start..range.end + body_start;
        if let Event::Start(Tag::BlockQuote(_)) = &event {
            if depth == 0 {
                let start = source[..range.start].rfind('\n').map_or(0, |n| n + 1);
                if let Some((kind, fold, title_range)) = header(source, start) {
                    current = Some((
                        start..range.end,
                        title_range,
                        Callout {
                            kind,
                            fold,
                            title: 0..0,
                            rows: Vec::new(),
                            fragments: Vec::new(),
                        },
                    ));
                    unsupported = false;
                    quote_depth = 0;
                    emphasis = 0;
                    strong = 0;
                    link = None;
                    lists.clear();
                }
            } else if current.is_some() {
                quote_depth += 1;
            }
        }
        if matches!(event, Event::End(TagEnd::BlockQuote(_))) && depth == 1 {
            if let Some((mut whole, _, mut callout)) = current.take() {
                while whole.end > whole.start
                    && matches!(source.as_bytes()[whole.end - 1], b'\r' | b'\n')
                {
                    whole.end -= 1;
                }
                if callout.title.is_empty() {
                    // No custom title: the type is presentation text only.
                    callout.fragments.insert(
                        0,
                        InlineFragment {
                            text: callout.kind.replace(['-', '_'], " "),
                            math: None,
                            emphasis: false,
                            strong: true,
                            code: false,
                            link: None,
                        },
                    );
                    callout.title = 0..1;
                    for row in &mut callout.rows {
                        row.fragments.start += 1;
                        row.fragments.end += 1;
                    }
                }
                if !unsupported {
                    crate::native_link_parser::relative(
                        &mut callout.fragments,
                        source,
                        whole.start,
                    );
                    let label = callout.fragments.iter().map(|f| f.text.as_str()).collect();
                    values.push((whole, label, InlineStyle::Callout(Rc::new(callout))));
                }
            }
        } else if let Some((_, title_range, callout)) = current.as_mut() {
            let is_header = range.start < title_range.end && range.end <= title_range.end;
            if range.start >= title_range.start
                && range.start < title_range.end
                && range.end > title_range.end
                && matches!(
                    event,
                    Event::Start(Tag::Emphasis | Tag::Strong | Tag::Link { .. })
                        | Event::Code(_)
                        | Event::InlineMath(_)
                )
            {
                unsupported = true;
            }
            match &event {
                Event::Start(Tag::Paragraph | Tag::Heading { .. }) => {
                    let heading = if let Event::Start(Tag::Heading { level, .. }) = event {
                        level as u8
                    } else {
                        0
                    };
                    if fresh_item && heading == 0 {
                        fresh_item = false;
                    } else {
                        if let Some(row) = callout.rows.last_mut() {
                            row.fragments.end = callout.fragments.len();
                        }
                        callout.rows.push(Row {
                            fragments: callout.fragments.len()..callout.fragments.len(),
                            quote_depth: quote_depth + lists.len().saturating_sub(1),
                            heading,
                        });
                    }
                }
                Event::End(TagEnd::Paragraph | TagEnd::Heading(_)) => {
                    if let Some(row) = callout.rows.last_mut() {
                        row.fragments.end = callout.fragments.len();
                    }
                }
                Event::Start(Tag::Emphasis) => emphasis += 1,
                Event::End(TagEnd::Emphasis) => emphasis = emphasis.saturating_sub(1),
                Event::Start(Tag::Strong) => strong += 1,
                Event::End(TagEnd::Strong) => strong = strong.saturating_sub(1),
                Event::Start(Tag::Link {
                    dest_url, title, ..
                }) if range.start >= title_range.start => {
                    link = projected.link(dest_url, title, range.clone());
                    unsupported |= link.is_none();
                }
                Event::End(TagEnd::Link) => link = None,
                Event::Start(Tag::List(number)) => lists.push(*number),
                Event::End(TagEnd::List(_)) => {
                    lists.pop();
                }
                Event::Start(Tag::Item) => {
                    fresh_item = true;
                    if let Some(row) = callout.rows.last_mut() {
                        row.fragments.end = callout.fragments.len();
                    }
                    let marker = match lists.last_mut() {
                        Some(Some(n)) => {
                            let s = format!("{n}. ");
                            *n = n.saturating_add(1);
                            s
                        }
                        _ => "• ".into(),
                    };
                    callout.rows.push(Row {
                        fragments: callout.fragments.len()..callout.fragments.len(),
                        quote_depth: quote_depth + lists.len().saturating_sub(1),
                        heading: 0,
                    });
                    callout.fragments.push(InlineFragment {
                        text: marker,
                        math: None,
                        emphasis: false,
                        strong: false,
                        code: false,
                        link: None,
                    });
                }
                Event::End(TagEnd::Item) => {
                    fresh_item = false;
                    if let Some(row) = callout.rows.last_mut() {
                        row.fragments.end = callout.fragments.len();
                    }
                }
                Event::TaskListMarker(checked) => callout.fragments.push(InlineFragment {
                    text: if *checked { "☑ " } else { "☐ " }.into(),
                    math: None,
                    emphasis: false,
                    strong: false,
                    code: false,
                    link: None,
                }),
                Event::Text(text) | Event::Code(text) => {
                    let text = if is_header {
                        // The header marker may arrive in several Text events.
                        if range.end <= title_range.start {
                            continue;
                        }
                        if range.start < title_range.start {
                            source[title_range.start..range.end].to_string()
                        } else {
                            text.to_string()
                        }
                    } else {
                        text.to_string()
                    };
                    if text.is_empty() {
                        continue;
                    }
                    let index = callout.fragments.len();
                    callout.fragments.push(InlineFragment {
                        text,
                        math: None,
                        emphasis: emphasis > 0,
                        strong: strong > 0,
                        code: matches!(event, Event::Code(_)),
                        link: link.clone(),
                    });
                    if is_header {
                        callout.title.end = index + 1;
                    }
                }
                Event::InlineMath(tex) => {
                    callout.fragments.push(InlineFragment {
                        text: source[range.clone()].into(),
                        math: crate::native_math_parser::value(source, &range, tex, false),
                        emphasis: emphasis > 0,
                        strong: strong > 0,
                        code: false,
                        link: link.clone(),
                    });
                    if is_header {
                        callout.title.end = callout.fragments.len();
                    }
                }
                Event::SoftBreak | Event::HardBreak => {
                    if range.start
                        <= title_range.end
                            + usize::from(source.as_bytes().get(title_range.end) == Some(&b'\r'))
                    {
                        // First body line shares CommonMark's first paragraph.
                        callout.rows.push(Row {
                            fragments: callout.fragments.len()..callout.fragments.len(),
                            quote_depth,
                            heading: 0,
                        });
                    } else {
                        callout.fragments.push(InlineFragment {
                            text: if matches!(event, Event::HardBreak) {
                                "\n"
                            } else {
                                " "
                            }
                            .into(),
                            math: None,
                            emphasis: false,
                            strong: false,
                            code: false,
                            link: None,
                        });
                    }
                }
                Event::Start(Tag::BlockQuote(_)) => {}
                Event::End(TagEnd::BlockQuote(_)) => quote_depth = quote_depth.saturating_sub(1),
                _ => unsupported = true,
            }
        }
        match event {
            Event::Start(_) => depth += 1,
            Event::End(_) => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    // The first paragraph starts with the title but it isn't a body row.
    for (_, _, style) in &mut values {
        if let InlineStyle::Callout(callout) = style {
            let callout = Rc::make_mut(callout);
            let title_end = callout.title.end;
            for row in &mut callout.rows {
                row.fragments.start = row.fragments.start.max(title_end);
            }
            callout
                .rows
                .retain(|row| row.fragments.start < row.fragments.end);
        }
    }
    values
}

#[cfg(test)]
mod tests {
    use super::*;
    fn callout(source: &str) -> Callout {
        let values = previews(source);
        assert_eq!(values.len(), 1, "{source}");
        let InlineStyle::Callout(value) = &values[0].2 else {
            panic!("callout")
        };
        assert_eq!(
            values[0].1,
            value
                .fragments
                .iter()
                .map(|f| f.text.as_str())
                .collect::<String>()
        );
        assert!(values[0].0.end <= source.len());
        (**value).clone()
    }
    fn text(value: &Callout, range: Range<usize>) -> String {
        value.fragments[range]
            .iter()
            .map(|f| f.text.as_str())
            .collect()
    }
    #[test]
    fn titles_bodies_nested_quotes_and_folding_keep_source() {
        let source="😀\r\n\r\n  > [!Warning]- **猫** and `literal`\r\n  > Body &amp; *text*.\r\n  >\r\n  > > Nested quote.\r\n\r\nend";
        let value = callout(source);
        assert_eq!(value.kind, "warning");
        assert_eq!(value.fold, Some(false));
        assert_eq!(text(&value, value.title.clone()), "猫 and literal");
        assert_eq!(value.rows.len(), 2);
        assert_eq!(
            text(&value, value.rows[0].fragments.clone()),
            "Body & text."
        );
        assert_eq!(
            text(&value, value.rows[1].fragments.clone()),
            "Nested quote."
        );
        assert_eq!(value.rows[1].quote_depth, 1);
        assert!(value.fragments.iter().any(|f| f.strong && f.text == "猫"));
        let range = previews(source)[0].0.clone();
        assert_eq!(&source[range],"  > [!Warning]- **猫** and `literal`\r\n  > Body &amp; *text*.\r\n  >\r\n  > > Nested quote.");
        let custom = callout("> [!my-type]+\n> Body\ncontinued lazily.\n");
        assert_eq!(custom.fold, Some(true));
        assert_eq!(text(&custom, custom.title.clone()), "my type");
        assert_eq!(
            text(&custom, custom.rows[0].fragments.clone()),
            "Body continued lazily."
        );
    }
    #[test]
    fn formatted_titles_links_math_lists_and_headings_share_literal_fragments() {
        let source="> [!note] [**Web**](https://example.org) $x$\n> [[Page|Alias]] and $y$\n>\n> ## Heading\n>\n> - [x] task\n> - child\n";
        let value = callout(source);
        assert_eq!(text(&value, value.title.clone()), "Web $x$");
        assert_eq!(
            value.fragments.iter().filter(|f| f.math.is_some()).count(),
            2
        );
        assert_eq!(
            value.fragments.iter().filter(|f| f.link.is_some()).count(),
            2
        );
        assert!(value
            .fragments
            .iter()
            .filter_map(|f| f.link.as_ref())
            .all(|l| l.range.end <= source.chars().count()));
        assert!(value.rows.iter().any(|r| r.heading == 2));
        assert!(value
            .rows
            .iter()
            .any(|r| text(&value, r.fragments.clone()).contains("☑ task")));
        // Each fragment is rendered exactly once (required by geometry's identity check).
        let mut owned = vec![0usize; value.fragments.len()];
        for index in value.title.clone() {
            owned[index] += 1;
        }
        for row in &value.rows {
            for index in row.fragments.clone() {
                owned[index] += 1;
            }
        }
        assert!(owned.iter().all(|n| *n == 1), "{value:?}");
    }
    #[test]
    fn metadata_code_non_callouts_and_unsupported_bodies_stay_source() {
        for source in [
            "> plain quote\n",
            "> [!note]bad\n",
            "> [!note] `title\n> body`\n",
            "> [!note] *title\n> body*\n",
            "> [!] title\n",
            "> [!bad type] title\n",
            "    > [!note]\n",
            "```\n> [!note]\n```\n",
            "---\nx: |\n  > [!note]\n---\n",
            "- > [!note]\n  > body\n",
            "> [!note]\n> ![image](pic.png)\n",
            "> [!note]\n> <b>HTML</b>\n",
            "> [!note]\n> ```\n> code\n> ```\n",
            "> [!note]\n>\n> a | b\n> --- | ---\n> c | d\n",
        ] {
            assert!(previews(source).is_empty(), "{source}");
        }
        assert_eq!(previews("> [!note]\n\n> [!tip] title\n").len(), 2);
    }
}
