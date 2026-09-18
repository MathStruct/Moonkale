use crate::panel::{Sessions, TerminalPanel};
use dioxus::prelude::*;
use moonkale_ext_api::prelude::*;

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
    fn manifest(&self) -> Manifest {
        Manifest {
            id: "dev.moonkale.editor-terminal",
            name: "Terminal",
        }
    }

    fn panels(&self, _ws: Workspace) -> Vec<PanelContribution> {
        vec![PanelContribution {
            id: PANEL_ID.into(),
            title: "Terminal".into(),
            home: PanelHome::Bottom,
            closable: false,
            dirty: false,
            node: None,
        }]
    }

    fn render(&self, _panel_id: &str, ws: Workspace) -> Element {
        rsx! { TerminalPanel { ws, sessions: self.sessions } }
    }
}
