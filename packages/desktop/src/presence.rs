//! Desktop presence client (Milestone 9): joins the hub named by
//! `MOONKALE_HUB` (`http://host:port` or `ws://…`), sending
//! `Authorization: Bearer $MOONKALE_TOKEN` when set. Speaks the same
//! `PresenceMessage` JSON frames as the web client (dioxus typed websockets
//! send JSON as binary frames and accept text or binary).

use dioxus::prelude::*;
use futures_util::{SinkExt, StreamExt};
use std::rc::Rc;
use tokio_tungstenite::tungstenite::{client::IntoClientRequest, Message};
use ui::{PresenceLink, PresenceMember};

type Msg = ui::PresenceMessage;

pub fn hub_url() -> Option<String> {
    let hub = std::env::var("MOONKALE_HUB").ok()?;
    let hub = hub.trim().trim_end_matches('/');
    if hub.is_empty() {
        return None;
    }
    let ws = hub
        .replacen("https://", "wss://", 1)
        .replacen("http://", "ws://", 1);
    Some(format!("{ws}/api/presence"))
}

pub struct NativePresence {
    tx: futures_channel::mpsc::UnboundedSender<Msg>,
}

impl PresenceLink for NativePresence {
    fn update(&self, member: PresenceMember) {
        let _ = self.tx.unbounded_send(Msg::Update { member });
    }
}

/// `WorkspaceConfig::presence` for the desktop.
pub fn join(
    room: String,
    member: PresenceMember,
    on_members: Callback<Vec<PresenceMember>>,
) -> Rc<dyn PresenceLink> {
    let (tx, mut rx) = futures_channel::mpsc::unbounded::<Msg>();
    let (lists_tx, mut lists_rx) = futures_channel::mpsc::unbounded::<Vec<PresenceMember>>();
    let Some(url) = hub_url() else {
        return Rc::new(NativePresence { tx });
    };
    // The socket lives on the tokio runtime; member lists cross back to the
    // UI thread through a channel the spawned future below drains.
    tokio::spawn(async move {
        let mut req = match url.clone().into_client_request() {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("presence: bad hub url {url}: {e}");
                return;
            }
        };
        if let Ok(token) = std::env::var("MOONKALE_TOKEN") {
            if let Ok(v) = format!("Bearer {}", token.trim()).parse() {
                req.headers_mut().insert("authorization", v);
            }
        }
        let (socket, _) = match tokio_tungstenite::connect_async(req).await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("presence: cannot reach the hub {url}: {e}");
                return;
            }
        };
        tracing::info!("presence: joined {url} as {}", member.name);
        let (mut sink, mut stream) = socket.split();
        let join = serde_json::to_vec(&Msg::Join { room, member }).unwrap_or_default();
        if sink.send(Message::Binary(join.into())).await.is_err() {
            return;
        }
        loop {
            tokio::select! {
                out = rx.next() => match out {
                    Some(m) => {
                        let bytes = serde_json::to_vec(&m).unwrap_or_default();
                        if sink.send(Message::Binary(bytes.into())).await.is_err() { break; }
                    }
                    None => break, // link dropped → leave
                },
                inc = stream.next() => match inc {
                    Some(Ok(Message::Binary(b))) => {
                        if let Ok(Msg::Members { members }) = serde_json::from_slice::<Msg>(&b) {
                            let _ = lists_tx.unbounded_send(members);
                        }
                    }
                    Some(Ok(Message::Text(t))) => {
                        if let Ok(Msg::Members { members }) = serde_json::from_str::<Msg>(&t) {
                            let _ = lists_tx.unbounded_send(members);
                        }
                    }
                    Some(Ok(_)) => {}
                    _ => break,
                },
            }
        }
        let _ = lists_tx.unbounded_send(Vec::new());
    });
    spawn(async move {
        while let Some(list) = lists_rx.next().await {
            on_members.call(list);
        }
    });
    Rc::new(NativePresence { tx })
}
