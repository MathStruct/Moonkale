//! `Workspace`: the entity log — record, compact, restore, persist. Split out of `workspace.rs` in Milestone 18 phase 3b
//! (no API change; phase 3c turns the areas into services).

use super::*;

impl Workspace {
    /// Read `.moonkale/history.jsonl` (Milestone 8); an absent file is an
    /// empty log.
    pub async fn load_history(mut self, folder: &SourceId) {
        let log = match self.node_at_path(folder, HISTORY_FILE).await {
            Some(node) => match self.source(folder) {
                Some(src) => match src.fetch_text(node.id).await {
                    Ok((text, _)) => moonkale_core::EntityLog::from_jsonl(&text),
                    Err(_) => moonkale_core::EntityLog::new(),
                },
                None => moonkale_core::EntityLog::new(),
            },
            None => moonkale_core::EntityLog::new(),
        };
        tracing::info!("history: {} events for {folder}", log.len());
        self.history.log.set(log);
    }

    /// The actor string for the user's own edits.
    pub fn user_actor(&self) -> String {
        format!("user:{}", self.settings.resolved.peek().user_name)
    }

    /// Append an event to the log and persist it (best effort, never blocks
    /// the write it records). Returns the event id.
    pub fn record(&mut self, kind: moonkale_core::EventKind) -> moonkale_core::EventId {
        self.record_as(self.user_actor(), kind)
    }

    pub fn record_as(
        &mut self,
        actor: String,
        kind: moonkale_core::EventKind,
    ) -> moonkale_core::EventId {
        self.record_event(actor, kind, None)
    }

    /// Like [`record_as`](Self::record_as) with the node's key for display.
    pub fn record_event(
        &mut self,
        actor: String,
        kind: moonkale_core::EventKind,
        key: Option<String>,
    ) -> moonkale_core::EventId {
        let at = now_ms();
        let mut event = moonkale_core::Event::new(at, actor, kind);
        if let Some(k) = key {
            event = event.with_key(k);
        }
        if let Some(node) = event.node() {
            if let Some(cause) = self.history.pending_cause.with_mut(|m| m.remove(&node)) {
                event.cause = Some(cause);
            }
        }
        let id = event.id;
        self.history.log.with_mut(|l| {
            l.append(event);
        });
        let ws = *self;
        spawn(async move { ws.persist_history().await });
        id
    }

    /// Fold everything but the last `keep` events into a snapshot
    /// (Milestone 9); returns how many events were folded.
    pub fn compact_history(&mut self, keep: usize) -> usize {
        let actor = self.user_actor();
        let at = now_ms();
        let folded = self.history.log.with_mut(|l| l.compact(keep, at, actor));
        if folded > 0 {
            let ws = *self;
            spawn(async move { ws.persist_history().await });
        }
        folded
    }

    /// Put a node's text as of `event` into its document as an unsaved edit
    /// (opening the document first if needed); the save records the
    /// restore with `cause = event`.
    pub async fn restore_text_at(
        mut self,
        node: NodeId,
        event: moonkale_core::EventId,
    ) -> Result<(), SourceError> {
        let text = self
            .history
            .log
            .peek()
            .text_at(node, Some(event))
            .ok_or_else(|| SourceError::Unsupported("no text recorded for that event".into()))?;
        if self.document(node).is_none() {
            let folder: Arc<dyn Source> = self
                .sources
                .open
                .peek()
                .iter()
                .find(|s| s.descriptor.family == moonkale_core::SourceFamily::Folder)
                .map(|s| s.source.clone())
                .ok_or(SourceError::NotFound)?;
            let n = folder
                .query(Query::Node(node))
                .await?
                .nodes
                .into_iter()
                .next()
                .ok_or(SourceError::NotFound)?;
            self.open_node(n).await?;
        }
        let mut doc = self.document(node).ok_or(SourceError::NotFound)?;
        doc.with_mut(|d| d.text = text);
        self.history.pending_cause.with_mut(|m| {
            m.insert(node, event);
        });
        self.docs.active.set(Some(node));
        self.set_status("Restored as an unsaved edit — save to keep it");
        Ok(())
    }

    pub(super) async fn persist_history(self) {
        let Some(folder) = self.settings.folder.peek().clone() else {
            return;
        };
        let Some(src) = self.source(&folder) else {
            return;
        };
        let text = self.history.log.peek().to_jsonl();
        let result = match self.node_at_path(&folder, HISTORY_FILE).await {
            Some(node) => {
                let chars = src
                    .fetch_text(node.id)
                    .await
                    .map(|(t, _)| t.chars().count())
                    .unwrap_or(0);
                src.apply(Transaction::write_text(
                    node.id,
                    node.version,
                    TextPatch::whole(&text, chars),
                ))
                .await
            }
            None => {
                let root = src.descriptor().root;
                src.apply(Transaction::create_text(root, HISTORY_FILE, text))
                    .await
            }
        };
        if let Err(e) = result {
            tracing::warn!("history: could not write {HISTORY_FILE}: {e}");
        }
    }
}

/// The entity log and what the next events are attributed to. (Milestone 18 phase 3c: the workspace's state, grouped by area.)
#[derive(Clone, Copy)]
pub struct HistoryState {
    /// The workspace's entity log (Milestone 8): every write appends; kept
    /// in `.moonkale/history.jsonl` of the open folder.
    pub log: Signal<moonkale_core::EntityLog>,
    /// Who the next edit of a document is attributed to when it is not the
    /// user (the agent host sets it after `editor.replace`).
    pub pending_actor: Signal<std::collections::HashMap<NodeId, String>>,
    /// The event a pending edit restores (Milestone 9): the next save's
    /// `Content` event gets it as `cause`.
    pub pending_cause: Signal<std::collections::HashMap<NodeId, moonkale_core::EventId>>,
}

impl HistoryState {
    pub(super) fn new() -> Self {
        Self {
            log: Signal::new_in_scope(moonkale_core::EntityLog::new(), ScopeId::ROOT),
            pending_actor: Signal::new_in_scope(std::collections::HashMap::new(), ScopeId::ROOT),
            pending_cause: Signal::new_in_scope(std::collections::HashMap::new(), ScopeId::ROOT),
        }
    }
}
