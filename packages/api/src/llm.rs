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
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex, OnceLock};
    static CACHE: OnceLock<Mutex<HashMap<moonkale_llm::LlmSettings, Arc<dyn Provider>>>> =
        OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut map = cache.lock().unwrap();
    if let Some(p) = map.get(settings) {
        return Some(p.clone());
    }
    let key = moonkale_llm::secrets::resolve(&settings.secret);
    let cfg = moonkale_llm::Config::from_settings(settings, key);
    eprintln!(
        "moonkale: llm provider {} (from client settings)",
        cfg.label()
    );
    let p: Arc<dyn Provider> = Arc::from(moonkale_llm::config::build(&cfg));
    map.insert(settings.clone(), p.clone());
    Some(p)
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
    let owned = settings.as_ref().and_then(provider_for);
    let p: &dyn Provider = match &owned {
        Some(p) => p.as_ref(),
        None => provider(),
    };
    let has_key = settings
        .as_ref()
        .map(|s| {
            s.provider == "mock"
                || s.provider == "ollama"
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
            ClientMsg::Complete { request, settings } => {
                let owned = settings.as_ref().and_then(provider_for);
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
                let owned = settings.as_ref().and_then(provider_for);
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
