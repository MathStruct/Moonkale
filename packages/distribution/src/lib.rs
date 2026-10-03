//! # moonkale-distribution — which extensions an app ships
//!
//! The catalogue (Milestone 18 phase 4.1): the shell (`ui`) knows no
//! extension but its own (Explorer, Search, Settings, Extensions); this crate
//! lists everything else, one Cargo feature per optional extension, and the
//! apps hand [`default_extensions`] to the shell. Adding an extension is a
//! crate plus one line here — never an edit of the shell.
//!
//! `--no-default-features` gives the minimum workbench: Explorer, Search,
//! Settings, Extensions, Graph, Links, Code and Markdown.

use moonkale_ext_api::Extension;

/// The extensions of this distribution, in contribution order. Which
/// documents each editor shows is decided by `Extension::claims` (Milestone 18
/// phase 2), not by this order; opt-in extensions (Flow, Lux, Git, History)
/// stay off until enabled in Settings → Extensions.
pub fn default_extensions() -> Vec<Box<dyn Extension>> {
    let mut v: Vec<Box<dyn Extension>> = ui::builtin_extensions();
    v.push(Box::new(moonkale_editor_graph::GraphExtension));
    v.push(Box::new(moonkale_editor_markdown::LinksExtension::new()));
    v.push(Box::new(moonkale_editor_code::CodeEditorExtension::new()));
    // Milestone 14: the Rust code editor; the user's choice breaks the tie.
    #[cfg(feature = "code-native")]
    v.push(Box::new(
        moonkale_editor_code_native::NativeCodeExtension::new(),
    ));
    #[cfg(feature = "table")]
    v.push(Box::new(moonkale_editor_table::TableExtension));
    #[cfg(feature = "image")]
    v.push(Box::new(moonkale_editor_image::ImageExtension));
    #[cfg(feature = "terminal")]
    v.push(Box::new(moonkale_editor_terminal::TerminalExtension::new()));
    #[cfg(feature = "terminal-native")]
    v.push(Box::new(
        moonkale_editor_terminal_native::NativeTerminalExtension::new(),
    ));
    #[cfg(feature = "agent")]
    v.push(Box::new(moonkale_editor_agent::AgentExtension));
    #[cfg(feature = "flow")]
    v.push(Box::new(moonkale_editor_flow::FlowExtension));
    #[cfg(feature = "lux")]
    v.push(Box::new(moonkale_ext_lux::LuxExtension));
    #[cfg(feature = "git")]
    v.push(Box::new(moonkale_ext_git::GitExtension::new()));
    #[cfg(feature = "history")]
    v.push(Box::new(moonkale_ext_history::HistoryExtension::new()));
    v
}
