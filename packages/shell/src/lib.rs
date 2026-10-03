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
mod extensions_panel;
mod frame;
pub mod icons;
mod palette;
mod remote_dialog;
mod search;
mod server_dialog;
mod settings_panel;
mod shell;
mod terminal_chooser;
pub mod theme;
mod titlebar;
mod touch_drag;

pub use explorer::ExplorerExtension;
pub use frame::{Frame, ResizeEdge, SessionFactory, ShellConfig, WindowControls};
pub use moonkale_ext_api::presence::{Member as PresenceMember, PresenceLink, PresenceMessage};
pub use moonkale_ext_api::remote;
pub use moonkale_ext_api::{installed_local_state, local_state};
pub use moonkale_ext_api::{
    AttachFuture, AttachSource, Command, CompileTypst, CompileTypstFuture, FolderAccess,
    LlmProvider, LlmProviderFuture, Network, OpenFolder, OpenFolderFuture, OpenOptions,
    Persistence, PickFolder, PickFolderFuture, Processes, Reveal, Runtimes, SecretStore,
    ServerClient, SessionBus, SessionMessage, SettingsFile, SettingsFuture, SettingsStore,
    StateAccess, WasmExtensions, WindowId, WorkspaceConfig,
};
pub use moonkale_lsp::{LspTransport, LspTransportFuture, SpawnLsp};
pub use moonkale_terminal::{SpawnTerminal, SpawnTerminalFuture, TerminalBackend};
pub use remote_dialog::RemoteDialog;
pub use server_dialog::ServerDialog;
pub use shell::Shell;
pub use titlebar::TitleBar;

use moonkale_ext_api::Extension;

/// This crate's strings (spec 030): English, German, Chinese.
pub static L: moonkale_ext_api::i18n::Locales = &[
    ("en", include_str!("../locales/en.ftl")),
    ("de", include_str!("../locales/de.ftl")),
    ("zh-CN", include_str!("../locales/zh-CN.ftl")),
];

/// `tl!(lang, <id>)`: a shell string in `lang` (where no `Workspace` is at
/// hand; components use `moonkale_ext_api::t!`).
#[macro_export]
macro_rules! tl {
    ($lang:expr, $key:literal) => {
        moonkale_ext_api::i18n::tr($crate::L, $lang, $key, None)
    };
}

/// The shell's own extensions (Explorer, Search, Settings, Extensions), in
/// contribution order. Everything else comes from a distribution
/// (`moonkale-distribution`, Milestone 18 phase 4.1), which starts its list
/// with these.
pub fn builtin_extensions() -> Vec<Box<dyn Extension>> {
    vec![
        Box::new(ExplorerExtension::new()),
        Box::new(search::SearchExtension),
        Box::new(settings_panel::SettingsExtension),
        Box::new(extensions_panel::ExtensionsExtension),
    ]
}

#[cfg(test)]
mod tests {
    #[test]
    fn workbench_templates_keep_their_braces() {
        for lang in ["en", "de", "zh-CN"] {
            let t = moonkale_ext_api::i18n::tr(super::L, lang, "wb-close-tab", None);
            assert!(t.contains("{title}"), "{lang}: {t}");
            let p = moonkale_ext_api::i18n::tr(super::L, lang, "wb-tab-group-labelled", None);
            assert!(p.contains("{panel}"), "{lang}: {p}");
        }
        assert!(moonkale_ext_api::i18n::check(super::L).is_empty());
    }
}
