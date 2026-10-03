use crate::panel::GraphPanel;
use crate::L;
use dioxus::prelude::*;
use moonkale_ext_api::prelude::*;
use moonkale_ext_api::{i18n::Locales, t};

pub const PANEL_ID: &str = "graph";

pub struct GraphExtension;

impl Extension for GraphExtension {
    fn locales(&self) -> Locales {
        L
    }

    fn manifest(&self) -> Manifest {
        Manifest::optional(
            "dev.moonkale.editor-graph",
            "Graph View",
            "The wgpu graph of the folder, databases and traces.",
        )
    }

    fn panels(&self, ws: Workspace) -> Vec<PanelContribution> {
        vec![
            PanelContribution::new(PANEL_ID, t!(ws, L, "graph-title"), PanelHome::Main)
                .closable(true)
                .activity(Activity::new("graph", 80, t!(ws, L, "graph-title"))),
        ]
    }

    fn render(&self, _panel_id: &str, ws: Workspace) -> Element {
        rsx! { GraphPanel { ws } }
    }
}
