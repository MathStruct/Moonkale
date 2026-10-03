use crate::panel::{Sessions, TerminalPanel};
use crate::L;
use dioxus::prelude::*;
use moonkale_ext_api::prelude::*;
use moonkale_ext_api::t;

pub const PANEL_ID: &str = "terminal";

pub struct TerminalExtension {
    sessions: Sessions,
}

impl Default for TerminalExtension {
    fn default() -> Self {
        Self::new()
    }
}

impl TerminalExtension {
    pub fn new() -> Self {
        Self {
            sessions: Sessions::new(),
        }
    }
}

impl Extension for TerminalExtension {
    fn locales(&self) -> moonkale_ext_api::i18n::Locales {
        L
    }

    fn manifest(&self) -> Manifest {
        Manifest::optional(
            "dev.moonkale.editor-terminal",
            "Terminal",
            "Shell sessions (local PTY on desktop, server relay on web).",
        )
        .with_permissions(&["run-commands"])
    }

    fn panels(&self, ws: Workspace) -> Vec<PanelContribution> {
        vec![
            PanelContribution::new(PANEL_ID, t!(ws, L, "terminal-title"), PanelHome::Bottom)
                .closable(true)
                .activity(
                    Activity::new("terminal", 70, t!(ws, L, "terminal-title"))
                        .badge(self.sessions.list.read().len() as u32),
                ),
        ]
    }

    fn render(&self, _panel_id: &str, ws: Workspace) -> Element {
        rsx! { TerminalPanel { ws, sessions: self.sessions } }
    }

    // Milestone 13: the terminal's settings live with the extension.
    fn settings(&self, ws: Workspace, target: SettingsTarget) -> Option<Element> {
        let shell = ws
            .settings
            .resolved
            .read()
            .terminal
            .shell
            .clone()
            .unwrap_or_default();
        // Which terminal opens is Settings → Which extension (Milestone 15).
        Some(rsx! {
            label { class: "mk-settings-field", "Shell"
                input { class: "mk-input", value: "{shell}", placeholder: "$SHELL",
                    onchange: move |e| { let v = e.value(); ws.update_settings_in(target, move |f| f.terminal.shell = if v.trim().is_empty() { None } else { Some(v) }); } }
            }
        })
    }
}
