//! The presence hub (Milestone 8): one room per folder id, members keyed
//! by window id, the full list broadcast on every change and on leave.
//! State lives in memory; the room empties when the last socket closes.

use dioxus::fullstack::{WebSocketOptions, Websocket};
use dioxus::prelude::*;
use moonkale_ext_api::presence::{Member, PresenceMessage};
use std::{cell::RefCell, rc::Rc};

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
    pub fn upsert(room: &str, mut member: Member) -> broadcast::Receiver<Vec<Member>> {
        if let Some(position) = member.sharing.as_mut() {
            if !position.valid() {
                member.sharing = None;
                member.line = None;
            } else {
                position.updated = moonkale_ext_api::workspace::now_ms();
            }
        }
        member.line = member
            .sharing
            .as_ref()
            .and_then(|p| p.cursor.as_ref().map(|c| c.line));
        let mut rooms = rooms().lock().unwrap();
        let r = rooms.entry(room.to_string()).or_insert_with(|| Room {
            members: HashMap::new(),
            tx: broadcast::channel(64).0,
        });
        if r.members.len() < 128 || r.members.contains_key(&member.window) {
            r.members.insert(member.window.clone(), member);
        }
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
        let now = moonkale_ext_api::workspace::now_ms();
        for member in &mut v {
            if member.sharing.as_ref().is_some_and(|p| !p.fresh(now)) {
                member.sharing = None;
                member.line = None;
            }
        }
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
        if room.len() > 8192 || member.window.len() > 128 || member.name.len() > 128 || member.active.as_ref().is_some_and(|a| a.len() > 4096) { return; }
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
                        if member.name.len() > 128 || member.active.as_ref().is_some_and(|a| a.len() > 4096) { break; }
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
    wake: RefCell<futures_channel::mpsc::Sender<()>>,
    pending: Rc<RefCell<Option<Member>>>,
}

impl RemotePresence {
    /// Connects in the background; member lists arrive on `on_members`.
    pub fn join(room: String, member: Member, on_members: Callback<Vec<Member>>) -> Self {
        let (wake, mut rx) = futures_channel::mpsc::channel::<()>(1);
        let pending = Rc::new(RefCell::new(Some(member)));
        let outgoing = pending.clone();
        spawn(async move {
            use futures_util::StreamExt;
            let socket = match presence_socket(WebSocketOptions::new()).await {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!("presence: {e}");
                    return;
                }
            };
            let member = outgoing.borrow_mut().take().expect("initial presence");
            if socket
                .send(PresenceMessage::Join { room, member })
                .await
                .is_err()
            {
                return;
            }
            loop {
                use futures_util::future::{select, Either};
                match select(Box::pin(rx.next()), Box::pin(socket.recv())).await {
                    Either::Left((Some(()), _)) => {
                        let member = outgoing.borrow_mut().take();
                        if let Some(member) = member {
                            if socket
                                .send(PresenceMessage::Update { member })
                                .await
                                .is_err()
                            {
                                break;
                            }
                        }
                    }
                    Either::Left((None, _)) => break,
                    Either::Right((Ok(PresenceMessage::Members { members }), _)) => {
                        on_members.call(members)
                    }
                    Either::Right((Ok(_), _)) => {}
                    Either::Right((Err(_), _)) => {
                        on_members.call(Vec::new());
                        break;
                    }
                }
            }
        });
        Self {
            wake: RefCell::new(wake),
            pending,
        }
    }
}

impl moonkale_ext_api::presence::PresenceLink for RemotePresence {
    fn update(&self, member: Member) {
        *self.pending.borrow_mut() = Some(member);
        // A full wake queue already guarantees a drain. Replace the sample,
        // including revocation, instead of retaining a movement backlog.
        let _ = self.wake.borrow_mut().try_send(());
    }
}

#[cfg(test)]
mod client_tests {
    use super::*;
    use moonkale_ext_api::presence::PresenceLink;

    #[test]
    fn stalled_transport_keeps_only_latest_state_and_stop_replaces_motion() {
        let (wake, mut rx) = futures_channel::mpsc::channel(1);
        let link = RemotePresence {
            wake: RefCell::new(wake),
            pending: Rc::new(RefCell::new(None)),
        };
        for n in 0..10_000 {
            link.update(Member {
                window: "peer".into(),
                name: n.to_string(),
                active: None,
                line: None,
                sharing: Some(Default::default()),
            });
        }
        assert_eq!(link.pending.borrow().as_ref().unwrap().name, "9999");
        link.update(Member {
            window: "peer".into(),
            name: "stopped".into(),
            active: None,
            line: None,
            sharing: None,
        });
        let last = link.pending.borrow_mut().take().unwrap();
        assert_eq!(last.name, "stopped");
        assert!(last.sharing.is_none());
        let mut wakes = 0;
        while let Ok(()) = rx.try_recv() {
            wakes += 1;
        }
        assert!(wakes <= 2, "wake queue grew with the movement backlog");
    }
}
