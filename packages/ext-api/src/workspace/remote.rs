//! `Workspace`: remote folders over SSH, a server's client, server-side agent sessions. Split out of `workspace.rs` in Milestone 18 phase 3b
//! (no API change; phase 3c turns the areas into services).

use super::*;

impl Workspace {
    // ---- Remote folders (Milestone 11) ----

    /// The server's agent sessions, when the sources live there (Milestone 12).
    pub fn agent_sessions(&self) -> Option<AgentSessions> {
        // Opt-in (`agent.on_server`), except on a platform without a local
        // provider (the phone), which always uses them when connected.
        if !self.settings.resolved.read().agent.on_server && self.config.runtimes.llm.is_some() {
            return None;
        }
        self.config
            .network
            .agent_sessions
            .filter(|a| (a.available)())
    }

    pub fn has_remote(&self) -> bool {
        self.config.network.remote.is_some()
    }

    /// Host aliases from `~/.ssh/config` (desktop), for the dialog.
    pub fn remote_hosts(&self) -> Vec<String> {
        self.config
            .network
            .remote
            .map(|r| (r.hosts)())
            .unwrap_or_default()
    }

    /// Saved SSH connections (Milestone 15; the user file).
    pub fn remote_saved(&self) -> Vec<crate::settings::SavedConnection> {
        self.settings.resolved.peek().remote_saved.clone()
    }

    /// Save (or replace by name) a connection in the user file.
    pub async fn save_remote(self, name: String, host: String, path: String) {
        let name = name.trim().to_string();
        if name.is_empty() {
            return;
        }
        self.update_user_settings(move |f| {
            f.remote.saved.retain(|c| c.name != name);
            f.remote
                .saved
                .push(crate::settings::SavedConnection { name, host, path });
            f.remote.saved.sort_by(|a, b| a.name.cmp(&b.name));
        })
        .await;
    }

    pub async fn forget_remote(self, name: String) {
        self.update_user_settings(move |f| f.remote.saved.retain(|c| c.name != name))
            .await;
    }

    /// Was `id` opened through the remote session?
    pub fn is_remote_source(&self, id: &SourceId) -> bool {
        self.remote
            .ssh
            .peek()
            .as_ref()
            .is_some_and(|r| r.sources.contains(id))
    }

    /// Start a session to `host` and open `path` there when it is up. The
    /// `ssh` process becomes a terminal tab (its prompts are answered
    /// there); phases arrive on a channel and drive the status bar.
    pub fn open_remote(mut self, host: String, path: String) {
        let Some(remote) = self.config.network.remote else {
            self.set_status("Remote folders are not available on this platform");
            return;
        };
        if self.remote.ssh.peek().is_some() {
            self.close_remote();
        }
        let host = host.trim().to_string();
        let path = path.trim().to_string();
        if host.is_empty() || path.is_empty() {
            self.set_status("Remote: a host and a path are needed");
            return;
        }
        let (tx, mut rx) = futures_channel::mpsc::unbounded::<crate::remote::RemotePhase>();
        let sink: crate::remote::PhaseSink = Box::new(move |p| {
            let _ = tx.unbounded_send(p);
        });
        let (backend, session) = match (remote.open)(host.clone(), path.clone(), sink) {
            Ok(x) => x,
            Err(e) => {
                self.set_status(format!("Remote: {e}"));
                return;
            }
        };
        self.remote.ssh.set(Some(crate::remote::RemoteState {
            host: host.clone(),
            path: path.clone(),
            phase: crate::remote::RemotePhase::Connecting,
            session,
            sources: Vec::new(),
        }));
        let title = backend.title();
        self.adopt_terminal(moonkale_terminal::Session {
            id: moonkale_terminal::SessionId::fresh(),
            title,
            cwd: None,
            backend,
        });
        self.set_status(format!("Remote: connecting to {host}…"));
        spawn(async move {
            use crate::remote::RemotePhase as P;
            use futures_util::StreamExt;
            while let Some(p) = rx.next().await {
                if self.remote.ssh.peek().is_none() {
                    break; // closed meanwhile
                }
                self.remote.ssh.with_mut(|r| {
                    if let Some(r) = r {
                        r.phase = p.clone();
                    }
                });
                match &p {
                    P::Prompt(line) => {
                        self.set_status(format!("ssh {host}: {line} — answer in the terminal"));
                        self.dispatch(Command::ShowPanel("terminal"));
                    }
                    P::Uploading => self.set_status(format!(
                        "Remote: {host} has no Moonkale server yet — uploading it (once per version)…"
                    )),
                    P::Starting => self.set_status(format!("Remote: starting the server on {host}…")),
                    P::Ready => {
                        self.set_status(format!("Remote: connected to {host}, opening {path}…"));
                        if let Err(e) = self.open_folder(path.clone()).await {
                            self.set_status(format!("Remote: {host} is connected but {path} did not open: {e}"));
                        }
                    }
                    P::Failed(e) => self.set_status(format!("Remote: {e}")),
                    P::Connecting | P::Closed => {}
                }
                if p.is_final() {
                    break;
                }
            }
        });
    }

    /// End the session: the sources it opened go, `ssh` and the remote
    /// server with it, and the desktop is local again.
    pub fn close_remote(&mut self) {
        let Some(state) = self.remote.ssh.take() else {
            return;
        };
        state.session.close();
        let closing = state.sources.clone();
        let docs: Vec<NodeId> = self
            .docs
            .open
            .peek()
            .iter()
            .filter(|(_, d)| closing.contains(&d.peek().node.source))
            .map(|(n, _)| *n)
            .collect();
        for n in docs {
            self.close_node(n);
        }
        if !closing.is_empty() {
            self.sources
                .open
                .with_mut(|v| v.retain(|s| !closing.contains(&s.descriptor.id)));
            if self
                .settings
                .folder
                .peek()
                .as_ref()
                .is_some_and(|f| closing.contains(f))
            {
                self.settings.folder.set(None);
                self.settings
                    .workspace
                    .set(crate::settings::SettingsFile::new());
                self.resolve_settings();
            }
            self.sources.graph_epoch.with_mut(|e| *e += 1);
        }
        self.set_status(format!("Remote: disconnected from {}", state.label()));
    }

    // ---- A server's client (Milestone 12) ----

    pub fn has_server_client(&self) -> bool {
        self.config.network.server.is_some()
    }

    /// Become `url`'s client and open its root folder.
    pub async fn connect_server(mut self, url: String, token: Option<String>) {
        let Some(sc) = self.config.network.server else {
            self.set_status("Connecting to a server is not available on this platform");
            return;
        };
        let url = url.trim().trim_end_matches('/').to_string();
        if url.is_empty() {
            self.set_status("Server: a URL is needed");
            return;
        }
        if self.remote.server.peek().is_some() {
            self.disconnect_server();
        }
        if let Err(e) = (sc.connect)(url.clone(), token.filter(|t| !t.trim().is_empty())) {
            self.set_status(format!("Server: {e}"));
            return;
        }
        self.remote.server.set(Some((url.clone(), Vec::new())));
        self.set_status(format!("Connected to {url}; opening its folder…"));
        if let Err(e) = self.open_folder(String::new()).await {
            self.set_status(format!(
                "Server {url}: connected, but its folder did not open: {e}"
            ));
        }
    }

    /// Drop the connection and the sources it opened.
    pub fn disconnect_server(&mut self) {
        let Some((url, ids)) = self.remote.server.take() else {
            return;
        };
        if let Some(sc) = self.config.network.server {
            (sc.disconnect)();
        }
        let docs: Vec<NodeId> = self
            .docs
            .open
            .peek()
            .iter()
            .filter(|(_, d)| ids.contains(&d.peek().node.source))
            .map(|(n, _)| *n)
            .collect();
        for n in docs {
            self.close_node(n);
        }
        if !ids.is_empty() {
            self.sources
                .open
                .with_mut(|v| v.retain(|s| !ids.contains(&s.descriptor.id)));
            if self
                .settings
                .folder
                .peek()
                .as_ref()
                .is_some_and(|f| ids.contains(f))
            {
                self.settings.folder.set(None);
                self.settings
                    .workspace
                    .set(crate::settings::SettingsFile::new());
                self.resolve_settings();
            }
            self.sources.graph_epoch.with_mut(|e| *e += 1);
        }
        self.set_status(format!("Disconnected from {url}"));
    }
}

/// Links to other machines: an SSH remote folder, a server this app is a client of. (Milestone 18 phase 3c: the workspace's state, grouped by area.)
#[derive(Clone, Copy)]
pub struct RemoteLinks {
    /// The window's remote session, if any (Milestone 11).
    pub ssh: Signal<Option<crate::remote::RemoteState>>,
    /// The server this app is a client of (Milestone 12): label and the
    /// sources opened through it.
    pub server: Signal<Option<(String, Vec<SourceId>)>>,
}

impl RemoteLinks {
    pub(super) fn new() -> Self {
        Self {
            ssh: Signal::new_in_scope(None, ScopeId::ROOT),
            server: Signal::new_in_scope(None, ScopeId::ROOT),
        }
    }
}
