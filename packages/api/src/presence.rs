//! The presence hub (Milestone 8): one room per folder id, members keyed
//! by window id, the full list broadcast on every change and on leave.
//! State lives in memory; the room empties when the last socket closes.

use dioxus::fullstack::{WebSocketOptions, Websocket};
use dioxus::prelude::*;
use moonkale_ext_api::presence::{Member, PresenceMessage};

#[cfg(feature = "server")]
mod hub {
    use super::Member;
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    use tokio::sync::broadcast;

    pub struct Room {
        pub members: HashMap<String, Member>,
        pub tx: broadcast::Sender<Vec<Member>>,
    }

    fn rooms() -> &'static Mutex<HashMap<String, Room>> {
        static ROOMS: OnceLock<Mutex<HashMap<String, Room>>> = OnceLock::new();
        ROOMS.get_or_init(|| Mutex::new(HashMap::new()))
    }

    /// Insert/replace a member and broadcast; returns a receiver for the
    /// room's updates (on first join).
    pub fn upsert(room: &str, member: Member) -> broadcast::Receiver<Vec<Member>> {
        let mut rooms = rooms().lock().unwrap();
        let r = rooms.entry(room.to_string()).or_insert_with(|| Room {
            members: HashMap::new(),
            tx: broadcast::channel(64).0,
        });
        r.members.insert(member.window.clone(), member);
        let rx = r.tx.subscribe();
        let _ = r.tx.send(sorted(&r.members));
        rx
    }

    pub fn leave(room: &str, window: &str) {
        let mut rooms = rooms().lock().unwrap();
        if let Some(r) = rooms.get_mut(room) {
            r.members.remove(window);
            let _ = r.tx.send(sorted(&r.members));
            if r.members.is_empty() {
                rooms.remove(room);
            }
        }
    }

    pub fn snapshot(room: &str) -> Vec<Member> {
        rooms()
            .lock()
            .unwrap()
            .get(room)
            .map(|r| sorted(&r.members))
            .unwrap_or_default()
    }

    fn sorted(m: &HashMap<String, Member>) -> Vec<Member> {
        let mut v: Vec<Member> = m.values().cloned().collect();
        v.sort_by(|a, b| a.name.cmp(&b.name).then(a.window.cmp(&b.window)));
        v
    }
}

/// Server function: a websocket into the hub. First message must be `Join`.
#[get("/api/presence")]
pub async fn presence_socket(
    options: WebSocketOptions,
) -> Result<Websocket<PresenceMessage, PresenceMessage>, ServerFnError> {
    Ok(options.on_upgrade(|mut socket| async move {
        let Ok(PresenceMessage::Join { room, member }) = socket.recv().await else {
            return;
        };
        let window = member.window.clone();
        let mut rx = hub::upsert(&room, member);
        // The joiner gets the current list right away.
        if socket.send(PresenceMessage::Members { members: hub::snapshot(&room) }).await.is_err() {
            hub::leave(&room, &window);
            return;
        }
        loop {
            tokio::select! {
                list = rx.recv() => match list {
                    Ok(members) => {
                        if socket.send(PresenceMessage::Members { members }).await.is_err() { break; }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        if socket.send(PresenceMessage::Members { members: hub::snapshot(&room) }).await.is_err() { break; }
                    }
                    Err(_) => break,
                },
                msg = socket.recv() => match msg {
                    Ok(PresenceMessage::Update { mut member }) => {
                        member.window = window.clone();
                        let _ = hub::upsert(&room, member);
                    }
                    Ok(_) => {}
                    Err(_) => break,
                },
            }
        }
        hub::leave(&room, &window);
    }))
}

/// Client side (web): the link the workspace holds while a folder is open.
pub struct RemotePresence {
    tx: futures_channel::mpsc::UnboundedSender<PresenceMessage>,
}

impl RemotePresence {
    /// Connects in the background; member lists arrive on `on_members`.
    pub fn join(room: String, member: Member, on_members: Callback<Vec<Member>>) -> Self {
        let (tx, mut rx) = futures_channel::mpsc::unbounded::<PresenceMessage>();
        spawn(async move {
            use futures_util::StreamExt;
            let socket = match presence_socket(WebSocketOptions::new()).await {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!("presence: {e}");
                    return;
                }
            };
            if socket
                .send(PresenceMessage::Join { room, member })
                .await
                .is_err()
            {
                return;
            }
            let socket = std::rc::Rc::new(socket);
            let s = socket.clone();
            spawn(async move {
                while let Some(msg) = rx.next().await {
                    if s.send(msg).await.is_err() {
                        break;
                    }
                }
            });
            loop {
                match socket.recv().await {
                    Ok(PresenceMessage::Members { members }) => on_members.call(members),
                    Ok(_) => {}
                    Err(_) => {
                        on_members.call(Vec::new());
                        break;
                    }
                }
            }
        });
        Self { tx }
    }
}

impl moonkale_ext_api::presence::PresenceLink for RemotePresence {
    fn update(&self, member: Member) {
        let _ = self.tx.unbounded_send(PresenceMessage::Update { member });
    }
}
