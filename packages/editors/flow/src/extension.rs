use crate::panel::FlowPanel;
use dioxus::prelude::*;
use moonkale_ext_api::prelude::*;

/// Same scheme as the code editor so tabs, closing and `active_panel` agree.
pub const EDITOR_PREFIX: &str = "editor:";

/// `*.flow.json` documents belong to the flow editor.
pub fn is_flow(node: &Node) -> bool {
    node.native_key
        .to_ascii_lowercase()
        .ends_with(moonkale_ext_api::flow::Flow::EXTENSION)
}

pub struct FlowExtension;

impl Extension for FlowExtension {
    fn manifest(&self) -> Manifest {
        Manifest::opt_in(
            "dev.moonkale.editor-flow",
            "Flow editor",
            "Drag-and-drop blocks wired by typed ports (*.flow.json). Block libraries come from other extensions.",
        )
        .with_permissions(&["write-files"])
    }

    fn panels(&self, ws: Workspace) -> Vec<PanelContribution> {
        ws.documents
            .read()
            .iter()
            .filter(|(_, d)| is_flow(&d.read().node))
            .map(|(id, doc)| {
                let d = doc.read();
                PanelContribution {
                    id: format!("{EDITOR_PREFIX}{id}"),
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
        match panel_id
            .strip_prefix(EDITOR_PREFIX)
            .and_then(|s| s.parse::<NodeId>().ok())
        {
            Some(node) => rsx! { FlowPanel { ws, node } },
            None => rsx! { "unknown panel {panel_id}" },
        }
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
