//! # ui — the workbench shell
//!
//! Owns the window frame ([`Frame`]: title bar with menus and, on desktop,
//! window controls), the `dioxus-workbench` workspace ([`Shell`]), and turns
//! every extension's `PanelContribution`s into panels. Contains no editor:
//! the explorer lives here only because it *is* the shell's file browser; it
//! is still written as an `Extension` so nothing in the shell is privileged.
//!
//! Platform crates never appear here. The desktop crate passes
//! [`WindowControls`] callbacks (minimize/maximize/close/drag/resize) into
//! the frame; web and mobile pass `None` and get a plain title bar.
//!
//! See `ui.md` next to this crate for the implementation notes.

pub mod commands;
mod explorer;
mod frame;
mod history;
mod palette;
mod search;
mod settings_panel;
mod shell;
mod titlebar;

pub use explorer::ExplorerExtension;
pub use frame::{Frame, ResizeEdge, SessionFactory, ShellConfig, WindowControls};
pub use moonkale_ext_api::git::{GitRequest, GitResponse};
pub use moonkale_ext_api::presence::{Member as PresenceMember, PresenceLink, PresenceMessage};
pub use moonkale_ext_api::{
    AttachFuture, AttachSource, Command, CompileTypst, CompileTypstFuture, LlmProvider,
    LlmProviderFuture, OpenFolder, OpenFolderFuture, OpenOptions, PickFolder, PickFolderFuture,
    Reveal, SecretStore, SessionBus, SessionMessage, SettingsFile, SettingsFuture, SettingsStore,
    WasmExtensions, WindowId, WorkspaceConfig,
};
pub use moonkale_lsp::{LspTransport, LspTransportFuture, SpawnLsp};
pub use moonkale_terminal::{SpawnTerminal, SpawnTerminalFuture, TerminalBackend};
pub use shell::Shell;
pub use titlebar::TitleBar;

use moonkale_ext_api::Extension;

/// The built-in extensions, in contribution order.
pub fn default_extensions() -> Vec<Box<dyn Extension>> {
    vec![
        Box::new(ExplorerExtension::new()),
        Box::new(search::SearchExtension),
        Box::new(settings_panel::SettingsExtension),
        Box::new(moonkale_editor_graph::GraphExtension),
        Box::new(moonkale_editor_markdown::LinksExtension::new()),
        Box::new(
            moonkale_editor_code::CodeEditorExtension::new().skipping(|n| {
                moonkale_editor_markdown::is_markdown(n) || moonkale_editor_flow::is_flow(n)
            }),
        ),
        Box::new(moonkale_editor_table::TableExtension),
        Box::new(moonkale_editor_terminal::TerminalExtension::new()),
        Box::new(moonkale_editor_agent::AgentExtension),
        // Opt-in (off until enabled in Settings → Extensions):
        Box::new(moonkale_editor_flow::FlowExtension),
        Box::new(moonkale_ext_lux::LuxExtension),
        Box::new(moonkale_ext_git::GitExtension::new()),
        Box::new(history::HistoryExtension::new()),
    ]
}
