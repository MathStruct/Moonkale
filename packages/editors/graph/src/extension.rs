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
        let mut panels =
            vec![
                PanelContribution::new(PANEL_ID, t!(ws, L, "graph-title"), PanelHome::Main)
                    .closable(true)
                    .activity(Activity::new("graph", 80, t!(ws, L, "graph-title"))),
            ];
        for id in ws.links.views.read().keys() {
            panels.push(
                PanelContribution::new(id.clone(), t!(ws, L, "graph-title"), PanelHome::Right)
                    .closable(true),
            );
        }
        panels
    }

    fn preview(
        &self,
        panel_id: &str,
        ws: Workspace,
        _size: moonkale_ext_api::preview::PreviewSize,
    ) -> Option<Element> {
        let _ = ws;
        let id = panel_id
            .strip_prefix("graph:")
            .map(|id| format!("mk-graph-panel-{id}"))
            .unwrap_or_else(|| "mk-graph-panel".into());
        moonkale_ext_api::preview::graph(id, false)
    }

    fn render(&self, panel_id: &str, ws: Workspace) -> Element {
        rsx! { GraphPanel { ws, panel_id: panel_id.to_string() } }
    }
}
