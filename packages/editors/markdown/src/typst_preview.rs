//! Live SVG preview of the active `.typ` document.
//!
//! Recompiles whenever the document text changes (coalesced: one compile
//! in flight, the latest text compiled afterwards). The compile itself is
//! the platform's `compile_typst` — in-process on desktop, a server function
//! on web — so this panel never links `typst`.

use crate::L;
use dioxus::prelude::*;
use moonkale_core::NodeId;
use moonkale_ext_api::{t, Workspace};

const CSS: Asset = asset!("/assets/typst.css");

#[derive(Clone, PartialEq)]
enum State {
    Idle,
    Pages(Vec<String>),
    Errors(Vec<String>),
}

/// The active document if it is a Typst file.
pub fn active_typst(ws: &Workspace) -> Option<NodeId> {
    let (id, doc) = ws.active_document()?;
    let d = doc.read();
    (d.node.native_key.ends_with(".typ")).then_some(id)
}

#[component]
pub fn TypstPreviewPanel(ws: Workspace) -> Element {
    let mut state = use_signal(|| State::Idle);
    let mut busy = use_signal(|| false);
    let mut pending: Signal<Option<(String, String, String)>> = use_signal(|| None);

    // Read the active Typst document's text reactively and request a compile.
    use_effect(move || {
        let Some(compile) = ws.compile_typst() else {
            return;
        };
        let Some((_, doc)) = ws.active_document() else {
            return;
        };
        let (root, rel, text) = {
            let d = doc.read();
            if !d.node.native_key.ends_with(".typ") {
                return;
            }
            let Some(root) = d.node.source.as_str().strip_prefix("folder:") else {
                return;
            };
            (
                root.to_string(),
                format!("/{}", d.node.native_key),
                d.text.clone(),
            )
        };
        // peek: this effect must not depend on its own busy flag.
        if *busy.peek() {
            pending.set(Some((root, rel, text)));
            return;
        }
        busy.set(true);
        spawn(async move {
            let mut job = (root, rel, text);
            loop {
                let out = compile(job.0.clone(), job.1.clone(), job.2.clone()).await;
                state.set(match out {
                    Ok(pages) => State::Pages(pages),
                    Err(errs) => State::Errors(errs),
                });
                match pending.write().take() {
                    Some(next) => job = next,
                    None => break,
                }
            }
            busy.set(false);
        });
    });

    rsx! {
        moonkale_ext_api::Stylesheet { href: CSS }
        div { class: "mk-typst",
            match state() {
                State::Idle => rsx! { p { class: "mk-typst-msg", "Open a .typ file to preview it." } },
                State::Errors(errs) => rsx! {
                    div { class: "mk-typst-errors",
                        for e in errs { div { class: "mk-typst-error", "{e}" } }
                    }
                },
                State::Pages(pages) => rsx! {
                    div { class: "mk-typst-meta", {t!(ws, L, "typst-pages", n = pages.len())} if busy() { {t!(ws, L, "typst-compiling")} } }
                    for (i, svg) in pages.iter().enumerate() {
                        if crate::typst_preview::plain_svg(svg) {
                            div { key: "{i}", class: "mk-typst-page", dangerous_inner_html: "{svg}" }
                        } else {
                            div { key: "{i}", class: "mk-typst-error", {t!(ws, L, "typst-unsafe-svg")} }
                        }
                    }
                },
            }
        }
    }
}

/// Whether a page from the Typst compiler is a plain SVG to put into the
/// page as HTML (#19): it must be an `<svg>` document with no script,
/// event handler, `javascript:` URL or embedded HTML. Typst emits none of
/// them; this is the check that it stays that way.
pub fn plain_svg(svg: &str) -> bool {
    let start = svg.trim_start();
    let start = start.strip_prefix("<?xml").map_or(start, |r| {
        r.split_once("?>").map_or("", |(_, r)| r.trim_start())
    });
    if !start.starts_with("<svg") {
        return false;
    }
    let lower = svg.to_ascii_lowercase();
    !["<script", "javascript:", "<foreignobject", "<iframe"]
        .iter()
        .any(|bad| lower.contains(bad))
        && !lower
            .split(|c: char| c.is_whitespace())
            .any(|w| w.starts_with("on") && w.contains('='))
}

#[cfg(test)]
mod svg_tests {
    #[test]
    fn only_plain_svg_is_put_into_the_page() {
        assert!(super::plain_svg(
            r#"<svg class="typst-doc" viewBox="0 0 10 10"><path d="M0 0"/></svg>"#
        ));
        assert!(super::plain_svg("<?xml version=\"1.0\"?>\n<svg></svg>"));
        assert!(!super::plain_svg("<svg><script>alert(1)</script></svg>"));
        assert!(!super::plain_svg(r#"<svg onload="alert(1)"></svg>"#));
        assert!(!super::plain_svg(
            r#"<svg><a href="javascript:x">l</a></svg>"#
        ));
        assert!(!super::plain_svg("<div>not svg</div>"));
    }
}
