//! Conservative CommonMark inline presentation over the bounded source prefix.
use crate::native_decorations::{Batch, Decoration, InlineFragment, InlineStyle, Kind, Provider};
use moonkale_ext_api::editor::DocumentRevision;
use pulldown_cmark::{Event, Parser, Tag, TagEnd};
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

pub(crate) fn batch(revision: DocumentRevision, source: &str) -> Batch {
    let mut frames: Vec<Frame> = Vec::new();
    let mut values = Vec::new();
    let mut blocked = 0usize;
    let mut emit = |mut range: Range<usize>, label: String, style| {
        if matches!(style, InlineStyle::Heading(..)) {
            while range.end > range.start
                && matches!(source.as_bytes()[range.end - 1], b'\n' | b'\r')
            {
                range.end -= 1;
            }
        }
        // Multiline constructs retain source in this bounded provider.
        if range.is_empty() || label.is_empty() || source[range.clone()].contains('\n') {
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
    for (event, range) in Parser::new(source).into_offset_iter() {
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
            Event::Text(text) => {
                let fragment = fragment(&frames, text.into_string(), false);
                if let Some(frame) = frames.last_mut() {
                    append(&mut frame.fragments, fragment);
                }
            }
            Event::Start(
                Tag::Link { .. } | Tag::Image { .. } | Tag::CodeBlock(_) | Tag::HtmlBlock,
            ) => {
                blocked += 1;
                if let Some(frame) = frames.last_mut() {
                    frame.unsupported = true;
                }
            }
            Event::End(TagEnd::Link | TagEnd::Image | TagEnd::CodeBlock | TagEnd::HtmlBlock) => {
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
    // The high bit identifies the provider; the other bits own reveal state.
    values.truncate(63);
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
            "```\n*code* `raw`\n```",
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
            63,
            "Nested children do not consume reveal bits"
        );
    }
    #[test]
    fn reveal_budget_is_deterministic_and_revision_bound() {
        let source = "*x* ".repeat(100);
        let result = batch(DocumentRevision(8), &source);
        assert_eq!(result.revision, DocumentRevision(8));
        assert_eq!(result.values.len(), 63);
        assert_eq!(result.values.last().unwrap().id, 248);
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
            "Title *source*\n---\n",
            "#No heading",
            "####### Too many",
            "# [**link**](url)",
            "# a <b>b</b>",
            "```\n# literal\n```",
            "#",
        ] {
            assert!(previews(source).is_empty(), "{source}");
        }
    }
}
