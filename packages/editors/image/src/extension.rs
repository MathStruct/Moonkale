use crate::panel::ImagePanel;
use crate::L;
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
    fn locales(&self) -> moonkale_ext_api::i18n::Locales {
        L
    }

    fn manifest(&self) -> Manifest {
        Manifest::optional(
            "dev.moonkale.editor-image",
            "Image viewer",
            "Shows png, jpg, gif, webp, svg, bmp, ico and avif files: zoom, pan, dimensions.",
        )
    }

    fn panels(&self, ws: Workspace) -> Vec<PanelContribution> {
        ws.docs
            .views
            .read()
            .iter()
            .filter(|n| crate::is_image(n))
            .map(|n| {
                PanelContribution::new(Self::panel_id(n.id), n.label.clone(), PanelHome::Main)
                    .closable(true)
                    .node(n.id)
            })
            .collect()
    }

    /// Its own format, above the code editors (Milestone 18 phase 2).
    fn claims(&self, node: &moonkale_core::Node) -> Option<u8> {
        (crate::is_image(node)).then_some(50)
    }

    fn render(&self, panel_id: &str, ws: Workspace) -> Element {
        match Self::node_of(panel_id)
            .and_then(|id| ws.docs.views.read().iter().find(|n| n.id == id).cloned())
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
