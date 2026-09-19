//! The command registry (Milestone 7): built-in commands and the enabled
//! extensions' contributions, with the effective keybindings (defaults
//! overridden by `settings.keybindings`). Rebuilt when settings change;
//! consumed by the palette, the menus and the global key handlers.

use crate::frame::Extensions_;
use dioxus::prelude::*;
use moonkale_ext_api::{Command, CommandContribution, Extension, Keybinding, Workspace};
use std::rc::Rc;

#[derive(Clone, Debug, PartialEq)]
pub enum Run {
    Builtin(Command),
    /// Index into the extension list.
    Extension(usize),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub id: String,
    pub title: String,
    pub binding: Option<Keybinding>,
    pub run: Run,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Registry {
    pub entries: Vec<Entry>,
}

/// The shell's own commands. Undo/Redo stay unbound here: the editors own
/// those keys (CodeMirror's history) and the menu dispatches them.
fn builtins() -> Vec<(CommandContribution, Command)> {
    let c = CommandContribution::new;
    vec![
        (
            c("workspace.openFolder", "File: Open Folder…").key("Ctrl+O"),
            Command::OpenFolder,
        ),
        (c("file.save", "File: Save").key("Ctrl+S"), Command::Save),
        (
            c("editor.close", "File: Close Editor").key("Ctrl+W"),
            Command::CloseEditor,
        ),
        (
            c("view.settings", "File: Settings…").key("Ctrl+,"),
            Command::Settings,
        ),
        (c("edit.undo", "Edit: Undo"), Command::Undo),
        (c("edit.redo", "Edit: Redo"), Command::Redo),
        (
            c("view.palette", "View: Command Palette…").key("Ctrl+Shift+P"),
            Command::Palette,
        ),
        (
            c("view.quickOpen", "Go to File…").key("Ctrl+P"),
            Command::QuickOpen,
        ),
        (
            c("search.workspace", "Search: Find in Workspace…").key("Ctrl+Shift+F"),
            Command::SearchWorkspace,
        ),
        (
            c("view.newWindow", "View: New Window").key("Ctrl+Shift+N"),
            Command::NewWindow,
        ),
        (
            c("view.newTerminal", "View: New Terminal").key("Ctrl+`"),
            Command::NewTerminal,
        ),
        (
            c("view.resetLayout", "View: Reset Layout"),
            Command::ResetLayout,
        ),
        (
            c("view.panel.explorer", "View: Show Explorer"),
            Command::ShowPanel("explorer"),
        ),
        (
            c("view.panel.search", "View: Show Search"),
            Command::ShowPanel("search"),
        ),
        (
            c("view.panel.graph", "View: Show Graph"),
            Command::ShowPanel("graph"),
        ),
        (
            c("view.panel.terminal", "View: Show Terminal"),
            Command::ShowPanel("terminal"),
        ),
        (
            c("view.panel.agent", "View: Show Agent"),
            Command::ShowPanel("agent"),
        ),
        (c("help.about", "Help: About Moonkale"), Command::About),
    ]
}

impl Registry {
    /// Built-ins first, then every enabled extension's contributions (first
    /// id wins on a clash), with `settings.keybindings` applied last.
    pub fn build(exts: &[Box<dyn Extension>], ws: Workspace) -> Self {
        let settings = ws.settings.read();
        let mut entries: Vec<Entry> = Vec::new();
        let mut push = |c: CommandContribution, run: Run| {
            if entries.iter().any(|e| e.id == c.id) {
                tracing::warn!("commands: duplicate id {} ignored", c.id);
                return;
            }
            let binding = match settings.keybindings.get(&c.id) {
                Some(text) if text.is_empty() => None,
                Some(text) => Keybinding::parse(text).or_else(|| {
                    tracing::warn!("commands: cannot parse keybinding {text:?} for {}", c.id);
                    c.keybinding.as_deref().and_then(Keybinding::parse)
                }),
                None => c.keybinding.as_deref().and_then(Keybinding::parse),
            };
            entries.push(Entry {
                id: c.id,
                title: c.title,
                binding,
                run,
            });
        };
        for (c, cmd) in builtins() {
            push(c, Run::Builtin(cmd));
        }
        for (i, ext) in exts.iter().enumerate() {
            if !settings.extensions.is_enabled(&ext.manifest()) {
                continue;
            }
            for c in ext.commands(ws) {
                push(c, Run::Extension(i));
            }
        }
        Self { entries }
    }

    pub fn find(&self, id: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// The command bound to a `keydown`, if any.
    pub fn resolve_key(
        &self,
        ctrl_or_meta: bool,
        shift: bool,
        alt: bool,
        key: &str,
    ) -> Option<&Entry> {
        self.entries.iter().find(|e| {
            e.binding
                .as_ref()
                .is_some_and(|b| b.matches(ctrl_or_meta, shift, alt, key))
        })
    }

    /// Display text of a command's binding (menus), empty when unbound.
    pub fn shortcut(&self, id: &str) -> String {
        self.find(id)
            .and_then(|e| e.binding.as_ref())
            .map(Keybinding::display)
            .unwrap_or_default()
    }
}

/// The registry as the frame provides it (`use_context`).
pub type CommandRegistry = Signal<Rc<Registry>>;

/// Run a command by id: built-ins dispatch through the workspace, extension
/// commands call the owning extension. `false` when unknown.
pub fn run(id: &str, mut ws: Workspace) -> bool {
    let reg = use_context::<CommandRegistry>();
    let reg = reg.peek().clone();
    let Some(entry) = reg.find(id) else {
        return false;
    };
    match &entry.run {
        Run::Builtin(cmd) => ws.dispatch(*cmd),
        Run::Extension(i) => {
            let exts = use_context::<Extensions_>().0;
            if let Some(ext) = exts.get(*i) {
                ext.run_command(id, ws);
            }
        }
    }
    true
}

/// Handle a key event against the registry. Returns `true` when a command
/// ran (the caller prevents the default).
pub fn handle_key(ws: Workspace, ctrl_or_meta: bool, shift: bool, alt: bool, key: &str) -> bool {
    let reg = use_context::<CommandRegistry>();
    let id = reg
        .peek()
        .resolve_key(ctrl_or_meta, shift, alt, key)
        .map(|e| e.id.clone());
    match id {
        Some(id) => run(&id, ws),
        None => false,
    }
}
