//! `ExplorerExtension` — open a folder, browse it, open files.
//!
//! Tree state (which directories are expanded, their loaded children) lives
//! in signals created at `ScopeId::ROOT` inside the extension, so it
//! survives the panel being docked elsewhere. Children are loaded lazily on
//! first expand with `Query::Children`.

use dioxus::prelude::*;
use moonkale_core::{ContentRef, Node, NodeId, NodeKind, Query, SourceError};
use moonkale_ext_api::prelude::*;
use moonkale_ext_api::Command;
use std::collections::{HashMap, HashSet};

pub(crate) const EXPLORER_CSS: Asset = asset!("/assets/styling/explorer.css");

#[derive(Clone, Copy)]
pub struct TreeState {
    expanded: Signal<HashSet<NodeId>>,
    children: Signal<HashMap<NodeId, Vec<Node>>>,
    error: Signal<Option<String>>,
    /// Milestone 7: the open context menu, the inline edit, the pending
    /// delete confirmation and the row being dragged.
    menu: Signal<Option<Menu>>,
    edit: Signal<Option<Edit>>,
    confirm: Signal<Option<Node>>,
    dragging: Signal<Option<Node>>,
}

/// A right-click menu for one node (or a source root).
#[derive(Clone, PartialEq)]
struct Menu {
    node: Node,
    is_dir: bool,
    x: f64,
    y: f64,
}

/// An inline text field in the tree: a new name under a directory, or a
/// rename of an existing node.
#[derive(Clone, PartialEq)]
enum Edit {
    NewFile { parent: Node },
    NewDir { parent: Node },
    Rename { node: Node },
}

impl Edit {
    fn parent_id(&self) -> NodeId {
        match self {
            Edit::NewFile { parent } | Edit::NewDir { parent } => parent.id,
            Edit::Rename { node } => node.id,
        }
    }
}

impl PartialEq for TreeState {
    fn eq(&self, other: &Self) -> bool {
        self.expanded == other.expanded && self.children == other.children
    }
}

pub struct ExplorerExtension {
    state: TreeState,
}

impl Default for ExplorerExtension {
    fn default() -> Self {
        Self::new()
    }
}

impl ExplorerExtension {
    pub fn new() -> Self {
        Self {
            state: TreeState {
                expanded: Signal::new_in_scope(HashSet::new(), ScopeId::ROOT),
                children: Signal::new_in_scope(HashMap::new(), ScopeId::ROOT),
                error: Signal::new_in_scope(None, ScopeId::ROOT),
                menu: Signal::new_in_scope(None, ScopeId::ROOT),
                edit: Signal::new_in_scope(None, ScopeId::ROOT),
                confirm: Signal::new_in_scope(None, ScopeId::ROOT),
                dragging: Signal::new_in_scope(None, ScopeId::ROOT),
            },
        }
    }
}

impl Extension for ExplorerExtension {
    fn manifest(&self) -> Manifest {
        Manifest::core(
            "dev.moonkale.explorer",
            "Explorer",
            "Folders, files and databases.",
        )
    }

    fn panels(&self, _ws: Workspace) -> Vec<PanelContribution> {
        vec![PanelContribution {
            id: "explorer".into(),
            title: "Explorer".into(),
            home: PanelHome::Side,
            closable: false,
            dirty: false,
            node: None,
        }]
    }

    fn render(&self, _panel_id: &str, ws: Workspace) -> Element {
        rsx! { ExplorerPanel { ws, state: self.state } }
    }
}

async fn toggle(ws: Workspace, mut state: TreeState, node: Node) {
    let id = node.id;
    let was_expanded = state.expanded.read().contains(&id);
    if was_expanded {
        state.expanded.with_mut(|e| {
            e.remove(&id);
        });
        return;
    }
    if !state.children.read().contains_key(&id) {
        match ws.query(&node.source, Query::Children(id)).await {
            Ok(res) => state.children.with_mut(|c| {
                c.insert(id, res.nodes);
            }),
            Err(e) => {
                state.error.set(Some(format!("{}: {e}", node.native_key)));
                return;
            }
        }
    }
    state.expanded.with_mut(|e| {
        e.insert(id);
    });
}

#[component]
fn ExplorerPanel(ws: Workspace, state: TreeState) -> Element {
    let mut path = use_signal(String::new);
    let mut opening = use_signal(|| false);
    let mut state = state;

    let open = move |_| async move {
        opening.set(true);
        state.error.set(None);
        let p = path.peek().clone();
        match ws.open_folder(p).await {
            Ok(desc) => {
                // Expand the root immediately so the tree isn't empty.
                if let Ok(res) = ws.query(&desc.id, Query::Children(desc.root)).await {
                    state.children.with_mut(|c| {
                        c.insert(desc.root, res.nodes);
                    });
                    state.expanded.with_mut(|e| {
                        e.insert(desc.root);
                    });
                }
            }
            Err(e) => state.error.set(Some(e.to_string())),
        }
        opening.set(false);
    };

    // Only sources with a browsable tree; the index shows up in the graph, not here.
    let sources: Vec<_> = ws
        .sources
        .read()
        .iter()
        .filter(|s| s.descriptor.family != moonkale_core::SourceFamily::Index)
        .cloned()
        .collect();
    let has_dialog = ws.has_folder_dialog();

    // Sources that arrived from elsewhere (another window of the session, a
    // reconnect) have no tree yet: load and expand their root once.
    use_effect(move || {
        let pending: Vec<_> = ws
            .sources
            .read()
            .iter()
            .filter(|s| !state.children.peek().contains_key(&s.descriptor.root))
            .map(|s| (s.descriptor.id.clone(), s.descriptor.root))
            .collect();
        for (id, root) in pending {
            spawn(async move {
                if let Ok(res) = ws.query(&id, Query::Children(root)).await {
                    state.children.with_mut(|c| {
                        c.insert(root, res.nodes);
                    });
                    state.expanded.with_mut(|e| {
                        e.insert(root);
                    });
                }
            });
        }
    });

    // After a file operation, reload every directory whose children are
    // shown (Milestone 7).
    let mut seen_epoch = use_signal(|| 0u64);
    use_effect(move || {
        let epoch = *ws.fs_epoch.read();
        if epoch == *seen_epoch.peek() {
            return;
        }
        seen_epoch.set(epoch);
        // Loaded directories, plus expanded ones that were never loaded
        // (New File… on a collapsed directory expands it).
        let mut loaded: Vec<NodeId> = state.children.peek().keys().copied().collect();
        for id in state.expanded.peek().iter() {
            if !loaded.contains(id) {
                loaded.push(*id);
            }
        }
        let sources: Vec<_> = ws
            .sources
            .peek()
            .iter()
            .map(|s| (s.descriptor.id.clone(), s.descriptor.root))
            .collect();
        spawn(async move {
            for dir in loaded {
                // Find the source owning this directory by asking each one.
                for (sid, _) in &sources {
                    match ws.query(sid, Query::Children(dir)).await {
                        Ok(res) => {
                            state.children.with_mut(|c| {
                                c.insert(dir, res.nodes);
                            });
                            break;
                        }
                        Err(SourceError::NotFound) => {
                            state.children.with_mut(|c| {
                                c.remove(&dir);
                            });
                            state.expanded.with_mut(|e| {
                                e.remove(&dir);
                            });
                            break;
                        }
                        Err(_) => continue,
                    }
                }
            }
        });
    });

    let menu = state.menu.read().clone();
    let confirm = state.confirm.read().clone();

    rsx! {
        document::Stylesheet { href: EXPLORER_CSS }
        div { class: "mk-explorer",
            onclick: move |_| { if state.menu.peek().is_some() { state.menu.set(None); } },
            if let Some(m) = menu {
                ContextMenu { ws, state, menu: m }
            }
            if let Some(n) = confirm {
                div { class: "mk-explorer-confirm", role: "alertdialog",
                    span { "Delete " b { "{n.native_key}" } "? (kept in .moonkale/trash)" }
                    button { class: "mk-btn mk-btn-danger", onclick: move |_| {
                        let n = n.clone();
                        state.confirm.set(None);
                        spawn(async move {
                            if let Err(e) = ws.delete_node(&n).await { state.error.set(Some(e.to_string())); }
                        });
                    }, "Delete" }
                    button { class: "mk-btn", onclick: move |_| state.confirm.set(None), "Cancel" }
                }
            }
            if has_dialog {
                div { class: "mk-explorer-open",
                    button { class: "mk-btn mk-btn-wide", r#type: "button", onclick: move |_| ws.dispatch(Command::OpenFolder), "Open Folder…" }
                }
            }
            form { class: "mk-explorer-open",
                class: if has_dialog { "mk-explorer-open-secondary" },
                onsubmit: move |e| { e.prevent_default(); spawn(open(())); },
                input {
                    class: "mk-input",
                    // No cfg!() here: fullstack renders this on the server first and
                    // hydration keeps the server's markup, so the text must be the same
                    // on every platform.
                    placeholder: "folder path (blank = default root)",
                    value: "{path}",
                    oninput: move |e| path.set(e.value()),
                }
                button { class: "mk-btn", r#type: "submit", disabled: opening(), if opening() { "Opening…" } else { "Open" } }
            }
            if let Some(err) = state.error.read().clone() {
                div { class: "mk-explorer-error", "{err}" }
            }
            if sources.is_empty() {
                p { class: "mk-muted", "Open a folder to browse its files." }
            }
            for s in sources {
                div { class: "mk-explorer-source",
                    div { class: "mk-explorer-source-name",
                        oncontextmenu: {
                            let root_id = s.descriptor.root;
                            let sid = s.descriptor.id.clone();
                            let name = s.descriptor.display_name.clone();
                            move |e| {
                                e.prevent_default();
                                let root = Node { id: root_id, source: sid.clone(), kind: NodeKind::Directory, label: name.clone(), native_key: String::new(), content: None, version: Default::default() };
                                let c = e.client_coordinates();
                                state.menu.set(Some(Menu { node: root, is_dir: true, x: c.x, y: c.y }));
                            }
                        },
                        "{s.descriptor.display_name}"
                    }
                    if let Some(edit) = state.edit.read().clone().filter(|e| e.parent_id() == s.descriptor.root && !matches!(e, Edit::Rename { .. })) {
                        InlineEdit { ws, state, edit, depth: 0 }
                    }
                    TreeLevel { ws, state, parent: s.descriptor.root, depth: 0 }
                }
            }
        }
    }
}

#[component]
fn TreeLevel(ws: Workspace, state: TreeState, parent: NodeId, depth: usize) -> Element {
    let children = state
        .children
        .read()
        .get(&parent)
        .cloned()
        .unwrap_or_default();
    let expanded = state.expanded.read().clone();
    // Version-control decorations (Milestone 7): a letter class per changed
    // file, a subtle mark on directories with changes below them.
    let vcs = ws.vcs_status.read().clone();
    let _ = ws.presence.read();
    let others = ws.others();
    rsx! {
        ul { class: "mk-tree", style: "--depth: {depth}",
            for node in children {
                {
                    let vcs_class = match vcs.get(&node.native_key) {
                        Some((i, w)) => {
                            let c = if *i == '?' { '?' } else if *w != '.' { *w } else { *i };
                            match c { 'M' => "mk-vcs-modified", 'A' | '?' => "mk-vcs-added", 'D' => "mk-vcs-deleted", 'R' | 'C' => "mk-vcs-renamed", 'U' => "mk-vcs-conflict", _ => "" }
                        }
                        None if node.kind == NodeKind::Directory => {
                            let prefix = format!("{}/", node.native_key);
                            if vcs.keys().any(|k| k.starts_with(&prefix)) { "mk-vcs-dir" } else { "" }
                        }
                        None => "",
                    };
                    // Database paths: a SQLite file, or a Ladybug database (a
                    // directory or a file named *.lbug / *.kuzu).
                    let is_db_path = (node.kind == NodeKind::File && moonkale_sources_sql::is_sqlite_path(&node.native_key))
                        || (matches!(node.kind, NodeKind::File | NodeKind::Directory) && moonkale_sources_graph::is_ladybug_path(&node.native_key));
                    let is_dir = node.kind == NodeKind::Directory && !is_db_path;
                    let open = expanded.contains(&node.id);
                    let is_text = matches!(node.content, Some(ContentRef::Text { .. }));
                    let is_db = node.kind == NodeKind::Table || is_db_path;
                    let n = node.clone();
                    let mut ws2 = ws;
                    let renaming = matches!(state.edit.read().as_ref(), Some(Edit::Rename { node: r }) if r.id == node.id);
                    let new_below = state.edit.read().clone().filter(|e| is_dir && !matches!(e, Edit::Rename { .. }) && e.parent_id() == node.id);
                    let drop_target = is_dir && state.dragging.read().as_ref().is_some_and(|d| d.id != node.id);
                    let (n_menu, n_drag, n_drop) = (node.clone(), node.clone(), node.clone());
                    rsx! {
                        li { key: "{node.id}",
                            div {
                                class: if is_dir { "mk-tree-row mk-tree-dir" } else { "mk-tree-row mk-tree-file" },
                                class: if !is_dir && !is_text && !is_db { "mk-tree-binary" },
                                class: if is_db { "mk-tree-db" },
                                class: if drop_target { "mk-tree-droppable" },
                                class: if !vcs_class.is_empty() { "{vcs_class}" },
                                title: "{node.native_key}",
                                draggable: !is_db,
                                oncontextmenu: move |e| {
                                    e.prevent_default();
                                    e.stop_propagation();
                                    let c = e.client_coordinates();
                                    state.menu.set(Some(Menu { node: n_menu.clone(), is_dir, x: c.x, y: c.y }));
                                },
                                ondragstart: move |_| state.dragging.set(Some(n_drag.clone())),
                                ondragend: move |_| state.dragging.set(None),
                                ondragover: move |e| { if drop_target { e.prevent_default(); } },
                                ondrop: move |e| {
                                    e.prevent_default();
                                    e.stop_propagation();
                                    let Some(dragged) = state.dragging.take() else { return };
                                    if !drop_target || dragged.source != n_drop.source { return; }
                                    let name = dragged.native_key.rsplit('/').next().unwrap_or("").to_string();
                                    let to = if n_drop.native_key.is_empty() { name } else { format!("{}/{name}", n_drop.native_key) };
                                    if to == dragged.native_key { return; }
                                    spawn(async move {
                                        if let Err(e) = ws.rename_node(&dragged, &to).await { state.error.set(Some(e.to_string())); }
                                    });
                                },
                                onclick: move |_| {
                                    if renaming { return; }
                                    let n = n.clone();
                                    if is_dir {
                                        spawn(toggle(ws, state, n));
                                    } else if n.kind == NodeKind::Table {
                                        // Database tables open in the table editor.
                                        spawn(async move {
                                            if let Err(e) = ws.open_node(n).await {
                                                ws2.set_status(e.to_string());
                                            }
                                        });
                                    } else if is_db_path {
                                        // A database inside the folder: open it as its own source.
                                        let path = match n.source.as_str().strip_prefix("folder:") {
                                            Some(root) => format!("{root}/{}", n.native_key),
                                            None => n.native_key.clone(),
                                        };
                                        spawn(async move {
                                            if let Err(e) = ws.open_folder(path).await {
                                                ws2.set_status(format!("Could not open database: {e}"));
                                            }
                                        });
                                    } else if is_text {
                                        spawn(async move {
                                            if let Err(e) = ws.open_node(n).await {
                                                ws2.set_status(match e { SourceError::Unsupported(m) => m, other => other.to_string() });
                                            }
                                        });
                                    } else {
                                        ws2.set_status(format!("{} is a binary file", n.native_key));
                                    }
                                },
                                span { class: "mk-tree-caret", if is_dir { if open { "▾" } else { "▸" } } else { "" } }
                                if renaming {
                                    InlineEdit { ws, state, edit: Edit::Rename { node: node.clone() }, depth }
                                } else {
                                    span { class: "mk-tree-label", "{node.label}" }
                                }
                                for m in others.iter().filter(|m| m.active.as_deref() == Some(node.native_key.as_str())) {
                                    span { class: "mk-tree-presence", title: "{m.name} has this open", "{m.initials()}" }
                                }
                                if is_dir && ws.spawn_terminal().is_some() {
                                    {
                                        let n2 = node.clone();
                                        rsx! {
                                            span { class: "mk-tree-action", title: "New terminal here",
                                                onclick: move |e| {
                                                    e.stop_propagation();
                                                    ws2.terminal_cwd.set(ws2.folder_path(&n2));
                                                    ws2.dispatch(Command::NewTerminal);
                                                },
                                                ">_"
                                            }
                                        }
                                    }
                                }
                            }
                            if let Some(edit) = new_below {
                                ul { class: "mk-tree", style: "--depth: {depth + 1}",
                                    li { div { class: "mk-tree-row", InlineEdit { ws, state, edit, depth: depth + 1 } } }
                                }
                            }
                            if is_dir && open {
                                TreeLevel { ws, state, parent: node.id, depth: depth + 1 }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The right-click menu (Milestone 7). Positioned at the pointer; any click
/// elsewhere in the explorer closes it.
#[component]
fn ContextMenu(ws: Workspace, state: TreeState, menu: Menu) -> Element {
    let mut state = state;
    let mut ws = ws;
    let is_root = menu.node.native_key.is_empty();
    let node = menu.node.clone();
    let (n1, n2, n3, n4, n5) = (
        node.clone(),
        node.clone(),
        node.clone(),
        node.clone(),
        node.clone(),
    );
    let can_terminal = menu.is_dir && ws.spawn_terminal().is_some();
    rsx! {
        div { class: "mk-ctx", role: "menu", style: "left: {menu.x}px; top: {menu.y}px;", onclick: |e| e.stop_propagation(),
            if menu.is_dir {
                button { class: "mk-ctx-item", role: "menuitem", onclick: move |_| { state.menu.set(None); state.edit.set(Some(Edit::NewFile { parent: n1.clone() })); state.expanded.with_mut(|e| { e.insert(n1.id); }); }, "New File…" }
                button { class: "mk-ctx-item", role: "menuitem", onclick: move |_| { state.menu.set(None); state.edit.set(Some(Edit::NewDir { parent: n2.clone() })); state.expanded.with_mut(|e| { e.insert(n2.id); }); }, "New Folder…" }
            }
            if !is_root {
                button { class: "mk-ctx-item", role: "menuitem", onclick: move |_| { state.menu.set(None); state.edit.set(Some(Edit::Rename { node: n3.clone() })); }, "Rename…" }
                button { class: "mk-ctx-item", role: "menuitem", onclick: move |_| { state.menu.set(None); state.confirm.set(Some(n4.clone())); }, "Delete…" }
            }
            if can_terminal {
                button { class: "mk-ctx-item", role: "menuitem", onclick: move |_| {
                    state.menu.set(None);
                    ws.terminal_cwd.set(ws.folder_path(&n5));
                    ws.dispatch(Command::NewTerminal);
                }, "Open in Terminal" }
            }
        }
    }
}

/// The inline text field for a new file/folder name or a rename. Enter
/// commits, Escape cancels, blur cancels.
#[component]
fn InlineEdit(ws: Workspace, state: TreeState, edit: Edit, depth: usize) -> Element {
    let mut state = state;
    let initial = match &edit {
        Edit::Rename { node } => node.label.clone(),
        _ => String::new(),
    };
    let mut value = use_signal(|| initial);
    let placeholder = match &edit {
        Edit::NewFile { .. } => "file name",
        Edit::NewDir { .. } => "folder name",
        Edit::Rename { .. } => "new name",
    };
    let commit = {
        let edit = edit.clone();
        move || {
            let name = value.peek().trim().trim_matches('/').to_string();
            state.edit.set(None);
            if name.is_empty() {
                return;
            }
            let edit = edit.clone();
            // This component unmounts with `edit = None`; a task spawned in
            // its scope would be dropped with it.
            dioxus::core::spawn_forever(async move {
                let result = match edit {
                    Edit::NewFile { parent } => ws
                        .create_text(&parent.source, parent.id, &name, "")
                        .await
                        .map(|n| {
                            spawn(async move {
                                let _ = ws.open_node(n).await;
                            });
                        }),
                    Edit::NewDir { parent } => ws
                        .create_dir(&parent.source, parent.id, &name)
                        .await
                        .map(|_| ()),
                    Edit::Rename { node } => {
                        let to = match node.native_key.rsplit_once('/') {
                            Some((dir, _)) => format!("{dir}/{name}"),
                            None => name.clone(),
                        };
                        ws.rename_node(&node, &to).await.map(|_| ())
                    }
                };
                if let Err(e) = result {
                    state.error.set(Some(e.to_string()));
                }
            });
        }
    };
    let mut commit_key = commit.clone();
    rsx! {
        input {
            class: "mk-input mk-tree-edit",
            id: "mk-tree-edit",
            placeholder,
            value: "{value}",
            autofocus: true,
            onmounted: move |e| { spawn(async move { let _ = e.set_focus(true).await; }); },
            onclick: |e| e.stop_propagation(),
            onmousedown: |e| e.stop_propagation(),
            oninput: move |e| value.set(e.value()),
            onkeydown: move |e| {
                e.stop_propagation();
                match e.key() {
                    Key::Enter => { e.prevent_default(); commit_key(); }
                    Key::Escape => { e.prevent_default(); state.edit.set(None); }
                    _ => {}
                }
            },
            onblur: move |_| { if state.edit.peek().is_some() { state.edit.set(None); } },
        }
    }
}
