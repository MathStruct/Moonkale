//! LLM provider relay: the server holds the keys and runs the HTTP
//! providers; the web client sees a `Provider` whose calls travel over a
//! websocket. One socket per completion / embedding call — simplest
//! possible concurrency model, and completions are long-lived anyway.
//! Same dev-server caveats as terminals and LSP (no auth).

use dioxus::fullstack::{WebSocketOptions, Websocket};
use dioxus::prelude::*;
#[cfg(any(feature = "server", target_arch = "wasm32"))]
use moonkale_llm::Provider;
use moonkale_llm::{Event, Request};
use serde::{Deserialize, Serialize};

#[cfg_attr(not(any(feature = "server", target_arch = "wasm32")), allow(dead_code))]
#[derive(Serialize, Deserialize)]
pub struct Frame(pub String);

// The desktop build compiles this file for the server-function stubs only.
#[cfg_attr(not(any(feature = "server", target_arch = "wasm32")), allow(dead_code))]
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ClientMsg {
    Complete {
        request: Request,
        #[serde(default)]
        settings: Option<moonkale_llm::LlmSettings>,
    },
    Embed {
        texts: Vec<String>,
        #[serde(default)]
        settings: Option<moonkale_llm::LlmSettings>,
    },
}

#[cfg_attr(not(any(feature = "server", target_arch = "wasm32")), allow(dead_code))]
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ServerMsg {
    Event { event: Event },
    Embedding { vectors: Vec<Vec<f32>> },
    Error { message: String },
}

/// Provider identity for the status bar / panel header.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderInfo {
    pub name: String,
    pub model: String,
    pub supports_embed: bool,
    /// The named secret resolves on the server (or the kind needs none).
    #[serde(default = "yes")]
    pub has_key: bool,
}

fn yes() -> bool {
    true
}

/// Providers built from client settings, cached by settings; the secret is
/// resolved on the server (`moonkale_llm::secrets`, i.e. its environment or
/// its secrets file). With no settings (older clients) the server's own
/// environment decides, as in Milestone 4.
#[cfg(feature = "server")]
pub(crate) fn provider_for(
    settings: &moonkale_llm::LlmSettings,
) -> Option<std::sync::Arc<dyn Provider>> {
    provider_checked(settings)
        .map_err(|e| eprintln!("moonkale: llm provider refused: {e}"))
        .ok()
}

/// The server decides (Milestone 18 phase 4.5, audit #1): what a client's
/// provider settings may make the server do. The client chooses the kind,
/// model and secret *name*; the server alone decides where a secret goes and
/// which program runs.
/// - `claude-code`: the client's binary path is ignored — the server runs
///   `MOONKALE_CLAUDE_BIN`, else `claude` on its own `PATH`.
/// - A provider that sends the resolved secret to `base_url` (OpenAI-
///   compatible) may only use its built-in endpoint or one the server's
///   operator listed in `MOONKALE_LLM_ENDPOINTS` (comma-separated URL
///   prefixes).
#[cfg(feature = "server")]
pub fn server_side(
    settings: &moonkale_llm::LlmSettings,
) -> Result<moonkale_llm::LlmSettings, String> {
    let mut s = settings.clone();
    if s.provider == "claude-code" {
        s.base_url.clear();
    }
    let sends_key = s.provider == "openai" && moonkale_llm::secrets::available(&s.secret);
    let url = s.base_url.trim();
    if sends_key && !url.is_empty() && url.trim_end_matches('/') != "https://api.openai.com/v1" {
        let allowed = std::env::var("MOONKALE_LLM_ENDPOINTS").unwrap_or_default();
        let listed = allowed
            .split(',')
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .any(|p| url.starts_with(p));
        if !listed {
            return Err(format!(
                "this server sends the secret {:?} only to the provider's own endpoint or one listed in MOONKALE_LLM_ENDPOINTS; {url} is not",
                s.secret
            ));
        }
    }
    Ok(s)
}

/// [`provider_for`] with the refusal of [`server_side`] as the error.
#[cfg(feature = "server")]
pub(crate) fn provider_checked(
    settings: &moonkale_llm::LlmSettings,
) -> Result<std::sync::Arc<dyn Provider>, String> {
    let settings = &server_side(settings)?;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex, OnceLock};
    static CACHE: OnceLock<Mutex<HashMap<moonkale_llm::LlmSettings, Arc<dyn Provider>>>> =
        OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut map = cache.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(p) = map.get(settings) {
        return Ok(p.clone());
    }
    // Every distinct settings value was kept forever (#14): a client could
    // grow the map without bound. A small cache, emptied when full.
    if map.len() >= 32 {
        map.clear();
    }
    let key = moonkale_llm::secrets::resolve(&settings.secret);
    let cfg = moonkale_llm::Config::from_settings(settings, key);
    eprintln!(
        "moonkale: llm provider {} (from client settings)",
        cfg.label()
    );
    let p: Arc<dyn Provider> = Arc::from(moonkale_llm::config::build(&cfg));
    map.insert(settings.clone(), p.clone());
    Ok(p)
}

#[cfg(feature = "server")]
pub(crate) fn provider() -> &'static dyn Provider {
    use std::sync::OnceLock;
    static PROVIDER: OnceLock<Box<dyn Provider>> = OnceLock::new();
    PROVIDER
        .get_or_init(|| {
            let cfg = moonkale_llm::Config::from_env();
            eprintln!("moonkale: llm provider {}", cfg.label());
            moonkale_llm::config::build(&cfg)
        })
        .as_ref()
}

#[post("/api/llm/info")]
pub async fn llm_info(
    settings: Option<moonkale_llm::LlmSettings>,
) -> Result<ProviderInfo, ServerFnError> {
    let owned = settings
        .as_ref()
        .map(provider_checked)
        .transpose()
        .map_err(ServerFnError::new)?;
    let p: &dyn Provider = match &owned {
        Some(p) => p.as_ref(),
        None => provider(),
    };
    let has_key = settings
        .as_ref()
        .map(|s| {
            s.provider == "mock"
                || s.provider == "ollama"
                || s.provider == "claude-code"
                || moonkale_llm::secrets::available(&s.secret)
        })
        .unwrap_or(true);
    Ok(ProviderInfo {
        name: p.name(),
        model: p.model(),
        supports_embed: p.supports_embed(),
        has_key,
    })
}

/// The provider's readiness on the server (Milestone 15): for `claude-code`
/// the CLI's version and login there.
#[post("/api/llm/status")]
pub async fn llm_status(
    settings: moonkale_llm::LlmSettings,
) -> Result<Option<moonkale_llm::ProviderStatus>, ServerFnError> {
    let p = provider_checked(&settings).map_err(ServerFnError::new)?;
    Ok(p.status().await)
}

#[get("/api/llm")]
pub async fn llm_socket(
    options: WebSocketOptions,
) -> Result<Websocket<Frame, Frame>, ServerFnError> {
    Ok(options.on_upgrade(|mut socket| async move {
        use futures_util::StreamExt;
        let Ok(Frame(first)) = socket.recv().await else {
            return;
        };
        let msg = match serde_json::from_str::<ClientMsg>(&first) {
            Ok(m) => m,
            Err(e) => {
                let _ = socket
                    .send(Frame(
                        serde_json::to_string(&ServerMsg::Error {
                            message: e.to_string(),
                        })
                        .unwrap(),
                    ))
                    .await;
                return;
            }
        };
        match msg {
            ClientMsg::Complete {
                mut request,
                settings,
            } => {
                // A turn acts in a folder (Milestone 12): inside the jail,
                // or the jail itself.
                request.cwd = match crate::state::jail_dir(request.cwd.as_deref()) {
                    Ok(dir) => Some(dir),
                    Err(e) => {
                        let _ = socket
                            .send(Frame(
                                serde_json::to_string(&ServerMsg::Error {
                                    message: format!("cwd refused: {e}"),
                                })
                                .unwrap(),
                            ))
                            .await;
                        return;
                    }
                };
                let owned = match settings.as_ref().map(provider_checked).transpose() {
                    Ok(o) => o,
                    Err(message) => {
                        let _ = socket
                            .send(Frame(
                                serde_json::to_string(&ServerMsg::Error { message }).unwrap(),
                            ))
                            .await;
                        return;
                    }
                };
                let p: &dyn Provider = match &owned {
                    Some(p) => p.as_ref(),
                    None => provider(),
                };
                let mut stream = p.complete(request);
                while let Some(event) = stream.next().await {
                    let done = matches!(event, Event::Done { .. } | Event::Error { .. });
                    if socket
                        .send(Frame(
                            serde_json::to_string(&ServerMsg::Event { event }).unwrap(),
                        ))
                        .await
                        .is_err()
                        || done
                    {
                        break;
                    }
                }
            }
            ClientMsg::Embed { texts, settings } => {
                let owned = match settings.as_ref().map(provider_checked).transpose() {
                    Ok(o) => o,
                    Err(message) => {
                        let _ = socket
                            .send(Frame(
                                serde_json::to_string(&ServerMsg::Error { message }).unwrap(),
                            ))
                            .await;
                        return;
                    }
                };
                let p: &dyn Provider = match &owned {
                    Some(p) => p.as_ref(),
                    None => provider(),
                };
                let out = match p.embed(texts).await {
                    Ok(vectors) => ServerMsg::Embedding { vectors },
                    Err(message) => ServerMsg::Error { message },
                };
                let _ = socket
                    .send(Frame(serde_json::to_string(&out).unwrap()))
                    .await;
            }
        }
    }))
}

/// Client-side provider over the relay (the web build; desktop runs
/// providers in-process).
#[cfg(target_arch = "wasm32")]
pub struct RemoteProvider {
    info: ProviderInfo,
    settings: moonkale_llm::LlmSettings,
}

#[cfg(target_arch = "wasm32")]
impl RemoteProvider {
    pub async fn connect(settings: moonkale_llm::LlmSettings) -> Result<Self, String> {
        let info = llm_info(Some(settings.clone()))
            .await
            .map_err(|e| e.to_string())?;
        if !info.has_key {
            return Err(format!(
                "no secret named {:?} on the server (MOONKALE_SECRET_… or its secrets file)",
                settings.secret
            ));
        }
        Ok(Self { info, settings })
    }
}

#[cfg(target_arch = "wasm32")]
impl Provider for RemoteProvider {
    fn status(&self) -> moonkale_llm::BoxFuture<Option<moonkale_llm::ProviderStatus>> {
        let settings = self.settings.clone();
        Box::pin(async move {
            llm_status(settings).await.ok().flatten().map(|mut s| {
                s.summary = format!("{} (on the server)", s.summary);
                // Logging in happens on the server machine, not from here.
                s.can_login = false;
                s
            })
        })
    }
    fn name(&self) -> String {
        format!("{} (server)", self.info.name)
    }
    fn model(&self) -> String {
        self.info.model.clone()
    }
    fn complete(&self, request: Request) -> moonkale_llm::EventStream {
        let (tx, rx) = futures_channel::mpsc::unbounded();
        let settings = self.settings.clone();
        spawn(async move {
            let socket = match llm_socket(WebSocketOptions::new()).await {
                Ok(s) => s,
                Err(e) => {
                    let _ = tx.unbounded_send(Event::Error {
                        message: e.to_string(),
                    });
                    return;
                }
            };
            let msg = serde_json::to_string(&ClientMsg::Complete {
                request,
                settings: Some(settings),
            })
            .unwrap();
            if let Err(e) = socket.send(Frame(msg)).await {
                let _ = tx.unbounded_send(Event::Error {
                    message: e.to_string(),
                });
                return;
            }
            while let Ok(Frame(m)) = socket.recv().await {
                match serde_json::from_str::<ServerMsg>(&m) {
                    Ok(ServerMsg::Event { event }) => {
                        let done = matches!(event, Event::Done { .. } | Event::Error { .. });
                        let _ = tx.unbounded_send(event);
                        if done {
                            return;
                        }
                    }
                    Ok(ServerMsg::Error { message }) => {
                        let _ = tx.unbounded_send(Event::Error { message });
                        return;
                    }
                    _ => {}
                }
            }
            let _ = tx.unbounded_send(Event::Error {
                message: "relay closed".into(),
            });
        });
        rx
    }
    fn embed(&self, texts: Vec<String>) -> moonkale_llm::BoxFuture<Result<Vec<Vec<f32>>, String>> {
        let settings = self.settings.clone();
        Box::pin(async move {
            let socket = llm_socket(WebSocketOptions::new())
                .await
                .map_err(|e| e.to_string())?;
            socket
                .send(Frame(
                    serde_json::to_string(&ClientMsg::Embed {
                        texts,
                        settings: Some(settings),
                    })
                    .unwrap(),
                ))
                .await
                .map_err(|e| e.to_string())?;
            let Frame(m) = socket.recv().await.map_err(|e| e.to_string())?;
            match serde_json::from_str::<ServerMsg>(&m).map_err(|e| e.to_string())? {
                ServerMsg::Embedding { vectors } => Ok(vectors),
                ServerMsg::Error { message } => Err(message),
                _ => Err("unexpected relay reply".into()),
            }
        })
    }
    fn supports_embed(&self) -> bool {
        self.info.supports_embed
    }
}

#[cfg(all(test, feature = "server"))]
mod server_decides {
    use super::server_side;
    use moonkale_llm::LlmSettings;

    fn openai(base_url: &str, secret: &str) -> LlmSettings {
        LlmSettings {
            provider: "openai".into(),
            model: String::new(),
            base_url: base_url.into(),
            embed_model: None,
            secret: secret.into(),
            options: Default::default(),
        }
    }

    /// Audit #1 (phase 4.5): a secret goes only where the server allows,
    /// and the client never chooses the program that runs.
    #[test]
    fn the_server_decides_where_a_secret_goes_and_what_runs() {
        std::env::set_var("MOONKALE_SECRET_E2E_KEY", "sk-test");
        std::env::remove_var("MOONKALE_LLM_ENDPOINTS");
        // The built-in endpoint, or none: fine.
        assert!(server_side(&openai("", "e2e_key")).is_ok());
        assert!(server_side(&openai("https://api.openai.com/v1/", "e2e_key")).is_ok());
        // Somewhere else with the secret: refused …
        let err = server_side(&openai("https://attacker.example/v1", "e2e_key")).unwrap_err();
        assert!(err.contains("MOONKALE_LLM_ENDPOINTS"), "{err}");
        // … unless the operator listed it.
        std::env::set_var(
            "MOONKALE_LLM_ENDPOINTS",
            "http://127.0.0.1:8000/, https://llm.lan/",
        );
        assert!(server_side(&openai("https://llm.lan/v1", "e2e_key")).is_ok());
        assert!(server_side(&openai("https://attacker.example/v1", "e2e_key")).is_err());
        std::env::remove_var("MOONKALE_LLM_ENDPOINTS");
        // No secret behind the name: nothing to leak, any endpoint (a local server).
        assert!(server_side(&openai("http://192.168.1.9:8000/v1", "no_such_secret")).is_ok());
        // Claude Code: the client's binary path is dropped.
        let mut cc = openai("/tmp/evil", "");
        cc.provider = "claude-code".into();
        assert_eq!(server_side(&cc).unwrap().base_url, "");
    }
}
