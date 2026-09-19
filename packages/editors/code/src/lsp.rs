//! `LspManager` — one language-server session per (language, folder root),
//! shared by every editor panel; diagnostics collected per document URI.

use dioxus::prelude::*;
use futures_util::StreamExt;
use moonkale_ext_api::Workspace;
use moonkale_lsp::{Diagnostic, LspEvent, LspSession};
use std::collections::HashMap;

#[derive(Clone, Copy)]
pub struct LspManager {
    sessions: Signal<HashMap<String, LspSession>>,
    starting: Signal<Vec<String>>,
    pub diagnostics: Signal<HashMap<String, Vec<Diagnostic>>>,
}

impl PartialEq for LspManager {
    fn eq(&self, o: &Self) -> bool {
        self.sessions == o.sessions && self.diagnostics == o.diagnostics
    }
}

impl Default for LspManager {
    fn default() -> Self {
        Self::new()
    }
}

impl LspManager {
    pub fn new() -> Self {
        Self {
            sessions: Signal::new_in_scope(HashMap::new(), ScopeId::ROOT),
            starting: Signal::new_in_scope(Vec::new(), ScopeId::ROOT),
            diagnostics: Signal::new_in_scope(HashMap::new(), ScopeId::ROOT),
        }
    }

    fn key(language: &str, root: &str) -> String {
        format!("{language}@{root}")
    }

    /// The session for `(language, root)` if it is already up and initialized.
    pub fn session(&self, language: &str, root: &str) -> Option<LspSession> {
        self.sessions
            .peek()
            .get(&Self::key(language, root))
            .filter(|s| s.is_initialized())
            .cloned()
    }

    /// Start a session unless one exists or is starting. Returns once the
    /// server is initialized (or immediately if unavailable).
    pub async fn ensure(self, mut ws: Workspace, language: &str, root: &str) -> Option<LspSession> {
        let key = Self::key(language, root);
        if let Some(s) = self.sessions.peek().get(&key) {
            return s.is_initialized().then(|| s.clone());
        }
        if self.starting.peek().contains(&key) {
            return None;
        }
        let spawn_lsp = ws.spawn_lsp()?;
        let mut this = self;
        this.starting.with_mut(|v| v.push(key.clone()));
        ws.lsp_status
            .set(Some(format!("{language}: starting language server…")));
        let transport = match spawn_lsp(language.to_string(), root.to_string()).await {
            Ok(t) => t,
            Err(e) => {
                ws.lsp_status.set(Some(format!("{language}: {e}")));
                this.starting.with_mut(|v| v.retain(|k| k != &key));
                return None;
            }
        };
        let (session, mut events) = LspSession::new(transport);
        spawn(session.clone().pump());
        let lang = language.to_string();
        spawn(async move {
            while let Some(ev) = events.next().await {
                match ev {
                    LspEvent::Initialized { server } => {
                        ws.lsp_status.set(Some(format!("{server}: ready")))
                    }
                    LspEvent::Status(s) => ws.lsp_status.set(Some(format!("{lang}: {s}"))),
                    LspEvent::Diagnostics { uri, diagnostics } => {
                        this.diagnostics.with_mut(|m| {
                            m.insert(uri, diagnostics);
                        });
                    }
                    LspEvent::Closed => {
                        ws.lsp_status
                            .set(Some(format!("{lang}: language server exited")));
                        break;
                    }
                }
            }
        });
        match session.initialize(root).await {
            Ok(_) => {
                this.sessions.with_mut(|m| {
                    m.insert(key.clone(), session.clone());
                });
                this.starting.with_mut(|v| v.retain(|k| k != &key));
                Some(session)
            }
            Err(e) => {
                ws.lsp_status
                    .set(Some(format!("{language}: initialize failed: {e}")));
                this.starting.with_mut(|v| v.retain(|k| k != &key));
                None
            }
        }
    }
}

/// Apply a server-provided edit to the workspace (Milestone 7: rename, code
/// actions). Files under `root` only. Open documents take the change as an
/// unsaved edit (the editor view follows through the document signal);
/// closed files are written through the folder source with a version check.
/// Returns how many files changed.
pub async fn apply_workspace_edit(
    mut ws: Workspace,
    root: &str,
    edit: &moonkale_lsp::WorkspaceEdit,
) -> Result<usize, String> {
    use moonkale_core::{Query, TextPatch, Transaction};
    let prefix = format!("file://{root}/");
    let mut changed = 0;
    for (uri, edits) in &edit.changes {
        let Some(rel) = uri.strip_prefix(&prefix) else {
            return Err(format!("{uri} is outside the folder"));
        };
        let folder = ws
            .sources
            .peek()
            .iter()
            .find(|s| s.descriptor.id.as_str() == format!("folder:{root}"))
            .map(|s| s.source.clone())
            .ok_or("folder not open")?;
        let node_id = moonkale_core::NodeId::derive(
            &moonkale_core::SourceId::new(format!("folder:{root}")),
            rel,
        );
        if let Some(mut doc) = ws.document(node_id) {
            let next = moonkale_lsp::WorkspaceEdit::apply_to_text(&doc.peek().text, edits);
            doc.with_mut(|d| d.text = next);
            changed += 1;
            continue;
        }
        // Not open: the node must exist (this also registers its id with the source).
        let node = folder
            .query(Query::Node(node_id))
            .await
            .map_err(|e| e.to_string())?
            .nodes
            .into_iter()
            .next()
            .ok_or_else(|| format!("{rel} not found"))?;
        let (text, version) = folder
            .fetch_text(node.id)
            .await
            .map_err(|e| e.to_string())?;
        let next = moonkale_lsp::WorkspaceEdit::apply_to_text(&text, edits);
        if next == text {
            continue;
        }
        let applied = folder
            .apply(Transaction::write_text(
                node.id,
                version,
                TextPatch::whole(next, text.chars().count()),
            ))
            .await
            .map_err(|e| e.to_string())?;
        if let Some(e) = applied.first_error() {
            return Err(format!("{rel}: {e}"));
        }
        changed += 1;
        // Re-index and let the server know the file changed on disk.
        let others: Vec<_> = ws
            .sources
            .peek()
            .iter()
            .filter(|s| s.descriptor.id.as_str() != format!("folder:{root}"))
            .map(|s| s.source.clone())
            .collect();
        for o in others {
            let _ = o.refresh(node.id).await;
        }
    }
    if changed > 0 {
        ws.graph_epoch.with_mut(|e| *e += 1);
    }
    Ok(changed)
}
