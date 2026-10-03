//! The command registry (Milestone 7): built-in commands and the enabled
//! extensions' contributions, with the effective keybindings (defaults
//! overridden by `settings.keybindings`). Rebuilt when settings change;
//! consumed by the palette, the menus and the global key handlers.

use crate::frame::Extensions_;
use crate::L;
use dioxus::prelude::*;
use moonkale_ext_api::t;
use moonkale_ext_api::{
    Command, CommandContribution, EditorAction, Extension, Keybinding, Workspace,
};
use std::rc::Rc;

#[derive(Clone, Debug, PartialEq)]
pub enum Run {
    Builtin(Command),
    /// Index into the extension list.
    Extension(usize),
    /// Show (reopening if closed, spec 011) a static panel by id.
    ShowPanel(String),
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
fn builtins(ws: Workspace) -> Vec<(CommandContribution, Command)> {
    let c = |id: &str, title: String| CommandContribution::new(id, title);
    vec![
        (
            c(
                "workspace.openFolder",
                t!(ws, L, "cmd-workspace-openFolder"),
            )
            .key("Ctrl+O"),
            Command::OpenFolder,
        ),
        (
            c(
                "workspace.closeFolder",
                t!(ws, L, "cmd-workspace-closeFolder"),
            ),
            Command::CloseFolder,
        ),
        (
            c("remote.open", t!(ws, L, "cmd-remote-open")),
            Command::OpenRemote,
        ),
        (
            c("remote.close", t!(ws, L, "cmd-remote-close")),
            Command::CloseRemote,
        ),
        (
            c("server.connect", t!(ws, L, "cmd-server-connect")),
            Command::ConnectServer,
        ),
        (
            c("server.disconnect", t!(ws, L, "cmd-server-disconnect")),
            Command::DisconnectServer,
        ),
        (
            c("file.new", t!(ws, L, "cmd-file-new")).key("Ctrl+N"),
            Command::NewFile("untitled.md", "# Untitled\n\n"),
        ),
        (
            c("file.save", t!(ws, L, "cmd-file-save")).key("Ctrl+S"),
            Command::Save,
        ),
        (
            c("file.saveAll", t!(ws, L, "cmd-file-saveAll")).key("Ctrl+Alt+S"),
            Command::SaveAll,
        ),
        (
            c("editor.closeAll", t!(ws, L, "cmd-editor-closeAll")).key("Ctrl+Shift+W"),
            Command::CloseAllEditors,
        ),
        (
            c("view.toggleSide", t!(ws, L, "cmd-view-toggleSide")).key("Ctrl+B"),
            Command::ToggleSide,
        ),
        (
            c("view.toggleBottom", t!(ws, L, "cmd-view-toggleBottom")).key("Ctrl+J"),
            Command::ToggleBottom,
        ),
        (
            c("editor.find", t!(ws, L, "cmd-editor-find")),
            Command::Editor(EditorAction::Find),
        ),
        (
            c("editor.replace", t!(ws, L, "cmd-editor-replace")),
            Command::Editor(EditorAction::Replace),
        ),
        (
            c("editor.rename", t!(ws, L, "cmd-editor-rename")),
            Command::Editor(EditorAction::Rename),
        ),
        (
            c("editor.codeActions", t!(ws, L, "cmd-editor-codeActions")),
            Command::Editor(EditorAction::CodeActions),
        ),
        (
            c("editor.definition", t!(ws, L, "cmd-editor-definition")),
            Command::Editor(EditorAction::Definition),
        ),
        (
            c("editor.references", t!(ws, L, "cmd-editor-references")),
            Command::Editor(EditorAction::References),
        ),
        (
            c(
                "editor.toggleComment",
                t!(ws, L, "cmd-editor-toggleComment"),
            ),
            Command::Editor(EditorAction::ToggleComment),
        ),
        (
            c("editor.foldAll", t!(ws, L, "cmd-editor-foldAll")),
            Command::Editor(EditorAction::FoldAll),
        ),
        (
            c("editor.unfoldAll", t!(ws, L, "cmd-editor-unfoldAll")),
            Command::Editor(EditorAction::UnfoldAll),
        ),
        (
            c("editor.close", t!(ws, L, "cmd-editor-close")).key("Ctrl+W"),
            Command::CloseEditor,
        ),
        (
            c("view.settings", t!(ws, L, "cmd-view-settings")).key("Ctrl+,"),
            Command::Settings,
        ),
        (c("edit.undo", t!(ws, L, "cmd-edit-undo")), Command::Undo),
        (c("edit.redo", t!(ws, L, "cmd-edit-redo")), Command::Redo),
        (
            c("view.palette", t!(ws, L, "cmd-view-palette")).key("Ctrl+Shift+P"),
            Command::Palette,
        ),
        (
            c("view.quickOpen", t!(ws, L, "cmd-view-quickOpen")).key("Ctrl+P"),
            Command::QuickOpen,
        ),
        (
            c("search.workspace", t!(ws, L, "cmd-search-workspace")).key("Ctrl+Shift+F"),
            Command::SearchWorkspace,
        ),
        (
            c("view.newWindow", t!(ws, L, "cmd-view-newWindow")).key("Ctrl+Shift+N"),
            Command::NewWindow,
        ),
        (
            c("view.newTerminal", t!(ws, L, "cmd-view-newTerminal")).key("Ctrl+`"),
            Command::NewTerminal,
        ),
        (
            c("view.resetLayout", t!(ws, L, "cmd-view-resetLayout")),
            Command::ResetLayout,
        ),
        (
            c("view.panel.explorer", t!(ws, L, "cmd-view-panel-explorer")),
            Command::ShowPanel("explorer"),
        ),
        (
            c("view.panel.search", t!(ws, L, "cmd-view-panel-search")),
            Command::ShowPanel("search"),
        ),
        (
            c("view.panel.graph", t!(ws, L, "cmd-view-panel-graph")),
            Command::ShowPanel("graph"),
        ),
        (
            c("view.panel.terminal", t!(ws, L, "cmd-view-panel-terminal")),
            Command::ShowPanel("terminal"),
        ),
        (
            c("view.panel.agent", t!(ws, L, "cmd-view-panel-agent")),
            Command::ShowPanel("agent"),
        ),
        (c("help.about", t!(ws, L, "cmd-help-about")), Command::About),
    ]
}

impl Registry {
    /// Built-ins first, then every enabled extension's contributions (first
    /// id wins on a clash), with `settings.keybindings` applied last.
    pub fn build(exts: &[Box<dyn Extension>], ws: Workspace) -> Self {
        let settings = ws.settings.resolved.read();
        let mut entries: Vec<Entry> = Vec::new();
        let mut push = |c: CommandContribution, run: Run| {
            if entries.iter().any(|e| e.id == c.id) {
                if !c.id.starts_with("view.panel.") {
                    tracing::warn!("commands: duplicate id {} ignored", c.id);
                }
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
        for (c, cmd) in builtins(ws) {
            push(c, Run::Builtin(cmd));
        }
        for (i, ext) in exts.iter().enumerate() {
            if !settings.extensions.is_enabled(&ext.manifest()) {
                continue;
            }
            for c in ext.commands(ws) {
                push(c, Run::Extension(i));
            }
            // `View: Show <panel>` for every static panel (spec 011), so a
            // closed one can always be brought back from the palette.
            for p in ext.panels(ws).into_iter().filter(|p| p.node.is_none()) {
                // (`push` itself ignores duplicates, so the five built-in
                // `view.panel.*` entries win.)
                let id = format!("view.panel.{}", p.id);
                push(
                    CommandContribution::new(
                        id,
                        t!(ws, L, "cmd-view-show-panel", panel = p.title.clone()),
                    ),
                    Run::ShowPanel(p.id.clone()),
                );
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
        Run::ShowPanel(id) => ws.show_panel(id),
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
