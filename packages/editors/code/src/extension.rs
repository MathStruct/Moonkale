//! `CodeEditorExtension` — one closable panel per open document.

use crate::L;
use dioxus::prelude::*;
use moonkale_code_view::{CodeEditorPanel, RustCodeEditorPanel};
use moonkale_core::NodeId;
use moonkale_ext_api::prelude::*;
use moonkale_ext_api::{i18n::Locales, t};

pub const PANEL_PREFIX: &str = "editor:";

pub struct CodeEditorExtension;

impl Default for CodeEditorExtension {
    fn default() -> Self {
        Self::new()
    }
}

impl CodeEditorExtension {
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

impl Extension for CodeEditorExtension {
    fn locales(&self) -> Locales {
        L
    }

    fn manifest(&self) -> Manifest {
        Manifest::core(
            "dev.moonkale.editor-code",
            "Code Editor",
            "Code editor with Rust and CodeMirror views and language-server support.",
        )
    }

    fn panels(&self, ws: Workspace) -> Vec<PanelContribution> {
        ws.docs
            .open
            .read()
            .iter()
            // Which documents end up here is the shell's decision
            // (`claims`): markdown and flow files go to their editors, and
            // the Rust editor takes the ones the user switched to it.
            .map(|(id, doc)| {
                let d = doc.read();
                PanelContribution::new(Self::panel_id(*id), d.node.label.clone(), PanelHome::Main)
                    .closable(true)
                    .dirty(d.dirty())
                    .node(*id)
            })
            .collect()
    }

    /// Any text document, at the lowest priority.
    fn claims(&self, node: &moonkale_core::Node) -> Option<u8> {
        matches!(node.content, Some(moonkale_core::ContentRef::Text { .. })).then_some(10)
    }

    fn render(&self, panel_id: &str, ws: Workspace) -> Element {
        match Self::node_of(panel_id) {
            Some(node) => {
                // Minimal distributions need no optional extension to honor the
                // backend setting. Explicit extension disabling still falls back.
                let native = ws.editor_for(node) == "native"
                    && ws.settings.resolved.read().extensions.is_enabled_id(
                        "dev.moonkale.editor-code-native",
                        moonkale_ext_api::settings::DEFAULT_CODE_EDITOR == "native",
                    );
                let lsp = moonkale_code_view::lsp::LspManager::for_workspace(ws);
                if native {
                    rsx! { RustCodeEditorPanel { ws, node, lsp } }
                } else {
                    rsx! { CodeEditorPanel { ws, node, lsp } }
                }
            }
            None => rsx! { "unknown panel {panel_id}" },
        }
    }

    fn on_panel_closed(&self, panel_id: &str, ws: Workspace) {
        if let Some(node) = Self::node_of(panel_id) {
            ws.close_node(node);
        }
    }

    fn commands(&self, ws: Workspace) -> Vec<CommandContribution> {
        let mut commands =
            vec![
                CommandContribution::new("editor.toggleWrap", t!(ws, L, "cmd-toggle-wrap"))
                    .key("Alt+Z"),
            ];
        if ws.settings.resolved.read().extensions.is_enabled_id(
            "dev.moonkale.editor-code-native",
            moonkale_ext_api::settings::DEFAULT_CODE_EDITOR == "native",
        ) {
            commands.extend(moonkale_code_view::native_commands(ws));
        }
        commands
    }

    fn run_command(&self, id: &str, ws: Workspace) {
        if id == "editor.toggleWrap" {
            moonkale_code_view::toggle_wrap(ws);
        } else {
            moonkale_code_view::run_native_command(id, ws);
        }
    }

    // Milestone 13: the editor's settings live with the extension.
    fn settings(&self, ws: Workspace, target: SettingsTarget) -> Option<Element> {
        Some(moonkale_code_view::editor_settings(
            ws,
            target,
            t!(ws, L, "code-wrap-setting"),
        ))
    }
}
