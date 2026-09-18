use crate::panel::AgentPanel;
use dioxus::prelude::*;
use moonkale_ext_api::prelude::*;

pub const PANEL_ID: &str = "agent";

pub struct AgentExtension;

impl Extension for AgentExtension {
    fn manifest(&self) -> Manifest {
        Manifest::optional("dev.moonkale.editor-agent", "Agent", "The in-app LLM assistant with tools under the policy gate.").with_permissions(&["read-sources", "write-files", "run-commands", "network"])
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
