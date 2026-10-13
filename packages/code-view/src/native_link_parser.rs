//! Wiki syntax projects to CommonMark only for parsing. Canonical bytes stay intact.
use crate::{native_decorations::InlineFragment, native_links::Target};
use pulldown_cmark::{Event, LinkType, Options, Parser, Tag};
use std::{borrow::Cow, ops::Range};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Link {
    pub target: Target,
    /// Scalars relative to the enclosing source replacement after `relative`.
    pub range: Range<usize>,
    pub title: String,
}

pub(crate) struct Projected<'a> {
    pub text: Cow<'a, str>,
    map: Option<Vec<usize>>,
    wiki: Vec<Target>,
    prefix: String,
}

impl<'a> Projected<'a> {
    pub(crate) fn new(source: &'a str) -> Self {
        if !source.contains("[[") {
            return Self {
                text: Cow::Borrowed(source),
                map: None,
                wiki: Vec::new(),
                prefix: "moonkale-preview-wiki:".into(),
            };
        }
        let raw_wiki = moonkale_ext_api::wiki::spans(source);
        let mut blocked = Vec::new();
        let mut ancestors = Vec::new();
        for (event, range) in Parser::new_ext(
            source,
            Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
                | Options::ENABLE_TASKLISTS
                | Options::ENABLE_MATH,
        )
        .into_offset_iter()
        {
            match event {
                Event::Start(tag) => {
                    if matches!(
                        tag,
                        Tag::CodeBlock(_)
                            | Tag::Image { .. }
                            | Tag::HtmlBlock
                            | Tag::MetadataBlock(_)
                    ) || matches!(
                        tag,
                        Tag::Link {
                            link_type: LinkType::Inline | LinkType::Autolink | LinkType::Email,
                            ..
                        }
                    ) {
                        blocked.push(range.clone());
                    }
                    ancestors.push(range);
                }
                Event::End(_) => {
                    ancestors.pop();
                }
                Event::Code(_) | Event::InlineMath(_) | Event::DisplayMath(_) => blocked.push(range),
                Event::InlineHtml(_) | Event::Html(_)
                    // Alias contents are literal text, even when they look like
                    // HTML. Actual surrounding HTML still blocks projection.
                    if !raw_wiki.iter().any(|(start, end, _, _, _)| {
                        *start <= range.start
                            && range.end <= *end
                            && source[..*start]
                                .bytes()
                                .rev()
                                .take_while(|b| *b == b'\\')
                                .count()
                                % 2
                                == 0
                            && (*start == 0 || source.as_bytes()[start - 1] != b'!')
                    }) => {
                        blocked.push(ancestors.last().cloned().unwrap_or(range));
                }
                _ => {}
            }
        }
        let mut prefix = "moonkale-preview-wiki:".to_string();
        while source.contains(&prefix) {
            prefix.push('x');
        }
        let mut text = String::new();
        let mut map = vec![0];
        let mut previous = 0;
        let mut wiki = Vec::new();
        for (start, end, page, heading, alias) in raw_wiki {
            let escapes = source[..start]
                .bytes()
                .rev()
                .take_while(|b| *b == b'\\')
                .count();
            if escapes % 2 != 0
                || start > 0 && source.as_bytes()[start - 1] == b'!'
                || blocked.iter().any(|r| r.start < end && start < r.end)
            {
                continue;
            }
            text.push_str(&source[previous..start]);
            map.extend(previous + 1..=start);
            let label = alias.unwrap_or_else(|| {
                format!(
                    "{page}{}",
                    heading
                        .as_ref()
                        .map(|h| format!("#{h}"))
                        .unwrap_or_default()
                )
            });
            let escaped = label
                .chars()
                .flat_map(|ch| {
                    if ch.is_ascii_punctuation() {
                        vec!['\\', ch]
                    } else {
                        vec![ch]
                    }
                })
                .collect::<String>();
            let projected = format!("[{escaped}]({prefix}{})", wiki.len());
            text.push_str(&projected);
            map.extend(std::iter::repeat_n(start, projected.len()));
            *map.last_mut().unwrap() = end;
            wiki.push(Target::Wiki { page, heading });
            previous = end;
        }
        if wiki.is_empty() {
            return Self {
                text: Cow::Borrowed(source),
                map: None,
                wiki,
                prefix,
            };
        }
        text.push_str(&source[previous..]);
        map.extend(previous + 1..=source.len());
        Self {
            text: Cow::Owned(text),
            map: Some(map),
            wiki,
            prefix,
        }
    }

    pub(crate) fn range(&self, range: Range<usize>) -> Range<usize> {
        self.map
            .as_ref()
            .map_or(range.clone(), |map| map[range.start]..map[range.end])
    }

    pub(crate) fn link(&self, destination: &str, title: &str, range: Range<usize>) -> Option<Link> {
        let target = if let Some(index) = destination
            .strip_prefix(&self.prefix)
            .and_then(|s| s.parse::<usize>().ok())
        {
            self.wiki.get(index)?.clone()
        } else {
            Target::markdown(destination)?
        };
        // Link destinations themselves were blocked above only to exclude wiki
        // lexing. HTML/code contexts are rejected by the presentation frame.
        Some(Link {
            target,
            range,
            title: title.to_string(),
        })
    }
}

pub(crate) fn relative(fragments: &mut [InlineFragment], source: &str, owner: usize) {
    for fragment in fragments {
        if let Some(link) = &mut fragment.link {
            link.range = source[owner..link.range.start].chars().count()
                ..source[owner..link.range.end].chars().count();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wiki_projection_preserves_original_utf8_token_ranges_and_literal_aliases() {
        let source = "😀 before [[Note#Part|猫 <safe> & *literal*]] after [ref](file.md).";
        let projection = Projected::new(source);
        let mut links = Vec::new();
        for (event, range) in Parser::new(&projection.text).into_offset_iter() {
            if let Event::Start(Tag::Link {
                dest_url, title, ..
            }) = event
            {
                let original = projection.range(range);
                links.push((
                    source[original.clone()].to_string(),
                    projection.link(&dest_url, &title, original).unwrap(),
                ));
            }
        }
        assert_eq!(links[0].0, "[[Note#Part|猫 <safe> & *literal*]]");
        assert_eq!(links[1].0, "[ref](file.md)");
        assert!(
            matches!(&links[0].1.target,Target::Wiki {page,heading:Some(heading)} if page=="Note" && heading=="Part")
        );
        assert_eq!(
            Parser::new(&projection.text)
                .filter_map(|e| if let Event::Text(text) = e {
                    Some(text.to_string())
                } else {
                    None
                })
                .collect::<String>(),
            "😀 before 猫 <safe> & *literal* after ref."
        );
    }
    #[test]
    fn projection_excludes_code_metadata_html_images_and_escaped_wiki_syntax() {
        for source in [
            "`[[Code]]`",
            "```\n[[Fence]]\n```",
            "    [[Indented]]",
            "---\ntitle: '[[Metadata]]'\n---",
            r"\[[Escaped]]",
            "![[Embed]]",
            "<div>\n[[Html]]\n</div>",
            "![alt [[Image]]](url)",
            "[label](https://example.org/[[Path]])",
        ] {
            let projected = Projected::new(source);
            assert!(projected.map.is_none(), "{source}");
        }
        assert!(Projected::new("[[#Local]]").map.is_some());
    }
}
