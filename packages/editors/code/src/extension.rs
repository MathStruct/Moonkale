//! `CodeEditorExtension` — one closable panel per open document.

use crate::panel::CodeEditorPanel;
use dioxus::prelude::*;
use moonkale_core::NodeId;
use moonkale_ext_api::prelude::*;

pub const PANEL_PREFIX: &str = "editor:";

pub struct CodeEditorExtension {
    lsp: crate::lsp::LspManager,
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
        }
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
        Manifest {
            id: "dev.moonkale.editor-code",
            name: "Code Editor",
        }
    }

    fn panels(&self, ws: Workspace) -> Vec<PanelContribution> {
        ws.documents
            .read()
            .iter()
            .map(|(id, doc)| {
                let d = doc.read();
                PanelContribution {
                    id: Self::panel_id(*id),
                    title: d.node.label.clone(),
                    home: PanelHome::Main,
                    closable: true,
                    dirty: d.dirty(),
                    node: Some(*id),
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
}
