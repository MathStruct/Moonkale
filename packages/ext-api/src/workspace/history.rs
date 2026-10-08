//! `Workspace`: the entity log — record, compact, restore, persist. Split out of `workspace.rs` in Milestone 18 phase 3b
//! (no API change; phase 3c turns the areas into services).

use super::*;
use crate::{t, L};
use moonkale_core::EntityLog;
use moonkale_state::Record;

impl Workspace {
    /// The folder's entity log: from the host's store (phase 5.10), importing
    /// `.moonkale/history.jsonl` once when the store has nothing for the
    /// folder yet; without a host store, from that file (Milestone 8). An
    /// absent file is an empty log.
    pub async fn load_history(mut self, folder: &SourceId) {
        let log = match self.config.persistence.host {
            Some(host) => self.load_history_from(host, folder).await,
            None => self.read_history_file(folder).await,
        };
        tracing::info!("history: {} events for {folder}", log.len());
        self.history.log.set(log);
    }

    async fn load_history_from(&self, host: StateAccess, folder: &SourceId) -> EntityLog {
        let prefix = moonkale_state::Key::new().str(folder.as_str()).into_bytes();
        let rows = match (host.scan)(EventRecord::TABLE.to_string(), prefix).await {
            Ok(rows) => rows,
            Err(e) => {
                tracing::warn!("history: the host's store failed ({e}); reading {HISTORY_FILE}");
                return self.read_history_file(folder).await;
            }
        };
        if !rows.is_empty() {
            let mut log = EntityLog::new();
            for (_, bytes) in rows {
                match moonkale_state::decode::<EventRecord>(&bytes) {
                    Ok(EventRecord(e)) => {
                        log.append(e);
                    }
                    Err(e) => tracing::warn!("history: an event ignored: {e}"),
                }
            }
            return log;
        }
        // Nothing stored yet: take over the file of an older build (it stays
        // where it is, and is no longer written).
        let log = self.read_history_file(folder).await;
        if !log.is_empty() {
            let batch = log
                .events()
                .iter()
                .fold(moonkale_state::Batch::new(), |b, e| put_event(b, folder, e));
            match (host.write)(batch).await {
                Ok(()) => tracing::info!(
                    "history: imported {} events of {HISTORY_FILE} into the store",
                    log.len()
                ),
                Err(e) => tracing::warn!("history: import of {HISTORY_FILE} failed: {e}"),
            }
        }
        log
    }

    async fn read_history_file(&self, folder: &SourceId) -> EntityLog {
        match self.node_at_path(folder, HISTORY_FILE).await {
            Some(node) => match self.source(folder) {
                Some(src) => match src.fetch_text(node.id).await {
                    Ok((text, _)) => {
                        let (log, bad) = moonkale_core::EntityLog::from_jsonl_lossy(&text);
                        if !bad.is_empty() {
                            self.quarantine(&src, bad).await;
                        }
                        log
                    }
                    Err(_) => moonkale_core::EntityLog::new(),
                },
                None => moonkale_core::EntityLog::new(),
            },
            None => moonkale_core::EntityLog::new(),
        }
    }

    /// Lines of the history file that could not be read: the file is
    /// rewritten on every save, so they are moved to a file of their own
    /// instead of disappearing (#17).
    async fn quarantine(
        &self,
        src: &std::sync::Arc<dyn moonkale_core::Source>,
        lines: Vec<String>,
    ) {
        tracing::warn!(
            "history: {} unreadable lines of {HISTORY_FILE} kept in {QUARANTINE_FILE}",
            lines.len()
        );
        let mut more = lines.join("\n");
        more.push('\n');
        let folder = src.descriptor().id.clone();
        let result = match self.node_at_path(&folder, QUARANTINE_FILE).await {
            Some(node) => match src.fetch_text(node.id).await {
                Ok((old, version)) => {
                    let chars = old.chars().count();
                    src.apply(Transaction::write_text(
                        node.id,
                        version,
                        TextPatch::whole(format!("{old}{more}"), chars),
                    ))
                    .await
                }
                Err(e) => Err(e),
            },
            None => {
                src.apply(Transaction::create_text(
                    src.descriptor().root,
                    QUARANTINE_FILE,
                    more,
                ))
                .await
            }
        };
        if let Err(e) = result {
            tracing::warn!("history: could not write {QUARANTINE_FILE}: {e}");
        }
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

    /// Record an event as `actor` (`"user:<name>"`, `"agent:<model>"`).
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
        let stored = event.clone();
        self.history.log.with_mut(|l| {
            l.append(event);
        });
        let ws = *self;
        match (
            self.config.persistence.host,
            self.settings.folder.peek().clone(),
        ) {
            (Some(host), Some(folder)) => spawn(async move {
                let batch = put_event(moonkale_state::Batch::new(), &folder, &stored);
                if let Err(e) = (host.write)(batch).await {
                    tracing::warn!("history: an event not stored: {e}");
                }
            }),
            _ => spawn(async move { ws.persist_history().await }),
        };
        id
    }

    /// Fold everything but the last `keep` events into a snapshot
    /// (Milestone 9); returns how many events were folded.
    pub fn compact_history(&mut self, keep: usize) -> usize {
        let actor = self.user_actor();
        let at = now_ms();
        let before: std::collections::HashSet<moonkale_core::EventId> = self
            .history
            .log
            .peek()
            .events()
            .iter()
            .map(|e| e.id)
            .collect();
        let folded = self.history.log.with_mut(|l| l.compact(keep, at, actor));
        if folded == 0 {
            return 0;
        }
        let ws = *self;
        match (
            self.config.persistence.host,
            self.settings.folder.peek().clone(),
        ) {
            (Some(host), Some(folder)) => {
                // One batch: the folded events go, the snapshot comes.
                let log = self.history.log.peek();
                let after: std::collections::HashSet<_> =
                    log.events().iter().map(|e| e.id).collect();
                let mut batch = moonkale_state::Batch::new();
                for id in before.difference(&after) {
                    batch = batch.delete(EventRecord::TABLE, event_key(&folder, *id));
                }
                for e in log.events().iter().filter(|e| !before.contains(&e.id)) {
                    batch = put_event(batch, &folder, e);
                }
                spawn(async move {
                    if let Err(e) = (host.write)(batch).await {
                        tracing::warn!("history: the compaction not stored: {e}");
                    }
                });
            }
            _ => {
                spawn(async move { ws.persist_history().await });
            }
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
        self.set_status(t!(self, L, "restored"));
        Ok(())
    }

    /// Rewrite `.moonkale/history.jsonl` — only without a host store (with
    /// one, every event is already stored as it happens).
    pub(super) async fn persist_history(self) {
        if self.config.persistence.host.is_some() {
            return;
        }
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

/// One entity-log event as stored (phase 5.10): table `events`, key
/// `str(folder id) · u128(event id)` — so a folder's rows scan in log order
/// and a replica's event lands on the same key twice, not twice in the log.
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct EventRecord(pub moonkale_core::Event);

impl moonkale_state::Record for EventRecord {
    const TABLE: &'static str = moonkale_state::tables::EVENTS;
    const VERSION: u32 = 1;
}

/// The store key of an event: `str(folder id) · u128(event id)`.
pub fn event_key(folder: &SourceId, id: moonkale_core::EventId) -> Vec<u8> {
    moonkale_state::Key::new()
        .str(folder.as_str())
        .u128(id.0)
        .into_bytes()
}

fn put_event(
    batch: moonkale_state::Batch,
    folder: &SourceId,
    event: &moonkale_core::Event,
) -> moonkale_state::Batch {
    batch.put(
        EventRecord::TABLE,
        event_key(folder, event.id),
        moonkale_state::encode(&EventRecord(event.clone())),
    )
}

/// The entity log and what the next events are attributed to. (Milestone 18 phase 3c: the workspace's state, grouped by area.)
#[derive(Clone, Copy)]
pub struct HistoryState {
    /// The workspace's entity log (Milestone 8): every write appends; kept
    /// in the host's store (`Persistence::host`, phase 5.10), or without one
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
