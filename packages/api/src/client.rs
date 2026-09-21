//! The client half of a Moonkale server, usable from any build that does not
//! *have* the `server` feature (the web client, the desktop app): the server
//! functions become HTTP calls to [`dioxus::fullstack::get_server_url`], and
//! this module owns that URL plus the bearer token (Milestone 11).
//!
//! On the web the URL is the page's origin and nothing here needs calling.
//! On the desktop, [`connect`] points every server function at a remote
//! Moonkale server (a machine reached over SSH, or a server on the LAN) and
//! the platform's `WorkspaceConfig` callbacks dispatch on [`active`]:
//! sources, terminal, LSP, git and Typst go to the server; the LLM provider
//! stays local, so API keys never leave the machine that owns them.

use moonkale_core::{Source, SourceDescriptor};
use moonkale_ext_api::git::{GitRequest, GitResponse};
use moonkale_ext_api::{AttachFuture, OpenFolderFuture, OpenOptions, SettingsFuture};
use std::sync::{Arc, RwLock};

/// The server this process talks to, when not its own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Remote {
    /// `http://127.0.0.1:41234` — the local end of an SSH forward, or a LAN address.
    pub url: String,
    /// What the user sees: `daniel@build-box:/srv/code` or the URL.
    pub label: String,
}

static REMOTE: RwLock<Option<Remote>> = RwLock::new(None);

/// Point every server function at `url`, with `token` as the bearer.
/// Not for the web client (its URL is the page's origin).
///
/// The server functions themselves always call the process's fixed server
/// URL — on the desktop the loopback relay installed before launch
/// ([`crate::relay::install`]), because dioxus's URL is a `OnceLock`
/// (P-098) — and the relay pipes to whatever this function last set.
pub fn connect(url: &str, token: Option<&str>, label: &str) {
    let url = url.trim_end_matches('/').to_string();
    let mut headers = http::HeaderMap::new();
    if let Some(t) = token {
        if let Ok(v) = http::HeaderValue::from_str(&format!("Bearer {t}")) {
            headers.insert(http::header::AUTHORIZATION, v);
        }
    }
    dioxus::fullstack::set_request_headers(headers);
    *REMOTE.write().unwrap() = Some(Remote {
        url,
        label: label.to_string(),
    });
}

/// Back to local sources; server functions are not called any more.
pub fn disconnect() {
    dioxus::fullstack::clear_request_headers();
    *REMOTE.write().unwrap() = None;
}

/// The connected server, if any.
pub fn active() -> Option<Remote> {
    REMOTE.read().unwrap().clone()
}

// ---- `WorkspaceConfig` callbacks over the server (the web client's wiring) ----

pub fn open_folder(path: String, options: OpenOptions) -> OpenFolderFuture {
    Box::pin(async move {
        crate::RemoteSource::open_folder(&path, options.embed)
            .await
            .map(|v| {
                v.into_iter()
                    .map(|s| Arc::new(s) as Arc<dyn Source>)
                    .collect()
            })
    })
}

/// Another window already opened this source on the server: wrap its descriptor.
pub fn attach_source(descriptor: SourceDescriptor) -> AttachFuture {
    Box::pin(async move {
        Ok(Arc::new(crate::RemoteSource::from_descriptor(descriptor)) as Arc<dyn Source>)
    })
}

pub fn spawn_terminal(
    cwd: Option<String>,
    cols: u16,
    rows: u16,
) -> moonkale_terminal::SpawnTerminalFuture {
    Box::pin(async move {
        crate::RemoteTerminal::connect(cwd, cols, rows)
            .await
            .map(|t| Box::new(t) as Box<dyn moonkale_terminal::TerminalBackend>)
    })
}

pub fn compile_typst(
    root: String,
    main_rel: String,
    text: String,
) -> moonkale_ext_api::CompileTypstFuture {
    Box::pin(async move {
        crate::compile_typst(root, main_rel, text)
            .await
            .unwrap_or_else(|e| Err(vec![e.to_string()]))
    })
}

pub fn spawn_lsp(language: String, root: String) -> moonkale_lsp::LspTransportFuture {
    Box::pin(async move {
        crate::RemoteLsp::connect(language, root)
            .await
            .map(|t| Box::new(t) as Box<dyn moonkale_lsp::LspTransport>)
    })
}

pub fn git(root: String, req: GitRequest) -> SettingsFuture<GitResponse> {
    Box::pin(async move {
        match crate::git_run(root, req).await {
            Ok(r) => r,
            Err(e) => Err(e.to_string()),
        }
    })
}

pub fn wasm_list(folder: Option<String>) -> SettingsFuture<Vec<moonkale_ext_host::WasmManifest>> {
    Box::pin(async move {
        crate::list_wasm_extensions(folder)
            .await
            .map_err(|e| e.to_string())
    })
}

pub fn wasm_run(
    ext: String,
    command: String,
    args: serde_json::Value,
    granted: Vec<String>,
) -> SettingsFuture<String> {
    Box::pin(async move {
        crate::run_wasm_command(ext, command, args, granted)
            .await
            .map_err(|e| e.to_string())?
    })
}

// ---- Agent sessions on the server (Milestone 12) ----

pub fn agent_available() -> bool {
    active().is_some()
}

pub fn agent_list(folder: String) -> SettingsFuture<Vec<moonkale_llm::sessions::SessionSummary>> {
    Box::pin(async move {
        crate::agent_sessions::agent_list(folder)
            .await
            .map_err(|e| e.to_string())
    })
}

pub fn agent_send(
    session: Option<String>,
    folder: String,
    text: String,
    settings: moonkale_llm::sessions::TurnSettings,
) -> SettingsFuture<String> {
    Box::pin(async move {
        crate::agent_sessions::agent_send(session, folder, text, settings)
            .await
            .map_err(|e| e.to_string())
    })
}

pub fn agent_events(
    session: String,
    since: usize,
) -> SettingsFuture<moonkale_llm::sessions::SessionState> {
    Box::pin(async move {
        crate::agent_sessions::agent_events(session, since)
            .await
            .map_err(|e| e.to_string())
    })
}

pub fn agent_approve(session: String, call_id: String, allow: bool) -> SettingsFuture<()> {
    Box::pin(async move {
        crate::agent_sessions::agent_approve(session, call_id, allow)
            .await
            .map_err(|e| e.to_string())
    })
}

/// The `WorkspaceConfig` half for server sessions: `available` says whether
/// the sources are a server's — always on the web, when connected on the desktop.
pub fn agent_sessions(available: fn() -> bool) -> moonkale_ext_api::AgentSessions {
    moonkale_ext_api::AgentSessions {
        available,
        list: agent_list,
        send: agent_send,
        events: agent_events,
        approve: agent_approve,
    }
}

/// A random session token: 32 bytes from the OS, hex — what the remote
/// server is told over stdin and what every request carries as the bearer.
pub fn session_token() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("OS randomness");
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connect_and_disconnect_track_the_remote() {
        assert!(active().is_none());
        // Installed once per process (a OnceLock in dioxus-fullstack).
        #[cfg(not(target_arch = "wasm32"))]
        {
            let relay = crate::relay::install().unwrap();
            assert!(relay.starts_with("http://127.0.0.1:"));
            dioxus::fullstack::set_server_url(relay.leak());
        }
        connect("http://127.0.0.1:9/", Some("abc"), "test");
        let r = active().unwrap();
        assert_eq!(r.url, "http://127.0.0.1:9");
        assert_eq!(
            dioxus::fullstack::get_request_headers()
                .get(http::header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok()),
            Some("Bearer abc")
        );
        disconnect();
        assert!(active().is_none());
        let t = session_token();
        assert_eq!(t.len(), 64);
        assert!(t.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
