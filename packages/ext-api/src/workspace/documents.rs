//! `Workspace`: documents and views — open, save, create, rename, delete, replace; the cursor; which editor shows what. Split out of `workspace.rs` in Milestone 18 phase 3b
//! (no API change; phase 3c turns the areas into services).

use super::*;

impl Workspace {
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

    /// The editor reports the cursor of `node`; published when it is the
    /// active document.
    pub fn set_cursor_line(&mut self, node: NodeId, line: u32) {
        if *self.active.peek() == Some(node) && *self.cursor_line.peek() != Some(line) {
            self.cursor_line.set(Some(line));
            self.publish_presence();
        }
    }

    /// The editor reports the caret of `node` (line, column; 0-based).
    /// Keeps `cursor_line` (presence) in step (Milestone 14).
    pub fn set_cursor(&mut self, node: NodeId, line: u32, col: u32) {
        if *self.active.peek() != Some(node) {
            return;
        }
        if *self.cursor.peek() != Some((line, col)) {
            self.cursor.set(Some((line, col)));
        }
        self.set_cursor_line(node, line);
    }

    /// The identifier under the active document's caret, if any.
    pub fn cursor_word(&self) -> Option<String> {
        let (line, col) = (*self.cursor.read())?;
        let (_, doc) = self.active_document()?;
        let text = doc.read().text.clone();
        word_at(&text, line, col)
    }

    /// Which code editor shows `node` (Milestone 14): the per-document
    /// choice, else `editor.implementation`, else the one enabled.
    pub fn editor_for(&self, node: NodeId) -> &'static str {
        if let Some(c) = self.editor_choice.read().get(&node) {
            return c;
        }
        let s = self.settings.read();
        let codemirror = s.extensions.is_enabled_id("dev.moonkale.editor-code", true);
        let native = s
            .extensions
            .is_enabled_id("dev.moonkale.editor-code-native", false);
        match (codemirror, native) {
            (true, false) => "codemirror",
            (false, true) => "native",
            _ => {
                if s.editor.implementation == "native" {
                    "native"
                } else {
                    "codemirror"
                }
            }
        }
    }

    /// Move `node` to the other code editor (the toolbar switch).
    /// Of several extensions claiming `node` with the same priority (the two
    /// code editors), the one the user chose ([`Workspace::editor_for`]).
    pub fn preferred_editor(&self, node: NodeId) -> &'static str {
        match self.editor_for(node) {
            "native" => "dev.moonkale.editor-code-native",
            _ => "dev.moonkale.editor-code",
        }
    }

    /// [`Workspace::preferred_editor`] without subscribing the caller to the
    /// settings: for event handlers and effects. (The shell's command effect
    /// re-ran its last command whenever the settings changed when it read
    /// them here — Milestone 18 phase 2.4.)
    pub fn preferred_editor_untracked(&self, node: NodeId) -> &'static str {
        let choice = self.editor_choice.peek().get(&node).copied();
        let which = choice.unwrap_or_else(|| {
            let s = self.settings.peek();
            let codemirror = s.extensions.is_enabled_id("dev.moonkale.editor-code", true);
            let native = s
                .extensions
                .is_enabled_id("dev.moonkale.editor-code-native", false);
            match (codemirror, native) {
                (true, false) => "codemirror",
                (false, true) => "native",
                _ if s.editor.implementation == "native" => "native",
                _ => "codemirror",
            }
        });
        match which {
            "native" => "dev.moonkale.editor-code-native",
            _ => "dev.moonkale.editor-code",
        }
    }

    /// The node behind a document or view panel, if it is open. Does not
    /// subscribe the caller.
    pub fn open_node_by_id(&self, node: NodeId) -> Option<Node> {
        if let Some((_, d)) = self.documents.peek().iter().find(|(id, _)| *id == node) {
            return Some(d.peek().node.clone());
        }
        self.views.peek().iter().find(|n| n.id == node).cloned()
    }

    pub fn choose_editor(&mut self, node: NodeId, which: &'static str) {
        self.editor_choice.with_mut(|m| {
            m.insert(node, which);
        });
    }

    /// The active document, if any.
    pub fn active_document(&self) -> Option<(NodeId, Signal<Document>)> {
        let id = (*self.active.read())?;
        self.document(id).map(|d| (id, d))
    }

    pub fn document(&self, node: NodeId) -> Option<Signal<Document>> {
        self.documents
            .read()
            .iter()
            .find(|(id, _)| *id == node)
            .map(|(_, d)| *d)
    }

    /// Load a node's text (if not already open) and make it the active
    /// document.
    pub async fn open_node(mut self, node: Node) -> Result<(), SourceError> {
        // Nodes without a text body (tables, images and other blobs) open as
        // views, not documents (spec 008).
        if matches!(node.kind, NodeKind::Table)
            || matches!(node.content, Some(moonkale_core::ContentRef::Blob { .. }))
        {
            if !self.views.peek().iter().any(|n| n.id == node.id) {
                self.views.with_mut(|v| v.push(node.clone()));
            }
            self.active.set(Some(node.id));
            self.set_status(format!("Opened {}", node.label));
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

    /// Open a blob as a text document anyway (an SVG's source, spec 008).
    pub async fn open_as_text(mut self, node: Node) -> Result<(), SourceError> {
        if self.document(node.id).is_none() {
            let source = self.source(&node.source).ok_or(SourceError::NotFound)?;
            let (text, version) = source.fetch_text(node.id).await?;
            let doc =
                Signal::new_in_scope(Document::new(node.clone(), text, version), ScopeId::ROOT);
            self.documents.with_mut(|v| v.push((node.id, doc)));
        }
        self.views.with_mut(|v| v.retain(|n| n.id != node.id));
        self.active.set(Some(node.id));
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
        let (source_id, version, patch, key, before) = {
            let d = doc.read();
            (
                d.node.source.clone(),
                d.version,
                d.patch(),
                d.node.native_key.clone(),
                d.saved.clone(),
            )
        };
        let source = self.source(&source_id).ok_or(SourceError::NotFound)?;
        let applied = source
            .apply(Transaction::write_text(node, version, patch.clone()))
            .await?;
        match applied.version_of(node) {
            Some(v) => {
                let chars_after = doc.peek().text.chars().count();
                doc.with_mut(|d| d.mark_saved(v));
                self.set_status(format!("Saved {key}"));
                // History: the patch the save carried, attributed to the
                // agent when it made the edit (the user still approved it).
                let actor = match self.pending_actor.with_mut(|m| m.remove(&node)) {
                    Some(a) => format!("{a} (saved by {})", self.settings.peek().user_name),
                    None => self.user_actor(),
                };
                // Files that predate the log get their pre-edit text as the base.
                let base = if self.history.peek().text_at(node, None).is_none() {
                    Some(before)
                } else {
                    None
                };
                self.record_event(
                    actor,
                    moonkale_core::EventKind::Content {
                        node,
                        patch,
                        chars_after,
                        base,
                    },
                    Some(key.clone()),
                );
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
        if !node.native_key.starts_with(".moonkale/") {
            self.record(moonkale_core::EventKind::Add {
                node: node.clone(),
                text: Some(text.to_string()),
            });
        }
        Ok(node)
    }

    /// Apply one op to a source and return the resulting node id, or the
    /// refusal as an error.
    pub(super) async fn apply_one(
        &self,
        source: &Arc<dyn Source>,
        tx: Transaction,
    ) -> Result<NodeId, SourceError> {
        let applied = source.apply(tx).await?;
        match applied.results.first() {
            Some(moonkale_core::OpResult::Ok { node, .. }) => Ok(*node),
            Some(moonkale_core::OpResult::Refused { error, .. }) => Err(error.clone()),
            None => Err(SourceError::Unsupported("refused".into())),
        }
    }

    /// Create a directory under `parent` (Milestone 7).
    pub async fn create_dir(
        mut self,
        source_id: &SourceId,
        parent: NodeId,
        name: &str,
    ) -> Result<NodeId, SourceError> {
        let source = self.source(source_id).ok_or(SourceError::NotFound)?;
        let id = self
            .apply_one(&source, Transaction::create_dir(parent, name))
            .await?;
        self.after_fs_change(source_id, &[id]).await;
        self.set_status(format!("Created {name}/"));
        if let Ok(r) = source.query(Query::Node(id)).await {
            if let Some(n) = r.nodes.into_iter().next() {
                self.record(moonkale_core::EventKind::Add {
                    node: n,
                    text: None,
                });
            }
        }
        Ok(id)
    }

    /// Rename or move a node to the relative path `to`. Open documents under
    /// the old path are re-keyed to their new ids (text, dirty state and
    /// version kept); the active document follows.
    pub async fn rename_node(mut self, node: &Node, to: &str) -> Result<NodeId, SourceError> {
        let source_id = node.source.clone();
        let source = self.source(&source_id).ok_or(SourceError::NotFound)?;
        // Who links here — asked before the rename (spec 012: links follow).
        let linking = if node.native_key.ends_with(".md") {
            self.wiki_backlinks(node.id).await
        } else {
            Vec::new()
        };
        let new_id = self
            .apply_one(&source, Transaction::rename(node.id, to))
            .await?;
        let to = to.trim_matches('/').to_string();
        // Re-key documents: the node itself, or anything below a directory.
        let from = node.native_key.clone();
        let prefix = format!("{from}/");
        let affected: Vec<(NodeId, Signal<Document>)> = self
            .documents
            .peek()
            .iter()
            .filter(|(_, d)| {
                let k = &d.peek().node.native_key;
                *k == from || k.starts_with(&prefix)
            })
            .cloned()
            .collect();
        let was_active = *self.active.peek();
        for (old_id, mut doc) in affected {
            let new_key = {
                let k = doc.peek().node.native_key.clone();
                if k == from {
                    to.clone()
                } else {
                    format!("{to}/{}", &k[prefix.len()..])
                }
            };
            let fresh = match source
                .query(Query::Node(NodeId::derive(&source_id, &new_key)))
                .await
            {
                Ok(r) => r.nodes.into_iter().next(),
                Err(_) => None,
            };
            let Some(fresh) = fresh else { continue };
            let fresh_id = fresh.id;
            doc.with_mut(|d| {
                d.node = fresh;
            });
            self.documents.with_mut(|v| {
                for (id, _) in v.iter_mut() {
                    if *id == old_id {
                        *id = fresh_id;
                    }
                }
            });
            self.views.with_mut(|v| {
                for n in v.iter_mut() {
                    if n.id == old_id {
                        n.id = fresh_id;
                        n.native_key = new_key.clone();
                    }
                }
            });
            if was_active == Some(old_id) {
                self.active.set(Some(fresh_id));
            }
        }
        self.after_fs_change(&source_id, &[node.id, new_id]).await;
        self.set_status(format!("Renamed {from} → {to}"));
        self.record(moonkale_core::EventKind::Rename {
            from: node.id,
            to: new_id,
            from_key: from.clone(),
            to_key: to.clone(),
        });
        if !linking.is_empty() {
            self.rewrite_wiki_links(linking, &from, &to).await;
        }
        Ok(new_id)
    }

    /// Delete a node (to `.moonkale/trash` on folders); documents under it
    /// are closed without saving.
    pub async fn delete_node(mut self, node: &Node) -> Result<(), SourceError> {
        let source_id = node.source.clone();
        let source = self.source(&source_id).ok_or(SourceError::NotFound)?;
        self.apply_one(&source, Transaction::delete(node.id))
            .await?;
        let prefix = format!("{}/", node.native_key);
        let closing: Vec<NodeId> = self
            .documents
            .peek()
            .iter()
            .filter(|(id, d)| *id == node.id || d.peek().node.native_key.starts_with(&prefix))
            .map(|(id, _)| *id)
            .collect();
        for id in closing {
            self.close_node(id);
        }
        self.after_fs_change(&source_id, &[node.id]).await;
        self.set_status(format!(
            "Deleted {} (kept in .moonkale/trash)",
            node.native_key
        ));
        self.record_event(
            self.user_actor(),
            moonkale_core::EventKind::Remove { node: node.id },
            Some(node.native_key.clone()),
        );
        Ok(())
    }

    /// Literal occurrences of `needle` in a file (open document text if it
    /// is open, else the source's), for a replace preview (Milestone 7).
    pub async fn count_occurrences(&self, node: &Node, needle: &str) -> Result<usize, SourceError> {
        if needle.is_empty() {
            return Ok(0);
        }
        let text = match self.document(node.id) {
            Some(d) => d.peek().text.clone(),
            None => {
                let source = self.source(&node.source).ok_or(SourceError::NotFound)?;
                source.fetch_text(node.id).await?.0
            }
        };
        Ok(text.matches(needle).count())
    }

    /// Replace every literal `needle` in one file. An open document takes
    /// the change as an unsaved edit (the user saves); a closed file is
    /// written through its source with a version check and the index is
    /// refreshed. Returns the number of replacements.
    pub async fn replace_in_file(
        mut self,
        node: &Node,
        needle: &str,
        replacement: &str,
    ) -> Result<usize, SourceError> {
        if needle.is_empty() {
            return Ok(0);
        }
        if let Some(mut doc) = self.document(node.id) {
            let (count, next) = {
                let d = doc.peek();
                (
                    d.text.matches(needle).count(),
                    d.text.replace(needle, replacement),
                )
            };
            if count > 0 {
                doc.with_mut(|d| d.text = next);
            }
            return Ok(count);
        }
        let source = self.source(&node.source).ok_or(SourceError::NotFound)?;
        let (text, version) = source.fetch_text(node.id).await?;
        let count = text.matches(needle).count();
        if count == 0 {
            return Ok(0);
        }
        let next = text.replace(needle, replacement);
        let applied = source
            .apply(Transaction::write_text(
                node.id,
                version,
                TextPatch::whole(next, text.chars().count()),
            ))
            .await?;
        if let Some(e) = applied.first_error() {
            return Err(e.clone());
        }
        let source_id = node.source.clone();
        self.after_fs_change(&source_id, &[node.id]).await;
        Ok(count)
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
