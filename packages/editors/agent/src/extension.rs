use crate::panel::AgentPanel;
use dioxus::prelude::*;
use moonkale_ext_api::prelude::*;

pub const PANEL_ID: &str = "agent";

pub struct AgentExtension;

impl Extension for AgentExtension {
    fn manifest(&self) -> Manifest {
        Manifest {
            id: "dev.moonkale.editor-agent",
            name: "Agent",
        }
    }

    fn panels(&self, _ws: Workspace) -> Vec<PanelContribution> {
        vec![PanelContribution {
            id: PANEL_ID.into(),
            title: "Agent".into(),
            home: PanelHome::Right,
            closable: false,
            dirty: false,
            node: None,
        }]
    }

    fn render(&self, _panel_id: &str, ws: Workspace) -> Element {
        rsx! { AgentPanel { ws } }
    }
}
