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

mod config;
mod documents;
mod extensions;
mod history;
mod processes;
mod remote;
mod session;
mod settings;
mod sources;

pub use config::*;

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
    pub wasm_extensions: Signal<Vec<moonkale_ext_abi::WasmManifest>>,
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
    /// Which code editor shows a document, when chosen by hand
    /// (Milestone 14): `"codemirror"` | `"native"`.
    pub editor_choice: Signal<std::collections::HashMap<NodeId, &'static str>>,
    /// The caret of the active document: (line, column), 0-based, from
    /// whichever editor shows it (Milestone 14).
    pub cursor: Signal<Option<(u32, u32)>>,
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
    /// Explorer reloads the directories it shows (Milestone 7) — and, since
    /// Milestone 16, after changes made on disk behind Moonkale's back.
    pub fs_epoch: Signal<u64>,
    /// Sources whose changes on disk are followed (Milestone 16): their
    /// `changes_since` answered. Every other source gets a refresh button.
    pub watched: Signal<std::collections::HashSet<SourceId>>,
    /// Sources with a follow loop running (answered or not yet).
    followed: Signal<std::collections::HashSet<SourceId>>,
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
            watched: Signal::new_in_scope(std::collections::HashSet::new(), ScopeId::ROOT),
            followed: Signal::new_in_scope(std::collections::HashSet::new(), ScopeId::ROOT),
            vcs_status: Signal::new_in_scope(std::collections::HashMap::new(), ScopeId::ROOT),
            history: Signal::new_in_scope(moonkale_core::EntityLog::new(), ScopeId::ROOT),
            presence: Signal::new_in_scope(Vec::new(), ScopeId::ROOT),
            presence_link: Signal::new_in_scope(None, ScopeId::ROOT),
            pending_actor: Signal::new_in_scope(std::collections::HashMap::new(), ScopeId::ROOT),
            pending_cause: Signal::new_in_scope(std::collections::HashMap::new(), ScopeId::ROOT),
            cursor_line: Signal::new_in_scope(None, ScopeId::ROOT),
            assets_epoch: Signal::new_in_scope(0, ScopeId::ROOT),
            terminal_cwd: Signal::new_in_scope(None, ScopeId::ROOT),
            editor_choice: Signal::new_in_scope(std::collections::HashMap::new(), ScopeId::ROOT),
            cursor: Signal::new_in_scope(None, ScopeId::ROOT),
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

    /// Focus an element by id once the next frame has rendered (commands
    /// that open a panel and want its input focused). Best effort.
    pub fn focus_element(&self, id: &str) {
        let js = format!(
            "requestAnimationFrame(() => {{ const el = document.getElementById({id:?}); if (el) el.focus(); }});"
        );
        let _ = dioxus::document::eval(&js);
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

    /// Dispatch an application command to whoever handles it.
    pub fn dispatch(&mut self, cmd: Command) {
        let seq = self.commands.peek().0 + 1;
        self.commands.set((seq, Some(cmd)));
    }

    pub fn set_status(&mut self, msg: impl Into<String>) {
        self.status.set(msg.into());
    }

    /// A per-window counter for ids of in-process sources.
    pub fn next_unique(&mut self) -> u64 {
        let n = *self.unique.peek() + 1;
        self.unique.set(n);
        n
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

/// The identifier (letters, digits, `_`) around column `col` of line `line`
/// (both 0-based, `col` in characters), or `None` on whitespace/punctuation.
pub fn word_at(text: &str, line: u32, col: u32) -> Option<String> {
    let l = text.lines().nth(line as usize)?;
    let chars: Vec<char> = l.chars().collect();
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let col = (col as usize).min(chars.len());
    // Prefer the character under the caret, else the one before it.
    let anchor = if col < chars.len() && is_word(chars[col]) {
        col
    } else if col > 0 && is_word(chars[col - 1]) {
        col - 1
    } else {
        return None;
    };
    let mut start = anchor;
    while start > 0 && is_word(chars[start - 1]) {
        start -= 1;
    }
    let mut end = anchor + 1;
    while end < chars.len() && is_word(chars[end]) {
        end += 1;
    }
    Some(chars[start..end].iter().collect())
}

#[cfg(test)]
mod word_tests {
    use super::word_at;

    #[test]
    fn finds_the_identifier_around_the_caret() {
        let t = "fn main() {\n    let total_sum = add(1, 2);\n}";
        assert_eq!(word_at(t, 0, 0).as_deref(), Some("fn"));
        assert_eq!(word_at(t, 0, 2).as_deref(), Some("fn")); // just after the word
        assert_eq!(word_at(t, 0, 3).as_deref(), Some("main"));
        assert_eq!(word_at(t, 1, 12).as_deref(), Some("total_sum"));
        assert_eq!(word_at(t, 1, 24).as_deref(), Some("1"));
        assert_eq!(word_at(t, 0, 10), None); // between `)` and `{`
        assert_eq!(word_at(t, 9, 0), None);
    }
}
