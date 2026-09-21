//! `Workspace` — the host handle extensions receive.
//!
//! A `Copy` bundle of signals plus the operations that mutate them. Every
//! field is a `Signal`, so components that read them re-render on change and
//! event handlers can mutate them without borrowing the workspace itself.
//!
//! Documents are keyed by `NodeId` and stored as their own `Signal` each, so
//! a keystroke re-renders only the editor of that document, not every reader
//! of the open-document list.

use crate::session::{SessionBus, SessionMessage, WindowId};
use crate::Document;
use dioxus::logger::tracing;
use dioxus::prelude::*;
use moonkale_core::{
    Node, NodeId, NodeKind, Query, QueryResult, Source, SourceDescriptor, SourceError, SourceId,
    TextPatch, Transaction,
};
use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;

/// A source the workspace has open.
#[derive(Clone)]
pub struct SourceHandle {
    pub descriptor: SourceDescriptor,
    pub source: Arc<dyn Source>,
}

impl PartialEq for SourceHandle {
    fn eq(&self, other: &Self) -> bool {
        self.descriptor == other.descriptor
    }
}

/// How this platform opens a folder: in-process (`FolderSource`) on desktop,
/// via the server (`RemoteSource`) on web. Installed by the platform crate.
/// The future an [`OpenFolder`] returns: the folder itself plus whatever the
/// platform derives from it (the index), all registered together.
pub type OpenFolderFuture =
    Pin<Box<dyn Future<Output = Result<Vec<Arc<dyn Source>>, SourceError>>>>;
/// What the platform needs besides the path when opening a folder: the
/// embedding provider settings for the index (`None` = BM25-only search).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OpenOptions {
    pub embed: Option<moonkale_llm::LlmSettings>,
}
pub type OpenFolder = fn(String, OpenOptions) -> OpenFolderFuture;
/// The future an [`AttachSource`] returns: one source, by descriptor.
pub type AttachFuture = Pin<Box<dyn Future<Output = Result<Arc<dyn Source>, SourceError>>>>;

/// A native folder picker: resolves to the chosen path, or `None` if the
/// user cancelled (or no dialog is available). Desktop provides one; web
/// and mobile pass `None` and fall back to typing a path.
pub type PickFolderFuture = Pin<Box<dyn Future<Output = Option<String>>>>;
pub type PickFolder = fn() -> PickFolderFuture;

/// Re-open a source another window already has, from its descriptor:
/// the process registry on desktop, `RemoteSource::from_descriptor` on web.
pub type AttachSource = fn(SourceDescriptor) -> AttachFuture;

/// Compile a Typst document: `(folder root, main path relative to it, text)` →
/// SVG pages, or diagnostics. Desktop compiles in-process, web asks the server.
pub type CompileTypstFuture = Pin<Box<dyn Future<Output = Result<Vec<String>, Vec<String>>>>>;
pub type CompileTypst = fn(String, String, String) -> CompileTypstFuture;

/// Builds the LLM provider for this platform (in-process on desktop, the
/// server relay on web). Async because the web build asks the server which
/// model it runs.
pub type LlmProviderFuture =
    Pin<Box<dyn Future<Output = Result<Arc<dyn moonkale_llm::Provider>, String>>>>;
/// Built from the resolved LLM settings (provider kind, model, endpoint,
/// secret name); the platform resolves the secret itself.
pub type LlmProvider = fn(moonkale_llm::LlmSettings) -> LlmProviderFuture;

/// User-scope settings persistence (per machine). Workspace-scope settings
/// go through the folder source (`.moonkale/settings.json`), so they need
/// no platform hook.
pub type SettingsFuture<T> = Pin<Box<dyn Future<Output = Result<T, String>>>>;
/// Store a secret by name where the platform keeps them (desktop: the
/// secrets file / keychain). `None` on web — secrets live on the server.
pub type SecretStore = fn(String, String) -> SettingsFuture<()>;

/// Third-party wasm extensions (Milestone 6): the platform lists what is
/// installed and runs commands where the runtime lives (desktop in-process,
/// web on the server). `granted` are the permissions the user ticked.
pub type WasmList = fn(Option<String>) -> SettingsFuture<Vec<moonkale_ext_host::WasmManifest>>;
pub type WasmRun = fn(String, String, serde_json::Value, Vec<String>) -> SettingsFuture<String>;
#[derive(Clone, Copy)]
pub struct WasmExtensions {
    /// Argument: the open folder's path (for `.moonkale/extensions`).
    pub list: WasmList,
    pub run: WasmRun,
}

/// Git on the platform that has the folder (Milestone 7): the folder's
/// absolute path (or server-relative on web) and a request.
pub type GitRun = fn(String, crate::git::GitRequest) -> SettingsFuture<crate::git::GitResponse>;

#[derive(Clone, Copy)]
pub struct SettingsStore {
    pub load: fn() -> SettingsFuture<crate::settings::SettingsFile>,
    pub save: fn(crate::settings::SettingsFile) -> SettingsFuture<()>,
}

/// "Open this node and put the cursor here" (search hits, trace frames,
/// go-to-definition). Lines and columns are 0-based.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reveal {
    pub node: NodeId,
    pub line: u32,
    pub col: u32,
    /// Bumped per request so the same position can be revealed twice.
    pub seq: u64,
}

/// What the platform hands the workspace at startup.
#[derive(Clone, Copy)]
pub struct WorkspaceConfig {
    pub open_folder: OpenFolder,
    pub pick_folder: Option<PickFolder>,
    pub attach_source: AttachSource,
    /// How to start a terminal on this platform (`None`: no terminals).
    pub spawn_terminal: Option<moonkale_terminal::SpawnTerminal>,
    /// Typst compiler (`None`: no preview).
    pub compile_typst: Option<CompileTypst>,
    /// Language-server launcher (`None`: no LSP features).
    pub spawn_lsp: Option<moonkale_lsp::SpawnLsp>,
    pub llm: Option<LlmProvider>,
    pub settings_store: Option<SettingsStore>,
    pub secret_store: Option<SecretStore>,
    /// Reopen the most recent folder when the app starts (desktop).
    pub reopen_last_folder: bool,
    pub wasm: Option<WasmExtensions>,
    /// Git for the open folder (`None`: no git on this platform).
    pub git: Option<GitRun>,
    /// Presence hub (Milestone 8); `None`: this window is alone.
    pub presence: Option<crate::presence::JoinPresence>,
    /// Where the browser runtime fetches a wasm extension's bytes (by id);
    /// `Some` enables running modules in the page (Milestone 8, web).
    pub wasm_module_url: Option<fn(String) -> String>,
    /// Remote folders over SSH (Milestone 11; desktop only).
    pub remote: Option<crate::remote::RemoteHosts>,
    /// Agent sessions that live on the server (Milestone 12): the web
    /// client always, the desktop while it is a server's client.
    pub agent_sessions: Option<AgentSessions>,
    /// *Connect to Server…* (Milestone 12): make this app a client of a
    /// Moonkale server by URL + token (desktop and mobile).
    pub server: Option<ServerClient>,
}

/// How a native app becomes a server's client (`api::client`).
#[derive(Clone, Copy)]
pub struct ServerClient {
    /// `(url, token)`; the sources then come from that server.
    pub connect: fn(String, Option<String>) -> Result<(), String>,
    pub disconnect: fn(),
    /// The connected server's label, if any.
    pub active: fn() -> Option<String>,
}

/// How a client reaches the server's agent sessions (`api::agent_sessions`).
#[derive(Clone, Copy)]
pub struct AgentSessions {
    /// Are the sources a server's right now?
    pub available: fn() -> bool,
    pub list: fn(String) -> SettingsFuture<Vec<moonkale_llm::sessions::SessionSummary>>,
    /// `(session or None, folder, text, settings)` → session id.
    pub send: fn(
        Option<String>,
        String,
        String,
        moonkale_llm::sessions::TurnSettings,
    ) -> SettingsFuture<String>,
    /// `(session, since)`.
    pub events: fn(String, usize) -> SettingsFuture<moonkale_llm::sessions::SessionState>,
    /// `(session, call id, allow)`.
    pub approve: fn(String, String, bool) -> SettingsFuture<()>,
}

/// One platform, one set of pointers: equal by construction (a prop).
impl PartialEq for AgentSessions {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

/// "Draw this query's result": set by the table editor's *Show in Graph*,
/// consumed by the Graph panel, which switches its source picker to it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraphRequest {
    pub source: SourceId,
    pub dialect: String,
    pub text: String,
}

/// A document being dragged out of another window of this session.
#[derive(Clone, PartialEq)]
pub struct ForeignDrag {
    pub from: WindowId,
    pub node: Node,
    pub source: SourceDescriptor,
    /// `true` while the mouse button is still down in the origin window (a
    /// real HTML5 drop can land here); `false` after the drag ended without
    /// a drop — the offer stays as a banner until accepted or dismissed, so
    /// platforms whose OS drag never crosses windows still get the move.
    pub live: bool,
}

/// Application-level commands: what menus, keybindings and (later) the
/// palette and LLM tools dispatch. Consumers watch [`Workspace::commands`]
/// and act on the commands that concern them (the active editor handles
/// `Undo`; the shell handles `ResetLayout`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    OpenFolder,
    /// Close the first folder source (spec 015); the Explorer's context
    /// menu closes a specific one through `Workspace::close_source`.
    CloseFolder,
    Save,
    CloseEditor,
    Undo,
    Redo,
    ResetLayout,
    NewWindow,
    /// Open a terminal; `Workspace::terminal_cwd` may carry a directory.
    /// The frame turns it into `NewTerminalIn` (Milestone 12).
    NewTerminal,
    /// Open a terminal in a named implementation: `"xterm"` (the JS
    /// panel) or `"native"` (the Rust panel).
    NewTerminalIn(&'static str),
    About,
    /// Bring a panel's tab to the front (the shell owns the layout); a
    /// closed static panel is reopened first (spec 011).
    ShowPanel(&'static str),
    /// Open the n-th entry of `settings.recent_folders`.
    OpenRecent(usize),
    /// Open the Settings panel (Ctrl+,).
    Settings,
    /// Create `<name>` (a file name; a numeric suffix is added if it exists)
    /// with `template` in the open folder's root and open it.
    NewFile(&'static str, &'static str),
    /// Open the command palette (Ctrl+Shift+P).
    Palette,
    /// Open quick open — files of the open folder (Ctrl+P).
    QuickOpen,
    /// Focus the workspace search (Ctrl+Shift+F).
    SearchWorkspace,
    /// Save every dirty document.
    SaveAll,
    /// Close every document (unsaved ones stay open).
    CloseAllEditors,
    /// Show/hide the side bar (Ctrl+B) and the bottom panel (Ctrl+J).
    ToggleSide,
    ToggleBottom,
    /// An action for the active editor (menus: Find, Rename, …).
    Editor(EditorAction),
    /// Open the documentation site (Help menu).
    Docs,
    /// Ask for a host and a path, then open that folder over SSH
    /// (Milestone 11).
    OpenRemote,
    /// End the SSH session and close what it opened.
    CloseRemote,
    /// Ask for a server URL + token and become its client (Milestone 12).
    ConnectServer,
    /// Drop the server connection and its sources.
    DisconnectServer,
}

/// Actions the menus can ask the active editor for (spec 009); the editor
/// forwards them to its view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditorAction {
    Find,
    Replace,
    Rename,
    CodeActions,
    Definition,
    References,
    ToggleComment,
    FoldAll,
    UnfoldAll,
}

#[derive(Clone, Copy)]
pub struct Workspace {
    pub sources: Signal<Vec<SourceHandle>>,
    /// Open documents in opening order (this is the tab order).
    pub documents: Signal<Vec<(NodeId, Signal<Document>)>>,
    /// Open non-text nodes (tables, later graphs/rows): shown by the editor
    /// extension that claims their kind.
    pub views: Signal<Vec<Node>>,
    pub active: Signal<Option<NodeId>>,
    /// One line for the status bar.
    pub status: Signal<String>,
    /// The last dispatched command with a sequence number, so consumers can
    /// tell a new dispatch of the same command from a re-render.
    pub commands: Signal<(u64, Option<Command>)>,
    /// This window's id in the session.
    pub window: Signal<WindowId>,
    /// A drag coming from another window, while it lasts.
    pub foreign_drag: Signal<Option<ForeignDrag>>,
    /// The node this window is currently dragging out, if any.
    pub own_drag: Signal<Option<NodeId>>,
    /// Other windows we have heard from (diagnostic: shown in the status bar).
    pub peers: Signal<Vec<WindowId>>,
    /// Language-server status for the status bar ("rust-analyzer: indexing…").
    pub lsp_status: Signal<Option<String>>,
    /// Last "Show in Graph" request (see [`GraphRequest`]).
    pub graph_request: Signal<Option<GraphRequest>>,
    /// Pending cursor placement for an editor (see [`Reveal`]).
    pub reveal: Signal<Option<Reveal>>,
    /// Counter for ids of in-process sources (traces).
    pub unique: Signal<u64>,
    /// Block libraries from the enabled extensions (the shell keeps it current).
    pub flow_libraries: Signal<Vec<crate::flow::FlowLibrary>>,
    /// Installed wasm extensions (manifests), refreshed at start and on folder open.
    pub wasm_extensions: Signal<Vec<moonkale_ext_host::WasmManifest>>,
    /// Persisted scopes and the resolved value (see `settings.rs`).
    pub settings_user: Signal<crate::settings::SettingsFile>,
    pub settings_workspace: Signal<crate::settings::SettingsFile>,
    pub settings: Signal<crate::settings::Settings>,
    /// The folder whose `.moonkale/settings.json` is loaded, if any.
    pub settings_folder: Signal<Option<SourceId>>,
    /// Static panels (Graph, Agent, Explorer, …) the user closed (spec 011);
    /// the shell contributes nothing for them until `show_panel` is called.
    pub closed_panels: Signal<std::collections::BTreeSet<String>>,
    /// Panels hidden by Ctrl+B / Ctrl+J, to bring back on the next toggle.
    pub hidden_tiles: Signal<std::collections::BTreeMap<String, Vec<String>>>,
    /// Directory for the next `NewTerminal` (set by "New terminal here").
    pub terminal_cwd: Signal<Option<String>>,
    /// Terminal sessions started elsewhere (the `ssh` of a remote session)
    /// that the terminal panel adopts as tabs (Milestone 11).
    pub adopt_terminals: Signal<Vec<Rc<RefCell<Option<moonkale_terminal::Session>>>>>,
    /// The window's remote session, if any (Milestone 11).
    pub remote: Signal<Option<crate::remote::RemoteState>>,
    /// The server this app is a client of (Milestone 12): label and the
    /// sources opened through it.
    pub server_link: Signal<Option<(String, Vec<SourceId>)>>,
    /// Bumped whenever derived data may have changed (after a save was
    /// refreshed into the index); graph/backlink panels re-query on it.
    pub graph_epoch: Signal<u64>,
    /// Bumped after a file operation (create/rename/delete/move) so the
    /// Explorer reloads the directories it shows (Milestone 7).
    pub fs_epoch: Signal<u64>,
    /// Who else is in the open folder (Milestone 8); this window included.
    pub presence: Signal<Vec<crate::presence::Member>>,
    presence_link: Signal<Option<Rc<dyn crate::presence::PresenceLink>>>,
    /// The workspace's entity log (Milestone 8): every write appends; kept
    /// in `.moonkale/history.jsonl` of the open folder.
    pub history: Signal<moonkale_core::EntityLog>,
    /// Who the next edit of a document is attributed to when it is not the
    /// user (the agent host sets it after `editor.replace`).
    pub pending_actor: Signal<std::collections::HashMap<NodeId, String>>,
    /// Bumped by the frame once the webview is up so `Stylesheet`s re-assert
    /// themselves (Milestone 9, P-087).
    pub assets_epoch: Signal<u64>,
    /// Cursor line of the active document, for presence (Milestone 9).
    pub cursor_line: Signal<Option<u32>>,
    /// The event a pending edit restores (Milestone 9): the next save's
    /// `Content` event gets it as `cause`.
    pub pending_cause: Signal<std::collections::HashMap<NodeId, moonkale_core::EventId>>,
    /// Version-control status per relative path: `(index, worktree)` letters
    /// from `git status`, published by the git extension for decorations.
    pub vcs_status: Signal<std::collections::HashMap<String, (char, char)>>,
    bus: Signal<Option<Rc<dyn SessionBus>>>,
    config: WorkspaceConfig,
}

impl PartialEq for Workspace {
    fn eq(&self, other: &Self) -> bool {
        self.sources == other.sources
            && self.documents == other.documents
            && self.active == other.active
    }
}

impl Workspace {
    /// Create the workspace. Call once, in the shell's `use_hook`, so the
    /// signals live for the app's lifetime.
    pub fn new(config: WorkspaceConfig) -> Self {
        Self {
            sources: Signal::new_in_scope(Vec::new(), ScopeId::ROOT),
            documents: Signal::new_in_scope(Vec::new(), ScopeId::ROOT),
            views: Signal::new_in_scope(Vec::new(), ScopeId::ROOT),
            active: Signal::new_in_scope(None, ScopeId::ROOT),
            status: Signal::new_in_scope("Ready".into(), ScopeId::ROOT),
            commands: Signal::new_in_scope((0, None), ScopeId::ROOT),
            window: Signal::new_in_scope(WindowId::fresh(), ScopeId::ROOT),
            foreign_drag: Signal::new_in_scope(None, ScopeId::ROOT),
            own_drag: Signal::new_in_scope(None, ScopeId::ROOT),
            peers: Signal::new_in_scope(Vec::new(), ScopeId::ROOT),
            graph_epoch: Signal::new_in_scope(0, ScopeId::ROOT),
            fs_epoch: Signal::new_in_scope(0, ScopeId::ROOT),
            vcs_status: Signal::new_in_scope(std::collections::HashMap::new(), ScopeId::ROOT),
            history: Signal::new_in_scope(moonkale_core::EntityLog::new(), ScopeId::ROOT),
            presence: Signal::new_in_scope(Vec::new(), ScopeId::ROOT),
            presence_link: Signal::new_in_scope(None, ScopeId::ROOT),
            pending_actor: Signal::new_in_scope(std::collections::HashMap::new(), ScopeId::ROOT),
            pending_cause: Signal::new_in_scope(std::collections::HashMap::new(), ScopeId::ROOT),
            cursor_line: Signal::new_in_scope(None, ScopeId::ROOT),
            assets_epoch: Signal::new_in_scope(0, ScopeId::ROOT),
            terminal_cwd: Signal::new_in_scope(None, ScopeId::ROOT),
            adopt_terminals: Signal::new_in_scope(Vec::new(), ScopeId::ROOT),
            remote: Signal::new_in_scope(None, ScopeId::ROOT),
            server_link: Signal::new_in_scope(None, ScopeId::ROOT),
            lsp_status: Signal::new_in_scope(None, ScopeId::ROOT),
            graph_request: Signal::new_in_scope(None, ScopeId::ROOT),
            reveal: Signal::new_in_scope(None, ScopeId::ROOT),
            unique: Signal::new_in_scope(0, ScopeId::ROOT),
            flow_libraries: Signal::new_in_scope(Vec::new(), ScopeId::ROOT),
            wasm_extensions: Signal::new_in_scope(Vec::new(), ScopeId::ROOT),
            settings_user: Signal::new_in_scope(
                crate::settings::SettingsFile::new(),
                ScopeId::ROOT,
            ),
            settings_workspace: Signal::new_in_scope(
                crate::settings::SettingsFile::new(),
                ScopeId::ROOT,
            ),
            settings: Signal::new_in_scope(
                crate::settings::Settings::resolve(
                    &crate::settings::SettingsFile::new(),
                    &crate::settings::SettingsFile::new(),
                    &crate::settings::Settings::env_overrides(),
                ),
                ScopeId::ROOT,
            ),
            settings_folder: Signal::new_in_scope(None, ScopeId::ROOT),
            closed_panels: Signal::new_in_scope(Default::default(), ScopeId::ROOT),
            hidden_tiles: Signal::new_in_scope(Default::default(), ScopeId::ROOT),
            bus: Signal::new_in_scope(None, ScopeId::ROOT),
            config,
        }
    }

    /// Install the platform's session transport and announce this window.
    pub fn connect_bus(&mut self, bus: Rc<dyn SessionBus>) {
        bus.send(SessionMessage::Hello {
            from: self.window.peek().clone(),
        });
        self.bus.set(Some(bus));
    }

    fn send(&self, msg: SessionMessage) {
        tracing::info!("session[{}] send {}", self.window.peek(), summary(&msg));
        if let Some(bus) = self.bus.peek().as_ref() {
            bus.send(msg);
        }
    }

    /// Start a cross-window drag from a workbench tab. `tab_id` is the DOM id
    /// of the dragged tab (`wb-tab-<panel id>`); any open document whose node
    /// id appears in it is the one being dragged, whatever the panel scheme.
    pub fn start_drag_from_tab(&mut self, tab_id: &str) -> bool {
        let node = self
            .documents
            .peek()
            .iter()
            .map(|(id, _)| *id)
            .find(|id| tab_id.contains(&id.to_string()));
        match node {
            Some(node) => {
                self.start_drag(node);
                true
            }
            None => false,
        }
    }

    /// React to a message from another window of this session.
    pub async fn handle_message(mut self, msg: SessionMessage) {
        let me = self.window.peek().clone();
        if msg.sender() == &me {
            return;
        }
        tracing::info!("session[{me}] recv {}", summary(&msg));
        let sender = msg.sender().clone();
        if !self.peers.peek().contains(&sender) {
            self.peers.with_mut(|p| p.push(sender));
        }
        match msg {
            SessionMessage::Welcome { .. } => {}
            SessionMessage::Hello { .. } => {
                self.send(SessionMessage::Welcome { from: me.clone() });
                // Tell the newcomer what we have open.
                let sources: Vec<_> = self
                    .sources
                    .peek()
                    .iter()
                    .map(|s| s.descriptor.clone())
                    .collect();
                for descriptor in sources {
                    self.send(SessionMessage::SourceOpened {
                        from: me.clone(),
                        descriptor,
                    });
                }
            }
            SessionMessage::SourceOpened { descriptor, .. } => {
                let _ = self.attach_source(descriptor).await;
            }
            SessionMessage::DragStarted { from, node, source } => {
                self.foreign_drag.set(Some(ForeignDrag {
                    from,
                    node,
                    source,
                    live: true,
                }));
            }
            SessionMessage::DragEnded { from } => {
                // Keep the offer, but mark it as no longer a live drag.
                let pending = self.foreign_drag.peek().clone();
                if let Some(mut d) = pending {
                    if d.from == from && d.live {
                        d.live = false;
                        self.foreign_drag.set(Some(d));
                    }
                }
            }
            SessionMessage::Moved { node, to, .. } => {
                // Our document landed in another window: close it here.
                if self.document(node).is_some() {
                    self.close_node(node);
                    self.set_status(format!("Moved to window {to}"));
                }
                // Someone accepted the offer: withdraw it everywhere.
                if self.foreign_drag.peek().as_ref().map(|d| d.node.id) == Some(node) {
                    self.foreign_drag.set(None);
                }
            }
        }
    }

    /// Open a source another window already has (no-op if we have it).
    /// Close a source (spec 015): its documents go (unsaved ones block the
    /// close with a status message), derived sources over it (the index)
    /// go with it, and its workspace state is left on disk. Closing the
    /// folder the workspace settings belong to also drops the history log
    /// and the presence room; nothing is reopened on the next start.
    pub async fn close_source(mut self, id: &SourceId) -> Result<(), SourceError> {
        let Some(handle) = self
            .sources
            .peek()
            .iter()
            .find(|s| &s.descriptor.id == id)
            .cloned()
        else {
            return Err(SourceError::NotFound);
        };
        // Everything derived from this source closes too.
        let derived: Vec<SourceId> = self
            .sources
            .peek()
            .iter()
            .filter(|s| {
                s.descriptor
                    .id
                    .as_str()
                    .ends_with(&format!(":{}", id.as_str()))
                    && s.descriptor.family == moonkale_core::SourceFamily::Index
            })
            .map(|s| s.descriptor.id.clone())
            .collect();
        let closing: Vec<SourceId> = std::iter::once(id.clone()).chain(derived).collect();
        let docs: Vec<(NodeId, bool)> = self
            .documents
            .peek()
            .iter()
            .filter(|(_, d)| closing.contains(&d.peek().node.source))
            .map(|(n, d)| (*n, d.peek().dirty()))
            .collect();
        let dirty = docs.iter().filter(|(_, d)| *d).count();
        if dirty > 0 {
            self.set_status(format!(
                "{}: save or reload {dirty} unsaved document(s) before closing it",
                handle.descriptor.display_name
            ));
            return Err(SourceError::Invalid(format!("{dirty} unsaved document(s)")));
        }
        for (n, _) in docs {
            self.close_node(n);
        }
        if self.settings_folder.peek().as_ref() == Some(id) {
            self.persist_history().await;
            self.history.set(moonkale_core::EntityLog::new());
            self.settings_workspace
                .set(crate::settings::SettingsFile::new());
            self.settings_folder.set(None);
            self.presence_link.set(None);
            self.presence.set(Vec::new());
            self.resolve_settings();
        }
        self.sources
            .with_mut(|v| v.retain(|s| !closing.contains(&s.descriptor.id)));
        self.graph_epoch.with_mut(|e| *e += 1);
        if self.is_remote_source(id) {
            self.close_remote();
        } else {
            self.update_user_settings(|f| f.reopen_last = Some(false))
                .await;
        }
        self.set_status(format!("Closed {}", handle.descriptor.display_name));
        Ok(())
    }

    // ---- Remote folders (Milestone 11) ----

    /// The server's agent sessions, when the sources live there (Milestone 12).
    pub fn agent_sessions(&self) -> Option<AgentSessions> {
        // Opt-in (`agent.on_server`), except on a platform without a local
        // provider (the phone), which always uses them when connected.
        if !self.settings.read().agent.on_server && self.config.llm.is_some() {
            return None;
        }
        self.config.agent_sessions.filter(|a| (a.available)())
    }

    /// Can this platform open folders over SSH?
    pub fn has_remote(&self) -> bool {
        self.config.remote.is_some()
    }

    /// Host aliases from `~/.ssh/config` (desktop), for the dialog.
    pub fn remote_hosts(&self) -> Vec<String> {
        self.config.remote.map(|r| (r.hosts)()).unwrap_or_default()
    }

    /// Was `id` opened through the remote session?
    pub fn is_remote_source(&self, id: &SourceId) -> bool {
        self.remote
            .peek()
            .as_ref()
            .is_some_and(|r| r.sources.contains(id))
    }

    /// Start a session to `host` and open `path` there when it is up. The
    /// `ssh` process becomes a terminal tab (its prompts are answered
    /// there); phases arrive on a channel and drive the status bar.
    pub fn open_remote(mut self, host: String, path: String) {
        let Some(remote) = self.config.remote else {
            self.set_status("Remote folders are not available on this platform");
            return;
        };
        if self.remote.peek().is_some() {
            self.close_remote();
        }
        let host = host.trim().to_string();
        let path = path.trim().to_string();
        if host.is_empty() || path.is_empty() {
            self.set_status("Remote: a host and a path are needed");
            return;
        }
        let (tx, mut rx) = futures_channel::mpsc::unbounded::<crate::remote::RemotePhase>();
        let sink: crate::remote::PhaseSink = Box::new(move |p| {
            let _ = tx.unbounded_send(p);
        });
        let (backend, session) = match (remote.open)(host.clone(), path.clone(), sink) {
            Ok(x) => x,
            Err(e) => {
                self.set_status(format!("Remote: {e}"));
                return;
            }
        };
        self.remote.set(Some(crate::remote::RemoteState {
            host: host.clone(),
            path: path.clone(),
            phase: crate::remote::RemotePhase::Connecting,
            session,
            sources: Vec::new(),
        }));
        let title = backend.title();
        self.adopt_terminal(moonkale_terminal::Session {
            id: moonkale_terminal::SessionId::fresh(),
            title,
            cwd: None,
            backend,
        });
        self.set_status(format!("Remote: connecting to {host}…"));
        spawn(async move {
            use crate::remote::RemotePhase as P;
            use futures_util::StreamExt;
            while let Some(p) = rx.next().await {
                if self.remote.peek().is_none() {
                    break; // closed meanwhile
                }
                self.remote.with_mut(|r| {
                    if let Some(r) = r {
                        r.phase = p.clone();
                    }
                });
                match &p {
                    P::Prompt(line) => {
                        self.set_status(format!("ssh {host}: {line} — answer in the terminal"));
                        self.dispatch(Command::ShowPanel("terminal"));
                    }
                    P::Uploading => self.set_status(format!(
                        "Remote: {host} has no Moonkale server yet — uploading it (once per version)…"
                    )),
                    P::Starting => self.set_status(format!("Remote: starting the server on {host}…")),
                    P::Ready => {
                        self.set_status(format!("Remote: connected to {host}, opening {path}…"));
                        if let Err(e) = self.open_folder(path.clone()).await {
                            self.set_status(format!("Remote: {host} is connected but {path} did not open: {e}"));
                        }
                    }
                    P::Failed(e) => self.set_status(format!("Remote: {e}")),
                    P::Connecting | P::Closed => {}
                }
                if p.is_final() {
                    break;
                }
            }
        });
    }

    /// End the session: the sources it opened go, `ssh` and the remote
    /// server with it, and the desktop is local again.
    pub fn close_remote(&mut self) {
        let Some(state) = self.remote.take() else {
            return;
        };
        state.session.close();
        let closing = state.sources.clone();
        let docs: Vec<NodeId> = self
            .documents
            .peek()
            .iter()
            .filter(|(_, d)| closing.contains(&d.peek().node.source))
            .map(|(n, _)| *n)
            .collect();
        for n in docs {
            self.close_node(n);
        }
        if !closing.is_empty() {
            self.sources
                .with_mut(|v| v.retain(|s| !closing.contains(&s.descriptor.id)));
            if self
                .settings_folder
                .peek()
                .as_ref()
                .is_some_and(|f| closing.contains(f))
            {
                self.settings_folder.set(None);
                self.settings_workspace
                    .set(crate::settings::SettingsFile::new());
                self.resolve_settings();
            }
            self.graph_epoch.with_mut(|e| *e += 1);
        }
        self.set_status(format!("Remote: disconnected from {}", state.label()));
    }

    // ---- A server's client (Milestone 12) ----

    pub fn has_server_client(&self) -> bool {
        self.config.server.is_some()
    }

    /// Become `url`'s client and open its root folder.
    pub async fn connect_server(mut self, url: String, token: Option<String>) {
        let Some(sc) = self.config.server else {
            self.set_status("Connecting to a server is not available on this platform");
            return;
        };
        let url = url.trim().trim_end_matches('/').to_string();
        if url.is_empty() {
            self.set_status("Server: a URL is needed");
            return;
        }
        if self.server_link.peek().is_some() {
            self.disconnect_server();
        }
        if let Err(e) = (sc.connect)(url.clone(), token.filter(|t| !t.trim().is_empty())) {
            self.set_status(format!("Server: {e}"));
            return;
        }
        self.server_link.set(Some((url.clone(), Vec::new())));
        self.set_status(format!("Connected to {url}; opening its folder…"));
        if let Err(e) = self.open_folder(String::new()).await {
            self.set_status(format!(
                "Server {url}: connected, but its folder did not open: {e}"
            ));
        }
    }

    /// Drop the connection and the sources it opened.
    pub fn disconnect_server(&mut self) {
        let Some((url, ids)) = self.server_link.take() else {
            return;
        };
        if let Some(sc) = self.config.server {
            (sc.disconnect)();
        }
        let docs: Vec<NodeId> = self
            .documents
            .peek()
            .iter()
            .filter(|(_, d)| ids.contains(&d.peek().node.source))
            .map(|(n, _)| *n)
            .collect();
        for n in docs {
            self.close_node(n);
        }
        if !ids.is_empty() {
            self.sources
                .with_mut(|v| v.retain(|s| !ids.contains(&s.descriptor.id)));
            if self
                .settings_folder
                .peek()
                .as_ref()
                .is_some_and(|f| ids.contains(f))
            {
                self.settings_folder.set(None);
                self.settings_workspace
                    .set(crate::settings::SettingsFile::new());
                self.resolve_settings();
            }
            self.graph_epoch.with_mut(|e| *e += 1);
        }
        self.set_status(format!("Disconnected from {url}"));
    }

    /// Hand a running terminal to the terminal panel (it becomes a tab).
    pub fn adopt_terminal(&mut self, session: moonkale_terminal::Session) {
        self.adopt_terminals
            .with_mut(|v| v.push(Rc::new(RefCell::new(Some(session)))));
        self.dispatch(Command::ShowPanel("terminal"));
    }

    pub async fn attach_source(mut self, descriptor: SourceDescriptor) -> Result<(), SourceError> {
        if self.source(&descriptor.id).is_some() {
            return Ok(());
        }
        let source = (self.config.attach_source)(descriptor.clone()).await?;
        self.sources
            .with_mut(|v| v.push(SourceHandle { descriptor, source }));
        Ok(())
    }

    /// Begin dragging one of our documents out (HTML5 `dragstart`).
    pub fn start_drag(&mut self, node: NodeId) {
        let Some((doc, source)) = self.document(node).and_then(|d| {
            let d = d.read();
            let src = self
                .sources
                .peek()
                .iter()
                .find(|s| s.descriptor.id == d.node.source)?
                .descriptor
                .clone();
            Some((d.node.clone(), src))
        }) else {
            return;
        };
        self.own_drag.set(Some(node));
        self.set_status(format!(
            "Dragging {} — drop it on another Moonkale window to move it there",
            doc.native_key
        ));
        self.send(SessionMessage::DragStarted {
            from: self.window.peek().clone(),
            node: doc,
            source,
        });
    }

    /// Decline an offer from another window.
    pub fn dismiss_drop(&mut self) {
        self.foreign_drag.set(None);
    }

    /// The drag ended without a drop elsewhere (`dragend`).
    pub fn end_drag(&mut self) {
        if self.own_drag.peek().is_some() {
            self.own_drag.set(None);
            self.send(SessionMessage::DragEnded {
                from: self.window.peek().clone(),
            });
        }
    }

    /// A foreign drag was dropped on this window: open the document here
    /// and tell the origin to close its copy.
    pub async fn accept_drop(mut self) -> Result<(), SourceError> {
        let Some(drag) = self.foreign_drag.peek().clone() else {
            return Ok(());
        };
        self.foreign_drag.set(None);
        self.attach_source(drag.source).await?;
        self.open_node(drag.node.clone()).await?;
        self.send(SessionMessage::Moved {
            node: drag.node.id,
            from: drag.from,
            to: self.window.peek().clone(),
        });
        Ok(())
    }

    pub fn spawn_lsp(&self) -> Option<moonkale_lsp::SpawnLsp> {
        self.config.spawn_lsp
    }

    pub fn llm(&self) -> Option<LlmProvider> {
        self.config.llm
    }

    /// Re-scan installed wasm extensions (user dir + the folder's).
    pub async fn refresh_wasm_extensions(mut self) {
        let Some(w) = self.config.wasm else { return };
        let folder = self
            .sources
            .peek()
            .iter()
            .find(|s| s.descriptor.family == moonkale_core::SourceFamily::Folder)
            .and_then(|s| {
                s.descriptor
                    .id
                    .as_str()
                    .strip_prefix("folder:")
                    .map(str::to_string)
            });
        match (w.list)(folder).await {
            Ok(list) => {
                if *self.wasm_extensions.peek() != list {
                    self.wasm_extensions.set(list);
                }
            }
            Err(e) => self.set_status(format!("Extensions not scanned: {e}")),
        }
    }

    /// Run a wasm extension's command with the permissions granted in settings.
    pub async fn run_wasm_command(
        &self,
        ext_id: &str,
        command: &str,
        args: serde_json::Value,
    ) -> Result<String, String> {
        let w = self
            .config
            .wasm
            .ok_or("wasm extensions are not available on this platform")?;
        let granted = self
            .settings
            .peek()
            .extensions
            .permissions
            .get(ext_id)
            .cloned()
            .unwrap_or_default();
        // Milestone 8: in a cross-origin-isolated browser the module runs
        // here, its host calls answered by this workspace's sources; the
        // server path stays as the fallback.
        if let Some(url) = self.config.wasm_module_url {
            match self
                .run_wasm_in_browser(&url(ext_id.to_string()), command, args.clone(), &granted)
                .await
            {
                Ok(BrowserRun::Done(r)) => return r,
                Ok(BrowserRun::Unavailable) => {}
                Err(e) => return Err(e),
            }
        }
        (w.run)(ext_id.to_string(), command.to_string(), args, granted).await
    }

    /// Run a module in the page's Worker runtime (`window.moonkale.wasmHost`),
    /// answering its host calls. `Unavailable` when the page cannot (no
    /// isolation, no runtime script): the caller falls back to the server.
    async fn run_wasm_in_browser(
        &self,
        url: &str,
        command: &str,
        args: serde_json::Value,
        granted: &[String],
    ) -> Result<BrowserRun, String> {
        use moonkale_ext_host::abi::{HostCall, HostReply};
        let mut ev = dioxus::document::eval(WASM_HOST_JS);
        let _ = ev.send(serde_json::json!({ "url": url, "command": command, "args": args }));
        loop {
            let msg: serde_json::Value = match ev.recv().await {
                Ok(v) => v,
                Err(e) => return Err(format!("browser runtime: {e}")),
            };
            match msg.get("kind").and_then(|k| k.as_str()) {
                Some("unavailable") => return Ok(BrowserRun::Unavailable),
                Some("log") => tracing::info!(
                    "[ext] {}",
                    msg.get("text").and_then(|t| t.as_str()).unwrap_or("")
                ),
                Some("call") => {
                    let json = msg.get("json").and_then(|j| j.as_str()).unwrap_or("");
                    let reply = match serde_json::from_str::<HostCall>(json) {
                        Ok(call) => self.answer_host_call(call, granted).await,
                        Err(e) => HostReply::err(format!("bad host call: {e}")),
                    };
                    let _ = ev.send(serde_json::to_value(reply).unwrap_or_default());
                }
                Some("done") => {
                    let reply = msg.get("reply").and_then(|r| r.as_str()).unwrap_or("");
                    let parsed: moonkale_ext_host::abi::RunReply = serde_json::from_str(reply)
                        .map_err(|e| format!("reply JSON: {e}: {reply}"))?;
                    return Ok(BrowserRun::Done(parsed.into_result()));
                }
                Some("error") => {
                    return Err(msg
                        .get("error")
                        .and_then(|e| e.as_str())
                        .unwrap_or("browser runtime failed")
                        .to_string())
                }
                _ => {}
            }
        }
    }

    /// The host side of the JSON ABI, over this workspace's sources, with
    /// the same permission check as the native runtime.
    async fn answer_host_call(
        &self,
        call: moonkale_ext_host::abi::HostCall,
        granted: &[String],
    ) -> moonkale_ext_host::abi::HostReply {
        use moonkale_ext_host::abi::{HostCall, HostReply};
        let needed = call.permission();
        if !granted.iter().any(|g| g == needed) {
            return HostReply::err(format!(
                "permission {needed} not granted (Settings → Extensions)"
            ));
        }
        match call {
            HostCall::ListSources => {
                let list: Vec<SourceDescriptor> = self
                    .sources
                    .peek()
                    .iter()
                    .map(|s| s.descriptor.clone())
                    .collect();
                HostReply::ok(serde_json::to_value(list).unwrap_or_default())
            }
            HostCall::Query { source, query } => {
                let Some(src) = self.source(&SourceId::new(source)) else {
                    return HostReply::err("unknown source");
                };
                let query: Query = match serde_json::from_value(query) {
                    Ok(q) => q,
                    Err(e) => return HostReply::err(format!("bad query: {e}")),
                };
                match src.query(query).await {
                    Ok(res) => HostReply::ok(serde_json::to_value(res).unwrap_or_default()),
                    Err(e) => HostReply::err(e.to_string()),
                }
            }
            HostCall::FetchText { source, node } => {
                let Some(src) = self.source(&SourceId::new(source)) else {
                    return HostReply::err("unknown source");
                };
                let Ok(node) = node.parse::<NodeId>() else {
                    return HostReply::err("bad node id");
                };
                match src.fetch_text(node).await {
                    Ok((text, _)) => HostReply::ok(serde_json::Value::String(text)),
                    Err(e) => HostReply::err(e.to_string()),
                }
            }
        }
    }

    pub fn has_settings_store(&self) -> bool {
        self.config.settings_store.is_some()
    }

    pub fn secret_store(&self) -> Option<SecretStore> {
        self.config.secret_store
    }

    // ---- settings -------------------------------------------------------

    /// Recompute the resolved settings from the two scopes + environment.
    fn resolve_settings(&mut self) {
        let resolved = crate::settings::Settings::resolve(
            &self.settings_user.peek(),
            &self.settings_workspace.peek(),
            &crate::settings::Settings::env_overrides(),
        );
        if *self.settings.peek() != resolved {
            self.settings.set(resolved);
        }
    }

    /// Load the user scope through the platform store (at startup).
    pub async fn load_user_settings(mut self) {
        let Some(store) = self.config.settings_store else {
            return;
        };
        match (store.load)().await {
            Ok(file) => {
                self.settings_user.set(file);
                self.resolve_settings();
            }
            Err(e) => self.set_status(format!("Settings not loaded: {e}")),
        }
        self.refresh_wasm_extensions().await;
        // Desktop: a remote folder asked for on the command line wins
        // (Milestone 11) …
        if let Some((host, path)) = self.config.remote.and_then(|r| (r.at_start)()) {
            tracing::info!("remote: opening {host}:{path} at start");
            self.open_remote(host, path);
            return;
        }
        // … else come back to where you were.
        if self.config.reopen_last_folder
            && self.sources.peek().is_empty()
            && self.settings_user.peek().reopen_last != Some(false)
        {
            let last = self.settings.peek().recent_folders.first().cloned();
            if let Some(path) = last {
                tracing::info!("settings: reopening last folder {path}");
                if let Err(e) = self.open_folder(path).await {
                    self.set_status(format!("Could not reopen the last folder: {e}"));
                }
            }
        }
    }

    /// Change the user scope and persist it.
    pub async fn update_user_settings(
        mut self,
        f: impl FnOnce(&mut crate::settings::SettingsFile),
    ) {
        let mut file = self.settings_user.peek().clone();
        f(&mut file);
        self.settings_user.set(file.clone());
        self.resolve_settings();
        if let Some(store) = self.config.settings_store {
            if let Err(e) = (store.save)(file).await {
                self.set_status(format!("Settings not saved: {e}"));
            }
        }
    }

    /// The raw bytes of a node (images, spec 008).
    pub async fn fetch_bytes(&self, node: &Node) -> Result<Vec<u8>, SourceError> {
        let source = self.source(&node.source).ok_or(SourceError::NotFound)?;
        source.fetch_bytes(node.id).await.map(|(b, _)| b)
    }

    /// The text of `rel` under `source`, `None` if it does not exist or is
    /// not readable (a small config file such as `.moonkale/katex.json`).
    pub async fn read_text_at(&self, source: &SourceId, rel: &str) -> Option<String> {
        let node = self.node_at_path(source, rel).await?;
        let src = self.source(source)?;
        src.fetch_text(node.id).await.ok().map(|(text, _)| text)
    }

    /// Read `.moonkale/settings.json` of `folder` (missing = defaults).
    pub async fn load_workspace_settings(mut self, folder: &SourceId) {
        let file = match self
            .node_at_path(folder, crate::settings::WORKSPACE_FILE)
            .await
        {
            Some(node) => match self.source(folder) {
                Some(src) => match src.fetch_text(node.id).await {
                    Ok((text, _)) => {
                        crate::settings::SettingsFile::parse(&text).unwrap_or_else(|e| {
                            self.set_status(format!("Workspace settings ignored: {e}"));
                            crate::settings::SettingsFile::new()
                        })
                    }
                    Err(_) => crate::settings::SettingsFile::new(),
                },
                None => crate::settings::SettingsFile::new(),
            },
            None => crate::settings::SettingsFile::new(),
        };
        tracing::info!(
            "settings: workspace {} → layout {}, {} open documents, active {:?}",
            folder,
            file.layout.is_some(),
            file.open_documents.len(),
            file.active_document
        );
        self.settings_folder.set(Some(folder.clone()));
        self.settings_workspace.set(file);
        self.resolve_settings();
        self.load_history(folder).await;
        self.join_presence(folder.as_str());
    }

    /// Read `.moonkale/history.jsonl` (Milestone 8); an absent file is an
    /// empty log.
    pub async fn load_history(mut self, folder: &SourceId) {
        let log = match self.node_at_path(folder, HISTORY_FILE).await {
            Some(node) => match self.source(folder) {
                Some(src) => match src.fetch_text(node.id).await {
                    Ok((text, _)) => moonkale_core::EntityLog::from_jsonl(&text),
                    Err(_) => moonkale_core::EntityLog::new(),
                },
                None => moonkale_core::EntityLog::new(),
            },
            None => moonkale_core::EntityLog::new(),
        };
        tracing::info!("history: {} events for {folder}", log.len());
        self.history.set(log);
    }

    /// The actor string for the user's own edits.
    pub fn user_actor(&self) -> String {
        format!("user:{}", self.settings.peek().user_name)
    }

    /// Append an event to the log and persist it (best effort, never blocks
    /// the write it records). Returns the event id.
    pub fn record(&mut self, kind: moonkale_core::EventKind) -> moonkale_core::EventId {
        self.record_as(self.user_actor(), kind)
    }

    pub fn record_as(
        &mut self,
        actor: String,
        kind: moonkale_core::EventKind,
    ) -> moonkale_core::EventId {
        self.record_event(actor, kind, None)
    }

    /// Like [`record_as`](Self::record_as) with the node's key for display.
    pub fn record_event(
        &mut self,
        actor: String,
        kind: moonkale_core::EventKind,
        key: Option<String>,
    ) -> moonkale_core::EventId {
        let at = now_ms();
        let mut event = moonkale_core::Event::new(at, actor, kind);
        if let Some(k) = key {
            event = event.with_key(k);
        }
        if let Some(node) = event.node() {
            if let Some(cause) = self.pending_cause.with_mut(|m| m.remove(&node)) {
                event.cause = Some(cause);
            }
        }
        let id = event.id;
        self.history.with_mut(|l| {
            l.append(event);
        });
        let ws = *self;
        spawn(async move { ws.persist_history().await });
        id
    }

    /// Fold everything but the last `keep` events into a snapshot
    /// (Milestone 9); returns how many events were folded.
    pub fn compact_history(&mut self, keep: usize) -> usize {
        let actor = self.user_actor();
        let at = now_ms();
        let folded = self.history.with_mut(|l| l.compact(keep, at, actor));
        if folded > 0 {
            let ws = *self;
            spawn(async move { ws.persist_history().await });
        }
        folded
    }

    /// Put a node's text as of `event` into its document as an unsaved edit
    /// (opening the document first if needed); the save records the
    /// restore with `cause = event`.
    pub async fn restore_text_at(
        mut self,
        node: NodeId,
        event: moonkale_core::EventId,
    ) -> Result<(), SourceError> {
        let text = self
            .history
            .peek()
            .text_at(node, Some(event))
            .ok_or_else(|| SourceError::Unsupported("no text recorded for that event".into()))?;
        if self.document(node).is_none() {
            let folder: Arc<dyn Source> = self
                .sources
                .peek()
                .iter()
                .find(|s| s.descriptor.family == moonkale_core::SourceFamily::Folder)
                .map(|s| s.source.clone())
                .ok_or(SourceError::NotFound)?;
            let n = folder
                .query(Query::Node(node))
                .await?
                .nodes
                .into_iter()
                .next()
                .ok_or(SourceError::NotFound)?;
            self.open_node(n).await?;
        }
        let mut doc = self.document(node).ok_or(SourceError::NotFound)?;
        doc.with_mut(|d| d.text = text);
        self.pending_cause.with_mut(|m| {
            m.insert(node, event);
        });
        self.active.set(Some(node));
        self.set_status("Restored as an unsaved edit — save to keep it");
        Ok(())
    }

    async fn persist_history(self) {
        let Some(folder) = self.settings_folder.peek().clone() else {
            return;
        };
        let Some(src) = self.source(&folder) else {
            return;
        };
        let text = self.history.peek().to_jsonl();
        let result = match self.node_at_path(&folder, HISTORY_FILE).await {
            Some(node) => {
                let chars = src
                    .fetch_text(node.id)
                    .await
                    .map(|(t, _)| t.chars().count())
                    .unwrap_or(0);
                src.apply(Transaction::write_text(
                    node.id,
                    node.version,
                    TextPatch::whole(&text, chars),
                ))
                .await
            }
            None => {
                let root = src.descriptor().root;
                src.apply(Transaction::create_text(root, HISTORY_FILE, text))
                    .await
            }
        };
        if let Err(e) = result {
            tracing::warn!("history: could not write {HISTORY_FILE}: {e}");
        }
    }

    /// Change the workspace scope and write it into the folder.
    pub async fn update_workspace_settings(
        mut self,
        f: impl FnOnce(&mut crate::settings::SettingsFile),
    ) {
        let mut file = self.settings_workspace.peek().clone();
        f(&mut file);
        self.settings_workspace.set(file.clone());
        self.resolve_settings();
        let Some(folder) = self.settings_folder.peek().clone() else {
            return;
        };
        let Some(src) = self.source(&folder) else {
            return;
        };
        let text = file.to_json();
        let result = match self
            .node_at_path(&folder, crate::settings::WORKSPACE_FILE)
            .await
        {
            Some(node) => {
                let chars = src
                    .fetch_text(node.id)
                    .await
                    .map(|(t, _)| t.chars().count())
                    .unwrap_or(0);
                src.apply(Transaction::write_text(
                    node.id,
                    node.version,
                    TextPatch::whole(&text, chars),
                ))
                .await
            }
            None => {
                let root = src.descriptor().root;
                src.apply(Transaction::create_text(
                    root,
                    crate::settings::WORKSPACE_FILE,
                    text,
                ))
                .await
            }
        };
        match result {
            Ok(applied) if applied.first_error().is_none() => {}
            Ok(applied) => {
                let e = applied.first_error().cloned().unwrap();
                self.set_status(format!("Workspace settings not saved: {e}"));
            }
            Err(e) => self.set_status(format!("Workspace settings not saved: {e}")),
        }
    }

    /// Resolve a relative path in a source: the folder source's `path`
    /// dialect (hidden files too), else a walk of the tree.
    pub async fn node_at_path(&self, source: &SourceId, rel: &str) -> Option<Node> {
        let src = self.source(source)?;
        if let Ok(r) = src
            .query(Query::Text {
                dialect: "path".into(),
                text: rel.into(),
            })
            .await
        {
            return r.nodes.into_iter().next();
        }
        let mut cur = src.descriptor().root;
        let mut found = None;
        for part in rel.split('/').filter(|p| !p.is_empty()) {
            let res = src.query(Query::Children(cur)).await.ok()?;
            let n = res.nodes.into_iter().find(|n| n.label == part)?;
            cur = n.id;
            found = Some(n);
        }
        found
    }

    /// Open `node` (if not already) and ask its editor to place the cursor.
    pub async fn reveal(mut self, node: Node, line: u32, col: u32) -> Result<(), SourceError> {
        let id = node.id;
        self.open_node(node).await?;
        let seq = self.reveal.peek().as_ref().map(|r| r.seq + 1).unwrap_or(1);
        self.reveal.set(Some(Reveal {
            node: id,
            line,
            col,
            seq,
        }));
        Ok(())
    }

    /// Open a file by path relative to any open folder (used by terminal
    /// links and go-to-definition). Returns the opened node.
    pub async fn open_relative_path(mut self, rel: &str) -> Result<Node, SourceError> {
        let rel = rel.trim_start_matches("./").to_string();
        let folders: Vec<_> = self
            .sources
            .peek()
            .iter()
            .filter(|s| s.descriptor.family == moonkale_core::SourceFamily::Folder)
            .cloned()
            .collect();
        for f in folders {
            let mut cur = f.descriptor.root;
            let mut found: Option<Node> = None;
            let mut ok = true;
            for part in rel.split('/').filter(|p| !p.is_empty()) {
                match f.source.query(Query::Children(cur)).await {
                    Ok(res) => match res.nodes.into_iter().find(|n| n.label == part) {
                        Some(n) => {
                            cur = n.id;
                            found = Some(n);
                        }
                        None => {
                            ok = false;
                            break;
                        }
                    },
                    Err(_) => {
                        ok = false;
                        break;
                    }
                }
            }
            if ok {
                if let Some(n) = found.filter(|n| n.kind == NodeKind::File) {
                    self.open_node(n.clone()).await?;
                    return Ok(n);
                }
            }
        }
        self.set_status(format!("{rel}: not found in the open folders"));
        Err(SourceError::NotFound)
    }

    pub fn compile_typst(&self) -> Option<CompileTypst> {
        self.config.compile_typst
    }

    /// The platform's terminal spawner, if any.
    pub fn spawn_terminal(&self) -> Option<moonkale_terminal::SpawnTerminal> {
        self.config.spawn_terminal
    }

    /// Absolute path of a folder node, when its source is a local folder.
    pub fn folder_path(&self, node: &Node) -> Option<String> {
        let root = node.source.as_str().strip_prefix("folder:")?;
        Some(if node.native_key.is_empty() {
            root.to_string()
        } else {
            format!("{root}/{}", node.native_key)
        })
    }

    /// Whether this platform has a native folder dialog.
    pub fn has_folder_dialog(&self) -> bool {
        self.config.pick_folder.is_some()
    }

    /// Focus an element by id once the next frame has rendered (commands
    /// that open a panel and want its input focused). Best effort.
    pub fn focus_element(&self, id: &str) {
        let js = format!(
            "requestAnimationFrame(() => {{ const el = document.getElementById({id:?}); if (el) el.focus(); }});"
        );
        let _ = dioxus::document::eval(&js);
    }

    /// My presence record as the hub should see it now.
    pub fn my_presence(&self) -> crate::presence::Member {
        let active = self
            .active
            .peek()
            .and_then(|n| self.document(n))
            .map(|d| d.peek().node.native_key.clone());
        let line = if active.is_some() {
            *self.cursor_line.peek()
        } else {
            None
        };
        crate::presence::Member {
            window: self.window.peek().to_string(),
            name: self.settings.peek().user_name.clone(),
            active,
            line,
        }
    }

    /// The editor reports the cursor of `node`; published when it is the
    /// active document.
    pub fn set_cursor_line(&mut self, node: NodeId, line: u32) {
        if *self.active.peek() == Some(node) && *self.cursor_line.peek() != Some(line) {
            self.cursor_line.set(Some(line));
            self.publish_presence();
        }
    }

    /// Join the folder's presence room (called when a folder opens); a
    /// no-op without a hub. Re-joining replaces the link.
    pub fn join_presence(&mut self, room: &str) {
        let Some(join) = self.config.presence else {
            return;
        };
        let mut members = self.presence;
        let on_members = Callback::new(move |list: Vec<crate::presence::Member>| members.set(list));
        let link = join(room.to_string(), self.my_presence(), on_members);
        self.presence_link.set(Some(link));
    }

    /// Reopen a closed static panel (spec 011) and bring it to the front.
    /// Ids come from contributions at runtime, so the one the command
    /// carries is interned (panel ids are few and stable).
    pub fn show_panel(&mut self, id: &str) {
        self.closed_panels.with_mut(|c| {
            c.remove(id);
        });
        let id: &'static str = intern_panel_id(id);
        self.dispatch(Command::ShowPanel(id));
    }

    /// Tell the hub what this window looks at now.
    pub fn publish_presence(&self) {
        if let Some(link) = self.presence_link.peek().as_ref() {
            link.update(self.my_presence());
        }
    }

    /// Members other than this window.
    pub fn others(&self) -> Vec<crate::presence::Member> {
        let me = self.window.peek().to_string();
        self.presence
            .peek()
            .iter()
            .filter(|m| m.window != me)
            .cloned()
            .collect()
    }

    /// The platform's git runner, if any.
    pub fn git(&self) -> Option<GitRun> {
        self.config.git
    }

    /// The first open folder's path (what `git` and terminals run in).
    pub fn folder_root(&self) -> Option<String> {
        self.sources
            .peek()
            .iter()
            .find(|s| s.descriptor.family == moonkale_core::SourceFamily::Folder)
            .and_then(|s| {
                s.descriptor
                    .id
                    .as_str()
                    .strip_prefix("folder:")
                    .map(str::to_string)
            })
    }

    /// Dispatch an application command to whoever handles it.
    pub fn dispatch(&mut self, cmd: Command) {
        let seq = self.commands.peek().0 + 1;
        self.commands.set((seq, Some(cmd)));
    }

    /// Show the native folder dialog (if any) and open the chosen folder.
    /// `Ok(None)` means cancelled or no dialog on this platform.
    pub async fn open_folder_dialog(mut self) -> Result<Option<SourceDescriptor>, SourceError> {
        let Some(pick) = self.config.pick_folder else {
            self.set_status("No folder dialog on this platform — type a path in the Explorer");
            return Ok(None);
        };
        match pick().await {
            Some(path) => self.open_folder(path).await.map(Some),
            None => Ok(None),
        }
    }

    /// The active document, if any.
    pub fn active_document(&self) -> Option<(NodeId, Signal<Document>)> {
        let id = (*self.active.read())?;
        self.document(id).map(|d| (id, d))
    }

    pub fn source(&self, id: &SourceId) -> Option<Arc<dyn Source>> {
        self.sources
            .read()
            .iter()
            .find(|s| &s.descriptor.id == id)
            .map(|s| s.source.clone())
    }

    pub fn document(&self, node: NodeId) -> Option<Signal<Document>> {
        self.documents
            .read()
            .iter()
            .find(|(id, _)| *id == node)
            .map(|(_, d)| *d)
    }

    pub fn set_status(&mut self, msg: impl Into<String>) {
        self.status.set(msg.into());
    }

    /// Add an in-process source (a parsed trace, a scratch graph) to this
    /// window. Not announced to the session bus: it has no path to reopen.
    pub fn add_source(&mut self, source: Arc<dyn Source>) -> SourceDescriptor {
        let descriptor = source.descriptor();
        self.sources.with_mut(|v| {
            v.retain(|s| s.descriptor.id != descriptor.id);
            v.push(SourceHandle {
                descriptor: descriptor.clone(),
                source,
            });
        });
        descriptor
    }

    /// A per-window counter for ids of in-process sources.
    pub fn next_unique(&mut self) -> u64 {
        let n = *self.unique.peek() + 1;
        self.unique.set(n);
        n
    }

    /// Open a folder through the platform's factory and add it to `sources`.
    pub async fn open_folder(mut self, path: String) -> Result<SourceDescriptor, SourceError> {
        let options = {
            let s = self.settings.peek();
            OpenOptions {
                embed: (s.search.embeddings && s.llm.embed_model.is_some()
                    || s.search.embeddings && s.llm.provider == "mock")
                    .then(|| s.llm.clone()),
            }
        };
        tracing::info!("open_folder: {path}");
        let sources = (self.config.open_folder)(path, options).await?;
        tracing::info!("open_folder: {} sources", sources.len());
        let mut first: Option<SourceDescriptor> = None;
        let mut opened: Vec<SourceId> = Vec::new();
        for source in sources {
            let descriptor = source.descriptor();
            opened.push(descriptor.id.clone());
            self.sources.with_mut(|v| {
                v.retain(|s| s.descriptor.id != descriptor.id);
                v.push(SourceHandle {
                    descriptor: descriptor.clone(),
                    source,
                });
            });
            self.send(SessionMessage::SourceOpened {
                from: self.window.peek().clone(),
                descriptor: descriptor.clone(),
            });
            first.get_or_insert(descriptor);
        }
        let first = first.ok_or_else(|| SourceError::Invalid("nothing opened".into()))?;
        self.set_status(format!("Opened {}", self.sources_summary()));
        tracing::info!("open_folder: first {} ({:?})", first.id, first.family);
        let remote = self
            .remote
            .peek()
            .as_ref()
            .is_some_and(|r| r.phase == crate::remote::RemotePhase::Ready);
        if remote {
            // Opened through the SSH session: remember it there, not in the
            // recent folders (the path is not on this machine).
            self.remote.with_mut(|r| {
                if let Some(r) = r {
                    r.sources.extend(opened.iter().cloned());
                }
            });
        }
        let via_server = self.server_link.peek().is_some();
        if via_server {
            self.server_link.with_mut(|l| {
                if let Some((_, ids)) = l {
                    ids.extend(opened.iter().cloned());
                }
            });
        }
        if first.family == moonkale_core::SourceFamily::Folder && !remote && !via_server {
            // Workspace settings + remember the folder.
            self.load_workspace_settings(&first.id).await;
            self.refresh_wasm_extensions().await;
            let path = first
                .id
                .as_str()
                .strip_prefix("folder:")
                .unwrap_or(first.id.as_str())
                .to_string();
            self.update_user_settings(|f| {
                f.push_recent(&path);
                f.reopen_last = Some(true);
            })
            .await;
        }
        Ok(first)
    }

    /// "folder · index: 12 files · 30 links" — for the status bar.
    pub fn sources_summary(&self) -> String {
        self.sources
            .peek()
            .iter()
            .map(|s| s.descriptor.display_name.clone())
            .collect::<Vec<_>>()
            .join(" · ")
    }

    /// The index source, if one is open (derived data: links, symbols).
    pub fn index(&self) -> Option<SourceHandle> {
        self.sources
            .peek()
            .iter()
            .find(|s| s.descriptor.family == moonkale_core::SourceFamily::Index)
            .cloned()
    }

    pub async fn query(&self, source: &SourceId, query: Query) -> Result<QueryResult, SourceError> {
        let s = self.source(source).ok_or(SourceError::NotFound)?;
        s.query(query).await
    }

    /// Load a node's text (if not already open) and make it the active
    /// document.
    pub async fn open_node(mut self, node: Node) -> Result<(), SourceError> {
        // Nodes without a text body (tables, images and other blobs) open as
        // views, not documents (spec 008).
        if matches!(node.kind, NodeKind::Table)
            || matches!(node.content, Some(moonkale_core::ContentRef::Blob { .. }))
        {
            if !self.views.peek().iter().any(|n| n.id == node.id) {
                self.views.with_mut(|v| v.push(node.clone()));
            }
            self.active.set(Some(node.id));
            self.set_status(format!("Opened {}", node.label));
            return Ok(());
        }
        if self.document(node.id).is_none() {
            let source = self.source(&node.source).ok_or(SourceError::NotFound)?;
            let (text, version) = source.fetch_text(node.id).await?;
            let doc =
                Signal::new_in_scope(Document::new(node.clone(), text, version), ScopeId::ROOT);
            self.documents.with_mut(|v| v.push((node.id, doc)));
        }
        self.active.set(Some(node.id));
        self.set_status(format!("Opened {}", node.native_key));
        Ok(())
    }

    /// Open a blob as a text document anyway (an SVG's source, spec 008).
    pub async fn open_as_text(mut self, node: Node) -> Result<(), SourceError> {
        if self.document(node.id).is_none() {
            let source = self.source(&node.source).ok_or(SourceError::NotFound)?;
            let (text, version) = source.fetch_text(node.id).await?;
            let doc =
                Signal::new_in_scope(Document::new(node.clone(), text, version), ScopeId::ROOT);
            self.documents.with_mut(|v| v.push((node.id, doc)));
        }
        self.views.with_mut(|v| v.retain(|n| n.id != node.id));
        self.active.set(Some(node.id));
        Ok(())
    }

    pub fn close_node(mut self, node: NodeId) {
        self.documents.with_mut(|v| v.retain(|(id, _)| *id != node));
        self.views.with_mut(|v| v.retain(|n| n.id != node));
        if self.active.read().as_ref() == Some(&node) {
            let next = self.documents.read().last().map(|(id, _)| *id);
            self.active.set(next);
        }
    }

    /// Save one document: build the patch, apply it through its source,
    /// record the new version. Conflicts surface as `SourceError::Conflict`
    /// and leave the document dirty.
    pub async fn save(mut self, node: NodeId) -> Result<(), SourceError> {
        let mut doc = self.document(node).ok_or(SourceError::NotFound)?;
        let (source_id, version, patch, key, before) = {
            let d = doc.read();
            (
                d.node.source.clone(),
                d.version,
                d.patch(),
                d.node.native_key.clone(),
                d.saved.clone(),
            )
        };
        let source = self.source(&source_id).ok_or(SourceError::NotFound)?;
        let applied = source
            .apply(Transaction::write_text(node, version, patch.clone()))
            .await?;
        match applied.version_of(node) {
            Some(v) => {
                let chars_after = doc.peek().text.chars().count();
                doc.with_mut(|d| d.mark_saved(v));
                self.set_status(format!("Saved {key}"));
                // History: the patch the save carried, attributed to the
                // agent when it made the edit (the user still approved it).
                let actor = match self.pending_actor.with_mut(|m| m.remove(&node)) {
                    Some(a) => format!("{a} (saved by {})", self.settings.peek().user_name),
                    None => self.user_actor(),
                };
                // Files that predate the log get their pre-edit text as the base.
                let base = if self.history.peek().text_at(node, None).is_none() {
                    Some(before)
                } else {
                    None
                };
                self.record_event(
                    actor,
                    moonkale_core::EventKind::Content {
                        node,
                        patch,
                        chars_after,
                        base,
                    },
                    Some(key.clone()),
                );
                // Let derived sources (the index) re-read the file.
                let others: Vec<Arc<dyn Source>> = self
                    .sources
                    .peek()
                    .iter()
                    .filter(|s| s.descriptor.id != source_id)
                    .map(|s| s.source.clone())
                    .collect();
                for other in others {
                    if let Err(e) = other.refresh(node).await {
                        tracing::warn!("refresh after save failed: {e}");
                    }
                }
                self.graph_epoch.with_mut(|e| *e += 1);
                Ok(())
            }
            None => {
                let err = applied
                    .first_error()
                    .cloned()
                    .unwrap_or(SourceError::Unsupported("write refused".into()));
                self.set_status(format!("Save failed: {err}"));
                Err(err)
            }
        }
    }

    /// Create a text node (a file) under `parent` in `source_id`, let derived
    /// sources index it, and return it. Used for saved transcripts.
    pub async fn create_text(
        mut self,
        source_id: &SourceId,
        parent: NodeId,
        name: &str,
        text: &str,
    ) -> Result<Node, SourceError> {
        let source = self.source(source_id).ok_or(SourceError::NotFound)?;
        let applied = source
            .apply(Transaction::create_text(parent, name, text))
            .await?;
        let node_id = match applied.results.first() {
            Some(moonkale_core::OpResult::Ok { node, .. }) => *node,
            Some(moonkale_core::OpResult::Refused { error, .. }) => return Err(error.clone()),
            None => return Err(SourceError::Unsupported("create refused".into())),
        };
        let node = source
            .query(Query::Node(node_id))
            .await?
            .nodes
            .into_iter()
            .next()
            .ok_or(SourceError::NotFound)?;
        let others: Vec<Arc<dyn Source>> = self
            .sources
            .peek()
            .iter()
            .filter(|s| &s.descriptor.id != source_id)
            .map(|s| s.source.clone())
            .collect();
        for other in others {
            let _ = other.refresh(node_id).await;
        }
        self.graph_epoch.with_mut(|e| *e += 1);
        self.set_status(format!("Created {}", node.native_key));
        if !node.native_key.starts_with(".moonkale/") {
            self.record(moonkale_core::EventKind::Add {
                node: node.clone(),
                text: Some(text.to_string()),
            });
        }
        Ok(node)
    }

    /// Apply one op to a source and return the resulting node id, or the
    /// refusal as an error.
    async fn apply_one(
        &self,
        source: &Arc<dyn Source>,
        tx: Transaction,
    ) -> Result<NodeId, SourceError> {
        let applied = source.apply(tx).await?;
        match applied.results.first() {
            Some(moonkale_core::OpResult::Ok { node, .. }) => Ok(*node),
            Some(moonkale_core::OpResult::Refused { error, .. }) => Err(error.clone()),
            None => Err(SourceError::Unsupported("refused".into())),
        }
    }

    /// Tell the other sources (the index) about a node that changed or
    /// vanished, then bump the epochs the panels watch.
    async fn after_fs_change(&mut self, source_id: &SourceId, nodes: &[NodeId]) {
        let others: Vec<Arc<dyn Source>> = self
            .sources
            .peek()
            .iter()
            .filter(|s| &s.descriptor.id != source_id)
            .map(|s| s.source.clone())
            .collect();
        for other in others {
            for n in nodes {
                let _ = other.refresh(*n).await;
            }
        }
        self.graph_epoch.with_mut(|e| *e += 1);
        self.fs_epoch.with_mut(|e| *e += 1);
    }

    /// Create a directory under `parent` (Milestone 7).
    pub async fn create_dir(
        mut self,
        source_id: &SourceId,
        parent: NodeId,
        name: &str,
    ) -> Result<NodeId, SourceError> {
        let source = self.source(source_id).ok_or(SourceError::NotFound)?;
        let id = self
            .apply_one(&source, Transaction::create_dir(parent, name))
            .await?;
        self.after_fs_change(source_id, &[id]).await;
        self.set_status(format!("Created {name}/"));
        if let Ok(r) = source.query(Query::Node(id)).await {
            if let Some(n) = r.nodes.into_iter().next() {
                self.record(moonkale_core::EventKind::Add {
                    node: n,
                    text: None,
                });
            }
        }
        Ok(id)
    }

    /// Rename or move a node to the relative path `to`. Open documents under
    /// the old path are re-keyed to their new ids (text, dirty state and
    /// version kept); the active document follows.
    pub async fn rename_node(mut self, node: &Node, to: &str) -> Result<NodeId, SourceError> {
        let source_id = node.source.clone();
        let source = self.source(&source_id).ok_or(SourceError::NotFound)?;
        // Who links here — asked before the rename (spec 012: links follow).
        let linking = if node.native_key.ends_with(".md") {
            self.wiki_backlinks(node.id).await
        } else {
            Vec::new()
        };
        let new_id = self
            .apply_one(&source, Transaction::rename(node.id, to))
            .await?;
        let to = to.trim_matches('/').to_string();
        // Re-key documents: the node itself, or anything below a directory.
        let from = node.native_key.clone();
        let prefix = format!("{from}/");
        let affected: Vec<(NodeId, Signal<Document>)> = self
            .documents
            .peek()
            .iter()
            .filter(|(_, d)| {
                let k = &d.peek().node.native_key;
                *k == from || k.starts_with(&prefix)
            })
            .cloned()
            .collect();
        let was_active = *self.active.peek();
        for (old_id, mut doc) in affected {
            let new_key = {
                let k = doc.peek().node.native_key.clone();
                if k == from {
                    to.clone()
                } else {
                    format!("{to}/{}", &k[prefix.len()..])
                }
            };
            let fresh = match source
                .query(Query::Node(NodeId::derive(&source_id, &new_key)))
                .await
            {
                Ok(r) => r.nodes.into_iter().next(),
                Err(_) => None,
            };
            let Some(fresh) = fresh else { continue };
            let fresh_id = fresh.id;
            doc.with_mut(|d| {
                d.node = fresh;
            });
            self.documents.with_mut(|v| {
                for (id, _) in v.iter_mut() {
                    if *id == old_id {
                        *id = fresh_id;
                    }
                }
            });
            self.views.with_mut(|v| {
                for n in v.iter_mut() {
                    if n.id == old_id {
                        n.id = fresh_id;
                        n.native_key = new_key.clone();
                    }
                }
            });
            if was_active == Some(old_id) {
                self.active.set(Some(fresh_id));
            }
        }
        self.after_fs_change(&source_id, &[node.id, new_id]).await;
        self.set_status(format!("Renamed {from} → {to}"));
        self.record(moonkale_core::EventKind::Rename {
            from: node.id,
            to: new_id,
            from_key: from.clone(),
            to_key: to.clone(),
        });
        if !linking.is_empty() {
            self.rewrite_wiki_links(linking, &from, &to).await;
        }
        Ok(new_id)
    }

    /// Delete a node (to `.moonkale/trash` on folders); documents under it
    /// are closed without saving.
    pub async fn delete_node(mut self, node: &Node) -> Result<(), SourceError> {
        let source_id = node.source.clone();
        let source = self.source(&source_id).ok_or(SourceError::NotFound)?;
        self.apply_one(&source, Transaction::delete(node.id))
            .await?;
        let prefix = format!("{}/", node.native_key);
        let closing: Vec<NodeId> = self
            .documents
            .peek()
            .iter()
            .filter(|(id, d)| *id == node.id || d.peek().node.native_key.starts_with(&prefix))
            .map(|(id, _)| *id)
            .collect();
        for id in closing {
            self.close_node(id);
        }
        self.after_fs_change(&source_id, &[node.id]).await;
        self.set_status(format!(
            "Deleted {} (kept in .moonkale/trash)",
            node.native_key
        ));
        self.record_event(
            self.user_actor(),
            moonkale_core::EventKind::Remove { node: node.id },
            Some(node.native_key.clone()),
        );
        Ok(())
    }

    /// Literal occurrences of `needle` in a file (open document text if it
    /// is open, else the source's), for a replace preview (Milestone 7).
    pub async fn count_occurrences(&self, node: &Node, needle: &str) -> Result<usize, SourceError> {
        if needle.is_empty() {
            return Ok(0);
        }
        let text = match self.document(node.id) {
            Some(d) => d.peek().text.clone(),
            None => {
                let source = self.source(&node.source).ok_or(SourceError::NotFound)?;
                source.fetch_text(node.id).await?.0
            }
        };
        Ok(text.matches(needle).count())
    }

    /// Replace every literal `needle` in one file. An open document takes
    /// the change as an unsaved edit (the user saves); a closed file is
    /// written through its source with a version check and the index is
    /// refreshed. Returns the number of replacements.
    pub async fn replace_in_file(
        mut self,
        node: &Node,
        needle: &str,
        replacement: &str,
    ) -> Result<usize, SourceError> {
        if needle.is_empty() {
            return Ok(0);
        }
        if let Some(mut doc) = self.document(node.id) {
            let (count, next) = {
                let d = doc.peek();
                (
                    d.text.matches(needle).count(),
                    d.text.replace(needle, replacement),
                )
            };
            if count > 0 {
                doc.with_mut(|d| d.text = next);
            }
            return Ok(count);
        }
        let source = self.source(&node.source).ok_or(SourceError::NotFound)?;
        let (text, version) = source.fetch_text(node.id).await?;
        let count = text.matches(needle).count();
        if count == 0 {
            return Ok(0);
        }
        let next = text.replace(needle, replacement);
        let applied = source
            .apply(Transaction::write_text(
                node.id,
                version,
                TextPatch::whole(next, text.chars().count()),
            ))
            .await?;
        if let Some(e) = applied.first_error() {
            return Err(e.clone());
        }
        let source_id = node.source.clone();
        self.after_fs_change(&source_id, &[node.id]).await;
        Ok(count)
    }

    /// Replace a document's text with what the source has now (after a
    /// conflict, or "revert").
    pub async fn reload(mut self, node: NodeId) -> Result<(), SourceError> {
        let mut doc = self.document(node).ok_or(SourceError::NotFound)?;
        let (source_id, n) = {
            let d = doc.read();
            (d.node.source.clone(), d.node.clone())
        };
        let source = self.source(&source_id).ok_or(SourceError::NotFound)?;
        let (text, version) = source.fetch_text(node).await?;
        doc.set(Document::new(n, text, version));
        self.set_status("Reloaded from disk");
        Ok(())
    }
}

/// One-line description for the session log (no document bodies).
fn summary(msg: &SessionMessage) -> String {
    match msg {
        SessionMessage::Hello { from } => format!("Hello from {from}"),
        SessionMessage::Welcome { from } => format!("Welcome from {from}"),
        SessionMessage::SourceOpened { from, descriptor } => {
            format!("SourceOpened {} from {from}", descriptor.id)
        }
        SessionMessage::DragStarted { from, node, .. } => {
            format!("DragStarted {} from {from}", node.native_key)
        }
        SessionMessage::DragEnded { from } => format!("DragEnded from {from}"),
        SessionMessage::Moved { node, from, to } => format!("Moved {node} {from} → {to}"),
    }
}

/// Where the entity log lives inside a workspace (Milestone 8).
pub const HISTORY_FILE: &str = ".moonkale/history.jsonl";

/// Milliseconds since the Unix epoch, on native and in the browser.
pub fn now_ms() -> u64 {
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Date::now() as u64
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
}

enum BrowserRun {
    Done(Result<String, String>),
    Unavailable,
}

/// Drives `window.moonkale.wasmHost` (packages/js/wasm-host) from Rust:
/// receives the run request, forwards host calls to Rust and back.
const WASM_HOST_JS: &str = r#"
const req = await dioxus.recv();
const host = window.moonkale && window.moonkale.wasmHost;
if (!host || !host.available()) { dioxus.send({ kind: "unavailable" }); return; }
try {
    const reply = await host.run(req.url, req.command, req.args, async (json) => {
        dioxus.send({ kind: "call", json });
        const r = await dioxus.recv();
        return JSON.stringify(r);
    }, (text) => dioxus.send({ kind: "log", text }));
    dioxus.send({ kind: "done", reply });
} catch (e) {
    dioxus.send({ kind: "error", error: String(e && e.message || e) });
}
"#;

/// `&'static str` for a panel id (a small, bounded set: one per static
/// panel), so `Command::ShowPanel` can carry ids only known at runtime.
fn intern_panel_id(id: &str) -> &'static str {
    use std::collections::BTreeSet;
    use std::sync::Mutex;
    static POOL: Mutex<BTreeSet<&'static str>> = Mutex::new(BTreeSet::new());
    let mut pool = POOL.lock().unwrap();
    if let Some(s) = pool.get(id) {
        return s;
    }
    let leaked: &'static str = Box::leak(id.to_string().into_boxed_str());
    pool.insert(leaked);
    leaked
}
