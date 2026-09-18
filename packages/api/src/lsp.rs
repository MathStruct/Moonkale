//! Remote language servers: spawned on the server, relayed 1:1 over a
//! websocket of JSON-RPC strings. Same dev-server caveats as terminals.

use dioxus::fullstack::{WebSocketOptions, Websocket};
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

/// One JSON-RPC frame (the server function macro rejects bare `String`).
#[derive(Serialize, Deserialize)]
pub struct Frame(pub String);

/// First client message: which server, for which root.
#[derive(Serialize, Deserialize)]
pub struct LspOpen {
    pub language: String,
    pub root: String,
}

#[get("/api/lsp")]
pub async fn lsp_socket(
    options: WebSocketOptions,
) -> Result<Websocket<Frame, Frame>, ServerFnError> {
    Ok(options.on_upgrade(|mut socket| async move {
        use futures_util::StreamExt;
        use moonkale_lsp::LspTransport;
        let Ok(Frame(first)) = socket.recv().await else {
            return;
        };
        let Ok(open) = serde_json::from_str::<LspOpen>(&first) else {
            let _ = socket
                .send(Frame(
                    r#"{"error":"expected {language, root}"}"#.to_string(),
                ))
                .await;
            return;
        };
        let root = match crate::state::jail_dir(Some(&open.root)) {
            Ok(r) => r,
            Err(e) => {
                let _ = socket.send(Frame(format!(r#"{{"error":"{e}"}}"#))).await;
                return;
            }
        };
        let Some(spec) = moonkale_lsp_local::discover::find(&open.language) else {
            let hint = moonkale_lsp_local::discover::install_hint(&open.language).unwrap_or("");
            let _ = socket
                .send(Frame(format!(
                    r#"{{"error":"no language server for {} on the server ({hint})"}}"#,
                    open.language
                )))
                .await;
            return;
        };
        let args: Vec<&str> = spec.args.iter().map(String::as_str).collect();
        let mut transport =
            match moonkale_lsp_local::StdioTransport::spawn(&spec.program, &args, &root) {
                Ok(t) => t,
                Err(e) => {
                    let _ = socket.send(Frame(format!(r#"{{"error":"{e}"}}"#))).await;
                    return;
                }
            };
        let mut incoming = transport.take_incoming().expect("fresh transport");
        loop {
            tokio::select! {
                msg = incoming.next() => match msg {
                    Some(m) => { if socket.send(Frame(m)).await.is_err() { break; } }
                    None => break,
                },
                msg = socket.recv() => match msg {
                    Ok(Frame(m)) => transport.send(m),
                    Err(_) => break,
                },
            }
        }
    }))
}

/// Client-side transport over the websocket above.
pub struct RemoteLsp {
    outgoing: futures_channel::mpsc::UnboundedSender<String>,
    incoming: Option<futures_channel::mpsc::UnboundedReceiver<String>>,
}

impl RemoteLsp {
    pub async fn connect(language: String, root: String) -> Result<Self, String> {
        use futures_util::StreamExt;
        let socket = lsp_socket(WebSocketOptions::new())
            .await
            .map_err(|e| e.to_string())?;
        socket
            .send(Frame(
                serde_json::to_string(&LspOpen { language, root }).unwrap(),
            ))
            .await
            .map_err(|e| e.to_string())?;
        let socket = std::rc::Rc::new(socket);
        let (out_tx, mut out_rx) = futures_channel::mpsc::unbounded::<String>();
        let (in_tx, in_rx) = futures_channel::mpsc::unbounded::<String>();
        let s = socket.clone();
        spawn(async move {
            while let Some(m) = out_rx.next().await {
                if s.send(Frame(m)).await.is_err() {
                    break;
                }
            }
        });
        let s = socket;
        spawn(async move {
            while let Ok(Frame(m)) = s.recv().await {
                if in_tx.unbounded_send(m).is_err() {
                    break;
                }
            }
        });
        Ok(Self {
            outgoing: out_tx,
            incoming: Some(in_rx),
        })
    }
}

impl moonkale_lsp::LspTransport for RemoteLsp {
    fn send(&self, message: String) {
        let _ = self.outgoing.unbounded_send(message);
    }
    fn take_incoming(&mut self) -> Option<futures_channel::mpsc::UnboundedReceiver<String>> {
        self.incoming.take()
    }
}
