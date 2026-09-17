//! `Workspace` — the host handle extensions receive.
//!
//! A `Copy` bundle of signals plus the operations that mutate them. Every
//! field is a `Signal`, so components that read them re-render on change and
//! event handlers can mutate them without borrowing the workspace itself.
//!
//! Documents are keyed by `NodeId` and stored as their own `Signal` each, so
//! a keystroke re-renders only the editor of that document, not every reader
//! of the open-document list.

use crate::Document;
use dioxus::prelude::*;
use moonkale_core::{
    Node, NodeId, Query, QueryResult, Source, SourceDescriptor, SourceError, SourceId, Transaction,
};
use std::future::Future;
use std::pin::Pin;
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
/// The future an [`OpenFolder`] returns.
pub type OpenFolderFuture = Pin<Box<dyn Future<Output = Result<Arc<dyn Source>, SourceError>>>>;
pub type OpenFolder = fn(String) -> OpenFolderFuture;

#[derive(Clone, Copy)]
pub struct Workspace {
    pub sources: Signal<Vec<SourceHandle>>,
    /// Open documents in opening order (this is the tab order).
    pub documents: Signal<Vec<(NodeId, Signal<Document>)>>,
    pub active: Signal<Option<NodeId>>,
    /// One line for the status bar.
    pub status: Signal<String>,
    open_folder: OpenFolder,
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
    pub fn new(open_folder: OpenFolder) -> Self {
        Self {
            sources: Signal::new_in_scope(Vec::new(), ScopeId::ROOT),
            documents: Signal::new_in_scope(Vec::new(), ScopeId::ROOT),
            active: Signal::new_in_scope(None, ScopeId::ROOT),
            status: Signal::new_in_scope("Ready".into(), ScopeId::ROOT),
            open_folder,
        }
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
        let source = (self.open_folder)(path).await?;
        let descriptor = source.descriptor();
        self.sources.with_mut(|v| {
            v.retain(|s| s.descriptor.id != descriptor.id);
            v.push(SourceHandle {
                descriptor: descriptor.clone(),
                source,
            });
        });
        self.set_status(format!("Opened {}", descriptor.display_name));
        Ok(descriptor)
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
