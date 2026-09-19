use crate::links_panel::LinksPanel;
use crate::rich::RichPanel;
use crate::typst_preview::TypstPreviewPanel;
use dioxus::prelude::*;
use moonkale_editor_code::{lsp::LspManager, CodeEditorPanel};
use moonkale_ext_api::prelude::*;
use std::collections::HashSet;

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
    /// Documents currently shown in Rich mode (per window).
    rich: Signal<HashSet<NodeId>>,
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
            rich: Signal::new_in_scope(HashSet::new(), ScopeId::ROOT),
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
            closable: false,
            dirty: false,
            node: None,
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
                closable: false,
                dirty: false,
                node: None,
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
}

/// Source (CodeMirror) or Rich (Milkdown) for one markdown document.
#[component]
fn MarkdownPanel(
    ws: Workspace,
    node: NodeId,
    lsp: LspManager,
    rich: Signal<HashSet<NodeId>>,
) -> Element {
    let is_rich = rich.read().contains(&node);
    rsx! {
        div { class: "mk-md",
            div { class: "mk-md-modes",
                button { class: if !is_rich { "mk-btn mk-btn-on" } else { "mk-btn" }, onclick: move |_| { rich.with_mut(|r| { r.remove(&node); }); }, title: "Markdown source (CodeMirror)", "Source" }
                button { class: if is_rich { "mk-btn mk-btn-on" } else { "mk-btn" }, onclick: move |_| { rich.with_mut(|r| { r.insert(node); }); }, title: "WYSIWYG (Milkdown) — the markdown is what gets saved", "Rich" }
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
