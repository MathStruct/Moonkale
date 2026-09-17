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

mod explorer;
mod frame;
mod shell;
mod titlebar;

pub use explorer::ExplorerExtension;
pub use frame::{Frame, ResizeEdge, ShellConfig, WindowControls};
pub use moonkale_ext_api::{
    Command, OpenFolder, OpenFolderFuture, PickFolder, PickFolderFuture, WorkspaceConfig,
};
pub use shell::Shell;
pub use titlebar::TitleBar;

use moonkale_ext_api::Extension;

/// The built-in extensions, in contribution order.
pub fn default_extensions() -> Vec<Box<dyn Extension>> {
    vec![
        Box::new(ExplorerExtension::new()),
        Box::new(moonkale_editor_code::CodeEditorExtension),
    ]
}
