//! `CodeEditorExtension` — one closable panel per open document.

use crate::panel::CodeEditorPanel;
use dioxus::prelude::*;
use moonkale_core::NodeId;
use moonkale_ext_api::prelude::*;

pub const PANEL_PREFIX: &str = "editor:";

pub struct CodeEditorExtension {
    lsp: crate::lsp::LspManager,
    /// Documents another extension claims (markdown → the markdown
    /// extension, which hosts this panel in its Source mode).
    skip: Option<fn(&moonkale_core::Node) -> bool>,
}

impl Default for CodeEditorExtension {
    fn default() -> Self {
        Self::new()
    }
}

impl CodeEditorExtension {
    pub fn new() -> Self {
        Self {
            lsp: crate::lsp::LspManager::new(),
            skip: None,
        }
    }

    /// Leave documents matching `f` to another extension.
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

impl Extension for CodeEditorExtension {
    fn manifest(&self) -> Manifest {
        Manifest::core(
            "dev.moonkale.editor-code",
            "Code Editor",
            "CodeMirror editor with language-server support.",
        )
    }

    fn panels(&self, ws: Workspace) -> Vec<PanelContribution> {
        ws.documents
            .read()
            .iter()
            .filter(|(_, doc)| !self.skip.is_some_and(|f| f(&doc.read().node)))
            // Milestone 14: a document shown by the Rust editor is not ours.
            .filter(|(id, _)| ws.editor_for(*id) == "codemirror")
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
            Some(node) => rsx! { CodeEditorPanel { ws, node, lsp: self.lsp } },
            None => rsx! { "unknown panel {panel_id}" },
        }
    }

    fn on_panel_closed(&self, panel_id: &str, ws: Workspace) {
        if let Some(node) = Self::node_of(panel_id) {
            ws.close_node(node);
        }
    }

    fn commands(&self, _ws: Workspace) -> Vec<CommandContribution> {
        vec![CommandContribution::new("editor.toggleWrap", "View: Toggle Word Wrap").key("Alt+Z")]
    }

    fn run_command(&self, id: &str, ws: Workspace) {
        if id == "editor.toggleWrap" {
            toggle_wrap(ws);
        }
    }

    // Milestone 13: the editor's settings live with the extension.
    fn settings(&self, ws: Workspace, target: SettingsTarget) -> Option<Element> {
        let wrap = ws.settings.read().editor.wrap;
        // Which editor opens a file is Settings → Which extension (Milestone 15).
        Some(rsx! {
            label { class: "mk-settings-check",
                input { r#type: "checkbox", checked: wrap,
                    onchange: move |e| { let v = e.checked(); ws.update_settings_in(target, move |f| f.editor.wrap = Some(v)); } }
                "Wrap long lines (Alt+Z toggles)"
            }
        })
    }
}

/// Flip `editor.wrap` in the user settings (spec 014); every open editor
/// follows through its settings effect.
pub fn toggle_wrap(ws: Workspace) {
    let next = !ws.settings.peek().editor.wrap;
    dioxus::core::spawn_forever(async move {
        ws.update_user_settings(|f| f.editor.wrap = Some(next))
            .await;
    });
}
