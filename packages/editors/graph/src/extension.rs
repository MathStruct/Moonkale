use crate::panel::GraphPanel;
use dioxus::prelude::*;
use moonkale_ext_api::prelude::*;

pub const PANEL_ID: &str = "graph";

pub struct GraphExtension;

impl Extension for GraphExtension {
    fn manifest(&self) -> Manifest {
        Manifest::optional(
            "dev.moonkale.editor-graph",
            "Graph View",
            "The wgpu graph of the folder, databases and traces.",
        )
    }

    fn panels(&self, _ws: Workspace) -> Vec<PanelContribution> {
        vec![PanelContribution::new(PANEL_ID, "Graph", PanelHome::Main)
            .closable(true)
            .activity(Activity::new("graph", 80, "Graph"))]
    }

    fn render(&self, _panel_id: &str, ws: Workspace) -> Element {
        rsx! { GraphPanel { ws } }
    }
}
