use crate::links_panel::LinksPanel;
use crate::rich::RichPanel;
use crate::typst_preview::TypstPreviewPanel;
use dioxus::prelude::*;
use moonkale_editor_code::{lsp::LspManager, CodeEditorPanel};
use moonkale_ext_api::prelude::*;
use std::collections::HashMap;

pub const PANEL_ID: &str = "links";
pub const PREVIEW_ID: &str = "typst-preview";
/// Same scheme as the code editor so tabs, closing and `active_panel` agree.
pub const EDITOR_PREFIX: &str = "editor:";

/// `.md` / `.markdown` documents are this extension's (Source | Rich).
pub fn is_markdown(node: &Node) -> bool {
    let k = node.native_key.to_ascii_lowercase();
    k.ends_with(".md") || k.ends_with(".markdown")
}

pub struct LinksExtension {
    lsp: LspManager,
    /// Documents whose mode the user chose by hand (per window): `true` =
    /// Rich, `false` = Source; the rest follow `editor.markdown_rich`.
    rich: Signal<HashMap<NodeId, bool>>,
}

impl Default for LinksExtension {
    fn default() -> Self {
        Self::new()
    }
}

impl LinksExtension {
    pub fn new() -> Self {
        Self {
            lsp: LspManager::new(),
            rich: Signal::new_in_scope(HashMap::new(), ScopeId::ROOT),
        }
    }
}

impl Extension for LinksExtension {
    fn manifest(&self) -> Manifest {
        Manifest::optional(
            "dev.moonkale.editor-markdown",
            "Markdown",
            "Links panel, Source | Rich markdown tabs, Typst preview.",
        )
    }

    fn panels(&self, ws: Workspace) -> Vec<PanelContribution> {
        let mut panels = vec![PanelContribution {
            id: PANEL_ID.into(),
            title: "Links".into(),
            home: PanelHome::Side,
            closable: true,
            dirty: false,
            node: None,
            activity: Some(Activity::new("links", 30, "Links").phone_secondary()),
        }];
        // A preview tab exists while any .typ document is open and the platform can compile.
        let any_typ = ws
            .documents
            .read()
            .iter()
            .any(|(_, d)| d.read().node.native_key.ends_with(".typ"));
        if any_typ && ws.compile_typst().is_some() {
            panels.push(PanelContribution {
                id: PREVIEW_ID.into(),
                title: "Typst preview".into(),
                home: PanelHome::Main,
                closable: true,
                dirty: false,
                node: None,
                activity: None,
            });
        }
        // Markdown documents: one editor tab each (Source | Rich).
        for (id, doc) in ws.documents.read().iter() {
            let d = doc.read();
            if is_markdown(&d.node) {
                panels.push(PanelContribution {
                    id: format!("{EDITOR_PREFIX}{id}"),
                    title: d.node.label.clone(),
                    home: PanelHome::Main,
                    closable: true,
                    dirty: d.dirty(),
                    node: Some(*id),
                    activity: None,
                });
            }
        }
        panels
    }

    fn render(&self, panel_id: &str, ws: Workspace) -> Element {
        if panel_id == PREVIEW_ID {
            return rsx! { TypstPreviewPanel { ws } };
        }
        if let Some(node) = panel_id
            .strip_prefix(EDITOR_PREFIX)
            .and_then(|s| s.parse::<NodeId>().ok())
        {
            return rsx! { MarkdownPanel { ws, node, lsp: self.lsp, rich: self.rich } };
        }
        rsx! { LinksPanel { ws } }
    }

    fn on_panel_closed(&self, panel_id: &str, ws: Workspace) {
        if let Some(node) = panel_id
            .strip_prefix(EDITOR_PREFIX)
            .and_then(|s| s.parse::<NodeId>().ok())
        {
            ws.close_node(node);
        }
    }

    // Milestone 13: the markdown editor's settings live with the extension.
    fn settings(&self, ws: Workspace, target: SettingsTarget) -> Option<Element> {
        let (rich, size, font, code_font) = {
            let s = ws.settings.read();
            (
                s.editor.markdown_rich,
                s.editor.rich_font_size,
                s.editor.rich_font.clone(),
                s.editor.rich_code_font.clone(),
            )
        };
        Some(rsx! {
            label { class: "mk-settings-check",
                input { r#type: "checkbox", checked: rich,
                    onchange: move |e| { let v = e.checked(); ws.update_settings_in(target, move |f| f.editor.markdown_rich = Some(v)); } }
                "Open markdown files in Rich mode (Source | Rich still switches)"
            }
            // Typography of the rich editor (Prompt23).
            label { class: "mk-settings-field", "Rich editor font size (px)"
                input { class: "mk-input mk-settings-rich-size", r#type: "number", min: "8", max: "48", value: "{size}",
                    onchange: move |e| { if let Ok(v) = e.value().parse::<u32>() { ws.update_settings_in(target, move |f| f.editor.rich_font_size = Some(v)); } } }
            }
            label { class: "mk-settings-field", "Rich editor font (CSS font-family; empty = the theme's)"
                input { class: "mk-input mk-settings-rich-font", value: "{font}", placeholder: "e.g. \"Source Serif 4\", Georgia, serif",
                    onchange: move |e| { let v = e.value(); ws.update_settings_in(target, move |f| f.editor.rich_font = if v.trim().is_empty() { None } else { Some(v) }); } }
            }
            label { class: "mk-settings-field", "Code font in rich notes (CSS font-family)"
                input { class: "mk-input mk-settings-rich-code-font", value: "{code_font}", placeholder: "e.g. \"JetBrains Mono\", monospace",
                    onchange: move |e| { let v = e.value(); ws.update_settings_in(target, move |f| f.editor.rich_code_font = if v.trim().is_empty() { None } else { Some(v) }); } }
            }
        })
    }
}

/// Source (CodeMirror) or Rich (Milkdown) for one markdown document.
#[component]
fn MarkdownPanel(
    ws: Workspace,
    node: NodeId,
    lsp: LspManager,
    rich: Signal<HashMap<NodeId, bool>>,
) -> Element {
    // The user's choice for this document, else the setting (spec 021).
    let is_rich = rich
        .read()
        .get(&node)
        .copied()
        .unwrap_or_else(|| ws.settings.read().editor.markdown_rich);
    rsx! {
        div { class: "mk-md",
            div { class: "mk-md-modes",
                button { class: if !is_rich { "mk-btn mk-btn-on" } else { "mk-btn" }, onclick: move |_| { rich.with_mut(|r| { r.insert(node, false); }); }, title: "Markdown source (CodeMirror)", "Source" }
                button { class: if is_rich { "mk-btn mk-btn-on" } else { "mk-btn" }, onclick: move |_| { rich.with_mut(|r| { r.insert(node, true); }); }, title: "WYSIWYG (Milkdown) — the markdown is what gets saved", "Rich" }
            }
            div { class: "mk-md-body",
                if is_rich {
                    RichPanel { ws, node }
                } else {
                    CodeEditorPanel { ws, node, lsp }
                }
            }
        }
    }
}
