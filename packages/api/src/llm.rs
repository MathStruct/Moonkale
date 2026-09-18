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
    Complete { request: Request },
    Embed { texts: Vec<String> },
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

/// The provider as an embedder for the index, when an embedding model is
/// configured (`MOONKALE_EMBED_MODEL`; the mock always embeds).
#[cfg(feature = "server")]
pub(crate) fn embedder() -> Option<std::sync::Arc<dyn Provider>> {
    use std::sync::{Arc, OnceLock};
    static EMBEDDER: OnceLock<Option<Arc<dyn Provider>>> = OnceLock::new();
    EMBEDDER
        .get_or_init(|| {
            let cfg = moonkale_llm::Config::from_env();
            cfg.embed_model
                .is_some()
                .then(|| Arc::from(moonkale_llm::config::build(&cfg)))
        })
        .clone()
}

#[post("/api/llm/info")]
pub async fn llm_info() -> Result<ProviderInfo, ServerFnError> {
    let p = provider();
    Ok(ProviderInfo {
        name: p.name(),
        model: p.model(),
        supports_embed: p.supports_embed(),
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
        let p = provider();
        match msg {
            ClientMsg::Complete { request } => {
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
            ClientMsg::Embed { texts } => {
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
}

#[cfg(target_arch = "wasm32")]
impl RemoteProvider {
    pub async fn connect() -> Result<Self, String> {
        let info = llm_info().await.map_err(|e| e.to_string())?;
        Ok(Self { info })
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
            let msg = serde_json::to_string(&ClientMsg::Complete { request }).unwrap();
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
        Box::pin(async move {
            let socket = llm_socket(WebSocketOptions::new())
                .await
                .map_err(|e| e.to_string())?;
            socket
                .send(Frame(
                    serde_json::to_string(&ClientMsg::Embed { texts }).unwrap(),
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
