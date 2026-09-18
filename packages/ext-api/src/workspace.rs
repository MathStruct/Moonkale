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
    Save,
    CloseEditor,
    Undo,
    Redo,
    ResetLayout,
    NewWindow,
    /// Open a terminal; `Workspace::terminal_cwd` may carry a directory.
    NewTerminal,
    About,
    /// Bring a panel's tab to the front (the shell owns the layout).
    ShowPanel(&'static str),
    /// Open the n-th entry of `settings.recent_folders`.
    OpenRecent(usize),
    /// Open the Settings panel (Ctrl+,).
    Settings,
    /// Create `<name>` (a file name; a numeric suffix is added if it exists)
    /// with `template` in the open folder's root and open it.
    NewFile(&'static str, &'static str),
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
    /// Persisted scopes and the resolved value (see `settings.rs`).
    pub settings_user: Signal<crate::settings::SettingsFile>,
    pub settings_workspace: Signal<crate::settings::SettingsFile>,
    pub settings: Signal<crate::settings::Settings>,
    /// The folder whose `.moonkale/settings.json` is loaded, if any.
    pub settings_folder: Signal<Option<SourceId>>,
    /// Directory for the next `NewTerminal` (set by "New terminal here").
    pub terminal_cwd: Signal<Option<String>>,
    /// Bumped whenever derived data may have changed (after a save was
    /// refreshed into the index); graph/backlink panels re-query on it.
    pub graph_epoch: Signal<u64>,
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
            terminal_cwd: Signal::new_in_scope(None, ScopeId::ROOT),
            lsp_status: Signal::new_in_scope(None, ScopeId::ROOT),
            graph_request: Signal::new_in_scope(None, ScopeId::ROOT),
            reveal: Signal::new_in_scope(None, ScopeId::ROOT),
            unique: Signal::new_in_scope(0, ScopeId::ROOT),
            flow_libraries: Signal::new_in_scope(Vec::new(), ScopeId::ROOT),
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
        // Desktop: come back to where you were.
        if self.config.reopen_last_folder && self.sources.peek().is_empty() {
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
        for source in sources {
            let descriptor = source.descriptor();
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
        if first.family == moonkale_core::SourceFamily::Folder {
            // Workspace settings + remember the folder.
            self.load_workspace_settings(&first.id).await;
            let path = first
                .id
                .as_str()
                .strip_prefix("folder:")
                .unwrap_or(first.id.as_str())
                .to_string();
            self.update_user_settings(|f| f.push_recent(&path)).await;
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
        // Nodes without a text body (tables) open as views, not documents.
        if matches!(node.kind, NodeKind::Table) {
            if !self.views.peek().iter().any(|n| n.id == node.id) {
                self.views.with_mut(|v| v.push(node.clone()));
            }
            self.active.set(Some(node.id));
            self.set_status(format!("Opened table {}", node.label));
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
        let (source_id, version, patch, key) = {
            let d = doc.read();
            (
                d.node.source.clone(),
                d.version,
                d.patch(),
                d.node.native_key.clone(),
            )
        };
        let source = self.source(&source_id).ok_or(SourceError::NotFound)?;
        let applied = source
            .apply(Transaction::write_text(node, version, patch))
            .await?;
        match applied.version_of(node) {
            Some(v) => {
                doc.with_mut(|d| d.mark_saved(v));
                self.set_status(format!("Saved {key}"));
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
        Ok(node)
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
