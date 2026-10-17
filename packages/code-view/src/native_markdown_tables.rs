//! Whole-table source replacements with literal, styled cell fragments.
use crate::native_decorations::{InlineFragment, InlineStyle};
use pulldown_cmark::{Alignment, Event, Options, Parser, Tag, TagEnd};
use std::{ops::Range, rc::Rc};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Table {
    pub alignments: Vec<&'static str>,
    pub rows: Vec<Vec<Range<usize>>>,
    pub fragments: Vec<InlineFragment>,
}

pub(crate) fn previews(source: &str) -> Vec<(Range<usize>, String, InlineStyle)> {
    crate::markdown_guard::parse(source, || previews_parsed(source)).unwrap_or_default()
}

fn previews_parsed(source: &str) -> Vec<(Range<usize>, String, InlineStyle)> {
    let body_start = Parser::new_ext(source, Options::ENABLE_YAML_STYLE_METADATA_BLOCKS)
        .into_offset_iter()
        .next()
        .filter(|(event, range)| {
            range.start == 0 && matches!(event, Event::Start(Tag::MetadataBlock(_)))
        })
        .map_or(0, |(_, range)| range.end);
    let projected = crate::native_link_parser::Projected::new(&source[body_start..]);
    let mut depth = 0usize;
    let mut table = None::<(Range<usize>, Table)>;
    let (mut unsupported, mut emphasis, mut strong) = (false, 0usize, 0usize);
    let mut link = None;
    let mut cell_start = 0;
    let mut values = Vec::new();
    for (event, range) in Parser::new_ext(
        &projected.text,
        Options::ENABLE_TABLES | Options::ENABLE_MATH,
    )
    .into_offset_iter()
    {
        let range = projected.range(range);
        let range = range.start + body_start..range.end + body_start;
        match &event {
            Event::Start(Tag::Table(alignments)) if depth == 0 => {
                table = Some((
                    source[..range.start]
                        .rfind('\n')
                        .map_or(0, |offset| offset + 1)..range.end,
                    Table {
                        alignments: alignments
                            .iter()
                            .map(|alignment| match alignment {
                                Alignment::Center => "center",
                                Alignment::Right => "right",
                                _ => "left",
                            })
                            .collect(),
                        rows: Vec::new(),
                        fragments: Vec::new(),
                    },
                ));
                unsupported = false;
                emphasis = 0;
                strong = 0;
                link = None;
            }
            Event::End(TagEnd::Table) if table.is_some() => {
                let (mut range, mut table) = table.take().unwrap();
                while range.end > range.start
                    && matches!(source.as_bytes()[range.end - 1], b'\r' | b'\n')
                {
                    range.end -= 1;
                }
                if !unsupported && !table.rows.is_empty() && !range.is_empty() {
                    crate::native_link_parser::relative(&mut table.fragments, source, range.start);
                    let label = table
                        .fragments
                        .iter()
                        .map(|fragment| fragment.text.as_str())
                        .collect();
                    values.push((range, label, InlineStyle::Table(Rc::new(table))));
                }
            }
            _ => {
                if let Some((_, table)) = table.as_mut() {
                    match &event {
                        Event::Start(Tag::TableHead | Tag::TableRow) => table.rows.push(Vec::new()),
                        Event::End(TagEnd::TableHead | TagEnd::TableRow) => {}
                        Event::Start(Tag::TableCell) => {
                            cell_start = table.fragments.len();
                            emphasis = 0;
                            strong = 0;
                            link = None;
                        }
                        Event::End(TagEnd::TableCell) => {
                            table
                                .rows
                                .last_mut()
                                .unwrap()
                                .push(cell_start..table.fragments.len());
                        }
                        Event::Start(Tag::Emphasis) => emphasis += 1,
                        Event::End(TagEnd::Emphasis) => emphasis = emphasis.saturating_sub(1),
                        Event::Start(Tag::Strong) => strong += 1,
                        Event::End(TagEnd::Strong) => strong = strong.saturating_sub(1),
                        Event::Start(Tag::Link {
                            dest_url, title, ..
                        }) => {
                            link = projected.link(dest_url, title, range.clone());
                            unsupported |= link.is_none();
                        }
                        Event::End(TagEnd::Link) => link = None,
                        Event::InlineMath(tex) => table.fragments.push(InlineFragment {
                            text: source[range.clone()].to_string(),
                            math: crate::native_math_parser::value(source, &range, tex, false),
                            code: false,
                            link: link.clone(),
                            emphasis: emphasis > 0,
                            strong: strong > 0,
                        }),
                        Event::Text(text) | Event::Code(text) => {
                            table.fragments.push(InlineFragment {
                                text: text.to_string(),
                                math: None,
                                emphasis: emphasis > 0,
                                strong: strong > 0,
                                code: matches!(event, Event::Code(_)),
                                link: link.clone(),
                            })
                        }
                        _ => unsupported = true,
                    }
                }
            }
        }
        match event {
            Event::Start(_) => depth += 1,
            Event::End(_) => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    values
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn aligned_cells_preserve_unicode_crlf_and_literal_pipes() {
        let source = "😀\r\n\r\n| **猫** | `a\\|b` | *right* |\r\n| :--- | :---: | ---: |\r\n| &amp; | | [[Note|Alias]] |\r\n\r\nend\r\n";
        let values = previews(source);
        assert_eq!(values.len(), 1);
        assert!(source[values[0].0.clone()].starts_with("| **猫**"));
        assert!(!source[values[0].0.clone()].ends_with('\n'));
        let InlineStyle::Table(table) = &values[0].2 else {
            panic!("table")
        };
        assert_eq!(table.alignments, ["left", "center", "right"]);
        assert_eq!(table.rows.len(), 2);
        assert_eq!(table.rows[1].len(), 3);
        assert!(table.fragments.iter().any(|f| f.code && f.text == "a|b"));
        assert!(table.fragments.iter().any(|f| f.strong && f.text == "猫"));
        assert!(table
            .fragments
            .iter()
            .any(|f| f.text == "Alias" && f.link.is_some()));
    }
    #[test]
    fn unsupported_containers_images_html_and_front_matter_keep_source() {
        for source in [
            "> a | b\n> --- | ---\n> c | d\n",
            "- a | b\n  --- | ---\n  c | d\n",
            "a | b\n--- | ---\n![x](pic.png) | d\n",
            "a | b\n--- | ---\n<i>x</i> | d\n",
            "---\na: x | y\n---\n",
            "```\na | b\n--- | ---\n```\n",
        ] {
            assert!(previews(source).is_empty(), "{source}");
        }
    }
    #[test]
    fn indentation_empty_cells_and_ragged_rows_keep_full_source_ownership() {
        let source =
            "  | a | b |\r\n  | --- | --- |\r\n  | one |\r\n  | two | three | ignored |\r\n\r\nend";
        let values = previews(source);
        assert_eq!(values.len(), 1);
        assert_eq!(values[0].0.start, 0);
        assert!(source[values[0].0.clone()].ends_with("ignored |"));
        let InlineStyle::Table(table) = &values[0].2 else {
            panic!("table")
        };
        assert!(table.rows.iter().all(|row| row.len() == 2));
        assert!(!table
            .fragments
            .iter()
            .any(|fragment| fragment.text.contains("ignored")));
        let empty = previews("| | |\n| --- | --- |\n| | |\n");
        assert_eq!(empty.len(), 1);
        assert!(empty[0].1.is_empty());
    }
}
