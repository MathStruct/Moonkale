//! What the platform hands the workspace: `WorkspaceConfig`, its callback
//! types, and the small types around them. Split out of `workspace.rs` in
//! Milestone 18 phase 3b; phase 3c groups the callbacks into platform services.

use super::*;

/// A source the workspace has open.
#[derive(Clone)]
pub struct SourceHandle {
    /// What the source says about itself.
    pub descriptor: SourceDescriptor,
    /// The source.
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
    /// The provider settings for embeddings (the server builds the index's embedder from them), if search uses embeddings.
    pub embed: Option<moonkale_llm_types::LlmSettings>,
}
/// Open a folder (or a database file) by path → its sources (the folder, then its index).
pub type OpenFolder = fn(String, OpenOptions) -> OpenFolderFuture;
/// The future an [`AttachSource`] returns: one source, by descriptor.
pub type AttachFuture = Pin<Box<dyn Future<Output = Result<Arc<dyn Source>, SourceError>>>>;

/// A native folder picker: resolves to the chosen path, or `None` if the
/// user cancelled (or no dialog is available). Desktop provides one; web
/// and mobile pass `None` and fall back to typing a path.
pub type PickFolderFuture = Pin<Box<dyn Future<Output = Option<String>>>>;
/// The platform's folder picker → the chosen path.
pub type PickFolder = fn() -> PickFolderFuture;

/// Re-open a source another window already has, from its descriptor:
/// the process registry on desktop, `RemoteSource::from_descriptor` on web.
pub type AttachSource = fn(SourceDescriptor) -> AttachFuture;

/// Compile a Typst document: `(folder root, main path relative to it, text)` →
/// SVG pages, or diagnostics. Desktop compiles in-process, web asks the server.
pub type CompileTypstFuture = Pin<Box<dyn Future<Output = Result<Vec<String>, Vec<String>>>>>;
/// `(root, main file relative to root, its text)` → SVG pages, or diagnostics.
pub type CompileTypst = fn(String, String, String) -> CompileTypstFuture;

/// Builds the LLM provider for this platform (in-process on desktop, the
/// server relay on web). Async because the web build asks the server which
/// model it runs.
pub type LlmProviderFuture =
    Pin<Box<dyn Future<Output = Result<Arc<dyn moonkale_llm_types::Provider>, String>>>>;
/// Built from the resolved LLM settings (provider kind, model, endpoint,
/// secret name); the platform resolves the secret itself.
pub type LlmProvider = fn(moonkale_llm_types::LlmSettings) -> LlmProviderFuture;

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
pub type WasmList = fn(Option<String>) -> SettingsFuture<Vec<moonkale_ext_abi::WasmManifest>>;
/// `(extension id, command, args, granted permissions)` → the command's result.
pub type WasmRun = fn(String, String, serde_json::Value, Vec<String>) -> SettingsFuture<String>;
/// Installed wasm extensions on this platform (Milestone 6).
#[derive(Clone, Copy)]
pub struct WasmExtensions {
    /// Argument: the open folder's path (for `.moonkale/extensions`).
    pub list: WasmList,
    /// Run one command of one module.
    pub run: WasmRun,
}

/// Where the user scope's settings file lives on this platform.
#[derive(Clone, Copy)]
pub struct SettingsStore {
    /// Read it (missing = defaults).
    pub load: fn() -> SettingsFuture<crate::settings::SettingsFile>,
    /// Write it.
    pub save: fn(crate::settings::SettingsFile) -> SettingsFuture<()>,
}

/// "Open this node and put the cursor here" (search hits, trace frames,
/// go-to-definition). Lines and columns are 0-based.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reveal {
    /// The node to open.
    pub node: NodeId,
    /// 0-based line.
    pub line: u32,
    /// 0-based column.
    pub col: u32,
    /// Bumped per request so the same position can be revealed twice.
    pub seq: u64,
}

/// What the platform hands the workspace at startup, grouped by what each
/// part needs from the platform (Milestone 18 phase 3c). Every group except
/// [`FolderAccess`] is all optional and `Default`: a platform fills in what
/// it has (the web client and the phone have no processes of their own).
#[derive(Clone, Copy)]
pub struct WorkspaceConfig {
    /// Opening sources.
    pub folders: FolderAccess,
    /// Programs on this machine (terminal, LSP).
    pub processes: Processes,
    /// Settings, secrets and state stores.
    pub persistence: Persistence,
    /// Servers, SSH remotes, presence, the session bus.
    pub network: Network,
    /// Typst, the language model, wasm extensions.
    pub runtimes: Runtimes,
    /// Platform services an extension defines and the app provides
    /// (Milestone 18 phase 4.2): looked up by type with
    /// [`crate::Workspace::service`]. The git extension's `GitRunner` is
    /// one; `ext-api` knows none of them.
    pub services: Services,
}

/// Values of types the extensions define, provided by the app: a static
/// list, e.g. `&[&GIT]` with `static GIT: GitRunner = GitRunner(run);`.
pub type Services = &'static [&'static (dyn std::any::Any + Sync)];

/// Opening sources.
#[derive(Clone, Copy)]
pub struct FolderAccess {
    /// Open a folder (or a database file) by path.
    pub open: OpenFolder,
    /// Attach a source another window opened, by descriptor.
    pub attach: AttachSource,
    /// The platform's folder picker (`None`: type a path).
    pub pick: Option<PickFolder>,
    /// Reopen the most recent folder when the app starts (desktop).
    pub reopen_last: bool,
    /// Which files and directories open as database sources, and how
    /// (Milestone 18 phase 2): assembled by the app from the driver crates'
    /// `openers()`, so the shell needs no driver of its own.
    pub openers: &'static moonkale_core::Openers,
}

/// Running programs on this machine (`None` everywhere on the web client and
/// the phone, which reach a server's processes through [`Network`]).
#[derive(Clone, Copy, Default)]
pub struct Processes {
    /// Start a terminal (`None`: no terminals).
    pub terminal: Option<moonkale_terminal::SpawnTerminal>,
    /// Run a program with arguments under a PTY (Milestone 15: `claude auth
    /// login` as a terminal tab).
    pub program: Option<SpawnProgram>,
    /// Start a language server (`None`: no LSP features).
    pub lsp: Option<moonkale_lsp::SpawnLsp>,
}

/// Where settings, secrets and this app's own state are kept.
#[derive(Clone, Copy, Default)]
pub struct Persistence {
    /// The user scope's settings file.
    pub settings: Option<SettingsStore>,
    /// Store a secret by name (`None`: secrets come from the environment only).
    pub secrets: Option<SecretStore>,
    /// The store for this machine's own state (ADR-0014): layouts and open
    /// documents per folder today. Desktop and phone: `state.sqlite` in the
    /// config directory ([`local_state`]); web: the browser's storage.
    /// `None`: the old behaviour (everything in the folder's settings file).
    pub state: Option<StateAccess>,
    /// The store on the machine that **hosts the open folder** (phase 5.10):
    /// the folder's entity log, one row per event. A local folder: this
    /// machine's store (the same one as `state`); a folder on a server or an
    /// SSH host, and every folder of the web build: the server's store,
    /// through server functions. `None`: the old behaviour (the log rewritten
    /// into `.moonkale/history.jsonl`).
    pub host: Option<StateAccess>,
    /// Keep the user's settings in `state` (phase 5.18; table `settings`,
    /// key `"user"`), importing what `settings` loads once and writing only
    /// the store after that. Desktop and phone: yes. Web: no — the browser's
    /// `localStorage` item already is that browser's store.
    pub user_settings_in_state: bool,
}

/// Async access to a [`moonkale_state::StateStore`], so a platform can put it
/// in-process, in the browser or behind a server.
/// `StateAccess::get`: `(table, key)` → the value, if any.
pub type StateGet = fn(String, Vec<u8>) -> SettingsFuture<Option<Vec<u8>>>;

/// See [`Persistence::state`]: three functions over a store, wherever it is.
#[derive(Clone, Copy)]
pub struct StateAccess {
    /// Read one key.
    pub get: StateGet,
    /// `(table, prefix)` → the entries under it, in key order.
    pub scan: fn(String, Vec<u8>) -> SettingsFuture<moonkale_state::Entries>,
    /// Apply a batch, all or nothing.
    pub write: fn(moonkale_state::Batch) -> SettingsFuture<()>,
}

static LOCAL_STATE: std::sync::OnceLock<Arc<dyn moonkale_state::StateStore>> =
    std::sync::OnceLock::new();

/// A [`StateAccess`] over a store in this process (desktop, phone). The first
/// store given wins; later calls reuse it.
pub fn local_state(backend: Arc<dyn moonkale_state::StateStore>) -> StateAccess {
    let _ = LOCAL_STATE.set(backend);
    installed_local_state()
}

/// The [`StateAccess`] of the store [`local_state`] installed (every call
/// fails with "no state store" before that) — for wiring that routes between
/// this process's store and a server's.
pub fn installed_local_state() -> StateAccess {
    fn store() -> Result<&'static Arc<dyn moonkale_state::StateStore>, String> {
        LOCAL_STATE
            .get()
            .ok_or_else(|| "no state store".to_string())
    }
    StateAccess {
        get: |t, k| Box::pin(async move { store()?.get(&t, &k).map_err(|e| e.to_string()) }),
        scan: |t, p| Box::pin(async move { store()?.scan(&t, &p).map_err(|e| e.to_string()) }),
        write: |b| Box::pin(async move { store()?.write(b).map_err(|e| e.to_string()) }),
    }
}

/// Other machines and other windows.
#[derive(Clone, Copy, Default)]
pub struct Network {
    /// *Connect to Server…* (Milestone 12): make this app a client of a
    /// Moonkale server by URL + token (desktop and mobile).
    pub server: Option<ServerClient>,
    /// Remote folders over SSH (Milestone 11; desktop only).
    pub remote: Option<crate::remote::RemoteHosts>,
    /// Presence hub (Milestone 8); `None`: this window is alone.
    pub presence: Option<crate::presence::JoinPresence>,
    /// Agent sessions that live on the server (Milestone 12): the web
    /// client always, the desktop while it is a server's client.
    pub agent_sessions: Option<AgentSessions>,
}

/// In-process engines.
#[derive(Clone, Copy, Default)]
pub struct Runtimes {
    /// The language-model provider for these settings (the Agent; embeddings).
    pub llm: Option<LlmProvider>,
    /// wasm extensions (`None`: none on this platform).
    pub wasm: Option<WasmExtensions>,
    /// Where the browser runtime fetches a wasm extension's bytes (by id);
    /// `Some` enables running modules in the page (Milestone 8, web).
    pub wasm_module_url: Option<fn(String) -> String>,
    /// Typst compiler (`None`: no preview).
    pub typst: Option<CompileTypst>,
}

/// `(program, args, cols, rows)` → a terminal backend running it.
pub type SpawnProgram = fn(String, Vec<String>, u16, u16) -> moonkale_terminal::SpawnTerminalFuture;

/// How a native app becomes a server's client (`api::client`).
#[derive(Clone, Copy)]
pub struct ServerClient {
    /// `(url, token)`; the sources then come from that server.
    pub connect: fn(String, Option<String>) -> Result<(), String>,
    /// Drop the connection; local sources again.
    pub disconnect: fn(),
    /// The connected server's label, if any.
    pub active: fn() -> Option<String>,
}

/// How a client reaches the server's agent sessions (`api::agent_sessions`).
#[derive(Clone, Copy)]
pub struct AgentSessions {
    /// Are the sources a server's right now?
    pub available: fn() -> bool,
    /// A folder's sessions on the server.
    pub list: fn(String) -> SettingsFuture<Vec<moonkale_llm_types::sessions::SessionSummary>>,
    /// `(session or None, folder, text, settings)` → session id.
    pub send: fn(
        Option<String>,
        String,
        String,
        moonkale_llm_types::sessions::TurnSettings,
    ) -> SettingsFuture<String>,
    /// `(session, since)`.
    pub events: fn(String, usize) -> SettingsFuture<moonkale_llm_types::sessions::SessionState>,
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
    /// The source to query.
    pub source: SourceId,
    /// Its text dialect (`sql`, `cypher`, …).
    pub dialect: String,
    /// The query.
    pub text: String,
}

/// A document being dragged out of another window of this session.
#[derive(Clone, PartialEq)]
pub struct ForeignDrag {
    /// The window the drag started in.
    pub from: WindowId,
    /// The document's node.
    pub node: Node,
    /// Its source, so this window can attach it.
    pub source: SourceDescriptor,
    /// `true` while the mouse button is still down in the origin window (a
    /// real HTML5 drop can land here); `false` after the drag ended without
    /// a drop — the offer stays as a banner until accepted or dismissed, so
    /// platforms whose OS drag never crosses windows still get the move.
    pub live: bool,
}

/// Application-level commands: what menus, keybindings and (later) the
/// palette and LLM tools dispatch. Consumers watch `Workspace::shell.commands`
/// and act on the commands that concern them (the active editor handles
/// `Undo`; the shell handles `ResetLayout`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    /// Open a folder (the Explorer's path field or the picker).
    OpenFolder,
    /// Close the first folder source (spec 015); the Explorer's context
    /// menu closes a specific one through `Workspace::close_source`.
    CloseFolder,
    /// Save the active document.
    Save,
    /// Close the active document.
    CloseEditor,
    /// Undo in the active editor.
    Undo,
    /// Redo in the active editor.
    Redo,
    /// Back to the default panel layout.
    ResetLayout,
    /// Open another window of this session.
    NewWindow,
    /// Open a terminal; `Workspace::terminal_cwd` may carry a directory.
    /// The frame turns it into `NewTerminalIn` (Milestone 12).
    NewTerminal,
    /// Open a terminal in a named implementation: `"xterm"` (the JS
    /// panel) or `"native"` (the Rust panel).
    NewTerminalIn(&'static str),
    /// Show the About dialog.
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
    /// Show/hide the bottom panel (Ctrl+J).
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
    /// Find in the document.
    Find,
    /// Find and replace.
    Replace,
    /// Rename the symbol (LSP).
    Rename,
    /// Code actions at the cursor (LSP).
    CodeActions,
    /// Go to definition (LSP).
    Definition,
    /// Find references (LSP).
    References,
    /// Comment or uncomment the selection.
    ToggleComment,
    /// Fold every region.
    FoldAll,
    /// Unfold every region.
    UnfoldAll,
}
