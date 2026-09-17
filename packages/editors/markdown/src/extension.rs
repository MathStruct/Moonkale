use crate::links_panel::LinksPanel;
use dioxus::prelude::*;
use moonkale_ext_api::prelude::*;

pub const PANEL_ID: &str = "links";

pub struct LinksExtension;

impl Extension for LinksExtension {
    fn manifest(&self) -> Manifest {
        Manifest {
            id: "dev.moonkale.editor-markdown",
            name: "Markdown",
        }
    }

    fn panels(&self, _ws: Workspace) -> Vec<PanelContribution> {
        vec![PanelContribution {
            id: PANEL_ID.into(),
            title: "Links".into(),
            home: PanelHome::Side,
            closable: false,
            dirty: false,
            node: None,
        }]
    }

    fn render(&self, _panel_id: &str, ws: Workspace) -> Element {
        rsx! { LinksPanel { ws } }
    }
}
