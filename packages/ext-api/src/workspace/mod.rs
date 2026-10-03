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
pub use documents::DocsState;
pub use extensions::ContribState;
pub use history::{event_key, EventRecord, HistoryState};
pub use processes::ProcessesState;
pub use remote::RemoteLinks;
pub use session::SessionState;
pub use settings::SettingsState;
pub use sources::SourcesState;

/// The workspace: every open source, document and setting, and the platform's services. `Copy` (signals inside); handed to every extension call. Its state is grouped by area; the methods are on this facade.
#[derive(Clone, Copy)]
pub struct Workspace {
    /// Open sources and their change epochs.
    pub sources: SourcesState,
    /// Open documents, views, the active one, the cursor.
    pub docs: DocsState,
    /// The entity log.
    pub history: HistoryState,
    /// The resolved settings and their scopes.
    pub settings: SettingsState,
    /// Status line, commands, closed panels.
    pub shell: ShellState,
    /// What extensions contributed (flow libraries, wasm modules, file marks).
    pub contrib: ContribState,
    /// This window, other windows, presence.
    pub session: SessionState,
    /// SSH remotes and the server connection.
    pub remote: RemoteLinks,
    /// LSP status and terminal requests.
    pub processes: ProcessesState,
    config: WorkspaceConfig,
}

impl PartialEq for Workspace {
    fn eq(&self, other: &Self) -> bool {
        self.sources.open == other.sources.open
            && self.docs.open == other.docs.open
            && self.docs.active == other.docs.active
    }
}

impl Workspace {
    /// Create the workspace. Call once, in the shell's `use_hook`, so the
    /// signals live for the app's lifetime.
    pub fn new(config: WorkspaceConfig) -> Self {
        Self {
            sources: SourcesState::new(),
            docs: DocsState::new(),
            history: HistoryState::new(),
            settings: SettingsState::new(),
            shell: ShellState::new(),
            contrib: ContribState::new(),
            session: SessionState::new(),
            remote: RemoteLinks::new(),
            processes: ProcessesState::new(),
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
        self.shell.closed_panels.with_mut(|c| {
            c.remove(id);
        });
        let id: &'static str = intern_panel_id(id);
        self.dispatch(Command::ShowPanel(id));
    }

    /// Dispatch an application command to whoever handles it.
    pub fn dispatch(&mut self, cmd: Command) {
        let seq = self.shell.commands.peek().0 + 1;
        self.shell.commands.set((seq, Some(cmd)));
    }

    /// Show `msg` in the status bar.
    pub fn set_status(&mut self, msg: impl Into<String>) {
        self.shell.status.set(msg.into());
    }

    /// A per-window counter for ids of in-process sources.
    pub fn next_unique(&mut self) -> u64 {
        let n = *self.shell.unique.peek() + 1;
        self.shell.unique.set(n);
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

/// The shell's own state: status line, commands, closed panels. (Milestone 18 phase 3c: the workspace's state, grouped by area.)
#[derive(Clone, Copy)]
pub struct ShellState {
    /// One line for the status bar.
    pub status: Signal<String>,
    /// The last dispatched command with a sequence number, so consumers can
    /// tell a new dispatch of the same command from a re-render.
    pub commands: Signal<(u64, Option<Command>)>,
    /// Static panels (Graph, Agent, Explorer, …) the user closed (spec 011);
    /// the shell contributes nothing for them until `show_panel` is called.
    pub closed_panels: Signal<std::collections::BTreeSet<String>>,
    /// Panels hidden by Ctrl+B / Ctrl+J, to bring back on the next toggle.
    pub hidden_tiles: Signal<std::collections::BTreeMap<String, Vec<String>>>,
    /// Counter for ids of in-process sources (traces).
    pub unique: Signal<u64>,
}

impl ShellState {
    pub(super) fn new() -> Self {
        Self {
            status: Signal::new_in_scope("Ready".into(), ScopeId::ROOT),
            commands: Signal::new_in_scope((0, None), ScopeId::ROOT),
            closed_panels: Signal::new_in_scope(Default::default(), ScopeId::ROOT),
            hidden_tiles: Signal::new_in_scope(Default::default(), ScopeId::ROOT),
            unique: Signal::new_in_scope(0, ScopeId::ROOT),
        }
    }
}
