//! Opt-in extension adapter to the shared Rust code view in `moonkale-code-view`.

use crate::L;
use dioxus::prelude::*;
use moonkale_code_view::RustCodeEditorPanel;
use moonkale_ext_api::prelude::*;

pub const PANEL_PREFIX: &str = "editor-native:";

#[derive(Default)]
pub struct NativeCodeExtension;

impl NativeCodeExtension {
    pub fn new() -> Self {
        Self
    }

    pub fn panel_id(node: NodeId) -> String {
        format!("{PANEL_PREFIX}{node}")
    }

    fn node_of(panel_id: &str) -> Option<NodeId> {
        panel_id.strip_prefix(PANEL_PREFIX)?.parse().ok()
    }
}

impl Extension for NativeCodeExtension {
    fn locales(&self) -> moonkale_ext_api::i18n::Locales {
        L
    }

    fn manifest(&self) -> Manifest {
        Manifest::opt_in(
            "dev.moonkale.editor-code-native",
            "Code Editor (Rust)",
            "A code editor without JavaScript bundles: editor-core, tree-sitter highlighting in Rust for every core language (Lean, Nix and Typst included). Language-server diagnostics included.",
        )
    }

    fn panels(&self, ws: Workspace) -> Vec<PanelContribution> {
        ws.docs
            .open
            .read()
            .iter()
            // The shell keeps the ones this editor wins (`claims`; a tie
            // with CodeMirror goes to the user's choice).
            .map(|(id, doc)| {
                let d = doc.read();
                PanelContribution::new(Self::panel_id(*id), d.node.label.clone(), PanelHome::Main)
                    .closable(true)
                    .dirty(d.dirty())
                    .node(*id)
            })
            .collect()
    }

    /// Any text document, like CodeMirror (the user's choice breaks the tie).
    fn claims(&self, node: &moonkale_core::Node) -> Option<u8> {
        matches!(node.content, Some(moonkale_core::ContentRef::Text { .. })).then_some(10)
    }

    fn render(&self, panel_id: &str, ws: Workspace) -> Element {
        match Self::node_of(panel_id) {
            Some(node) => {
                rsx! { RustCodeEditorPanel { ws, node, lsp: moonkale_code_view::lsp::LspManager::for_workspace(ws) } }
            }
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
