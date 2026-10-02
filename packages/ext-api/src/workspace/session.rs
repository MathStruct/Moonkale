//! `Workspace`: the session bus between windows, cross-window drag and drop, presence. Split out of `workspace.rs` in Milestone 18 phase 3b
//! (no API change; phase 3c turns the areas into services).

use super::*;

impl Workspace {
    /// Install the platform's session transport and announce this window.
    pub fn connect_bus(&mut self, bus: Rc<dyn SessionBus>) {
        bus.send(SessionMessage::Hello {
            from: self.window.peek().clone(),
        });
        self.bus.set(Some(bus));
    }

    pub(super) fn send(&self, msg: SessionMessage) {
        tracing::info!("session[{}] send {}", self.window.peek(), summary(&msg));
        if let Some(bus) = self.bus.peek().as_ref() {
            bus.send(msg);
        }
    }

    /// Start a cross-window drag from a workbench tab. `tab_id` is the DOM id
    /// of the dragged tab (`wb-tab-<panel id>`); any open document whose node
    /// id appears in it is the one being dragged, whatever the panel scheme.
    pub fn start_drag_from_tab(&mut self, tab_id: &str) -> bool {
        let node = self
            .documents
            .peek()
            .iter()
            .map(|(id, _)| *id)
            .find(|id| tab_id.contains(&id.to_string()));
        match node {
            Some(node) => {
                self.start_drag(node);
                true
            }
            None => false,
        }
    }

    /// React to a message from another window of this session.
    pub async fn handle_message(mut self, msg: SessionMessage) {
        let me = self.window.peek().clone();
        if msg.sender() == &me {
            return;
        }
        tracing::info!("session[{me}] recv {}", summary(&msg));
        let sender = msg.sender().clone();
        if !self.peers.peek().contains(&sender) {
            self.peers.with_mut(|p| p.push(sender));
        }
        match msg {
            SessionMessage::Welcome { .. } => {}
            SessionMessage::Hello { .. } => {
                self.send(SessionMessage::Welcome { from: me.clone() });
                // Tell the newcomer what we have open.
                let sources: Vec<_> = self
                    .sources
                    .peek()
                    .iter()
                    .map(|s| s.descriptor.clone())
                    .collect();
                for descriptor in sources {
                    self.send(SessionMessage::SourceOpened {
                        from: me.clone(),
                        descriptor,
                    });
                }
            }
            SessionMessage::SourceOpened { descriptor, .. } => {
                let _ = self.attach_source(descriptor).await;
            }
            SessionMessage::DragStarted { from, node, source } => {
                self.foreign_drag.set(Some(ForeignDrag {
                    from,
                    node,
                    source,
                    live: true,
                }));
            }
            SessionMessage::DragEnded { from } => {
                // Keep the offer, but mark it as no longer a live drag.
                let pending = self.foreign_drag.peek().clone();
                if let Some(mut d) = pending {
                    if d.from == from && d.live {
                        d.live = false;
                        self.foreign_drag.set(Some(d));
                    }
                }
            }
            SessionMessage::Moved { node, to, .. } => {
                // Our document landed in another window: close it here.
                if self.document(node).is_some() {
                    self.close_node(node);
                    self.set_status(format!("Moved to window {to}"));
                }
                // Someone accepted the offer: withdraw it everywhere.
                if self.foreign_drag.peek().as_ref().map(|d| d.node.id) == Some(node) {
                    self.foreign_drag.set(None);
                }
            }
        }
    }

    /// Begin dragging one of our documents out (HTML5 `dragstart`).
    pub fn start_drag(&mut self, node: NodeId) {
        let Some((doc, source)) = self.document(node).and_then(|d| {
            let d = d.read();
            let src = self
                .sources
                .peek()
                .iter()
                .find(|s| s.descriptor.id == d.node.source)?
                .descriptor
                .clone();
            Some((d.node.clone(), src))
        }) else {
            return;
        };
        self.own_drag.set(Some(node));
        self.set_status(format!(
            "Dragging {} — drop it on another Moonkale window to move it there",
            doc.native_key
        ));
        self.send(SessionMessage::DragStarted {
            from: self.window.peek().clone(),
            node: doc,
            source,
        });
    }

    /// Decline an offer from another window.
    pub fn dismiss_drop(&mut self) {
        self.foreign_drag.set(None);
    }

    /// The drag ended without a drop elsewhere (`dragend`).
    pub fn end_drag(&mut self) {
        if self.own_drag.peek().is_some() {
            self.own_drag.set(None);
            self.send(SessionMessage::DragEnded {
                from: self.window.peek().clone(),
            });
        }
    }

    /// A foreign drag was dropped on this window: open the document here
    /// and tell the origin to close its copy.
    pub async fn accept_drop(mut self) -> Result<(), SourceError> {
        let Some(drag) = self.foreign_drag.peek().clone() else {
            return Ok(());
        };
        self.foreign_drag.set(None);
        self.attach_source(drag.source).await?;
        self.open_node(drag.node.clone()).await?;
        self.send(SessionMessage::Moved {
            node: drag.node.id,
            from: drag.from,
            to: self.window.peek().clone(),
        });
        Ok(())
    }

    /// My presence record as the hub should see it now.
    pub fn my_presence(&self) -> crate::presence::Member {
        let active = self
            .active
            .peek()
            .and_then(|n| self.document(n))
            .map(|d| d.peek().node.native_key.clone());
        let line = if active.is_some() {
            *self.cursor_line.peek()
        } else {
            None
        };
        crate::presence::Member {
            window: self.window.peek().to_string(),
            name: self.settings.peek().user_name.clone(),
            active,
            line,
        }
    }

    /// Join the folder's presence room (called when a folder opens); a
    /// no-op without a hub. Re-joining replaces the link.
    pub fn join_presence(&mut self, room: &str) {
        let Some(join) = self.config.network.presence else {
            return;
        };
        let mut members = self.presence;
        let on_members = Callback::new(move |list: Vec<crate::presence::Member>| members.set(list));
        let link = join(room.to_string(), self.my_presence(), on_members);
        self.presence_link.set(Some(link));
    }

    /// Tell the hub what this window looks at now.
    pub fn publish_presence(&self) {
        if let Some(link) = self.presence_link.peek().as_ref() {
            link.update(self.my_presence());
        }
    }

    /// Members other than this window.
    pub fn others(&self) -> Vec<crate::presence::Member> {
        let me = self.window.peek().to_string();
        self.presence
            .peek()
            .iter()
            .filter(|m| m.window != me)
            .cloned()
            .collect()
    }
}
