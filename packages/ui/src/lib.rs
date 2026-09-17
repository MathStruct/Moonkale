//! # ui — the workbench shell
//!
//! Owns the `dioxus-workbench` workspace, the activity rail and status bar,
//! and turns every extension's `PanelContribution`s into panels. Contains no
//! editor: the explorer lives here only because it *is* the shell's file
//! browser; it is still written as an `Extension` so nothing in the shell is
//! privileged.
//!
//! Milestone 1: [`Shell`], [`ExplorerExtension`], [`default_extensions`].
//! See `ui.md` next to this crate for the implementation notes.

mod explorer;
mod navbar;
mod shell;

pub use explorer::ExplorerExtension;
pub use moonkale_ext_api::{OpenFolder, OpenFolderFuture};
pub use navbar::Navbar;
pub use shell::{Shell, ShellConfig};

use moonkale_ext_api::Extension;

/// The built-in extensions, in contribution order.
pub fn default_extensions() -> Vec<Box<dyn Extension>> {
    vec![
        Box::new(ExplorerExtension::new()),
        Box::new(moonkale_editor_code::CodeEditorExtension),
    ]
}
