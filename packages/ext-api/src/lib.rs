//! # moonkale-ext-api
//!
//! **This crate is the contract.** Everything an extension can see or do is
//! declared here. The built-in editors are extensions that happen to be
//! compiled in; there is no privileged path.
//!
//! **Milestone 1 scope** — the *static* half of the design, sized for two
//! extensions (an explorer and a code editor):
//!
//! - [`manifest::Manifest`] — id and name.
//! - [`contrib::PanelContribution`] — dockable panels; the only contribution
//!   point so far.
//! - [`extension::Extension`] — `manifest()`, `panels()`, `render()`.
//! - [`workspace::Workspace`] — the host handle: open sources, open
//!   documents, the active document, status. It is the design's `Host`
//!   reduced to what M1 needs. Documents live *here*, not in panels, so a
//!   dock/undock (which remounts panel content) cannot lose edits.
//!
//! Not started: WASM extensions and the declarative `ui::Tree`, permissions,
//! commands/keybindings/languages as contributions. Their design notes are
//! in the vault (`architecture/Extension System.md`).
//!
//! This crate depends on `dioxus` because static extensions return
//! `Element`s. The WASM path will not; it will render through `ui::Tree`.

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
    EditorAction, ForeignDrag, GraphRequest, LlmProvider, LlmProviderFuture, OpenFolder,
    OpenFolderFuture, OpenOptions, PickFolder, PickFolderFuture, Reveal, SecretStore, ServerClient,
    SettingsFuture, SettingsStore, SourceHandle, WasmExtensions, WasmList, WasmRun, Workspace,
    WorkspaceConfig,
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
