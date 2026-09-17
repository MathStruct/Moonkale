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

const EXPLORER_CSS: Asset = asset!("/assets/styling/explorer.css");

#[derive(Clone, Copy)]
pub struct TreeState {
    expanded: Signal<HashSet<NodeId>>,
    children: Signal<HashMap<NodeId, Vec<Node>>>,
    error: Signal<Option<String>>,
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
            },
        }
    }
}

impl Extension for ExplorerExtension {
    fn manifest(&self) -> Manifest {
        Manifest {
            id: "dev.moonkale.explorer",
            name: "Explorer",
        }
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

    rsx! {
        document::Stylesheet { href: EXPLORER_CSS }
        div { class: "mk-explorer",
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
                    div { class: "mk-explorer-source-name", "{s.descriptor.display_name}" }
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
    rsx! {
        ul { class: "mk-tree", style: "--depth: {depth}",
            for node in children {
                {
                    let is_dir = node.kind == NodeKind::Directory;
                    let open = expanded.contains(&node.id);
                    let is_text = matches!(node.content, Some(ContentRef::Text { .. }));
                    let is_db = node.kind == NodeKind::Table
                        || (node.kind == NodeKind::File && moonkale_sources_sql::is_sqlite_path(&node.native_key));
                    let n = node.clone();
                    let mut ws2 = ws;
                    rsx! {
                        li { key: "{node.id}",
                            div {
                                class: if is_dir { "mk-tree-row mk-tree-dir" } else { "mk-tree-row mk-tree-file" },
                                class: if !is_dir && !is_text && !is_db { "mk-tree-binary" },
                                class: if is_db { "mk-tree-db" },
                                title: "{node.native_key}",
                                onclick: move |_| {
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
                                    } else if n.kind == NodeKind::File && moonkale_sources_sql::is_sqlite_path(&n.native_key) {
                                        // A SQLite file inside the folder: open it as a database source.
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
                                span { class: "mk-tree-label", "{node.label}" }
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
