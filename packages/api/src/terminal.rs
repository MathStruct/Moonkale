//! Remote terminals: a PTY on the server, a websocket to the client.
//!
//! **Dev-server feature.** The shell runs as the server's user; the only
//! restriction is that the working directory is confined to `MOONKALE_ROOT`.
//! The security list in the vault's Platform Matrix (auth, jail, limits,
//! audit — P-20) is a prerequisite before exposing this beyond localhost.

use dioxus::fullstack::{WebSocketOptions, Websocket};
use dioxus::prelude::*;
use moonkale_terminal::TerminalMessage;

/// Server function: upgrade to a websocket speaking [`TerminalMessage`]
/// both ways. The first client message must be `Open`.
#[get("/api/terminal")]
pub async fn terminal_socket(
    options: WebSocketOptions,
) -> Result<Websocket<TerminalMessage, TerminalMessage>, ServerFnError> {
    Ok(options.on_upgrade(|mut socket| async move {
        use base64::Engine;
        use futures_util::StreamExt;
        use moonkale_terminal::TerminalBackend;
        let b64 = base64::engine::general_purpose::STANDARD;

        let Ok(TerminalMessage::Open { cwd, cols, rows }) = socket.recv().await else {
            let _ = socket.send(TerminalMessage::Exit { code: None, message: Some("expected an Open message".into()) }).await;
            return;
        };
        // Confine the working directory to the allowed root.
        let cwd = match crate::state::jail_dir(cwd.as_deref()) {
            Ok(p) => p,
            Err(e) => {
                let _ = socket.send(TerminalMessage::Exit { code: None, message: Some(e.to_string()) }).await;
                return;
            }
        };
        let mut pty = match moonkale_terminal_pty::PtyBackend::spawn(Some(&cwd), None, cols.max(2), rows.max(1)) {
            Ok(p) => p,
            Err(e) => {
                let _ = socket.send(TerminalMessage::Exit { code: None, message: Some(e) }).await;
                return;
            }
        };
        let mut out = pty.take_output().expect("fresh pty");
        loop {
            tokio::select! {
                chunk = out.next() => match chunk {
                    Some(bytes) => {
                        if socket.send(TerminalMessage::Output { data: b64.encode(&bytes) }).await.is_err() { break; }
                    }
                    None => {
                        let _ = socket.send(TerminalMessage::Exit { code: None, message: None }).await;
                        break;
                    }
                },
                msg = socket.recv() => match msg {
                    Ok(TerminalMessage::Input { data }) => {
                        if let Ok(bytes) = b64.decode(data) { pty.write(&bytes); }
                    }
                    Ok(TerminalMessage::Resize { cols, rows }) => pty.resize(cols, rows),
                    Ok(_) => {}
                    Err(_) => break, // client went away → pty dropped → process killed
                },
            }
        }
    }))
}

/// Client-side backend: a [`moonkale_terminal::TerminalBackend`] over the
/// websocket above. Input is queued and pumped by a task; output arrives on
/// the usual channel. Compiled on every client; only used on web.
pub struct RemoteTerminal {
    input: futures_channel::mpsc::UnboundedSender<TerminalMessage>,
    output: Option<moonkale_terminal::Output>,
}

impl RemoteTerminal {
    pub async fn connect(cwd: Option<String>, cols: u16, rows: u16) -> Result<Self, String> {
        use base64::Engine;
        use futures_util::StreamExt;
        let socket = terminal_socket(WebSocketOptions::new())
            .await
            .map_err(|e| e.to_string())?;
        socket
            .send(TerminalMessage::Open { cwd, cols, rows })
            .await
            .map_err(|e| e.to_string())?;
        let socket = std::rc::Rc::new(socket);
        let (in_tx, mut in_rx) = futures_channel::mpsc::unbounded::<TerminalMessage>();
        let (out_tx, out_rx) = futures_channel::mpsc::unbounded::<Vec<u8>>();
        // input pump
        let s = socket.clone();
        spawn(async move {
            while let Some(msg) = in_rx.next().await {
                if s.send(msg).await.is_err() {
                    break;
                }
            }
        });
        // output pump
        let s = socket;
        spawn(async move {
            loop {
                match s.recv().await {
                    Ok(TerminalMessage::Output { data }) => {
                        if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(data) {
                            if out_tx.unbounded_send(bytes).is_err() {
                                break;
                            }
                        }
                    }
                    Ok(TerminalMessage::Exit { message, .. }) => {
                        if let Some(m) = message {
                            let _ = out_tx.unbounded_send(format!("\r\n[{m}]\r\n").into_bytes());
                        }
                        break;
                    }
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
        });
        Ok(Self {
            input: in_tx,
            output: Some(out_rx),
        })
    }
}

impl moonkale_terminal::TerminalBackend for RemoteTerminal {
    fn write(&self, data: &[u8]) {
        use base64::Engine;
        let _ = self.input.unbounded_send(TerminalMessage::Input {
            data: base64::engine::general_purpose::STANDARD.encode(data),
        });
    }
    fn resize(&self, cols: u16, rows: u16) {
        let _ = self
            .input
            .unbounded_send(TerminalMessage::Resize { cols, rows });
    }
    fn take_output(&mut self) -> Option<moonkale_terminal::Output> {
        self.output.take()
    }
    fn title(&self) -> String {
        "remote".into()
    }
}
