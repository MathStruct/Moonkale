//! Workspace routing for opt-in positions, shared by both transports.
use super::*;
use crate::sharing::{Cursor, Position, Sharing};

impl Workspace {
    /// Change this participant's ephemeral switches. Stop clears queued samples too.
    pub fn share_positions(&mut self, cursor: bool, pointer: bool) {
        self.session.sharing.with_mut(|s| {
            s.cursor = cursor;
            s.pointer = pointer;
            if !pointer {
                s.position = None;
            }
            s.last = None;
            s.sent = 0;
        });
        self.publish_presence();
    }

    /// Revoke all position sharing and follow mode in one action.
    pub fn stop_sharing(&mut self) {
        self.session.sharing.set(Sharing::default());
        self.publish_presence();
    }

    /// Capture only graph coordinates; this never observes input or document bodies.
    pub fn share_pointer(&mut self, pointer: Option<crate::sharing::Pointer>) {
        if self.session.sharing.peek().pointer {
            self.session.sharing.with_mut(|s| s.position = pointer);
        }
    }

    /// Agent's approved editor.open/ui.reveal target, with the same opt-in switch.
    pub fn share_agent_cursor(&mut self, name: String, cursor: Cursor) {
        if self.session.sharing.peek().agent_enabled && name.len() <= 128 {
            self.session
                .sharing
                .with_mut(|s| s.agent = Some((name, cursor)));
        }
    }

    pub(super) fn shared_position(&self) -> Option<Position> {
        let state = self.session.sharing.peek();
        if !state.cursor && !state.pointer && !state.agent_enabled {
            return None;
        }
        let cursor = state
            .cursor
            .then(|| {
                let (node, doc) = self.active_document()?;
                let doc = doc.peek();
                // Same exclusion as local recovery; no terminals, prompts or credential paths.
                if !crate::preview::allowed_path(&doc.node.native_key) {
                    return None;
                }
                let (line, column) = (*self.docs.cursor.peek()).unwrap_or((0, 0));
                let (anchor, head) = (*self.docs.selection.peek()).unwrap_or((0, 0));
                let _ = node;
                Some(Cursor {
                    source: doc.node.source.to_string(),
                    document: doc.node.native_key.clone(),
                    line,
                    column,
                    anchor,
                    head,
                })
            })
            .flatten();
        Some(Position {
            updated: now_ms(),
            cursor,
            pointer: state.pointer.then(|| state.position.clone()).flatten(),
            agent: state.agent_enabled.then(|| state.agent.clone()).flatten(),
        })
    }

    /// Coalesce samples, expire receivers and follow through already-authorized sources.
    pub fn sharing_tick(&mut self) {
        let now = now_ms();
        if let Some(position) = self.shared_position() {
            if self.session.sharing.peek().due(&position, now) {
                self.session.sharing.with_mut(|s| {
                    s.last = Some(position);
                    s.sent = now;
                });
                self.publish_presence();
            }
        }
        let stale = self
            .session
            .presence
            .peek()
            .iter()
            .any(|m| m.sharing.as_ref().is_some_and(|p| !p.fresh(now)));
        if stale {
            self.session.presence.with_mut(|members| {
                for member in members {
                    if member.sharing.as_ref().is_some_and(|p| !p.fresh(now)) {
                        member.sharing = None;
                        member.line = None;
                    }
                }
            });
        }
        let following = self.session.sharing.peek().following.clone();
        if following.is_none() {
            return;
        }
        let Some(member) = self
            .others()
            .into_iter()
            .find(|m| Some(&m.window) == following.as_ref())
        else {
            self.session.sharing.with_mut(|s| s.following = None);
            return;
        };
        let Some(cursor) = member.sharing.and_then(|p| p.cursor) else {
            self.session.sharing.with_mut(|s| s.following = None);
            return;
        };
        let node = self.docs.open.peek().iter().find_map(|(id, doc)| {
            (doc.peek().node.native_key == cursor.document
                && doc.peek().node.source.as_str() == cursor.source)
                .then_some(*id)
        });
        let Some(node) = node else {
            if !crate::preview::allowed_path(&cursor.document)
                || cursor.document.split('/').count() > 64
            {
                return;
            }
            let request = (
                member.window,
                cursor.source.clone(),
                cursor.document.clone(),
            );
            let state = self.session.sharing.peek();
            if state.follow_request.as_ref() == Some(&request)
                || state.follow_opened >= 32
                || now.saturating_sub(state.follow_read_at) < 1000
            {
                return;
            }
            drop(state);
            // Only an already-open Source can authorize this read. No provider buffer is sent.
            if self.source(&SourceId::new(&cursor.source)).is_none() {
                return;
            }
            self.session.sharing.with_mut(|s| {
                s.follow_request = Some(request.clone());
                s.follow_read_at = now;
                s.follow_opened += 1;
            });
            let outer = self.saved_sessions.active.peek().clone();
            let ws = *self;
            dioxus::core::spawn_forever(async move {
                let current = || {
                    *ws.saved_sessions.active.peek() == outer
                        && ws.session.sharing.peek().following.as_ref() == Some(&request.0)
                        && ws.others().iter().any(|m| {
                            m.window == request.0
                                && m.sharing
                                    .as_ref()
                                    .and_then(|p| p.cursor.as_ref())
                                    .is_some_and(|c| {
                                        c.source == request.1 && c.document == request.2
                                    })
                        })
                };
                if !current() {
                    return;
                }
                if let Some(node) = ws
                    .node_at_path(&SourceId::new(&request.1), &request.2)
                    .await
                {
                    if current() {
                        let _ = ws.load_node_guarded(node, current).await;
                    }
                }
            });
            return;
        };
        if *self.docs.active.peek() == Some(node)
            && *self.docs.cursor.peek() == Some((cursor.line, cursor.column))
        {
            return;
        }
        self.set_active_document(Some(node));
        self.docs.cursor.set(Some((cursor.line, cursor.column)));
        let seq = self
            .docs
            .reveal
            .peek()
            .as_ref()
            .map_or(1, |r| r.seq.saturating_add(1));
        self.docs.reveal.set(Some(Reveal {
            node,
            line: cursor.line,
            col: cursor.column,
            seq,
        }));
    }
}
