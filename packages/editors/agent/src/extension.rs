use crate::panel::AgentPanel;
use dioxus::prelude::*;
use moonkale_ext_api::prelude::*;

pub const PANEL_ID: &str = "agent";

pub struct AgentExtension;

impl Extension for AgentExtension {
    fn manifest(&self) -> Manifest {
        Manifest::optional(
            "dev.moonkale.editor-agent",
            "Agent",
            "The in-app LLM assistant with tools under the policy gate.",
        )
        .with_permissions(&["read-sources", "write-files", "run-commands", "network"])
    }

    fn panels(&self, _ws: Workspace) -> Vec<PanelContribution> {
        vec![PanelContribution {
            id: PANEL_ID.into(),
            title: "Agent".into(),
            home: PanelHome::Right,
            closable: true,
            dirty: false,
            node: None,
            activity: Some(Activity::new("agent", 60, "Agent")),
        }]
    }

    fn render(&self, _panel_id: &str, ws: Workspace) -> Element {
        // Sources on a server → the session runs there (Milestone 12).
        match ws.agent_sessions() {
            Some(api) => rsx! { crate::server_panel::ServerAgentPanel { ws, api } },
            None => rsx! { AgentPanel { ws } },
        }
    }

    // Milestone 7: the first extension-contributed command.
    fn commands(&self, _ws: Workspace) -> Vec<CommandContribution> {
        vec![CommandContribution::new("agent.focus", "Agent: Ask the agent…").key("Ctrl+Shift+A")]
    }

    fn run_command(&self, id: &str, mut ws: Workspace) {
        if id == "agent.focus" {
            ws.dispatch(Command::ShowPanel(PANEL_ID));
            ws.focus_element(crate::panel::INPUT_ID);
        }
    }

    // Milestone 13: the agent's own settings; the language model stays under Settings.
    fn settings(&self, ws: Workspace, target: SettingsTarget) -> Option<Element> {
        let on_server = ws.settings.read().agent.on_server;
        Some(rsx! {
            label { class: "mk-settings-check",
                input { r#type: "checkbox", checked: on_server,
                    onchange: move |e| { let v = e.checked(); ws.update_settings_in(target, move |f| f.agent.on_server = Some(v)); } }
                "Run turns on the server — they finish without a window, and another device sees the state (web, or a desktop connected to a server)"
            }
            p { class: "mk-muted", "The language model, its keys and the policy are under Settings → Language model." }
        })
    }
}
