//! Standalone source-owned image paragraphs; unsupported placements stay source.
#[cfg(feature = "markdown-preview")]
use crate::native_links::Target;
#[cfg(feature = "markdown-preview")]
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
#[cfg(feature = "markdown-preview")]
use std::ops::Range;

#[cfg_attr(not(feature = "markdown-preview"), allow(dead_code))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Destination {
    File(String),
    Wiki(String),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Image {
    pub destination: Destination,
    pub alt: String,
    pub title: String,
}
#[cfg_attr(not(feature = "markdown-preview"), allow(dead_code))]
impl Image {
    pub(crate) fn label(&self) -> String {
        if !self.alt.is_empty() {
            return self.alt.clone();
        }
        match &self.destination {
            Destination::File(path) | Destination::Wiki(path) => {
                path.rsplit('/').next().unwrap_or(path).to_string()
            }
        }
    }
}
#[cfg(feature = "markdown-preview")]
fn supported(path: &str) -> bool {
    path.rsplit('.').next().is_some_and(|ext| {
        moonkale_ext_api::media::image::IMAGE_EXTENSIONS
            .contains(&ext.to_ascii_lowercase().as_str())
    })
}
#[cfg(feature = "markdown-preview")]
fn wiki_image(source: &str) -> Option<Image> {
    let token = source.trim();
    if !token.starts_with("![[") {
        return None;
    }
    let values = moonkale_ext_api::wiki::spans(&token[1..]);
    let [(start, end, page, heading, alias)] = values.as_slice() else {
        return None;
    };
    if *start != 0
        || *end != token.len() - 1
        || heading.is_some()
        || !supported(page)
        || page.starts_with("//")
        || url::Url::parse(page).is_ok()
        || page
            .split('/')
            .next()
            .is_some_and(|part| part.contains(':'))
    {
        return None;
    }
    if alias.as_ref().is_some_and(|alias| {
        !alias.is_empty() && alias.chars().all(|ch| ch.is_ascii_digit() || ch == 'x')
    }) {
        return None;
    }
    Some(Image {
        destination: Destination::Wiki(page.clone()),
        alt: alias.clone().unwrap_or_default(),
        title: String::new(),
    })
}
#[cfg(feature = "markdown-preview")]
pub(crate) fn previews(source: &str) -> Vec<(Range<usize>, Image)> {
    crate::markdown_guard::parse(source, || previews_parsed(source)).unwrap_or_default()
}

#[cfg(feature = "markdown-preview")]
fn previews_parsed(source: &str) -> Vec<(Range<usize>, Image)> {
    let mut depth = 0usize;
    let mut paragraph = None::<Range<usize>>;
    let mut image = None::<Image>;
    let mut inside = false;
    let mut unsupported = false;
    let mut count = 0;
    let mut values = Vec::new();
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
            Event::Start(Tag::Paragraph) if depth == 0 => {
                paragraph =
                    Some(source[..range.start].rfind('\n').map_or(0, |byte| byte + 1)..range.end);
                image = None;
                count = 0;
                unsupported = false;
                inside = false;
                depth += 1;
            }
            Event::Start(Tag::Image {
                dest_url, title, ..
            }) if paragraph.is_some() => {
                count += 1;
                inside = true;
                image = match Target::markdown(&dest_url) {
                    Some(Target::File {
                        path,
                        heading: None,
                    }) if supported(&path) => Some(Image {
                        destination: Destination::File(path),
                        alt: String::new(),
                        title: title.into_string(),
                    }),
                    _ => None,
                };
                depth += 1;
            }
            Event::Start(tag) => {
                if paragraph.is_some() && !(inside && matches!(tag, Tag::Emphasis | Tag::Strong)) {
                    unsupported = true;
                }
                depth += 1;
            }
            Event::End(TagEnd::Image) if paragraph.is_some() => {
                inside = false;
                depth = depth.saturating_sub(1);
            }
            Event::End(TagEnd::Paragraph) if paragraph.is_some() && depth == 1 => {
                let mut range = paragraph.take().unwrap();
                while range.end > range.start
                    && matches!(source.as_bytes()[range.end - 1], b'\n' | b'\r')
                {
                    range.end -= 1;
                }
                let candidate = wiki_image(&source[range.clone()]).or_else(|| {
                    if count == 1 && !unsupported {
                        image.take()
                    } else {
                        None
                    }
                });
                if let Some(image) = candidate {
                    values.push((range, image));
                }
                depth -= 1;
            }
            Event::End(_) => {
                depth = depth.saturating_sub(1);
            }
            Event::Text(text) | Event::Code(text) if paragraph.is_some() => {
                if inside {
                    if let Some(image) = &mut image {
                        image.alt.push_str(&text);
                    }
                } else if !text.trim().is_empty() {
                    unsupported = true;
                }
            }
            Event::SoftBreak | Event::HardBreak if paragraph.is_some() => {
                if inside {
                    if let Some(image) = &mut image {
                        image.alt.push(' ');
                    }
                } else {
                    unsupported = true;
                }
            }
            Event::Html(_) | Event::InlineHtml(_) if paragraph.is_some() => {
                unsupported = true;
            }
            _ => {}
        }
    }
    values
}
#[cfg(all(test, feature = "markdown-preview"))]
mod tests {
    use super::*;
    #[test]
    fn standalone_images_and_references_keep_canonical_ranges() {
        let source="😀\r\n\r\n![**猫**](pics/Caf%C3%A9.png \"Title\")\r\n\r\n![[logo.svg|<safe>]]\r\n\r\n![ref][id]\r\n\r\n[id]: pic.png\r\n";
        let result = previews(source);
        assert_eq!(result.len(), 3);
        assert_eq!(
            &source[result[0].0.clone()],
            "![**猫**](pics/Caf%C3%A9.png \"Title\")"
        );
        assert_eq!(result[0].1.alt, "猫");
        assert_eq!(
            result[0].1.destination,
            Destination::File("pics/Café.png".into())
        );
        assert_eq!(result[1].1.alt, "<safe>");
        assert_eq!(result[2].1.destination, Destination::File("pic.png".into()));
    }
    #[test]
    fn complex_remote_and_nonimage_constructs_stay_source() {
        for source in [
            "before ![alt](pic.png)",
            "# ![alt](pic.png)",
            "- ![alt](pic.png)",
            "> ![alt](pic.png)",
            "[![alt](pic.png)](Note.md)",
            "![remote](https://example.org/pic.png)",
            "![[https://example.org/pic.png]]",
            "![[//example.org/pic.png]]",
            "![[data:image/png;base64,pic.png]]",
            "![[Note.md]]",
            "![[pic.png|200]]",
            "![[pic.png#part]]",
            "`![[pic.png]]`",
            "```\n![alt](pic.png)\n```",
            "---\ntitle: ![[pic.png]]\n---\n",
        ] {
            assert!(previews(source).is_empty(), "{source}");
        }
    }
}
