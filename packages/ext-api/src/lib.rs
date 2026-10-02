//! # moonkale-ext-api
//!
//! **This crate is the contract.** Everything an extension can see or do is
//! declared here. The built-in editors are extensions that happen to be
//! compiled in; there is no privileged path.
//!
//! What is here:
//! - [`manifest::Manifest`] — id, name, description, tier, permissions.
//! - [`extension::Extension`] — panels, commands, settings, flow libraries.
//! - [`contrib`] — `PanelContribution` (+ the activity-bar `Activity`),
//!   [`CommandContribution`] and keybindings.
//! - [`workspace::Workspace`] — the host handle: sources, documents, history,
//!   settings, presence, remote … Documents live *here*, not in panels, so a
//!   dock/undock (which remounts panel content) cannot lose edits. It is far
//!   larger than a contract should be; Milestone 18 phase 3 splits it into
//!   services.
//! - [`settings`], [`flow`], [`wiki`], [`git`], [`presence`], [`remote`],
//!   [`session`] — types shared between the shell, extensions and the server.
//!
//! Wasm extensions use a separate JSON ABI (`moonkale-ext-host::abi`). The
//! target design (a `moonkale.toml` manifest, a `Host` handle, `ui::Tree`
//! panels) is in the vault: `extensions/Writing an Extension.md`, Part B.
//!
//! This crate depends on `dioxus` because static extensions return
//! `Element`s.

mod assets;
mod command;
pub mod contrib;
pub mod document;
pub mod extension;
pub mod keys;
pub mod manifest;
pub mod session;
pub mod workspace;

pub use assets::{Stylesheet, StylesheetUrl};
pub use command::{fuzzy_score, CommandContribution, Keybinding};
pub use contrib::{Activity, PanelContribution, PanelHome};
pub use document::Document;
pub use extension::Extension;
pub mod flow;
pub mod git;
pub mod presence;
pub mod remote;
pub mod settings;
pub mod wiki;
pub use extension::SettingsTarget;
pub use manifest::Manifest;
pub use session::{SessionBus, SessionMessage, WindowId};
pub use settings::{ExtensionsSettings, Scope, SecretRef, Settings, SettingsFile};
pub use workspace::{
    AgentSessions, AttachFuture, AttachSource, Command, CompileTypst, CompileTypstFuture,
    EditorAction, FolderAccess, ForeignDrag, GraphRequest, LlmProvider, LlmProviderFuture, Network,
    OpenFolder, OpenFolderFuture, OpenOptions, Persistence, PickFolder, PickFolderFuture,
    Processes, Reveal, Runtimes, SecretStore, ServerClient, SettingsFuture, SettingsStore,
    SourceHandle, WasmExtensions, WasmList, WasmRun, Workspace, WorkspaceConfig,
};

/// Everything an extension typically needs.
pub mod prelude {
    pub use crate::{
        Activity, Command, CommandContribution, Document, Extension, Manifest, PanelContribution,
        PanelHome, SettingsTarget, SourceHandle, Workspace,
    };
    pub use moonkale_core::{
        Node, NodeId, NodeKind, Query, Source, SourceError, SourceId, Version,
    };
}
