//! Conservative CommonMark inline presentation over the bounded source prefix.
use crate::native_decorations::{Batch, Decoration, InlineStyle, Kind, Provider};
use moonkale_ext_api::editor::DocumentRevision;
use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use std::ops::Range;

struct Frame {
    range: Range<usize>,
    style: InlineStyle,
    label: String,
    unsupported: bool,
}

pub(crate) fn batch(revision: DocumentRevision, source: &str) -> Batch {
    let mut frames: Vec<Frame> = Vec::new();
    let mut values = Vec::new();
    let mut blocked = 0usize;
    let mut emit = |range: Range<usize>, label: String, style| {
        // Multiline/nested inlines remain source in this initial provider.
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
            Event::Start(Tag::Emphasis | Tag::Strong) => {
                let nested = !frames.is_empty();
                if let Some(parent) = frames.last_mut() {
                    parent.unsupported = true;
                }
                let style = if matches!(event, Event::Start(Tag::Strong)) {
                    InlineStyle::Strong
                } else {
                    InlineStyle::Emphasis
                };
                frames.push(Frame {
                    range,
                    style,
                    label: String::new(),
                    unsupported: nested || blocked > 0,
                });
            }
            Event::End(TagEnd::Emphasis | TagEnd::Strong) => {
                if let Some(frame) = frames.pop() {
                    if !frame.unsupported {
                        emit(frame.range, frame.label, frame.style);
                    }
                }
            }
            Event::Code(text) => {
                if let Some(frame) = frames.last_mut() {
                    frame.unsupported = true;
                } else if blocked == 0 {
                    emit(range, text.into_string(), InlineStyle::Code);
                }
            }
            Event::Text(text) => {
                if let Some(frame) = frames.last_mut() {
                    frame.label.push_str(&text);
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
    fn incomplete_nested_multiline_links_html_and_fenced_code_keep_source() {
        for source in [
            "*unfinished",
            "a_b_c",
            "\\*literal*",
            "***nested***",
            "*with **nested** text*",
            "**with `code`**",
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
    fn reveal_budget_is_deterministic_and_revision_bound() {
        let source = "*x* ".repeat(100);
        let result = batch(DocumentRevision(8), &source);
        assert_eq!(result.revision, DocumentRevision(8));
        assert_eq!(result.values.len(), 63);
        assert_eq!(result.values.last().unwrap().id, 248);
    }
}
