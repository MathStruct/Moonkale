//! Rich (WYSIWYG) markdown mode: Milkdown's Crepe editor behind
//! [`RichTextBackend`]. Rust owns the markdown (`Document`); the view emits
//! whole-document changes, like the code editor did in Milestone 1.

use dioxus::document::{self, Eval};
use dioxus::prelude::*;
use moonkale_ext_api::Workspace;
use serde::{Deserialize, Serialize};

pub const BUNDLE: Asset = asset!("/assets/milkdown.js");
pub const BUNDLE_CSS: Asset = asset!("/assets/milkdown.css");
const CSS: Asset = asset!("/assets/rich.css");
/// KaTeX's stylesheet and woff2 fonts (spec 013): a folder asset so the
/// relative `fonts/…` urls inside `katex.min.css` keep working.
const KATEX_DIR: Asset = asset!("/assets/katex", AssetOptions::folder());
/// Per-folder KaTeX macros, relative to the folder root.
pub const KATEX_FILE: &str = ".moonkale/katex.json";

/// The replaceable contract (a Rust-native rich text view would implement it).
pub trait RichTextBackend {
    fn set_text(&self, markdown: &str);
    fn focus(&self);
    /// Which `[[targets]]` of the document resolve (spec 012).
    fn set_wiki_status(&self, spans: &[moonkale_ext_api::wiki::WikiSpan]);
    /// The answer to a `[[` completion request.
    fn wiki_candidates(&self, id: u32, items: &[moonkale_ext_api::wiki::WikiCandidate]);
}

pub enum RichEvent {
    Ready,
    Changed(String),
    /// A link was clicked (plain or Ctrl): follow it, creating the page if
    /// it does not exist.
    WikiLink(String),
    /// `[[query` typed: answer with `wiki_candidates(id, …)`.
    WikiQuery {
        id: u32,
        query: String,
    },
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum ToJs<'a> {
    Init {
        text: &'a str,
        /// KaTeX `\newcommand`s from `.moonkale/katex.json` (spec 013).
        /// (`rename_all` on the enum does not reach variant fields, P-077.)
        #[serde(rename = "katexMacros")]
        katex_macros: serde_json::Value,
    },
    SetText {
        text: &'a str,
    },
    Focus,
    Destroy,
    WikiStatus {
        entries: Vec<WikiStatusEntry>,
    },
    WikiCandidates {
        id: u32,
        items: &'a [moonkale_ext_api::wiki::WikiCandidate],
    },
}

#[derive(Serialize)]
struct WikiStatusEntry {
    target: String,
    resolved: bool,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum FromJs {
    Ready,
    Change { text: String },
    WikiLink { target: String },
    WikiQuery { id: u32, query: String },
    Error { message: String },
}

const SCRIPT: &str = r#"
const el = document.getElementById(ELEMENT_ID);
if (!el) { return; }
while (!(window.moonkale && window.moonkale.milkdown)) {
    await new Promise((r) => setTimeout(r, 20));
}
const md = window.moonkale.milkdown;
const init = await dioxus.recv();
try {
    await md.mount(el, init.text, (text) => dioxus.send({ kind: "change", text }), (target) => dioxus.send({ kind: "wikiLink", target }), { katexMacros: init.katexMacros || {}, onWikiQuery: (id, query) => dioxus.send({ kind: "wikiQuery", id, query }) });
} catch (e) {
    dioxus.send({ kind: "error", message: String(e && e.message ? e.message : e) });
    for (;;) { const m = await dioxus.recv(); if (m.kind === "destroy") return; }
}
dioxus.send({ kind: "ready" });
for (;;) {
    const msg = await dioxus.recv();
    if (msg.kind === "setText") md.setText(el, msg.text);
    else if (msg.kind === "wikiStatus") md.setWikiStatus(el, msg.entries);
    else if (msg.kind === "wikiCandidates") md.wikiCandidates(el, msg.id, msg.items);
    else if (msg.kind === "focus") md.focus(el);
    else if (msg.kind === "destroy") { md.destroy(el); break; }
}
"#;

pub struct MilkdownBackend {
    eval: Eval,
}

impl MilkdownBackend {
    pub fn mount(
        element_id: String,
        initial: String,
        katex_macros: serde_json::Value,
        on_event: Callback<RichEvent>,
    ) -> Self {
        let script = SCRIPT.replace("ELEMENT_ID", &serde_json::to_string(&element_id).unwrap());
        let eval = document::eval(&script);
        let _ = eval.send(ToJs::Init {
            text: &initial,
            katex_macros,
        });
        let mut rx = eval;
        spawn(async move {
            loop {
                match rx.recv::<FromJs>().await {
                    Ok(FromJs::Ready) => on_event.call(RichEvent::Ready),
                    Ok(FromJs::Change { text }) => on_event.call(RichEvent::Changed(text)),
                    Ok(FromJs::WikiLink { target }) => on_event.call(RichEvent::WikiLink(target)),
                    Ok(FromJs::WikiQuery { id, query }) => {
                        on_event.call(RichEvent::WikiQuery { id, query })
                    }
                    Ok(FromJs::Error { message }) => {
                        tracing::warn!("milkdown: {message}");
                        break;
                    }
                    Err(dioxus::document::EvalError::Serialization(_)) => continue,
                    Err(_) => break,
                }
            }
        });
        Self { eval }
    }
}

impl RichTextBackend for MilkdownBackend {
    fn set_text(&self, markdown: &str) {
        let _ = self.eval.send(ToJs::SetText { text: markdown });
    }
    fn focus(&self) {
        let _ = self.eval.send(ToJs::Focus);
    }
    fn set_wiki_status(&self, spans: &[moonkale_ext_api::wiki::WikiSpan]) {
        let entries = spans
            .iter()
            .map(|s| WikiStatusEntry {
                target: s.target.clone(),
                resolved: s.resolved,
            })
            .collect();
        let _ = self.eval.send(ToJs::WikiStatus { entries });
    }
    fn wiki_candidates(&self, id: u32, items: &[moonkale_ext_api::wiki::WikiCandidate]) {
        let _ = self.eval.send(ToJs::WikiCandidates { id, items });
    }
}

impl Drop for MilkdownBackend {
    fn drop(&mut self) {
        let _ = self.eval.send(ToJs::Destroy);
    }
}

/// The rich view of one markdown document.
#[component]
pub fn RichPanel(ws: Workspace, node: moonkale_core::NodeId) -> Element {
    let Some(mut doc) = ws.document(node) else {
        return rsx! { div { class: "mk-editor-missing", "Document is not open." } };
    };
    let element_id = format!("mk-rich-{node}");
    let mut backend: Signal<Option<std::rc::Rc<MilkdownBackend>>> = use_signal(|| None);
    let mut ready = use_signal(|| false);
    let mut error: Signal<Option<String>> = use_signal(|| None);
    let mut props_open = use_signal(|| false);
    // Bumped on ready and on every change; the effect below re-checks which
    // links resolve (debounced) and pushes the status into the view.
    let mut wiki_epoch = use_signal(|| 0u64);
    {
        use_effect(move || {
            let epoch = wiki_epoch();
            let _ = ws.graph_epoch.read();
            if epoch == 0 {
                return;
            }
            spawn(async move {
                futures_timer::Delay::new(std::time::Duration::from_millis(300)).await;
                if *wiki_epoch.peek() != epoch {
                    return; // superseded by a newer change
                }
                let (from, text) = {
                    let d = doc.peek();
                    (d.node.clone(), d.text.clone())
                };
                let spans = ws.wiki_spans(&from, &text).await;
                if let Some(b) = backend.peek().as_ref() {
                    b.set_wiki_status(&spans);
                }
            });
        });
    }

    let mount = {
        let element_id = element_id.clone();
        move |_| {
            if backend.peek().is_some() {
                return;
            }
            // The view shows the body only; the YAML front matter stays in
            // Rust and is edited in the Properties bar (spec 019).
            let initial = {
                let full = doc.peek().text.clone();
                split_frontmatter(&full).1.to_string()
            };
            let on_event = Callback::new(move |ev: RichEvent| match ev {
                RichEvent::Ready => {
                    ready.set(true);
                    wiki_epoch += 1;
                }
                RichEvent::Changed(body) => {
                    doc.with_mut(|d| {
                        let (fm, _) = split_frontmatter(&d.text);
                        d.text = format!("{fm}{body}");
                    });
                    wiki_epoch += 1;
                }
                RichEvent::WikiLink(target) => {
                    let from = doc.peek().node.clone();
                    spawn(async move {
                        let _ = ws.follow_wiki(&from, &target, true).await;
                    });
                }
                RichEvent::WikiQuery { id, query } => {
                    spawn(async move {
                        let items = ws.wiki_candidates(&query, 12).await;
                        if let Some(b) = backend.peek().as_ref() {
                            b.wiki_candidates(id, &items);
                        }
                    });
                }
            });
            let element_id = element_id.clone();
            let source = doc.peek().node.source.clone();
            spawn(async move {
                let macros = katex_macros(ws, &source).await;
                backend.set(Some(std::rc::Rc::new(MilkdownBackend::mount(
                    element_id, initial, macros, on_event,
                ))));
            });
        }
    };

    // Text replaced from outside (reload / revert): push it into the view.
    {
        let mut last_seen = use_signal(String::new);
        use_effect(move || {
            let (text, version) = {
                let d = doc.read();
                (d.text.clone(), d.version)
            };
            let _ = version;
            if !ready() {
                return;
            }
            // Only when the body jumped (not our own keystrokes, not a
            // front-matter edit): the view already has what it emitted.
            let (_, body) = split_frontmatter(&text);
            if *last_seen.peek() == body {
                return;
            }
            last_seen.set(body.to_string());
            if let Some(b) = backend.peek().as_ref() {
                b.set_text(body);
            }
        });
    }

    let typography = {
        let s = ws.settings.read();
        let mut css = format!("--mk-rich-size: {}px;", s.editor.rich_font_size);
        if !s.editor.rich_font.trim().is_empty() {
            css.push_str(&format!(" --mk-rich-font: {};", s.editor.rich_font.trim()));
        }
        if !s.editor.rich_code_font.trim().is_empty() {
            css.push_str(&format!(
                " --mk-rich-code-font: {};",
                s.editor.rich_code_font.trim()
            ));
        }
        css
    };
    let dirty = doc.read().dirty();
    let save = move |_: ()| {
        spawn(async move {
            if let Err(e) = ws.save(node).await {
                error.set(Some(e.to_string()));
            } else {
                error.set(None);
            }
        });
    };
    let reload = move |_| {
        spawn(async move {
            let _ = ws.reload(node).await;
        });
    };

    rsx! {
        moonkale_ext_api::Stylesheet { href: BUNDLE_CSS }
        moonkale_ext_api::StylesheetUrl { href: format!("{KATEX_DIR}/katex.min.css") }
        moonkale_ext_api::Stylesheet { href: CSS }
        document::Script { src: BUNDLE, defer: true }
        div {
            class: "mk-rich",
            onkeydown: move |e| {
                if (e.modifiers().ctrl() || e.modifiers().meta()) && e.key() == Key::Character("s".into()) {
                    e.prevent_default();
                    e.stop_propagation(); // the frame would dispatch Save again
                    save(());
                }
            },
            div { class: "mk-editor-toolbar",
                span { class: "mk-editor-path", "{doc.read().node.native_key}" }
                if dirty { span { class: "mk-editor-dirty", title: "Unsaved changes", "●" } }
                span { class: "mk-editor-spacer" }
                span { class: "mk-editor-meta", "rich · click a [[link]] to follow it · type [[ to link a page" }
                button { class: "mk-btn", disabled: !dirty, onclick: move |_| save(()), "Save" }
                button { class: "mk-btn", onclick: reload, title: "Discard edits and reload from the source", "Reload" }
            }
            if let Some(e) = error() {
                div { class: "mk-editor-error", "{e}" }
            }
            // Front matter as a Properties bar: a one-line summary, click to
            // edit the YAML itself (spec 019).
            {
                let text = doc.read().text.clone();
                let (fm, _) = split_frontmatter(&text);
                let yaml = frontmatter_yaml(fm).to_string();
                let summary = frontmatter_summary(&yaml);
                let has = !fm.is_empty();
                rsx! {
                    div { class: if props_open() { "mk-props mk-props-open" } else { "mk-props" },
                        button { class: "mk-props-head", r#type: "button", title: if has { "Front matter (YAML) — click to edit" } else { "Add front matter" },
                            onclick: move |_| props_open.toggle(),
                            span { class: "mk-props-caret", if props_open() { "▾" } else { "▸" } }
                            span { class: "mk-props-label", "Properties" }
                            span { class: "mk-props-summary", if has { "{summary}" } else { "none" } }
                        }
                        if props_open() {
                            textarea { class: "mk-props-yaml", rows: "{yaml.lines().count().max(2) + 1}", spellcheck: "false",
                                placeholder: "title: …\ntags: [a, b]",
                                value: "{yaml}",
                                oninput: move |e| {
                                    let v = e.value();
                                    doc.with_mut(|d| {
                                        let (_, body) = split_frontmatter(&d.text);
                                        let body = body.to_string();
                                        d.text = join_frontmatter(&v, &body);
                                    });
                                },
                            }
                        }
                    }
                }
            }
            if !ready() {
                div { class: "mk-rich-loading", "Loading rich editor…" }
            }
            // Typography from the settings (Prompt23): CSS variables the
            // stylesheet applies to the ProseMirror content and code blocks.
            div { id: "{element_id}", class: "mk-rich-host", style: "{typography}", onmounted: mount }
        }
    }
}

/// KaTeX macros for a folder: `.moonkale/katex.json` →
/// `{ "macros": { "\\R": "\\mathbb{R}" } }` (spec 013); `{}` when absent
/// or malformed (the status bar says so).
async fn katex_macros(mut ws: Workspace, source: &moonkale_core::SourceId) -> serde_json::Value {
    let Some(text) = ws.read_text_at(source, KATEX_FILE).await else {
        return serde_json::json!({});
    };
    match serde_json::from_str::<serde_json::Value>(&text) {
        Ok(v) => v
            .get("macros")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({})),
        Err(e) => {
            ws.set_status(format!("{KATEX_FILE} ignored: {e}"));
            serde_json::json!({})
        }
    }
}

/// Split leading YAML front matter (`---\n…\n---\n`) from the body. The
/// first part keeps its delimiters and trailing newline so the two halves
/// concatenate back to the original text.
pub fn split_frontmatter(text: &str) -> (&str, &str) {
    let Some(rest) = text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))
    else {
        return ("", text);
    };
    let mut offset = text.len() - rest.len();
    for line in rest.split_inclusive('\n') {
        offset += line.len();
        if line.trim_end() == "---" {
            return (&text[..offset], &text[offset..]);
        }
    }
    ("", text)
}

/// The YAML between the delimiters of a front-matter block ("" when none).
pub fn frontmatter_yaml(fm: &str) -> &str {
    let inner = fm
        .strip_prefix("---\n")
        .or_else(|| fm.strip_prefix("---\r\n"))
        .unwrap_or("");
    let inner = inner.trim_end_matches('\n').trim_end_matches('\r');
    inner
        .strip_suffix("---")
        .map(|s| s.trim_end_matches(['\n', '\r']))
        .unwrap_or("")
}

/// Front matter from YAML + body; empty YAML removes the block.
pub fn join_frontmatter(yaml: &str, body: &str) -> String {
    if yaml.trim().is_empty() {
        return body.to_string();
    }
    format!("---\n{}\n---\n{body}", yaml.trim_end_matches('\n'))
}

/// `title: "X" · tags: [a, b] · …` for the collapsed bar (first three keys).
fn frontmatter_summary(yaml: &str) -> String {
    let parts: Vec<String> = yaml
        .lines()
        .filter(|l| !l.starts_with(' ') && l.contains(':'))
        .take(3)
        .map(|l| {
            let (k, v) = l.split_once(':').unwrap_or((l, ""));
            let v = v.trim().trim_matches('"');
            let v: String = if v.chars().count() > 40 {
                format!("{}…", v.chars().take(40).collect::<String>())
            } else {
                v.to_string()
            };
            if v.is_empty() {
                k.trim().to_string()
            } else {
                format!("{}: {v}", k.trim())
            }
        })
        .collect();
    parts.join(" · ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontmatter_round_trips() {
        let text = "---\ntitle: \"A\"\ntags: [x]\n---\n# A\n\nbody\n";
        let (fm, body) = split_frontmatter(text);
        assert_eq!(fm, "---\ntitle: \"A\"\ntags: [x]\n---\n");
        assert_eq!(body, "# A\n\nbody\n");
        assert_eq!(format!("{fm}{body}"), text);
        assert_eq!(frontmatter_yaml(fm), "title: \"A\"\ntags: [x]");
        assert_eq!(join_frontmatter(frontmatter_yaml(fm), body), text);
        assert_eq!(join_frontmatter("", body), body);
        assert_eq!(
            split_frontmatter("no front matter\n---\n"),
            ("", "no front matter\n---\n")
        );
        assert_eq!(
            frontmatter_summary("title: \"A\"\ntags: [x]\ndescription: long\nmore: 1"),
            "title: A · tags: [x] · description: long"
        );
    }
}
