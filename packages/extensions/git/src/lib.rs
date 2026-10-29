//! # moonkale-ext-git
//!
//! Git as a first-class part of the workspace (Milestone 7): a *Changes*
//! panel (status, stage/unstage/discard, commit, branch, log), diff views,
//! status marks for the Explorer and tabs (`Workspace::file_marks`), and the
//! history as a graph source the Graph panel can draw.
//!
//! The `git` binary runs where the folder lives — in-process on desktop and
//! on the server for web — through `WorkspaceConfig::git`; this crate's
//! [`cli`] module (feature `cli`) is that runner. Everything else is
//! platform-neutral.

#[cfg(all(feature = "cli", not(target_arch = "wasm32")))]
pub mod cli;
pub mod conflict;
mod conflict_model;
#[cfg(all(feature = "cli", not(target_arch = "wasm32")))]
mod everyday;
#[cfg(all(feature = "cli", not(target_arch = "wasm32")))]
mod everyday_model;
mod everyday_panel;
pub mod everyday_types;
pub mod history;
mod panel;
pub mod server;
#[cfg(all(feature = "cli", not(target_arch = "wasm32")))]
mod sync;
pub mod types;

pub use server::{git_run, remote};
pub use types::{GitRequest, GitResponse};

use dioxus::prelude::*;
use moonkale_ext_api::prelude::*;
use moonkale_ext_api::{i18n::Locales, t};

/// This crate's strings (spec 030): English, German, Chinese.
pub(crate) static L: moonkale_ext_api::i18n::Locales = &[
    ("en", include_str!("../locales/en.ftl")),
    ("de", include_str!("../locales/de.ftl")),
    ("zh-CN", include_str!("../locales/zh-CN.ftl")),
];

pub const PANEL_ID: &str = "git";
const DIFF_PREFIX: &str = "git-diff:";

/// The platform's git (Milestone 7; a service since Milestone 18 phase
/// 4.2): the folder's absolute path (server-relative on the web) and a
/// request. The app provides it in `WorkspaceConfig::services` — the local
/// [`cli`] runner on desktop, [`remote`] where the folder is on a server —
/// and the panel finds it with `ws.service::<GitRunner>()`.
pub struct GitRunner(pub fn(String, GitRequest) -> moonkale_ext_api::SettingsFuture<GitResponse>);

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
        let mut out =
            vec![
                PanelContribution::new(PANEL_ID, t!(ws, L, "git-changes"), PanelHome::Side)
                    .closable(true)
                    .activity(
                        Activity::new("git", 40, t!(ws, L, "git-activity"))
                            .phone_secondary()
                            .badge(ws.contrib.file_marks.read().len() as u32),
                    ),
            ];
        if ws.service::<GitRunner>().is_none() {
            return out;
        }
        for d in self.state.diffs.read().iter() {
            out.push(
                PanelContribution::new(
                    format!("{DIFF_PREFIX}{}", d.key()),
                    format!(
                        "{}{}",
                        d.path.rsplit('/').next().unwrap_or(&d.path),
                        if d.staged {
                            t!(ws, L, "git-staged-suffix")
                        } else {
                            String::new()
                        }
                    ),
                    PanelHome::Main,
                )
                .closable(true),
            );
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

    fn commands(&self, ws: Workspace) -> Vec<CommandContribution> {
        let mut commands = vec![
            CommandContribution::new("git.refresh", t!(ws, L, "cmd-git-refresh")),
            CommandContribution::new("git.commit", t!(ws, L, "cmd-git-commit")).key("Ctrl+Shift+G"),
            CommandContribution::new("git.history", t!(ws, L, "cmd-git-history")),
            CommandContribution::new("git.sync", t!(ws, L, "git-sync")).key("Ctrl+Alt+G"),
            CommandContribution::new("git.continue", t!(ws, L, "git-continue"))
                .key("Ctrl+Alt+Shift+Enter"),
            CommandContribution::new("git.abort", t!(ws, L, "git-abort")).key("Ctrl+Alt+Backspace"),
        ];
        for ((id, label, _), key) in panel::sync_actions(ws).into_iter().zip([
            "Ctrl+Alt+F",
            "Ctrl+Alt+L",
            "Ctrl+Alt+U",
            "Ctrl+Alt+Shift+P",
            "Ctrl+Alt+Shift+U",
        ]) {
            commands.push(CommandContribution::new(id, label).key(key));
        }
        for ((id, label, _), key) in everyday_panel::actions(ws).into_iter().zip([
            "Ctrl+Alt+B",
            "Ctrl+Alt+Shift+Z",
            "Ctrl+Alt+Shift+S",
            "Ctrl+Alt+Shift+B",
        ]) {
            commands.push(CommandContribution::new(id, label).key(key));
        }
        commands
    }

    fn locales(&self) -> Locales {
        L
    }

    fn run_command(&self, id: &str, mut ws: Workspace) {
        if let Some((_, _, tab)) = everyday_panel::actions(ws)
            .into_iter()
            .find(|(cmd, _, _)| *cmd == id)
        {
            ws.dispatch(Command::ShowPanel(PANEL_ID));
            let mut mode = self.state.everyday;
            mode.set(Some(tab));
            return;
        }
        let sync = if id == "git.sync" {
            Some(types::SyncAction::Sync)
        } else {
            panel::sync_actions(ws)
                .into_iter()
                .find(|(cmd, _, _)| *cmd == id)
                .map(|(_, _, a)| a)
        };
        if let Some(action) = sync {
            ws.dispatch(Command::ShowPanel(PANEL_ID));
            panel::act(
                ws,
                self.state,
                GitRequest::Sync {
                    action,
                    expected: None,
                },
            );
            return;
        }
        match id {
            "git.continue" => panel::act(ws, self.state, GitRequest::Continue),
            "git.abort" => panel::act(ws, self.state, GitRequest::Abort),
            "git.refresh" => self.state.bump(),
            "git.commit" => {
                ws.dispatch(Command::ShowPanel(PANEL_ID));
                ws.focus_element(panel::COMMIT_ID);
            }
            "git.history" => {
                let state = self.state;
                moonkale_ext_api::crash::spawn(async move {
                    panel::show_history(ws, state).await;
                });
            }
            _ => {}
        }
    }
}
