use crate::panel::ImagePanel;
use dioxus::prelude::*;
use moonkale_core::NodeId;
use moonkale_ext_api::prelude::*;

pub const PANEL_PREFIX: &str = "image:";

pub struct ImageExtension;

impl ImageExtension {
    pub fn panel_id(node: NodeId) -> String {
        format!("{PANEL_PREFIX}{node}")
    }
    fn node_of(panel_id: &str) -> Option<NodeId> {
        panel_id.strip_prefix(PANEL_PREFIX)?.parse().ok()
    }
}

impl Extension for ImageExtension {
    fn manifest(&self) -> Manifest {
        Manifest::optional(
            "dev.moonkale.editor-image",
            "Image viewer",
            "Shows png, jpg, gif, webp, svg, bmp, ico and avif files: zoom, pan, dimensions.",
        )
    }

    fn panels(&self, ws: Workspace) -> Vec<PanelContribution> {
        ws.views
            .read()
            .iter()
            .filter(|n| crate::is_image(n))
            .map(|n| PanelContribution {
                id: Self::panel_id(n.id),
                title: n.label.clone(),
                home: PanelHome::Main,
                closable: true,
                dirty: false,
                node: Some(n.id),
                activity: None,
            })
            .collect()
    }

    fn render(&self, panel_id: &str, ws: Workspace) -> Element {
        match Self::node_of(panel_id)
            .and_then(|id| ws.views.read().iter().find(|n| n.id == id).cloned())
        {
            Some(node) => rsx! { ImagePanel { ws, node } },
            None => rsx! { "unknown image panel {panel_id}" },
        }
    }

    fn on_panel_closed(&self, panel_id: &str, ws: Workspace) {
        if let Some(node) = Self::node_of(panel_id) {
            ws.close_node(node);
        }
    }
}
