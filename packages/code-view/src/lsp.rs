//! `LspManager` — one language-server session per (language, folder root),
//! shared by every editor panel; diagnostics collected per document URI.

use crate::L;
use dioxus::prelude::*;
use futures_util::{
    future::{select, Either},
    StreamExt,
};
use moonkale_ext_api::{t, Workspace};
use moonkale_lsp::{Diagnostic, LspEvent, LspSession};
use std::{collections::HashMap, rc::Rc};

#[derive(Clone)]
struct OpenDocument {
    session: LspSession,
    clients: usize,
    retained: Option<Rc<()>>,
    version: i32,
    text: String,
    saved: String,
    key: String,
}

#[derive(Clone, Copy)]
pub struct LspManager {
    sessions: Signal<HashMap<String, LspSession>>,
    starting: Signal<Vec<String>>,
    documents: Signal<HashMap<String, OpenDocument>>,
    connections: Signal<HashMap<String, LspSession>>,
    pub diagnostics: Signal<HashMap<String, Vec<Diagnostic>>>,
    diagnostic_versions: Signal<HashMap<String, Option<i32>>>,
}

// Cancellation and startup timeout must release both the starting marker and
// any root-owned pumps created before initialization completed.
struct StartingSession {
    starting: Signal<Vec<String>>,
    key: String,
    tasks: Vec<dioxus::core::Task>,
}
impl Drop for StartingSession {
    fn drop(&mut self) {
        self.starting
            .with_mut(|keys| keys.retain(|key| key != &self.key));
        for task in self.tasks.drain(..) {
            task.cancel();
        }
    }
}

async fn startup_timeout<T>(future: impl std::future::Future<Output = T>) -> Option<T> {
    match select(
        Box::pin(future),
        Box::pin(futures_timer::Delay::new(std::time::Duration::from_secs(
            30,
        ))),
    )
    .await
    {
        Either::Left((value, _)) => Some(value),
        Either::Right(_) => None,
    }
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
    /// All editor implementations in a workspace share servers and URI versions.
    pub fn for_workspace(ws: Workspace) -> Self {
        ws.shared_state(Self::new)
    }

    pub fn new() -> Self {
        Self {
            sessions: Signal::new_in_scope(HashMap::new(), ScopeId::ROOT),
            starting: Signal::new_in_scope(Vec::new(), ScopeId::ROOT),
            documents: Signal::new_in_scope(HashMap::new(), ScopeId::ROOT),
            connections: Signal::new_in_scope(HashMap::new(), ScopeId::ROOT),
            diagnostics: Signal::new_in_scope(HashMap::new(), ScopeId::ROOT),
            diagnostic_versions: Signal::new_in_scope(HashMap::new(), ScopeId::ROOT),
        }
    }

    fn publish_diagnostics(
        mut self,
        uri: &str,
        version: Option<i32>,
        diagnostics: Vec<Diagnostic>,
    ) {
        let uri = crate::uri::key(uri);
        if self
            .documents
            .peek()
            .get(&uri)
            .is_some_and(|document| version.is_some_and(|version| version != document.version))
        {
            return;
        }
        self.diagnostic_versions.with_mut(|versions| {
            versions.insert(uri.clone(), version);
        });
        self.diagnostics.with_mut(|values| {
            values.insert(uri, diagnostics);
        });
    }

    pub(crate) fn document_session(&self, uri: &str) -> Option<LspSession> {
        self.connections.read().get(&crate::uri::key(uri)).cloned()
    }

    pub(crate) fn peek_document_version(&self, uri: &str) -> Option<i32> {
        let normalized = crate::uri::key(uri);
        let uri = normalized.as_str();
        self.documents
            .peek()
            .get(uri)
            .map(|document| document.version)
    }

    pub(crate) fn peek_document_session(&self, uri: &str) -> Option<LspSession> {
        let normalized = crate::uri::key(uri);
        let uri = normalized.as_str();
        self.documents
            .peek()
            .get(uri)
            .map(|document| document.session.clone())
    }

    pub(crate) fn open_document(
        mut self,
        session: LspSession,
        uri: &str,
        language: &str,
        root: &str,
        text: String,
        saved: String,
    ) {
        let normalized = crate::uri::key(uri);
        let uri = normalized.as_str();
        if !self.documents.peek().contains_key(uri) {
            self.connections.with_mut(|values| {
                values.remove(uri);
            });
            self.diagnostic_versions.with_mut(|values| {
                values.remove(uri);
            });
            self.diagnostics.with_mut(|values| {
                values.remove(uri);
            });
        }
        self.documents.with_mut(|documents| {
            if let Some(document) = documents.get_mut(uri) {
                document.clients += 1;
                return;
            }
            session.did_open(uri, language, 1, &text);
            self.connections.with_mut(|connections| {
                connections.insert(uri.to_string(), session.clone());
            });
            documents.insert(
                uri.to_string(),
                OpenDocument {
                    session,
                    clients: 1,
                    retained: None,
                    version: 1,
                    text,
                    saved,
                    key: Self::key(language, root),
                },
            );
        });
    }

    pub(crate) fn update_document(mut self, uri: &str, text: &str, saved: &str) {
        let normalized = crate::uri::key(uri);
        let uri = normalized.as_str();
        let Some((changed, saved_changed)) = self
            .documents
            .peek()
            .get(uri)
            .map(|document| (document.text != text, document.saved != saved))
        else {
            return;
        };
        if !changed && !saved_changed {
            return;
        }
        self.documents.with_mut(|documents| {
            let Some(document) = documents.get_mut(uri) else {
                return;
            };
            if changed {
                document.version += 1;
                document.text = text.to_string();
                document.session.did_change(uri, document.version, text);
            }
            if saved_changed {
                document.saved = saved.to_string();
                document.session.did_save(uri);
            }
        });
        // Versioned ranges are stale after an edit; save-time reports without
        // a version remain visible until the server publishes a replacement.
        if changed
            && self
                .diagnostic_versions
                .peek()
                .get(uri)
                .is_some_and(Option::is_some)
        {
            self.diagnostics.with_mut(|values| {
                values.remove(uri);
            });
            self.diagnostic_versions.with_mut(|values| {
                values.remove(uri);
            });
        }
    }

    // Rename creates unsaved documents even when no view is mounted. Keep a
    // Workspace lease for edited files so later requests see their current text.
    pub(crate) fn retain_workspace_document(
        mut self,
        ws: Workspace,
        node: &moonkale_core::Node,
        session: LspSession,
        root: &str,
        server_language: &str,
    ) {
        let uri = crate::native_diagnostics::file_uri(root, &node.native_key);
        let Some(doc) = ws
            .docs
            .open
            .peek()
            .iter()
            .find(|(id, _)| *id == node.id)
            .map(|(_, doc)| *doc)
        else {
            return;
        };
        if self
            .documents
            .peek()
            .get(&uri)
            .is_some_and(|document| document.retained.is_some())
        {
            let doc = doc.peek();
            self.update_document(&uri, &doc.text, &doc.saved);
            return;
        }
        let already_open = self.documents.peek().contains_key(&uri);
        let document = doc.peek();
        self.open_document(
            session,
            &uri,
            node.language_hint().unwrap_or("plaintext"),
            root,
            document.text.clone(),
            document.saved.clone(),
        );
        drop(document);
        let lease = Rc::new(());
        self.documents.with_mut(|documents| {
            if let Some(document) = documents.get_mut(&uri) {
                document.retained = Some(lease.clone());
                if !already_open {
                    document.key = Self::key(server_language, root);
                }
            }
        });
        let source = node.source.clone();
        let node = node.id;
        dioxus::core::spawn_forever(async move {
            let (context, mut changed) = dioxus::core::ReactiveContext::new();
            loop {
                let keep = context.reset_and_run_in(|| {
                    let _ = self.connections.read();
                    if !self.documents.peek().get(&uri).is_some_and(|document| {
                        document
                            .retained
                            .as_ref()
                            .is_some_and(|active| Rc::ptr_eq(active, &lease))
                    }) {
                        return false; // The server exited or a new session owns this URI.
                    }
                    if !ws
                        .sources
                        .open
                        .read()
                        .iter()
                        .any(|open| open.descriptor.id == source)
                    {
                        return false;
                    }
                    let doc = ws
                        .docs
                        .open
                        .read()
                        .iter()
                        .find(|(id, _)| *id == node)
                        .map(|(_, doc)| *doc);
                    let Some(doc) = doc else {
                        return false;
                    };
                    {
                        let doc = doc.read();
                        self.update_document(&uri, &doc.text, &doc.saved);
                    }
                    true
                });
                if !keep {
                    break;
                }
                if changed.next().await.is_none() {
                    break;
                }
            }
            if !self.documents.peek().get(&uri).is_some_and(|document| {
                document
                    .retained
                    .as_ref()
                    .is_some_and(|active| Rc::ptr_eq(active, &lease))
            }) {
                return;
            }
            self.documents.with_mut(|documents| {
                if let Some(document) = documents.get_mut(&uri) {
                    document.retained = None;
                }
            });
            self.close_document(&uri);
        });
    }

    pub(crate) fn sync_workspace_documents(self, ws: Workspace) {
        for (_, doc) in ws.docs.open.peek().iter() {
            let doc = doc.peek();
            if let Some(root) = doc.node.source.as_str().strip_prefix("folder:") {
                let uri = crate::native_diagnostics::file_uri(root, &doc.node.native_key);
                if self.documents.peek().contains_key(&uri) {
                    self.update_document(&uri, &doc.text, &doc.saved);
                }
            }
        }
    }

    pub(crate) fn close_connection(self, uri: &str, session: &LspSession) {
        if self
            .peek_document_session(uri)
            .is_some_and(|current| current.same_connection(session))
        {
            self.close_document(uri);
        }
    }

    pub(crate) fn close_document(mut self, uri: &str) {
        let normalized = crate::uri::key(uri);
        let uri = normalized.as_str();
        self.documents.with_mut(|documents| {
            if let Some(document) = documents.get_mut(uri) {
                document.clients = document.clients.saturating_sub(1);
                if document.clients == 0 {
                    document.session.did_close(uri);
                    documents.remove(uri);
                }
            }
        });
        if !self.documents.peek().contains_key(uri) {
            self.connections.with_mut(|values| {
                values.remove(uri);
            });
            self.diagnostic_versions.with_mut(|values| {
                values.remove(uri);
            });
            self.diagnostics.with_mut(|values| {
                values.remove(uri);
            });
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
    /// server is initialized, including when another caller started it (or
    /// immediately if unavailable).
    pub async fn ensure(self, mut ws: Workspace, language: &str, root: &str) -> Option<LspSession> {
        let key = Self::key(language, root);
        if let Some(s) = self.sessions.peek().get(&key) {
            return s.is_initialized().then(|| s.clone());
        }
        startup_timeout(async {
            while self.starting.peek().contains(&key) {
                futures_timer::Delay::new(std::time::Duration::from_millis(25)).await;
            }
        })
        .await?;
        if let Some(session) = self.session(language, root) {
            return Some(session);
        }
        let spawn_lsp = ws.spawn_lsp()?;
        let mut this = self;
        this.starting.with_mut(|v| v.push(key.clone()));
        let mut startup = StartingSession {
            starting: this.starting,
            key: key.clone(),
            tasks: Vec::new(),
        };
        ws.processes.lsp_status.set(Some(t!(
            ws,
            L,
            "lsp-starting",
            language = language.to_string()
        )));
        let transport =
            match startup_timeout(spawn_lsp(language.to_string(), root.to_string())).await {
                Some(Ok(t)) => t,
                Some(Err(e)) => {
                    ws.processes
                        .lsp_status
                        .set(Some(format!("{language}: {e}")));
                    this.starting.with_mut(|v| v.retain(|k| k != &key));
                    return None;
                }
                None => {
                    ws.processes.lsp_status.set(Some(t!(
                        ws,
                        L,
                        "lsp-init-failed",
                        language = language.to_string(),
                        error = "startup timeout".to_string()
                    )));
                    return None;
                }
            };
        let (session, mut events) = LspSession::new(transport);
        startup
            .tasks
            .push(dioxus::core::spawn_forever(session.clone().pump()));
        let lang = language.to_string();
        let event_key = key.clone();
        let event_session = session.clone();
        startup.tasks.push(dioxus::core::spawn_forever(async move {
            while let Some(ev) = events.next().await {
                if this
                    .sessions
                    .peek()
                    .get(&event_key)
                    .is_some_and(|current| !current.same_connection(&event_session))
                {
                    continue;
                }
                match ev {
                    LspEvent::Initialized { server } => ws.processes.lsp_status.set(Some(t!(
                        ws,
                        L,
                        "lsp-ready",
                        server = server.clone()
                    ))),
                    LspEvent::Status(s) => {
                        ws.processes.lsp_status.set(Some(format!("{lang}: {s}")))
                    }
                    LspEvent::Diagnostics {
                        uri,
                        version,
                        diagnostics,
                    } => {
                        this.publish_diagnostics(&uri, version, diagnostics);
                    }
                    LspEvent::Closed => {
                        this.sessions.with_mut(|sessions| {
                            sessions.remove(&event_key);
                        });
                        let uris: Vec<_> = this
                            .documents
                            .peek()
                            .iter()
                            .filter(|(_, doc)| doc.key == event_key)
                            .map(|(uri, _)| uri.clone())
                            .collect();
                        this.connections.with_mut(|connections| {
                            for uri in &uris {
                                connections.remove(uri);
                            }
                        });
                        this.documents.with_mut(|documents| {
                            for uri in &uris {
                                documents.remove(uri);
                            }
                        });
                        this.diagnostics.with_mut(|values| {
                            for uri in uris {
                                values.remove(&uri);
                            }
                        });
                        this.diagnostic_versions.with_mut(|versions| {
                            versions.retain(|uri, _| this.diagnostics.peek().contains_key(uri));
                        });
                        ws.processes.lsp_status.set(Some(t!(
                            ws,
                            L,
                            "lsp-exited",
                            language = lang.clone()
                        )));
                        break;
                    }
                }
            }
        }));
        match startup_timeout(session.initialize(root)).await {
            Some(Ok(_)) => {
                startup.tasks.clear();
                this.sessions.with_mut(|m| {
                    m.insert(key.clone(), session.clone());
                });
                this.starting.with_mut(|v| v.retain(|k| k != &key));
                Some(session)
            }
            result => {
                let e = result
                    .and_then(Result::err)
                    .unwrap_or_else(|| "initialization timeout".into());
                ws.processes.lsp_status.set(Some(t!(
                    ws,
                    L,
                    "lsp-init-failed",
                    language = language.to_string(),
                    error = e.to_string()
                )));
                this.starting.with_mut(|v| v.retain(|k| k != &key));
                None
            }
        }
    }
}

pub(crate) fn use_document(
    ws: Workspace,
    doc: Signal<moonkale_ext_api::Document>,
    manager: LspManager,
    identity: Option<(String, String, String)>,
) -> Signal<Option<LspSession>> {
    let mut output = use_signal(|| None::<LspSession>);
    let lease = use_hook(|| Rc::new(std::cell::RefCell::new(None::<(String, LspSession)>)));
    let pending = use_hook(|| Rc::new(std::cell::Cell::new(false)));
    let mut retry = use_signal(|| 0u64);
    let alive = use_hook(|| Rc::new(std::cell::Cell::new(true)));
    let release = lease.clone();
    let drop_alive = alive.clone();
    use_drop(move || {
        drop_alive.set(false);
        if let Some((uri, session)) = release.borrow_mut().take() {
            manager.close_connection(&uri, &session);
        }
    });
    use_effect(move || {
        let _ = retry();
        let Some((language, root, uri)) = identity.clone() else {
            return;
        };
        let current = manager
            .sessions
            .read()
            .get(&LspManager::key(&language, &root))
            .cloned();
        if lease.borrow().as_ref().is_some_and(|(_, old)| {
            current
                .as_ref()
                .is_some_and(|current| old.same_connection(current))
        }) || pending.get()
        {
            return;
        }
        if let Some((old_uri, old)) = lease.borrow_mut().take() {
            manager.close_connection(&old_uri, &old);
        }
        output.set(None);
        pending.set(true);
        let lease = lease.clone();
        let pending = pending.clone();
        let alive = alive.clone();
        spawn(async move {
            let session = manager.ensure(ws, &language, &root).await;
            pending.set(false);
            if !alive.get() {
                return;
            }
            let obsolete = session.as_ref().is_some_and(|session| {
                !manager
                    .session(&language, &root)
                    .is_some_and(|current| session.same_connection(&current))
            });
            if obsolete {
                retry.with_mut(|value| *value = value.wrapping_add(1));
                return;
            }
            if let Some(session) = session.filter(|session| {
                manager
                    .session(&language, &root)
                    .is_some_and(|current| session.same_connection(&current))
            }) {
                let document = doc.peek();
                manager.open_document(
                    session.clone(),
                    &uri,
                    &language,
                    &root,
                    document.text.clone(),
                    document.saved.clone(),
                );
                *lease.borrow_mut() = Some((uri, session.clone()));
                output.set(Some(session));
            }
        });
    });
    output
}

/// Apply checked, unsaved, undoable edits through the shared Workspace path.
pub async fn apply_workspace_edit(
    ws: Workspace,
    root: &str,
    edit: &moonkale_lsp::WorkspaceEdit,
) -> Result<usize, String> {
    let active = *ws.docs.active.peek();
    let origin = ws
        .docs
        .open
        .peek()
        .iter()
        .filter_map(|(id, doc)| {
            let doc = doc.peek();
            (doc.node.source.as_str() == format!("folder:{root}")).then(|| (*id, doc.node.clone()))
        })
        .min_by_key(|(id, _)| Some(*id) != active)
        .map(|(_, node)| node)
        .ok_or("origin document is not open")?;
    crate::native_workspace_edit::apply_guarded(
        ws,
        &origin,
        LspManager::for_workspace(ws),
        edit,
        || true,
    )
    .await
}

#[cfg(test)]
mod ownership_tests {
    use super::*;
    use moonkale_core::SourceError;
    use moonkale_ext_api::{
        AttachFuture, FolderAccess, OpenFolderFuture, OpenOptions, WorkspaceConfig,
    };
    use std::cell::RefCell;

    fn open(_: String, _: OpenOptions) -> OpenFolderFuture {
        Box::pin(async { Err(SourceError::NotFound) })
    }
    fn attach(_: moonkale_core::SourceDescriptor) -> AttachFuture {
        Box::pin(async { Err(SourceError::NotFound) })
    }
    fn workspace() -> Workspace {
        Workspace::new(WorkspaceConfig {
            folders: FolderAccess {
                open,
                pick: None,
                attach,
                reopen_last: false,
                openers: &moonkale_core::source::opener::NO_OPENERS,
            },
            processes: Default::default(),
            persistence: Default::default(),
            network: Default::default(),
            runtimes: Default::default(),
            services: &[],
        })
    }
    struct Transport(Rc<RefCell<Vec<serde_json::Value>>>);
    impl moonkale_lsp::LspTransport for Transport {
        fn send(&self, message: String) {
            self.0
                .borrow_mut()
                .push(serde_json::from_str(&message).unwrap());
        }
        fn take_incoming(&mut self) -> Option<futures_channel::mpsc::UnboundedReceiver<String>> {
            None
        }
    }
    fn app() -> Element {
        rsx! { div {} }
    }

    #[test]
    fn editor_implementations_share_uri_lifecycle_and_monotonic_versions() {
        let mut dom = VirtualDom::new(app);
        dom.rebuild_in_place();
        dom.in_scope(ScopeId::ROOT, || {
            let ws = workspace();
            let native = LspManager::for_workspace(ws);
            let codemirror = LspManager::for_workspace(ws);
            assert!(native == codemirror);
            assert!(native != LspManager::for_workspace(workspace()));
            let sent = Rc::new(RefCell::new(Vec::new()));
            let (session, _) = LspSession::new(Box::new(Transport(sent.clone())));
            let uri = "file:///tmp/project/main.rs";
            native.open_document(
                session.clone(),
                uri,
                "rust",
                "/tmp/project",
                "old".into(),
                "old".into(),
            );
            codemirror.open_document(
                session,
                uri,
                "rust",
                "/tmp/project",
                "old".into(),
                "old".into(),
            );
            codemirror.update_document(uri, "first", "old");
            native.update_document(uri, "second", "old");
            native.close_document(uri);
            assert_eq!(codemirror.peek_document_version(uri), Some(3));
            codemirror.close_document(uri);
            assert!(native.peek_document_version(uri).is_none());
            let sent = sent.borrow();
            let methods: Vec<_> = sent
                .iter()
                .map(|message| message["method"].as_str().unwrap())
                .collect();
            assert_eq!(
                methods,
                [
                    "textDocument/didOpen",
                    "textDocument/didChange",
                    "textDocument/didChange",
                    "textDocument/didClose"
                ]
            );
            assert_eq!(sent[1]["params"]["textDocument"]["version"], 2);
            assert_eq!(sent[2]["params"]["textDocument"]["version"], 3);
        });
    }

    #[test]
    fn encoded_aliases_and_old_connection_releases_preserve_current_lease() {
        let mut dom = VirtualDom::new(app);
        dom.rebuild_in_place();
        dom.in_scope(ScopeId::ROOT, || {
            let manager = LspManager::for_workspace(workspace());
            let (old, _) = LspSession::new(Box::new(Transport(Rc::new(RefCell::new(Vec::new())))));
            let sent = Rc::new(RefCell::new(Vec::new()));
            let (current, _) = LspSession::new(Box::new(Transport(sent.clone())));
            let uri = "file:///tmp/project/foo+bar.rs";
            manager.open_document(
                current.clone(),
                uri,
                "rust",
                "/tmp/project",
                "old".into(),
                "old".into(),
            );
            assert_eq!(
                manager.peek_document_version("file:///tmp/project/foo%2Bbar.rs"),
                Some(1)
            );
            manager.close_connection(uri, &old);
            assert!(manager
                .peek_document_session(uri)
                .unwrap()
                .same_connection(&current));
            manager.update_document("file:///tmp/project/foo%2bbar.rs", "new", "old");
            assert_eq!(manager.peek_document_version(uri), Some(2));
            assert_eq!(sent.borrow().len(), 2);
            manager.close_connection(uri, &current);
            assert!(manager.peek_document_session(uri).is_none());
            assert_eq!(
                sent.borrow().last().unwrap()["method"],
                "textDocument/didClose"
            );
        });
    }
    #[test]
    fn alias_diagnostics_use_current_version_and_other_edits_do_not_notify_connections() {
        let mut dom = VirtualDom::new(app);
        dom.rebuild_in_place();
        dom.in_scope(ScopeId::ROOT, || {
            let manager = LspManager::for_workspace(workspace());
            let (session, _) =
                LspSession::new(Box::new(Transport(Rc::new(RefCell::new(Vec::new())))));
            let uri = "file:///tmp/project/foo%2Bbar.rs";
            manager.open_document(
                session.clone(),
                uri,
                "rust",
                "/tmp/project",
                "old".into(),
                "old".into(),
            );
            manager.open_document(
                session,
                "file:///tmp/project/other.rs",
                "rust",
                "/tmp/project",
                "old".into(),
                "old".into(),
            );
            let (context, mut changed) = dioxus::core::ReactiveContext::new();
            context.reset_and_run_in(|| {
                assert!(manager.document_session(uri).is_some());
            });
            manager.update_document("file:///tmp/project/other.rs", "new", "old");
            use futures_util::FutureExt;
            assert!(changed.next().now_or_never().is_none());
            let diagnostic = Diagnostic {
                raw: None,
                line: 0,
                col: 0,
                end_line: 0,
                end_col: 1,
                severity: "error",
                message: "alias".into(),
            };
            manager.publish_diagnostics(
                "file:///tmp/project/foo+bar.rs",
                Some(99),
                vec![diagnostic.clone()],
            );
            assert!(!manager.diagnostics.peek().contains_key(uri));
            manager.publish_diagnostics(
                "file:///tmp/project/foo+bar.rs",
                Some(1),
                vec![diagnostic.clone()],
            );
            assert_eq!(manager.diagnostics.peek()[uri], vec![diagnostic]);
            manager.update_document(uri, "new", "old");
            assert!(!manager.diagnostics.peek().contains_key(uri));
        });
    }

    #[test]
    fn typing_retains_unversioned_diagnostics_but_invalidates_versioned_ranges() {
        let mut dom = VirtualDom::new(app);
        dom.rebuild_in_place();
        dom.in_scope(ScopeId::ROOT, || {
            let mut manager = LspManager::for_workspace(workspace());
            let sent = Rc::new(RefCell::new(Vec::new()));
            let (session, _) = LspSession::new(Box::new(Transport(sent)));
            let uri = "file:///tmp/project/main.rs";
            manager.open_document(
                session,
                uri,
                "rust",
                "/tmp/project",
                "old".into(),
                "old".into(),
            );
            let diagnostic = Diagnostic {
                raw: None,
                line: 0,
                col: 0,
                end_line: 0,
                end_col: 3,
                severity: "error",
                message: "cargo check".into(),
            };
            manager.diagnostics.with_mut(|values| {
                values.insert(uri.into(), vec![diagnostic.clone()]);
            });
            manager.diagnostic_versions.with_mut(|values| {
                values.insert(uri.into(), None);
            });
            manager.update_document(uri, "first", "old");
            assert_eq!(manager.diagnostics.peek()[uri], vec![diagnostic]);
            manager.diagnostic_versions.with_mut(|values| {
                values.insert(uri.into(), Some(2));
            });
            manager.update_document(uri, "second", "old");
            assert!(!manager.diagnostics.peek().contains_key(uri));
        });
    }
}
