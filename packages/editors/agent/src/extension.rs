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
        let mut panels =
            vec![
                PanelContribution::new(PANEL_ID, t!(ws, L, "agent-title"), PanelHome::Right)
                    .closable(true)
                    .activity(Activity::new("agent", 60, t!(ws, L, "agent-title"))),
            ];
        if crate::proposals::Inbox::opened(ws) {
            panels.push(
                PanelContribution::new("proposals", t!(ws, L, "proposal-title"), PanelHome::Main)
                    .closable(true),
            );
        }
        let state = crate::display::Displays::get(ws);
        let outer = ws.saved_sessions.active.read().clone();
        for (group, display) in state.panels.read().iter().filter(|(_, d)| {
            d.outer == outer
                && matches!(
                    d.content,
                    Some(moonkale_llm::display::Canvas::Markdown { .. })
                )
        }) {
            panels.push(
                PanelContribution::new(
                    format!("agent-canvas:{group}"),
                    display.name.clone(),
                    PanelHome::Right,
                )
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
        if let Some(group) = panel_id.strip_prefix("agent-canvas:") {
            let state = crate::display::Displays::get(ws);
            let panels = state.panels.peek();
            if let Some(moonkale_llm::display::Canvas::Markdown { text }) =
                panels.get(group).and_then(|d| d.content.as_ref())
            {
                return moonkale_ext_api::preview::text(text);
            }
            return None;
        }
        if ws.agent_sessions().is_some() {
            return crate::server_panel::preview(ws);
        }
        let session = crate::panel::Chats::get(ws).current_session()?;
        let items = session.items.peek();
        let text = items
            .iter()
            .rev()
            .take(8)
            .filter_map(|item| match item {
                crate::panel::Item::User(s) | crate::panel::Item::Assistant(s) => {
                    Some(moonkale_ext_api::preview::bounded_text(s))
                }
                _ => None,
            })
            .take(2)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n");
        drop(items);
        let body = moonkale_ext_api::preview::text(&text);
        Some(rsx! { {body} crate::panel::PreviewStatus { session } })
    }

    fn render(&self, panel_id: &str, ws: Workspace) -> Element {
        if let Some(group) = panel_id.strip_prefix("agent-canvas:") {
            return rsx! { crate::display::CanvasPanel { ws, group: group.to_string() } };
        }
        if panel_id == "proposals" {
            return rsx! { crate::proposals::Panel { ws } };
        }
        // Sources on a server → the session runs there (Milestone 12).
        match ws.agent_sessions() {
            Some(api) => {
                rsx! { crate::display::Offers { ws } crate::server_panel::ServerAgentPanel { ws, api } }
            }
            None => rsx! { crate::display::Offers { ws } AgentPanel { ws } },
        }
    }

    // Milestone 7: the first extension-contributed command.
    fn commands(&self, ws: Workspace) -> Vec<CommandContribution> {
        vec![
            CommandContribution::new("agent.proposals", t!(ws, L, "proposal-title")),
            CommandContribution::new("agent.focus", t!(ws, L, "cmd-agent-focus"))
                .key("Ctrl+Shift+A"),
        ]
    }

    fn locales(&self) -> Locales {
        L
    }

    fn run_command(&self, id: &str, mut ws: Workspace) {
        if id == "agent.proposals" {
            crate::proposals::Inbox::open(ws);
        }
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
                    onchange: moonkale_ext_api::crash::event(move |e: dioxus::prelude::FormEvent| { let v = e.checked(); ws.update_settings_in(target, move |f| f.policy.allow_writes = Some(v)); } )}
                {t!(ws, L, "agent-allow-writes")}
            }
            label { class: "mk-settings-field", "Denied tools"
                input { class: "mk-input", value: "{denied}", placeholder: "comma-separated tool names",
                    onchange: moonkale_ext_api::crash::event(move |e: dioxus::prelude::FormEvent| { let v: Vec<String> = e.value().split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(); ws.update_settings_in(target, move |f| f.policy.denied_tools = Some(v)); } )}
            }
            label { class: "mk-settings-check",
                input { r#type: "checkbox", checked: on_server,
                    onchange: moonkale_ext_api::crash::event(move |e: dioxus::prelude::FormEvent| { let v = e.checked(); ws.update_settings_in(target, move |f| f.agent.on_server = Some(v)); } )}
                {t!(ws, L, "agent-on-server")}
            }
            p { class: "mk-muted", "The saved agents (language models, keys) are under Settings → Agents; each session picks one." }
        })
    }
}
