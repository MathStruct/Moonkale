use crate::panel::TablePanel;
use dioxus::prelude::*;
use moonkale_core::{NodeId, NodeKind};
use moonkale_ext_api::prelude::*;

pub const PANEL_PREFIX: &str = "table:";

pub struct TableExtension;

impl TableExtension {
    pub fn panel_id(node: NodeId) -> String {
        format!("{PANEL_PREFIX}{node}")
    }
    fn node_of(panel_id: &str) -> Option<NodeId> {
        panel_id.strip_prefix(PANEL_PREFIX)?.parse().ok()
    }
}

impl Extension for TableExtension {
    fn manifest(&self) -> Manifest {
        Manifest::optional(
            "dev.moonkale.editor-table",
            "Tables",
            "SQL / Cypher query box and result grid for database sources.",
        )
    }

    fn panels(&self, ws: Workspace) -> Vec<PanelContribution> {
        ws.views
            .read()
            .iter()
            .filter(|n| n.kind == NodeKind::Table)
            .map(|n| PanelContribution {
                id: Self::panel_id(n.id),
                title: n.label.clone(),
                home: PanelHome::Main,
                closable: true,
                dirty: false,
                node: Some(n.id),
            })
            .collect()
    }

    fn render(&self, panel_id: &str, ws: Workspace) -> Element {
        match Self::node_of(panel_id)
            .and_then(|id| ws.views.read().iter().find(|n| n.id == id).cloned())
        {
            Some(node) => rsx! { TablePanel { ws, node } },
            None => rsx! { "unknown table panel {panel_id}" },
        }
    }

    fn on_panel_closed(&self, panel_id: &str, ws: Workspace) {
        if let Some(node) = Self::node_of(panel_id) {
            ws.close_node(node);
        }
    }
}
