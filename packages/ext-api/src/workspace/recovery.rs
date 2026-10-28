//! Recovery is client-local; folder content and source write authority stay put.
use super::*;
use moonkale_state::{
    recovery::{Budgets, Snapshot},
    Batch, Key, Record,
};

impl Workspace {
    fn recovery_gate(self) -> Rc<futures_util::lock::Mutex<()>> {
        #[derive(Clone)]
        struct Gate(Rc<futures_util::lock::Mutex<()>>);
        self.shared_state(|| Gate(Rc::new(futures_util::lock::Mutex::new(()))))
            .0
    }

    /// Remove every recovery version of a document after saving or discarding.
    pub async fn forget_recovery(self, source: &str, path: &str) {
        let Some(store) = self.config.persistence.state else {
            return;
        };
        let gate = self.recovery_gate();
        let _guard = gate.lock().await;
        let prefix = Key::new().str(source).str(path);
        if let Ok(rows) = (store.scan)(Snapshot::TABLE.into(), prefix.into_bytes()).await {
            let batch = rows
                .into_iter()
                .fold(Batch::new(), |b, (k, _)| b.delete(Snapshot::TABLE, k));
            let _ = (store.write)(batch).await;
        }
    }

    /// List bounded, admissible snapshots. Errors reveal no buffer contents.
    pub async fn recovery_snapshots(self) -> Vec<Snapshot> {
        let Some(store) = self.config.persistence.state else {
            return Vec::new();
        };
        let budget = Budgets::platform();
        let Ok(rows) = (store.scan)(Snapshot::TABLE.into(), Vec::new()).await else {
            return Vec::new();
        };
        rows.into_iter()
            .filter_map(|(_, value)| {
                if value.len() > budget.recovery {
                    return None;
                }
                let record: Snapshot = moonkale_state::decode(&value).ok()?;
                record.admissible(budget).then_some(record)
            })
            .collect()
    }

    /// Snapshot the working set off the input path. Unanswered offers must not
    /// be replaced by the clean source text loaded during tab restoration.
    pub async fn snapshot_documents(self, pending: &[Snapshot]) {
        let Some(store) = self.config.persistence.state else {
            return;
        };
        let gate = self.recovery_gate();
        let _guard = gate.lock().await;
        let docs: Vec<_> = self.docs.open.peek().iter().map(|(_, doc)| *doc).collect();
        for doc in docs {
            let Ok(rows) = (store.scan)(Snapshot::TABLE.into(), Vec::new()).await else {
                continue;
            };
            let record = {
                let d = doc.peek();
                if pending
                    .iter()
                    .any(|s| s.source == d.node.source.0 && s.document == d.node.native_key)
                {
                    continue;
                }
                if !self.sources.open.peek().iter().any(|s| {
                    s.descriptor.id == d.node.source
                        && s.descriptor.family == moonkale_core::SourceFamily::Folder
                }) {
                    continue;
                }
                if !moonkale_state::recovery::allowed_path(&d.node.native_key)
                    || d.saved.len().saturating_add(d.text.len()) > Budgets::platform().buffer
                {
                    continue;
                }
                Snapshot {
                    source: d.node.source.0.clone(),
                    document: d.node.native_key.clone(),
                    base_version: d.version.to_string(),
                    base: d.saved.clone(),
                    text: d.text.clone(),
                    touched: now_ms(),
                }
            };
            // The source decides ignore/symlink admission, including after a
            // previously open file becomes ignored. Unsupported sources opt out.
            if record.base != record.text {
                let source = self.source(&SourceId(record.source.clone()));
                let admitted = if let Some(source) = source {
                    source
                        .query(Query::Text {
                            dialect: "recovery-path".into(),
                            text: record.document.clone(),
                        })
                        .await
                        .ok()
                        .is_some_and(|r| r.nodes.iter().any(|n| n.id == doc.peek().node.id))
                } else {
                    false
                };
                if !admitted {
                    let prefix = record.prefix();
                    let batch = rows
                        .into_iter()
                        .filter(|(k, _)| k.starts_with(prefix.as_bytes()))
                        .fold(Batch::new(), |b, (k, _)| b.delete(Snapshot::TABLE, k));
                    if !batch.is_empty() {
                        let _ = (store.write)(batch).await;
                    }
                    continue;
                }
            }
            // Skip unchanged records: no disk traffic every two seconds.
            let same = rows
                .iter()
                .find(|(k, _)| k == record.key().as_bytes())
                .and_then(|(_, v)| moonkale_state::decode::<Snapshot>(v).ok())
                .is_some_and(|s| s.base == record.base && s.text == record.text);
            if same {
                continue;
            }
            let batch = moonkale_state::recovery::replace(rows, &record, Budgets::platform());
            if !batch.is_empty() && (store.write)(batch).await.is_err() {
                tracing::warn!("recovery: snapshot write failed");
            }
        }
    }
}
