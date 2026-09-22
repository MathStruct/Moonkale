//! The panel: one `dioxus-code-editor` per document the extension claims,
//! the same toolbar as the CodeMirror panel, and the caret reported to
//! the workspace.

use dioxus::prelude::*;
use dioxus_code::{CodeTheme, Theme};
use dioxus_code_editor::{CodeEditor, Language};
use moonkale_ext_api::prelude::*;

pub const PANEL_PREFIX: &str = "editor-native:";
const CSS: Asset = asset!("/assets/code-native.css");

pub struct NativeCodeExtension {
    skip: Option<fn(&moonkale_core::Node) -> bool>,
}

impl Default for NativeCodeExtension {
    fn default() -> Self {
        Self::new()
    }
}

impl NativeCodeExtension {
    pub fn new() -> Self {
        Self { skip: None }
    }

    /// Leave documents matching `f` to another extension (markdown, flow).
    pub fn skipping(mut self, f: fn(&moonkale_core::Node) -> bool) -> Self {
        self.skip = Some(f);
        self
    }

    pub fn panel_id(node: NodeId) -> String {
        format!("{PANEL_PREFIX}{node}")
    }

    fn node_of(panel_id: &str) -> Option<NodeId> {
        panel_id.strip_prefix(PANEL_PREFIX)?.parse().ok()
    }
}

impl Extension for NativeCodeExtension {
    fn manifest(&self) -> Manifest {
        Manifest::opt_in(
            "dev.moonkale.editor-code-native",
            "Code Editor (Rust)",
            "A code editor without JavaScript bundles: dioxus-code-editor, tree-sitter highlighting in Rust for every core language (Lean, Nix and Typst included). No language-server features yet.",
        )
    }

    fn panels(&self, ws: Workspace) -> Vec<PanelContribution> {
        ws.documents
            .read()
            .iter()
            .filter(|(_, doc)| !self.skip.is_some_and(|f| f(&doc.read().node)))
            .filter(|(id, _)| ws.editor_for(*id) == "native")
            .map(|(id, doc)| {
                let d = doc.read();
                PanelContribution {
                    id: Self::panel_id(*id),
                    title: d.node.label.clone(),
                    home: PanelHome::Main,
                    closable: true,
                    dirty: d.dirty(),
                    node: Some(*id),
                    activity: None,
                }
            })
            .collect()
    }

    fn render(&self, panel_id: &str, ws: Workspace) -> Element {
        match Self::node_of(panel_id) {
            Some(node) => rsx! { NativeCodePanel { ws, node } },
            None => rsx! { "unknown panel {panel_id}" },
        }
    }

    fn on_panel_closed(&self, panel_id: &str, ws: Workspace) {
        // Closing the tab closes the document — unless it just moved to the
        // other editor (then it is not ours any more and stays open).
        if let Some(node) = Self::node_of(panel_id) {
            if ws.editor_for(node) == "native" {
                ws.close_node(node);
            }
        }
    }

    // Which editor opens a file is Settings → Which extension (Milestone 15);
    // this extension has no settings of its own yet.
}

/// `Node::language_hint` → the grammar; unknown or none → plain text
/// (`Language::from_slug("text")` is absent, so Markdown's plain-ish grammar
/// is not used either: the editor then highlights nothing).
fn language_of(hint: Option<&str>) -> Option<Language> {
    let slug = match hint? {
        "c" => "c",
        "cpp" => "cpp",
        "javascript" => "javascript",
        "typescript" => "typescript",
        "tsx" => "tsx",
        "shell" | "bash" => "bash",
        "pixi" => "toml",
        "postgres" => "sql",
        other => other,
    };
    Language::from_slug(slug)
}

#[component]
fn NativeCodePanel(ws: Workspace, node: NodeId) -> Element {
    let Some(doc) = ws.document(node) else {
        return rsx! { div { class: "mk-editor-failed", "This document is not open any more." } };
    };
    let mut doc = doc;
    let mut last_error: Signal<Option<String>> = use_signal(|| None);
    let element_id = use_hook(|| format!("mk-cn-{}", node.0.simple()));

    let save_now = Callback::new(move |_: ()| {
        spawn(async move {
            match ws.save(node).await {
                Ok(()) => last_error.set(None),
                Err(e) => last_error.set(Some(e.to_string())),
            }
        });
    });
    let reload = move |_| async move {
        if ws.reload(node).await.is_ok() {
            last_error.set(None);
        }
    };

    // The caret: the textarea's selectionStart, read on caret-moving events
    // (the one JavaScript touch in this editor — Dioxus's eval, no bundle),
    // turned into (line, col) in Rust for `Workspace::cursor`.
    let report_caret = {
        let element_id = element_id.clone();
        move || {
            let js = format!(
                "const t = document.querySelector('#{element_id} textarea'); return t ? t.selectionStart : -1;"
            );
            spawn(async move {
                let eval = document::eval(&js);
                if let Ok(v) = eval.await {
                    if let Some(offset) = v.as_i64().filter(|o| *o >= 0) {
                        let text = doc.peek().text.clone();
                        let (line, col) = line_col(&text, offset as usize);
                        let mut ws = ws;
                        ws.set_cursor(node, line, col);
                    }
                }
            });
        }
    };
    let caret_key = report_caret.clone();
    let caret_click = report_caret.clone();
    let caret_select = report_caret;

    let d = doc.read();
    let title = d.node.native_key.clone();
    let dirty = d.dirty();
    let version = d.version.0;
    let hint = d.node.language_hint();
    let lang = hint.unwrap_or("text").to_string();
    let language = language_of(hint);
    let text = d.text.clone();
    drop(d);
    let dark = ws.settings.read().theme != "light";
    let theme = if dark {
        CodeTheme::fixed(Theme::TOKYO_NIGHT)
    } else {
        CodeTheme::fixed(Theme::GITHUB_LIGHT)
    };
    let word = ws.cursor_word();
    let codemirror_on = ws
        .settings
        .read()
        .extensions
        .is_enabled_id("dev.moonkale.editor-code", true);

    rsx! {
        moonkale_ext_api::Stylesheet { href: CSS }
        div {
            class: "mk-editor mk-cn",
            id: "{element_id}",
            "data-editor": "native",
            onkeydown: move |e| {
                let mods = e.modifiers();
                if (mods.ctrl() || mods.meta()) && e.key() == Key::Character("s".into()) {
                    e.prevent_default();
                    e.stop_propagation();
                    save_now.call(());
                }
            },
            onkeyup: move |_| caret_key(),
            onclick: move |_| caret_click(),
            onselect: move |_| caret_select(),
            div { class: "mk-editor-toolbar",
                span { class: "mk-editor-path", "{title}" }
                if dirty { span { class: "mk-editor-dirty", title: "Unsaved changes", "●" } }
                span { class: "mk-editor-spacer" }
                if let Some(w) = word {
                    span { class: "mk-cn-word", title: "The identifier under the caret (Workspace::cursor_word)", "‹{w}›" }
                }
                span { class: "mk-editor-meta", "{lang} · v{version} · Rust editor" }
                button { class: "mk-btn", disabled: !dirty, onclick: move |_| save_now.call(()), "Save" }
                button { class: "mk-btn", onclick: reload, title: "Discard edits and reload from the source", "Reload" }
                if codemirror_on {
                    button { class: "mk-btn mk-editor-switch", title: "Show this file in CodeMirror (language server, wiki-links, wrap)", onclick: move |_| { let mut ws = ws; ws.choose_editor(node, "codemirror"); }, "CodeMirror" }
                }
            }
            if let Some(e) = last_error() {
                div { class: "mk-editor-bar mk-editor-error", "{e}" }
            }
            div { class: "mk-cn-body",
                match language {
                    Some(language) => rsx! {
                        CodeEditor {
                            value: text,
                            language,
                            theme,
                            line_numbers: true,
                            aria_label: "{title}",
                            oninput: move |value: String| doc.with_mut(|d| d.text = value),
                        }
                    },
                    None => rsx! {
                        CodeEditor {
                            value: text,
                            theme,
                            line_numbers: true,
                            aria_label: "{title}",
                            oninput: move |value: String| doc.with_mut(|d| d.text = value),
                        }
                    },
                }
            }
        }
    }
}

/// Byte-agnostic: `offset` counts UTF-16 code units the way the browser
/// does; Moonkale's lines are counted by `\n`, columns in characters.
fn line_col(text: &str, offset: usize) -> (u32, u32) {
    let mut units = 0usize;
    let mut line = 0u32;
    let mut col = 0u32;
    for c in text.chars() {
        if units >= offset {
            break;
        }
        units += c.len_utf16();
        if c == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    (line, col)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offsets_become_lines_and_columns() {
        assert_eq!(line_col("ab\ncd", 0), (0, 0));
        assert_eq!(line_col("ab\ncd", 2), (0, 2));
        assert_eq!(line_col("ab\ncd", 3), (1, 0));
        assert_eq!(line_col("ab\ncd", 5), (1, 2));
        assert_eq!(line_col("é😀x", 3), (0, 2)); // 😀 is two UTF-16 units
    }

    #[test]
    fn hints_map_to_grammars() {
        assert!(language_of(Some("rust")).is_some());
        assert!(language_of(Some("julia")).is_some());
        assert!(language_of(Some("lean")).is_some());
        assert!(language_of(Some("nix")).is_some());
        assert!(language_of(Some("typst")).is_some());
        assert!(language_of(Some("pixi")).is_some());
        assert!(language_of(Some("no-such-language")).is_none());
        assert!(language_of(None).is_none());
    }
}
