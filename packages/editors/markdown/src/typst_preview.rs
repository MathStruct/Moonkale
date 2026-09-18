//! Live SVG preview of the active `.typ` document.
//!
//! Recompiles whenever the document text changes (coalesced: one compile
//! in flight, the latest text compiled afterwards). The compile itself is
//! the platform's `compile_typst` — in-process on desktop, a server function
//! on web — so this panel never links `typst`.

use dioxus::prelude::*;
use moonkale_core::NodeId;
use moonkale_ext_api::Workspace;

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
        document::Stylesheet { href: CSS }
        div { class: "mk-typst",
            match state() {
                State::Idle => rsx! { p { class: "mk-typst-msg", "Open a .typ file to preview it." } },
                State::Errors(errs) => rsx! {
                    div { class: "mk-typst-errors",
                        for e in errs { div { class: "mk-typst-error", "{e}" } }
                    }
                },
                State::Pages(pages) => rsx! {
                    div { class: "mk-typst-meta", "{pages.len()} page(s)" if busy() { " · compiling…" } }
                    for (i, svg) in pages.iter().enumerate() {
                        div { key: "{i}", class: "mk-typst-page", dangerous_inner_html: "{svg}" }
                    }
                },
            }
        }
    }
}
