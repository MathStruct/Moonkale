//! Read-only wiki excerpts; neither resolution nor fetching opens a document.
use crate::native_links::Target;
use dioxus::prelude::*;
use futures_util::{
    future::{select, Either},
    FutureExt,
};
use moonkale_core::{ContentRef, Node, NodeId};
use moonkale_ext_api::{editor::DocumentRevision, t, Workspace};
use std::time::Duration;

const MAX_TEXT: usize = 256 * 1024;
#[cfg(feature = "markdown-preview")]
const MAX_EXCERPT: usize = 1200;
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Request {
    pub revision: DocumentRevision,
    pub identity: String,
    pub target: Target,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Token {
    start: (usize, usize),
    end: (usize, usize),
    target: Target,
}
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(not(feature = "markdown-preview"), allow(dead_code))]
pub(crate) enum Status {
    Loading,
    Missing,
    Unavailable,
    TooLarge,
    HeadingMissing,
    Ready,
}
#[derive(Clone, PartialEq)]
pub(crate) struct Popup {
    pub title: String,
    pub text: String,
    pub status: Status,
    pub heading: Option<String>,
    pub point: (f64, f64),
}
#[derive(Clone)]
struct ResultValue {
    request: Request,
    epoch: u64,
    origin: Node,
    node: Option<Node>,
    from_open: bool,
    status: Status,
    text: String,
}
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct Control {
    pub rendered: Signal<Option<Request>>,
    pub point: Signal<(f64, f64)>,
    pub inside: Signal<bool>,
    pointer_active: Signal<bool>,
    source_position: Signal<Option<(usize, usize)>>,
    pub popup: Memo<Option<Popup>>,
    desired: Signal<Option<Request>>,
    source: Signal<Option<(usize, usize)>>,
}
impl Control {
    pub(crate) fn leave_text(mut self) {
        self.pointer_active.set(false);
        self.source_position.set(None);
        self.rendered.set(None);
    }
    pub(crate) fn leave(mut self) {
        self.leave_text();
        self.source.set(None);
        self.inside.set(false);
    }
    pub(crate) fn dismiss(mut self) {
        self.leave();
        self.desired.set(None);
    }
    pub(crate) fn pointer(
        mut self,
        request: Option<Request>,
        point: (f64, f64),
        position: Option<(usize, usize)>,
    ) {
        if *self.source_position.peek() != position {
            self.source_position.set(position);
        }
        if !*self.pointer_active.peek() {
            self.pointer_active.set(true);
        }
        if *self.rendered.peek() != request {
            self.point.set(point);
            self.rendered.set(request);
        } else if self.rendered.peek().is_none() {
            self.point.set(point);
        }
    }
}

pub(crate) fn use_hover(
    ws: Workspace,
    node: NodeId,
    source: Signal<Option<(usize, usize)>>,
) -> Control {
    let doc = ws.document(node).expect("open document");
    let session = ws.editor_session(node).expect("open session");
    let revision = use_memo(move || session.read().snapshot().revision);
    let from = use_memo(move || doc.read().node.clone());
    let pointer_active = use_signal(|| false);
    let source_position = use_signal(|| None::<(usize, usize)>);
    let rendered = use_signal(|| None::<Request>);
    let source_enabled =
        use_memo(move || pointer_active() && source_position().is_some() && rendered().is_none());
    // Retain source tokens for this revision across neighboring cell exits.
    // The disabled branch reads no document signals, so typing stays dormant.
    let token_cache = use_hook(|| {
        std::rc::Rc::new(std::cell::RefCell::new(
            None::<(DocumentRevision, std::rc::Rc<Vec<Token>>)>,
        ))
    });
    let tokens = use_memo(move || {
        if !source_enabled() {
            return token_cache
                .borrow()
                .as_ref()
                .map(|(_, values)| values.clone())
                .unwrap_or_default();
        }
        let doc = doc.read();
        if doc.node.language_hint() != Some("markdown") {
            return std::rc::Rc::new(Vec::new());
        }
        let revision = revision();
        if let Some((_, values)) = token_cache
            .borrow()
            .as_ref()
            .filter(|(cached, _)| *cached == revision)
        {
            return values.clone();
        }
        let values = std::rc::Rc::new(tokens(&doc.text));
        *token_cache.borrow_mut() = Some((revision, values.clone()));
        values
    });
    let point = use_signal(|| (0.0, 0.0));
    let inside = use_signal(|| false);
    let mut desired = use_signal(|| None::<Request>);
    let mut anchor = use_signal(|| (0.0, 0.0));
    let wanted = use_memo(move || {
        if !pointer_active()
            || from().language_hint() != Some("markdown")
            || *ws.docs.active.read() != Some(node)
        {
            return None;
        }
        if let Some(request) = rendered().filter(|r| r.revision == revision()) {
            return Some(request);
        }
        let position = source_position()?;
        let tokens = tokens.read();
        let index = tokens.partition_point(|token| token.end <= position);
        let token = tokens
            .get(index)
            .filter(|t| t.start <= position && position < t.end)?;
        Some(Request {
            revision: revision(),
            identity: format!("source:{}:{}", token.start.0, token.start.1),
            target: token.target.clone(),
        })
    });
    moonkale_ext_api::crash::use_effect(move || {
        if let Some(request) = wanted() {
            if desired.peek().as_ref() != Some(&request) {
                anchor.set(*point.peek());
                desired.set(Some(request));
            }
        } else if !inside() {
            let previous = desired.peek().clone();
            moonkale_ext_api::crash::spawn(async move {
                futures_timer::Delay::new(Duration::from_millis(200)).await;
                if wanted.peek().is_none() && !*inside.peek() && *desired.peek() == previous {
                    desired.set(None);
                }
            });
        }
    });
    let mut loading = use_signal(|| None::<(Request, u64)>);
    let result = moonkale_ext_api::crash::use_resource(move || {
        let request = desired();
        if request.is_some() {
            let _ = ws.docs.open.read();
        }
        let revision = revision();
        let from = from();
        let epoch = *ws.sources.graph_epoch.read();
        async move {
            let request = request.filter(|r| r.revision == revision)?;
            futures_timer::Delay::new(Duration::from_millis(250)).await;
            loading.set(Some((request.clone(), epoch)));
            let value = select(
                fetch(ws, from.clone(), request.clone(), epoch).boxed_local(),
                futures_timer::Delay::new(Duration::from_secs(5)).boxed_local(),
            )
            .await;
            Some(match value {
                Either::Left((value, _)) => value,
                _ => ResultValue {
                    request,
                    epoch,
                    origin: from,
                    node: None,
                    from_open: false,
                    status: Status::Unavailable,
                    text: String::new(),
                },
            })
        }
    });
    let popup = use_memo(move || {
        let request = desired()?;
        if from().language_hint() != Some("markdown")
            || request.revision != revision()
            || *ws.docs.active.read() != Some(node)
        {
            return None;
        }
        let epoch = *ws.sources.graph_epoch.read();
        let result = result.read();
        let value = result.as_ref().and_then(|v| v.as_ref()).filter(|v| {
            v.request == request
                && v.epoch == epoch
                && v.origin == from()
                && (!v.from_open || v.node.as_ref().is_some_and(|n| ws.document(n.id).is_some()))
        });
        let mut popup = Popup {
            title: request.target.description(),
            text: String::new(),
            status: Status::Loading,
            heading: match &request.target {
                Target::Wiki { heading, .. } => heading.clone(),
                _ => None,
            },
            point: anchor(),
        };
        if let Some(value) = value {
            popup.status = value.status;
            popup.text = value.text.clone();
            if let Some(target) = &value.node {
                popup.title = target.label.clone();
                if let Some(open) = ws
                    .document(target.id)
                    .filter(|_| target.language_hint() == Some("markdown"))
                {
                    let doc = open.read();
                    popup.title = doc.node.label.clone();
                    let (status, text) = snippet(&doc.text, &request.target);
                    popup.status = status;
                    popup.text = text;
                }
            }
        } else if loading.read().as_ref() != Some(&(request, epoch)) {
            return None;
        }
        Some(popup)
    });
    let control = Control {
        pointer_active,
        source_position,
        rendered,
        point,
        inside,
        popup,
        desired,
        source,
    };
    let seen = use_hook(move || std::rc::Rc::new(std::cell::Cell::new(*revision.peek())));
    moonkale_ext_api::crash::use_effect(move || {
        let next = revision();
        if seen.replace(next) != next {
            control.dismiss();
        }
    });
    control
}

async fn fetch(ws: Workspace, from: Node, request: Request, epoch: u64) -> ResultValue {
    let Target::Wiki { page, .. } = &request.target else {
        return ResultValue {
            request,
            epoch,
            origin: from,
            node: None,
            from_open: false,
            status: Status::Unavailable,
            text: String::new(),
        };
    };
    let node = if page.is_empty() {
        Some(from.clone())
    } else {
        ws.resolve_wiki(&from, page).await
    };
    let mut result = ResultValue {
        request,
        epoch,
        origin: from,
        node: node.clone(),
        from_open: false,
        status: Status::Missing,
        text: String::new(),
    };
    let Some(node) = node else {
        return result;
    };
    if node.language_hint() != Some("markdown") {
        result.status = Status::Unavailable;
        return result;
    }
    if let Some(open) = ws.document(node.id) {
        result.from_open = true;
        (result.status, result.text) = snippet(&open.peek().text, &result.request.target);
        return result;
    }
    if matches!(node.content,Some(ContentRef::Blob{len,..}|ContentRef::Text{len,..}) if len>MAX_TEXT as u64)
    {
        result.status = Status::TooLarge;
        return result;
    }
    let Some(source) = ws.source(&node.source) else {
        result.status = Status::Unavailable;
        return result;
    };
    match source.fetch_text(node.id).await {
        Ok((text, _)) => (result.status, result.text) = snippet(&text, &result.request.target),
        Err(moonkale_core::SourceError::NotFound) => result.status = Status::Missing,
        Err(_) => result.status = Status::Unavailable,
    }
    result
}

#[cfg(feature = "markdown-preview")]
fn snippet(text: &str, target: &Target) -> (Status, String) {
    crate::markdown_guard::parse(text, || snippet_parsed(text, target))
        .unwrap_or_else(|| (Status::Unavailable, String::new()))
}

#[cfg(feature = "markdown-preview")]
fn snippet_parsed(text: &str, target: &Target) -> (Status, String) {
    use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
    if text.contains('\0') {
        return (Status::Unavailable, String::new());
    }
    if text.len() > MAX_TEXT {
        return (Status::TooLarge, String::new());
    }
    let Target::Wiki { heading, .. } = target else {
        return (Status::Unavailable, String::new());
    };
    let metadata = Parser::new_ext(text, Options::ENABLE_YAML_STYLE_METADATA_BLOCKS)
        .into_offset_iter()
        .next()
        .filter(|(e, r)| r.start == 0 && matches!(e, Event::Start(Tag::MetadataBlock(_))))
        .map_or(0, |(_, r)| r.end);
    let start = if let Some(heading) = heading.as_ref().filter(|h| !h.is_empty()) {
        let Some((line, _)) = crate::native_links::heading_position(text, heading) else {
            return (Status::HeadingMissing, String::new());
        };
        text.split_inclusive('\n')
            .take(line as usize)
            .map(str::len)
            .sum()
    } else {
        metadata
    };
    let body = &text[start..];
    let scan_end = body.char_indices().nth(4096).map_or(body.len(), |(i, _)| i);
    let projected = crate::native_link_parser::Projected::new(&body[..scan_end]);
    let mut section_level = None;
    let mut plain = String::new();
    for event in Parser::new_ext(
        &projected.text,
        Options::ENABLE_TASKLISTS | Options::ENABLE_TABLES,
    ) {
        match event {
            Event::Start(Tag::Heading { level, .. })
                if heading.as_ref().is_some_and(|h| !h.is_empty()) =>
            {
                if section_level.is_some_and(|previous| level <= previous) {
                    break;
                }
                section_level.get_or_insert(level);
            }
            Event::Text(t) | Event::Code(t) => plain.push_str(&t),
            Event::SoftBreak | Event::HardBreak => plain.push('\n'),
            Event::End(
                TagEnd::Paragraph
                | TagEnd::Heading(_)
                | TagEnd::Item
                | TagEnd::CodeBlock
                | TagEnd::TableRow,
            ) => plain.push('\n'),
            Event::Start(Tag::Item) => plain.push_str("• "),
            Event::TaskListMarker(checked) => plain.push_str(if checked { "☑ " } else { "☐ " }),
            Event::End(TagEnd::TableCell) => plain.push_str("  "),
            _ => {}
        }
        if plain.len() > MAX_EXCERPT * 4 {
            break;
        }
    }
    let plain = plain.trim();
    let end = plain
        .char_indices()
        .nth(MAX_EXCERPT)
        .map_or(plain.len(), |(i, _)| i);
    let mut value = plain[..end].lines().take(14).collect::<Vec<_>>().join("\n");
    if end < plain.len() || plain.lines().count() > 14 || scan_end < body.len() {
        value.push('…');
    }
    (Status::Ready, value)
}
#[cfg(not(feature = "markdown-preview"))]
fn snippet(_: &str, _: &Target) -> (Status, String) {
    (Status::Unavailable, String::new())
}

#[cfg(feature = "markdown-preview")]
fn tokens(text: &str) -> Vec<Token> {
    crate::markdown_guard::parse(text, || tokens_parsed(text)).unwrap_or_default()
}

#[cfg(feature = "markdown-preview")]
fn tokens_parsed(text: &str) -> Vec<Token> {
    use pulldown_cmark::{Event, Options, Parser, Tag};
    let raw = moonkale_ext_api::wiki::spans(text);
    if raw.is_empty() {
        return Vec::new();
    }
    let metadata = Parser::new_ext(text, Options::ENABLE_YAML_STYLE_METADATA_BLOCKS)
        .into_offset_iter()
        .next()
        .filter(|(e, r)| r.start == 0 && matches!(e, Event::Start(Tag::MetadataBlock(_))))
        .map_or(0, |(_, r)| r.end);
    let mut blocked = Vec::new();
    if metadata > 0 {
        blocked.push(0..metadata);
    }
    blocked.extend(
        Parser::new_ext(&text[metadata..], Options::ENABLE_MATH)
            .into_offset_iter()
            .filter_map(|(e, r)| {
                let r = r.start + metadata..r.end + metadata;
                match e {
                    Event::Start(
                        Tag::CodeBlock(_) | Tag::HtmlBlock | Tag::Image { .. } | Tag::Link { .. },
                    )
                    | Event::Code(_)
                    | Event::InlineMath(_)
                    | Event::DisplayMath(_) => Some(r),
                    Event::Html(_) | Event::InlineHtml(_)
                        if !raw
                            .iter()
                            .any(|(a, b, _, _, _)| *a <= r.start && r.end <= *b) =>
                    {
                        Some(r)
                    }
                    _ => None,
                }
            }),
    );
    let mut previous = 0;
    let mut position = (0usize, 0usize);
    let mut values = Vec::new();
    let advance = |text: &str, position: &mut (usize, usize)| {
        let mut chars = text.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '\n' {
                position.0 += 1;
                position.1 = 0;
            } else if ch != '\r' || chars.peek() != Some(&'\n') {
                position.1 += 1;
            }
        }
    };
    for (start, end, page, heading, _) in raw {
        if start > 0
            && (text.as_bytes()[start - 1] == b'!'
                || text[..start]
                    .bytes()
                    .rev()
                    .take_while(|b| *b == b'\\')
                    .count()
                    % 2
                    == 1)
            || blocked.iter().any(|r| r.start < end && start < r.end)
        {
            continue;
        }
        advance(&text[previous..start], &mut position);
        let begin = position;
        advance(&text[start..end], &mut position);
        previous = end;
        values.push(Token {
            start: begin,
            end: position,
            target: Target::Wiki { page, heading },
        });
    }
    values
}
#[cfg(not(feature = "markdown-preview"))]
fn tokens(_: &str) -> Vec<Token> {
    Vec::new()
}

#[component]
pub(crate) fn WikiPopup(control: Control, ws: Workspace, onclose: Callback<()>) -> Element {
    let Some(popup) = (control.popup)() else {
        return rsx! {};
    };
    let message = match popup.status {
        Status::Loading => t!(ws, crate::L, "editor-wiki-preview-loading"),
        Status::Missing => t!(ws, crate::L, "editor-wiki-preview-missing"),
        Status::Unavailable => t!(ws, crate::L, "editor-wiki-preview-unavailable"),
        Status::TooLarge => t!(ws, crate::L, "editor-wiki-preview-too-large"),
        Status::HeadingMissing => t!(
            ws,
            crate::L,
            "editor-link-heading-missing",
            heading = popup.heading.as_deref().unwrap_or("")
        ),
        Status::Ready => popup.text,
    };
    let mut control = control;
    rsx! {div {
        class:"mk-native-wiki-hover mk-hover",role:"tooltip",tabindex:0,aria_label:t!(ws,crate::L,"editor-wiki-preview"),
        "data-preview-status":format!("{:?}",popup.status).to_ascii_lowercase(),
        style:format!("left:clamp(8px,{}px,calc(100vw - min(420px,100vw - 16px) - 8px));top:clamp(8px,{}px,calc(100vh - 276px));",popup.point.0,popup.point.1+12.0),
        onmouseenter:moonkale_ext_api::crash::event(move |_|control.inside.set(true)),onmouseleave:moonkale_ext_api::crash::event(move |_|control.leave()),
        onmousedown:moonkale_ext_api::crash::event(move |event: dioxus::prelude::MouseEvent|event.stop_propagation()),oncopy:moonkale_ext_api::crash::event(move |event: dioxus::prelude::ClipboardEvent|event.stop_propagation()),oncut:moonkale_ext_api::crash::event(move |event: dioxus::prelude::ClipboardEvent|{event.prevent_default();event.stop_propagation();}),onpaste:moonkale_ext_api::crash::event(move |event: dioxus::prelude::ClipboardEvent|{event.prevent_default();event.stop_propagation();}),
        onwheel:moonkale_ext_api::crash::event(move |event: dioxus::prelude::WheelEvent|event.stop_propagation()),onscroll:moonkale_ext_api::crash::event(move |event: dioxus::prelude::ScrollEvent|event.stop_propagation()),
        onkeydown:moonkale_ext_api::crash::event(move |event: dioxus::prelude::KeyboardEvent|{event.stop_propagation();if event.key()==Key::Escape {event.prevent_default();control.dismiss();onclose.call(());}}),
        div {class:"mk-wiki-hover-title","{popup.title}"}
        div {class:"mk-wiki-hover-body","{message}"}
    }}
}

#[cfg(all(test, feature = "markdown-preview"))]
mod tests {
    use super::*;
    fn wiki(heading: Option<&str>) -> Target {
        Target::Wiki {
            page: "Note".into(),
            heading: heading.map(str::to_string),
        }
    }
    #[test]
    fn source_tokens_keep_crlf_unicode_positions_and_exclude_literal_syntax() {
        let source="---\r\ntitle: '[[Meta]]'\r\n---\r\n😀中 [[Note é#Heading|alias]]\r\n`[[Code]]` ![[Embed]] \\[[Escaped]] $\\text{[[Math]]}$\r\n\r\n```\r\n[[Fence]]\r\n```\r\n\r\n[[#local|Local]]\r\n";
        let values = tokens(source);
        assert_eq!(values.len(), 2);
        assert_eq!(values[0].start, (3, 3));
        assert_eq!(values[0].end, (3, 27));
        assert_eq!(
            values[0].target,
            Target::Wiki {
                page: "Note é".into(),
                heading: Some("Heading".into())
            }
        );
        assert!(
            matches!(&values[1].target,Target::Wiki{page,heading:Some(h)} if page.is_empty()&&h=="local")
        );
        assert_eq!(tokens("before\n\n---\n[[Later]]\n---\n").len(), 1);
        let alias = tokens("[[Note|Alias <safe>]]");
        assert_eq!(alias.len(), 1);
    }
    #[test]
    fn excerpts_strip_front_matter_render_literal_text_and_respect_sections() {
        let text="---\r\ntitle: Secret metadata\r\n---\r\n# Heading 猫\r\n**Bold** &amp; `code` and [[Page|Alias <safe>]].\r\n\r\n## Child\r\nchild body\r\n\r\n# Next\r\nOther section\r\n";
        let (status, body) = snippet(text, &wiki(None));
        assert_eq!(status, Status::Ready);
        assert!(!body.contains("Secret metadata"));
        assert!(body.contains("Bold & code and Alias <safe>."));
        let (_, body) = snippet(text, &wiki(Some("heading-猫")));
        assert!(body.starts_with("Heading 猫"));
        assert!(body.contains("child body"));
        assert!(!body.contains("Other section"));
        assert_eq!(
            snippet(text, &wiki(Some("absent"))).0,
            Status::HeadingMissing
        );
        assert_eq!(
            snippet("<script>alert(1)</script>\n\nSafe", &wiki(None)).1,
            "Safe"
        );
    }
    #[test]
    fn bounded_excerpts_preserve_unicode_and_reject_large_or_binary_text() {
        let (_, body) = snippet(&"猫😀".repeat(4000), &wiki(None));
        assert_eq!(body.chars().count(), MAX_EXCERPT + 1);
        assert!(body.ends_with('…'));
        let (_, body) = snippet(&"line\n\n".repeat(100), &wiki(None));
        assert!(body.lines().count() <= 14);
        assert!(body.ends_with('…'));
        assert_eq!(
            snippet(&"x".repeat(MAX_TEXT + 1), &wiki(None)).0,
            Status::TooLarge
        );
        assert_eq!(snippet("binary\0text", &wiki(None)).0, Status::Unavailable);
    }
}

#[cfg(all(test, feature = "markdown-preview"))]
mod p212_regression {
    #[test]
    fn crash_retains_unparsed_source() {
        for source in crate::markdown_guard::CRASH_INPUTS {
            let target = super::Target::Wiki {
                page: "Note".into(),
                heading: None,
            };
            assert!(matches!(
                super::snippet(source, &target).0,
                super::Status::Unavailable
            ));
            assert!(super::tokens(source).is_empty());
            // Force the wiki-token parser path even for the minimized input.
            assert!(super::tokens(&format!("{source}\n[[Note]]")).is_empty());
        }
    }
}
