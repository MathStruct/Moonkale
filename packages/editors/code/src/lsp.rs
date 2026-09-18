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
