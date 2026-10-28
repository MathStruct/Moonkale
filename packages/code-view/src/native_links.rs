//! Link destinations and guarded navigation; the platform opens external URLs.
use dioxus::prelude::*;
use moonkale_core::NodeId;
use moonkale_ext_api::{t, Workspace};

/// Register through `WorkspaceConfig::services`. Called from the user gesture.
pub struct ExternalLinkOpener(pub fn(String) -> Result<(), String>);

#[cfg_attr(not(feature = "markdown-preview"), allow(dead_code))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Target {
    Wiki {
        page: String,
        heading: Option<String>,
    },
    File {
        path: String,
        heading: Option<String>,
    },
    External(String),
}

#[cfg_attr(not(feature = "markdown-preview"), allow(dead_code))]
impl Target {
    pub(crate) fn markdown(value: &str) -> Option<Self> {
        if value.starts_with("//") {
            return Self::markdown(&format!("https:{value}"));
        }
        if let Ok(url) = url::Url::parse(value) {
            return matches!(url.scheme(), "http" | "https" | "mailto")
                .then(|| Self::External(url.to_string()));
        }
        if value
            .split('/')
            .next()
            .is_some_and(|part| part.contains(':'))
        {
            return None;
        }
        let (path, heading) = value
            .split_once('#')
            .map_or((value, None), |(a, b)| (a, Some(b)));
        let path = decode(path.split('?').next().unwrap_or(path))?;
        let heading = heading.map(str::to_string);
        (!path.is_empty() || heading.is_some()).then_some(Self::File { path, heading })
    }

    pub(crate) fn description(&self) -> String {
        match self {
            Self::External(url) => url.clone(),
            Self::File {
                path: page,
                heading,
            }
            | Self::Wiki { page, heading } => {
                format!(
                    "{page}{}",
                    heading
                        .as_ref()
                        .map(|h| format!("#{h}"))
                        .unwrap_or_default()
                )
            }
        }
    }
}

#[cfg_attr(not(feature = "markdown-preview"), allow(dead_code))]
fn decode(value: &str) -> Option<String> {
    let mut bytes = Vec::new();
    let mut source = value.bytes();
    while let Some(byte) = source.next() {
        if byte == b'%' {
            let hi = (source.next()? as char).to_digit(16)?;
            let lo = (source.next()? as char).to_digit(16)?;
            bytes.push((hi * 16 + lo) as u8);
        } else {
            bytes.push(byte);
        }
    }
    String::from_utf8(bytes)
        .ok()
        .filter(|value| !value.chars().any(|ch| ch.is_control()))
}

pub(crate) fn file_path(from: &str, path: &str) -> Option<String> {
    if path.contains('\\') {
        return None;
    }
    let base = if path.starts_with('/') {
        ""
    } else {
        from.rsplit_once('/').map_or("", |(dir, _)| dir)
    };
    let joined = format!("{base}/{path}");
    let mut parts = Vec::new();
    for part in joined.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    Some(parts.join("/"))
}

#[cfg(feature = "markdown-preview")]
pub(crate) fn heading_position(text: &str, fragment: &str) -> Option<(u32, u32)> {
    crate::markdown_guard::parse(text, || heading_position_parsed(text, fragment)).flatten()
}

#[cfg(feature = "markdown-preview")]
fn heading_position_parsed(text: &str, fragment: &str) -> Option<(u32, u32)> {
    use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
    let fragment = decode(fragment)?;
    if fragment.is_empty() {
        return Some((0, 0));
    }
    let mut current = None;
    let mut seen = std::collections::HashMap::<String, usize>::new();
    let body_start = Parser::new_ext(text, Options::ENABLE_YAML_STYLE_METADATA_BLOCKS)
        .into_offset_iter()
        .next()
        .filter(|(event, range)| {
            range.start == 0 && matches!(event, Event::Start(Tag::MetadataBlock(_)))
        })
        .map_or(0, |(_, range)| range.end);
    let projected = crate::native_link_parser::Projected::new(&text[body_start..]);
    for (event, range) in Parser::new(&projected.text).into_offset_iter() {
        let range = projected.range(range);
        let range = range.start + body_start..range.end + body_start;
        match event {
            Event::Start(Tag::Heading { .. }) => current = Some((range.start, String::new())),
            Event::Text(value) | Event::Code(value) => {
                if let Some((_, label)) = &mut current {
                    label.push_str(&value);
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if let Some((_, label)) = &mut current {
                    label.push(' ');
                }
            }
            Event::End(TagEnd::Heading(_)) => {
                let (start, label) = current.take()?;
                let slug = label
                    .to_lowercase()
                    .chars()
                    .filter(|c| c.is_alphanumeric() || c.is_whitespace() || matches!(c, '-' | '_'))
                    .map(|c| if c.is_whitespace() { '-' } else { c })
                    .collect::<String>();
                let count = seen.entry(slug.clone()).or_default();
                let slug = if *count == 0 {
                    slug
                } else {
                    format!("{slug}-{count}")
                };
                *count += 1;
                if slug == fragment || label.to_lowercase() == fragment.to_lowercase() {
                    return Some((
                        text[..start].bytes().filter(|byte| *byte == b'\n').count() as u32,
                        0,
                    ));
                }
            }
            _ => {}
        }
    }
    None
}

pub(crate) fn use_follow(ws: Workspace, node: NodeId) -> Callback<Target> {
    Callback::new(move |target: Target| {
        if *ws.docs.active.peek() != Some(node) {
            return;
        }
        let Some(doc) = ws.document(node) else {
            return;
        };
        let from = doc.peek().node.clone();
        let text = doc.peek().text.clone();
        let session = ws.editor_session(node).expect("open session");
        let (revision, selection) = {
            let state = session.peek();
            (state.snapshot().revision, state.snapshot().selection)
        };
        if let Target::External(url) = target {
            let result = ws
                .service::<ExternalLinkOpener>()
                .ok_or_else(|| "No external link opener".to_string())
                .and_then(|opener| (opener.0)(url));
            if let Err(error) = result {
                let mut ws = ws;
                ws.set_status(t!(ws, crate::L, "editor-link-failed", error = error));
            }
            return;
        }
        moonkale_ext_api::crash::spawn(async move {
            let origin_current = || {
                doc.try_peek()
                    .is_ok_and(|doc| doc.node == from && doc.text == text)
                    && session.try_peek().is_ok_and(|session| {
                        session.snapshot().revision == revision
                            && session.snapshot().selection == selection
                    })
            };
            let current = || origin_current() && *ws.docs.active.peek() == Some(node);
            let (page, heading, wiki) = match target {
                Target::Wiki { page, heading } => (page, heading, true),
                Target::File { path, heading } => (path, heading, false),
                Target::External(_) => unreachable!(),
            };
            let resolved = if page.is_empty() {
                Some(from.clone())
            } else if wiki {
                ws.resolve_wiki(&from, &page).await
            } else if let Some(path) = file_path(&from.native_key, &page) {
                ws.node_at_path(&from.source, &path).await
            } else {
                None
            };
            if !current() {
                return;
            }
            let result = async {
                let target = match resolved {
                    Some(target) => target,
                    None if wiki => {
                        let created = ws.follow_wiki_guarded(&from, &page, true, current).await?;
                        #[cfg(feature = "markdown-preview")]
                        if let (Some(target), Some(heading)) = (created, heading) {
                            if let Some(target_doc) = ws.document(target.id) {
                                let target_text = target_doc.peek().text.clone();
                                let position = heading_position(&target_text, &heading);
                                let guard = || {
                                    origin_current()
                                        && *ws.docs.active.peek() == Some(target.id)
                                        && target_doc
                                            .try_peek()
                                            .is_ok_and(|doc| doc.text == target_text)
                                };
                                if guard() {
                                    let (line, col) = position.unwrap_or((0, 0));
                                    ws.reveal_guarded(target.clone(), line, col, guard).await?;
                                    if position.is_none() {
                                        let mut ws = ws;
                                        ws.set_status(t!(
                                            ws,
                                            crate::L,
                                            "editor-link-heading-missing",
                                            heading = heading
                                        ));
                                    }
                                }
                            }
                        }
                        #[cfg(not(feature = "markdown-preview"))]
                        let _ = created;
                        return Ok(());
                    }
                    None => return Err(moonkale_core::SourceError::NotFound),
                };
                if !ws.load_node_guarded(target.clone(), current).await? {
                    return Ok(());
                }
                #[cfg(feature = "markdown-preview")]
                if let Some(heading) = heading {
                    if let Some(target_doc) = ws.document(target.id) {
                        let target_text = target_doc.peek().text.clone();
                        let position = heading_position(&target_text, &heading);
                        let guarded = || {
                            current()
                                && target_doc
                                    .try_peek()
                                    .is_ok_and(|doc| doc.text == target_text)
                        };
                        if let Some((line, col)) = position {
                            ws.reveal_guarded(target, line, col, guarded).await?;
                        } else if guarded() {
                            ws.reveal_guarded(target, 0, 0, guarded).await?;
                            let mut ws = ws;
                            ws.set_status(t!(
                                ws,
                                crate::L,
                                "editor-link-heading-missing",
                                heading = heading
                            ));
                        }
                        return Ok(());
                    }
                }
                #[cfg(not(feature = "markdown-preview"))]
                let _ = heading;
                ws.reveal_guarded(target, 0, 0, current).await?;
                Ok(())
            }
            .await;
            if let Err(error) = result {
                let mut ws = ws;
                ws.set_status(t!(
                    ws,
                    crate::L,
                    "editor-link-failed",
                    error = error.to_string()
                ));
            }
        });
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn destinations_decode_paths_once_and_confine_relative_navigation() {
        assert_eq!(
            Target::markdown("notes/Caf%C3%A9.md#100%25"),
            Some(Target::File {
                path: "notes/Café.md".into(),
                heading: Some("100%25".into())
            })
        );
        assert_eq!(
            file_path("notes/current.md", "../Other.md"),
            Some("Other.md".into())
        );
        assert_eq!(file_path("notes/current.md", "../../Other.md"), None);
        assert_eq!(
            file_path("notes/current.md", "/Root.md"),
            Some("Root.md".into())
        );
        for url in [
            "javascript:alert(1)",
            "data:text/html,hi",
            "file:///etc/passwd",
            "note%00.md",
            "%ff.md",
        ] {
            assert!(Target::markdown(url).is_none(), "{url}");
        }
        for url in [
            "https://example.org/path?q=1#part",
            "http://example.org",
            "mailto:a@example.org",
            "//example.org",
        ] {
            assert!(
                matches!(Target::markdown(url), Some(Target::External(_))),
                "{url}"
            );
        }
    }
    #[cfg(feature = "markdown-preview")]
    #[test]
    fn anchors_use_rendered_heading_text_unsaved_source_unicode_and_duplicate_slugs() {
        let text="---\r\ntitle: Hidden\r\n---\r\n\r\n# Hello **猫**\r\n\r\n## Duplicate\r\n\r\n## Duplicate\r\n\r\n# [[Note|Alias]]\r\n# 100%\r\n";
        assert_eq!(heading_position(text, "hello-%E7%8C%AB"), Some((4, 0)));
        assert_eq!(heading_position(text, "duplicate-1"), Some((8, 0)));
        assert_eq!(heading_position(text, "Hello 猫"), Some((4, 0)));
        assert_eq!(heading_position(text, "alias"), Some((10, 0)));
        assert_eq!(heading_position(text, "100%25"), Some((11, 0)));
        assert_eq!(heading_position(text, "Hidden"), None);
        assert_eq!(heading_position(text, "missing"), None);
    }
}
