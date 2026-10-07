//! # moonkale-ext-api
//!
//! **This crate is the contract.** Everything an extension can see or do is
//! declared here. The built-in editors are extensions that happen to be
//! compiled in; there is no privileged path. Versioned since `lib-v1`
//! (Milestone 18 phase 6.3): see `CHANGELOG.md` next to this crate for what
//! each `lib-vN` tag changed and the compatibility rules.
//!
//! What an extension uses:
//! - [`Extension`] — the trait: [`Manifest`], panels and their rendering,
//!   and optionally commands, settings, document claims, flow libraries,
//!   strings ([`i18n`]) and file marks.
//! - [`contrib`] — [`PanelContribution`] (built with
//!   [`PanelContribution::new`]), [`Activity`], [`FileMark`];
//!   [`CommandContribution`] and [`Keybinding`].
//! - [`Workspace`] — the host handle: sources, documents, history, settings,
//!   presence, remotes, and platform services by type
//!   ([`Workspace::service`]). Documents live here, not in panels, so a
//!   dock/undock (which remounts panel content) cannot lose edits. Its state
//!   is grouped by area (`ws.sources`, `ws.docs`, `ws.settings`, …); the
//!   methods are on the facade.
//! - [`settings`], [`flow`], [`wiki`], [`presence`], [`remote`], [`session`]
//!   — types shared between the shell, extensions and the server.
//!
//! What an app (a platform crate) uses: [`WorkspaceConfig`] — what the
//! platform gives the workspace, grouped as folders, processes,
//! persistence, network, runtimes and services.
//!
//! Wasm extensions use a separate JSON ABI (`moonkale-ext-abi`, hosted by
//! `moonkale-ext-host`; [[ADR-0013 JSON ABI before components]]).
//!
//! This crate depends on `dioxus` because static extensions return
//! `Element`s; on `moonkale-core` for the model; on the protocol crates
//! `moonkale-lsp`, `moonkale-terminal`, `moonkale-llm-types`, `moonkale-ext-abi`
//! and on `moonkale-state` — nothing else (checked by `tools/check-deps.py`).
#![warn(missing_docs)]

mod assets;
mod command;
pub mod contrib;
pub mod document;
pub mod extension;

/// The namespace of the built-in extensions' ids; third-party (wasm)
/// extensions may not use it (audit #4).
pub const BUILTIN_ID_PREFIX: &str = "dev.moonkale.";
pub mod i18n;

/// `ext-api`'s own strings (spec 030): the workspace's status messages.
pub(crate) static L: i18n::Locales = &[
    ("en", include_str!("../locales/en.ftl")),
    ("de", include_str!("../locales/de.ftl")),
    ("zh-CN", include_str!("../locales/zh-CN.ftl")),
];
pub mod keys;
pub mod manifest;
pub mod session;
pub mod workspace;

pub use assets::{Stylesheet, StylesheetUrl};
pub use command::{fuzzy_score, CommandContribution, Keybinding};
pub use contrib::{Activity, FileMark, PanelContribution, PanelHome};
pub use document::Document;
pub use extension::Extension;
pub mod flow;
pub mod presence;
pub mod remote;
pub mod settings;
pub mod wiki;
pub use extension::SettingsTarget;
pub use manifest::Manifest;
pub use session::{SessionBus, SessionMessage, WindowId};
pub use settings::{ExtensionsSettings, Scope, SecretRef, Settings, SettingsFile};
pub use workspace::{installed_local_state, local_state};
pub use workspace::{
    AgentSessions, AttachFuture, AttachSource, Command, CompileTypst, CompileTypstFuture,
    EditorAction, FolderAccess, ForeignDrag, GraphRequest, LlmProvider, LlmProviderFuture, Network,
    OpenFolder, OpenFolderFuture, OpenOptions, Persistence, PickFolder, PickFolderFuture,
    Processes, Reveal, Runtimes, SecretStore, ServerClient, SettingsFuture, SettingsStore,
    SourceHandle, StateAccess, WasmExtensions, WasmList, WasmRun, Workspace, WorkspaceConfig,
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
