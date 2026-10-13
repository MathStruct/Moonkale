//! Canonical math ranges; TeX is presentation data, never Markdown projection.
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::ops::Range;
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Math {
    pub tex: String,
    pub display: bool,
}
pub(crate) const MAX_TEX: usize = 16 * 1024;
pub(crate) fn value(source: &str, range: &Range<usize>, tex: &str, display: bool) -> Option<Math> {
    if tex.len() > MAX_TEX
        || (!display
            && source
                .as_bytes()
                .get(range.end)
                .is_some_and(u8::is_ascii_digit))
    {
        return None;
    }
    Some(Math {
        tex: tex.into(),
        display,
    })
}
pub(crate) fn displays(source: &str) -> Vec<(Range<usize>, Math)> {
    let body_start = Parser::new_ext(source, Options::ENABLE_YAML_STYLE_METADATA_BLOCKS)
        .into_offset_iter()
        .next()
        .filter(|(event, range)| {
            range.start == 0 && matches!(event, Event::Start(Tag::MetadataBlock(_)))
        })
        .map_or(0, |(_, range)| range.end);
    let mut depth = 0usize;
    let mut paragraph = None;
    let mut math = None;
    let mut values = Vec::new();
    for (event, range) in Parser::new_ext(
        &source[body_start..],
        Options::ENABLE_MATH | Options::ENABLE_TABLES,
    )
    .into_offset_iter()
    {
        let range = range.start + body_start..range.end + body_start;
        match &event {
            Event::Start(Tag::Paragraph) if depth == 0 => {
                paragraph = Some(range.clone());
                math = None;
            }
            Event::DisplayMath(tex) if paragraph.is_some() => {
                math = Some((range.clone(), value(source, &range, tex, true)));
            }
            Event::End(TagEnd::Paragraph) if depth == 1 => {
                if let Some(mut whole) = paragraph.take() {
                    if let Some((range, Some(math))) = math.take() {
                        if source[whole.clone()].trim() == source[range].trim() {
                            whole.start =
                                source[..whole.start].rfind('\n').map_or(0, |byte| byte + 1);
                            while whole.end > whole.start
                                && matches!(source.as_bytes()[whole.end - 1], b'\r' | b'\n')
                            {
                                whole.end -= 1;
                            }
                            values.push((whole, math));
                        }
                    }
                }
            }
            _ => {}
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
    fn display_blocks_keep_unicode_crlf_and_delimiters() {
        let source = "😀\r\n\r\n  $$\r\n\\frac{a}{b}\r\n$$\r\n\r\nend";
        let values = displays(source);
        assert_eq!(values.len(), 1);
        assert_eq!(&source[values[0].0.clone()], "  $$\r\n\\frac{a}{b}\r\n$$");
        assert!(values[0].1.tex.contains("\\frac{a}{b}"));
    }
    #[test]
    fn complex_display_code_metadata_and_oversized_tex_keep_source() {
        for source in [
            "before $$x$$ after",
            "# $$x$$",
            "- $$x$$",
            "> $$x$$",
            "`$$x$$`",
            "```\n$$x$$\n```",
            "---\ntitle: $$x$$\n---",
            "$$unclosed",
        ] {
            assert!(displays(source).is_empty(), "{source}");
        }
        assert!(displays(&format!("$${}$$", "x".repeat(MAX_TEX + 1))).is_empty());
        assert!(value("$5,$10", &(0..4), "5,", false).is_none());
    }
}
