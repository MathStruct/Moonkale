use crate::panel::GraphPanel;
use dioxus::prelude::*;
use moonkale_ext_api::prelude::*;

pub const PANEL_ID: &str = "graph";

pub struct GraphExtension;

impl Extension for GraphExtension {
    fn manifest(&self) -> Manifest {
        Manifest::optional("dev.moonkale.editor-graph", "Graph View", "The wgpu graph of the folder, databases and traces.")
    }

    fn panels(&self, _ws: Workspace) -> Vec<PanelContribution> {
        vec![PanelContribution {
            id: PANEL_ID.into(),
            title: "Graph".into(),
            home: PanelHome::Main,
            closable: false,
            dirty: false,
            node: None,
        }]
    }

    fn render(&self, _panel_id: &str, ws: Workspace) -> Element {
        rsx! { GraphPanel { ws } }
    }
}
