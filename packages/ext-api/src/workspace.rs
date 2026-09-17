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
    Node, NodeId, Query, QueryResult, Source, SourceDescriptor, SourceError, SourceId, Transaction,
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
pub type OpenFolder = fn(String) -> OpenFolderFuture;
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

/// What the platform hands the workspace at startup.
#[derive(Clone, Copy)]
pub struct WorkspaceConfig {
    pub open_folder: OpenFolder,
    pub pick_folder: Option<PickFolder>,
    pub attach_source: AttachSource,
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
    About,
}

#[derive(Clone, Copy)]
pub struct Workspace {
    pub sources: Signal<Vec<SourceHandle>>,
    /// Open documents in opening order (this is the tab order).
    pub documents: Signal<Vec<(NodeId, Signal<Document>)>>,
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
            active: Signal::new_in_scope(None, ScopeId::ROOT),
            status: Signal::new_in_scope("Ready".into(), ScopeId::ROOT),
            commands: Signal::new_in_scope((0, None), ScopeId::ROOT),
            window: Signal::new_in_scope(WindowId::fresh(), ScopeId::ROOT),
            foreign_drag: Signal::new_in_scope(None, ScopeId::ROOT),
            own_drag: Signal::new_in_scope(None, ScopeId::ROOT),
            peers: Signal::new_in_scope(Vec::new(), ScopeId::ROOT),
            graph_epoch: Signal::new_in_scope(0, ScopeId::ROOT),
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

    /// Open a folder through the platform's factory and add it to `sources`.
    pub async fn open_folder(mut self, path: String) -> Result<SourceDescriptor, SourceError> {
        let sources = (self.config.open_folder)(path).await?;
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
