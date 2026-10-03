use crate::panel::AgentPanel;
use crate::L;
use dioxus::prelude::*;
use moonkale_ext_api::prelude::*;
use moonkale_ext_api::{i18n::Locales, t};

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

    fn panels(&self, ws: Workspace) -> Vec<PanelContribution> {
        vec![
            PanelContribution::new(PANEL_ID, t!(ws, L, "agent-title"), PanelHome::Right)
                .closable(true)
                .activity(Activity::new("agent", 60, t!(ws, L, "agent-title"))),
        ]
    }

    fn render(&self, _panel_id: &str, ws: Workspace) -> Element {
        // Sources on a server → the session runs there (Milestone 12).
        match ws.agent_sessions() {
            Some(api) => rsx! { crate::server_panel::ServerAgentPanel { ws, api } },
            None => rsx! { AgentPanel { ws } },
        }
    }

    // Milestone 7: the first extension-contributed command.
    fn commands(&self, ws: Workspace) -> Vec<CommandContribution> {
        vec![
            CommandContribution::new("agent.focus", t!(ws, L, "cmd-agent-focus"))
                .key("Ctrl+Shift+A"),
        ]
    }

    fn locales(&self) -> Locales {
        L
    }

    fn run_command(&self, id: &str, mut ws: Workspace) {
        if id == "agent.focus" {
            ws.dispatch(Command::ShowPanel(PANEL_ID));
            ws.focus_element(crate::panel::INPUT_ID);
        }
    }

    // Milestone 13/15: the agent's own settings — the policy and where turns
    // run. The saved agents (language models, keys) are Settings → Agents.
    fn settings(&self, ws: Workspace, target: SettingsTarget) -> Option<Element> {
        let (on_server, allow_writes, denied) = {
            let s = ws.settings.resolved.read();
            (
                s.agent.on_server,
                s.policy.allow_writes,
                s.policy.denied_tools.join(", "),
            )
        };
        Some(rsx! {
            // A folder may not auto-approve writes (Milestone 18 phase 4.5):
            // in the workspace target the switch is shown, not offered.
            label { class: "mk-settings-check",
                title: if target == SettingsTarget::Workspace { t!(ws, L, "agent-allow-writes-folder") } else { String::new() },
                input { r#type: "checkbox", checked: allow_writes, disabled: target == SettingsTarget::Workspace,
                    onchange: move |e| { let v = e.checked(); ws.update_settings_in(target, move |f| f.policy.allow_writes = Some(v)); } }
                {t!(ws, L, "agent-allow-writes")}
            }
            label { class: "mk-settings-field", "Denied tools"
                input { class: "mk-input", value: "{denied}", placeholder: "comma-separated tool names",
                    onchange: move |e| { let v: Vec<String> = e.value().split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(); ws.update_settings_in(target, move |f| f.policy.denied_tools = Some(v)); } }
            }
            label { class: "mk-settings-check",
                input { r#type: "checkbox", checked: on_server,
                    onchange: move |e| { let v = e.checked(); ws.update_settings_in(target, move |f| f.agent.on_server = Some(v)); } }
                {t!(ws, L, "agent-on-server")}
            }
            p { class: "mk-muted", "The saved agents (language models, keys) are under Settings → Agents; each session picks one." }
        })
    }
}
