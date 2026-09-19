//! # moonkale-ext-git
//!
//! Git as a first-class part of the workspace (Milestone 7): a *Changes*
//! panel (status, stage/unstage/discard, commit, branch, log), diff views,
//! status decorations for the Explorer (`Workspace::vcs_status`), and the
//! history as a graph source the Graph panel can draw.
//!
//! The `git` binary runs where the folder lives — in-process on desktop and
//! on the server for web — through `WorkspaceConfig::git`; this crate's
//! [`cli`] module (feature `cli`) is that runner. Everything else is
//! platform-neutral.

#[cfg(all(feature = "cli", not(target_arch = "wasm32")))]
pub mod cli;
pub mod history;
mod panel;

use dioxus::prelude::*;
use moonkale_ext_api::prelude::*;

pub const PANEL_ID: &str = "git";
const DIFF_PREFIX: &str = "git-diff:";

pub struct GitExtension {
    state: panel::GitState,
}

impl Default for GitExtension {
    fn default() -> Self {
        Self::new()
    }
}

impl GitExtension {
    pub fn new() -> Self {
        Self {
            state: panel::GitState::new(),
        }
    }
}

impl Extension for GitExtension {
    fn manifest(&self) -> Manifest {
        Manifest::optional(
            "dev.moonkale.ext-git",
            "Git",
            "Changes, diffs, staging and commits for the open folder; history as a graph.",
        )
        .with_permissions(&["read-sources", "write-files", "run-commands"])
    }

    fn panels(&self, ws: Workspace) -> Vec<PanelContribution> {
        let mut out = vec![PanelContribution {
            id: PANEL_ID.into(),
            title: "Changes".into(),
            home: PanelHome::Side,
            closable: false,
            dirty: false,
            node: None,
        }];
        if ws.git().is_none() {
            return out;
        }
        for d in self.state.diffs.read().iter() {
            out.push(PanelContribution {
                id: format!("{DIFF_PREFIX}{}", d.key()),
                title: format!(
                    "{}{}",
                    d.path.rsplit('/').next().unwrap_or(&d.path),
                    if d.staged { " (staged)" } else { "" }
                ),
                home: PanelHome::Main,
                closable: true,
                dirty: false,
                node: None,
            });
        }
        out
    }

    fn render(&self, panel_id: &str, ws: Workspace) -> Element {
        let state = self.state;
        if let Some(key) = panel_id.strip_prefix(DIFF_PREFIX) {
            let view_key = key.to_string();
            return rsx! { panel::DiffPanel { ws, state, view_key } };
        }
        rsx! { panel::ChangesPanel { ws, state } }
    }

    fn on_panel_closed(&self, panel_id: &str, _ws: Workspace) {
        if let Some(key) = panel_id.strip_prefix(DIFF_PREFIX) {
            let key = key.to_string();
            let mut diffs = self.state.diffs;
            diffs.with_mut(|v| v.retain(|d| d.key() != key));
        }
    }

    fn commands(&self, _ws: Workspace) -> Vec<CommandContribution> {
        vec![
            CommandContribution::new("git.refresh", "Git: Refresh Status"),
            CommandContribution::new("git.commit", "Git: Commit…").key("Ctrl+Shift+G"),
            CommandContribution::new("git.history", "Git: Show History as Graph"),
        ]
    }

    fn run_command(&self, id: &str, mut ws: Workspace) {
        match id {
            "git.refresh" => self.state.bump(),
            "git.commit" => {
                ws.dispatch(Command::ShowPanel(PANEL_ID));
                ws.focus_element(panel::COMMIT_ID);
            }
            "git.history" => {
                let state = self.state;
                spawn(async move {
                    panel::show_history(ws, state).await;
                });
            }
            _ => {}
        }
    }
}
