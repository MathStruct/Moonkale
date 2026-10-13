//! Item-sized source replacements; nested children keep independent ownership.
use crate::native_decorations::{InlineFragment, InlineStyle};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::ops::Range;

struct Item {
    range: Range<usize>,
    prefix_end: Option<usize>,
    marker: String,
    depth: usize,
    checked: Option<bool>,
    fragments: Vec<InlineFragment>,
    emphasis: usize,
    strong: usize,
    paragraphs: usize,
    unsupported: bool,
}

impl Item {
    fn text(&mut self, text: String, code: bool) {
        if self.prefix_end.is_some() {
            self.unsupported = true;
        }
        let fragment = InlineFragment {
            text,
            code,
            emphasis: self.emphasis > 0,
            strong: self.strong > 0,
        };
        if let Some(last) = self.fragments.last_mut().filter(|last| {
            last.code == fragment.code
                && last.emphasis == fragment.emphasis
                && last.strong == fragment.strong
        }) {
            last.text.push_str(&fragment.text);
        } else {
            self.fragments.push(fragment);
        }
    }
}

pub(crate) fn previews(source: &str) -> Vec<(Range<usize>, String, InlineStyle)> {
    let mut lists: Vec<Option<u64>> = Vec::new();
    let mut items: Vec<Item> = Vec::new();
    let mut values = Vec::new();
    let mut quotes = 0usize;
    let line_start = |byte| source[..byte].rfind('\n').map_or(0, |offset| offset + 1);
    let body_start = Parser::new_ext(source, Options::ENABLE_YAML_STYLE_METADATA_BLOCKS)
        .into_offset_iter()
        .next()
        .filter(|(event, range)| {
            range.start == 0 && matches!(event, Event::Start(Tag::MetadataBlock(_)))
        })
        .map_or(0, |(_, range)| range.end);
    for (event, range) in
        Parser::new_ext(&source[body_start..], Options::ENABLE_TASKLISTS).into_offset_iter()
    {
        let range = range.start + body_start..range.end + body_start;
        match event {
            Event::Start(Tag::List(number)) => {
                if let Some(parent) = items.last_mut() {
                    parent.prefix_end.get_or_insert(line_start(range.start));
                }
                lists.push(number);
            }
            Event::End(TagEnd::List(_)) => {
                lists.pop();
            }
            Event::Start(Tag::Item) => {
                let marker = match lists.last_mut() {
                    Some(Some(number)) => {
                        let marker = format!("{number}.");
                        *number = number.saturating_add(1);
                        marker
                    }
                    _ => "•".into(),
                };
                items.push(Item {
                    range: line_start(range.start)..range.end,
                    prefix_end: None,
                    marker,
                    depth: lists.len().saturating_sub(1),
                    checked: None,
                    fragments: Vec::new(),
                    emphasis: 0,
                    strong: 0,
                    paragraphs: 0,
                    unsupported: quotes > 0,
                });
            }
            Event::End(TagEnd::Item) => {
                if let Some(mut item) = items.pop() {
                    item.range.end = item.prefix_end.unwrap_or(item.range.end);
                    while item.range.end > item.range.start
                        && matches!(source.as_bytes()[item.range.end - 1], b'\n' | b'\r')
                    {
                        item.range.end -= 1;
                    }
                    if item.unsupported
                        || item.paragraphs > 1
                        || item.range.is_empty()
                        || source[item.range.clone()].contains("[[")
                    {
                        continue;
                    }
                    if let Some(checked) = item.checked {
                        let checkbox = if checked { "☑" } else { "☐" };
                        item.marker = if item.marker == "•" {
                            checkbox.into()
                        } else {
                            format!("{} {checkbox}", item.marker)
                        };
                    }
                    let text = item
                        .fragments
                        .iter()
                        .map(|fragment| fragment.text.as_str())
                        .collect::<String>();
                    let label = format!("{} {text}", item.marker);
                    values.push((
                        item.range,
                        label,
                        InlineStyle::ListItem {
                            depth: item.depth,
                            marker: item.marker,
                            checked: item.checked,
                            fragments: std::rc::Rc::new(item.fragments),
                        },
                    ));
                }
            }
            Event::Start(Tag::BlockQuote(_)) => {
                quotes += 1;
                if let Some(item) = items.last_mut() {
                    item.unsupported = true;
                }
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                quotes = quotes.saturating_sub(1);
            }
            event => {
                let Some(item) = items.last_mut() else {
                    continue;
                };
                match event {
                    Event::Start(Tag::Paragraph) => {
                        item.paragraphs += 1;
                    }
                    Event::End(TagEnd::Paragraph) => {}
                    Event::Start(Tag::Emphasis) => {
                        item.emphasis += 1;
                    }
                    Event::End(TagEnd::Emphasis) => {
                        item.emphasis = item.emphasis.saturating_sub(1);
                    }
                    Event::Start(Tag::Strong) => {
                        item.strong += 1;
                    }
                    Event::End(TagEnd::Strong) => {
                        item.strong = item.strong.saturating_sub(1);
                    }
                    Event::TaskListMarker(checked) => {
                        item.checked = Some(checked);
                    }
                    Event::Text(text) => {
                        item.text(text.into_string(), false);
                    }
                    Event::Code(text) => {
                        item.text(text.into_string(), true);
                    }
                    Event::SoftBreak => {
                        item.text(" ".into(), false);
                    }
                    Event::HardBreak => {
                        item.text("\n".into(), false);
                    }
                    _ => {
                        item.unsupported = true;
                    }
                }
            }
        }
    }
    values.sort_by_key(|(range, _, _)| range.start);
    values
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn items_are_disjoint_and_nested_tasks_and_numbers_keep_independent_source_ranges() {
        let source = "- parent **猫**\r\n  - child `x`\r\n  - [x] done\r\n- sibling\r\n\r\n7. first\r\n8. second\r\n\r\n- [ ] todo\r\n- [X] done\r\n";
        let items = previews(source);
        assert_eq!(
            items
                .iter()
                .map(|(_, text, _)| text.as_str())
                .collect::<Vec<_>>(),
            [
                "• parent 猫",
                "• child x",
                "☑ done",
                "• sibling",
                "7. first",
                "8. second",
                "☐ todo",
                "☑ done"
            ]
        );
        assert_eq!(&source[items[0].0.clone()], "- parent **猫**");
        assert_eq!(&source[items[1].0.clone()], "  - child `x`");
        assert!(items
            .windows(2)
            .all(|items| items[0].0.end <= items[1].0.start));
        assert!(
            matches!(&items[1].2, InlineStyle::ListItem { depth: 1, fragments, .. } if fragments.iter().any(|fragment| fragment.code))
        );
        assert!(matches!(
            &items[2].2,
            InlineStyle::ListItem {
                checked: Some(true),
                ..
            }
        ));
        assert!(matches!(
            &items[6].2,
            InlineStyle::ListItem {
                checked: Some(false),
                ..
            }
        ));
    }

    #[test]
    fn wrapped_items_decode_text_and_style_without_changing_source_boundaries() {
        let source = "- wrapped\n  continuation &amp; **bold** and *italic*\n- next\n";
        let items = previews(source);
        assert_eq!(items[0].1, "• wrapped continuation & bold and italic");
        assert_eq!(
            &source[items[0].0.clone()],
            "- wrapped\n  continuation &amp; **bold** and *italic*"
        );
        let InlineStyle::ListItem { fragments, .. } = &items[0].2 else {
            panic!()
        };
        assert!(fragments
            .iter()
            .any(|fragment| fragment.strong && fragment.text == "bold"));
        assert!(fragments
            .iter()
            .any(|fragment| fragment.emphasis && fragment.text == "italic"));
    }

    #[test]
    fn unsupported_blocks_and_front_matter_keep_their_source() {
        for source in [
            "- [link](url)\n",
            "- [[Wiki]]\n",
            "- ![image](url)\n",
            "> - quote\n",
            "- first\n\n  second paragraph\n",
            "- ```rust\n  code\n  ```\n",
            "\\- literal\n",
            "---\ntags:\n  - metadata\n---\n",
        ] {
            assert!(previews(source).is_empty(), "{source}");
        }
        let source = "- parent\n  - child\n\n  trailing paragraph\n";
        assert_eq!(
            previews(source)
                .iter()
                .map(|(_, text, _)| text.as_str())
                .collect::<Vec<_>>(),
            ["• child"]
        );
        let source = "---\ntags:\n  - metadata\n---\n\n- [ ] real\n";
        assert_eq!(previews(source)[0].1, "☐ real");
    }
}
