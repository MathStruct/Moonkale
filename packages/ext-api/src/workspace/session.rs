//! `Workspace`: the session bus between windows, cross-window drag and drop, presence. Split out of `workspace.rs` in Milestone 18 phase 3b
//! (no API change; phase 3c turns the areas into services).

use super::*;
use crate::{t, L};

impl Workspace {
    /// Install the platform's session transport and announce this window.
    pub fn connect_bus(&mut self, bus: Rc<dyn SessionBus>) {
        self.session.bus.set(Some(bus));
        self.send(SessionMessage::Hello {
            from: self.session.window.peek().clone(),
        });
    }

    pub(super) fn send(&self, msg: SessionMessage) {
        tracing::info!(
            "session[{}] send {}",
            self.session.window.peek(),
            summary(&msg)
        );
        if let Some(bus) = self.session.bus.peek().as_ref() {
            bus.send(SessionMessage::Scoped {
                session: self.saved_sessions.active.peek().clone(),
                message: Box::new(msg),
            });
        }
    }

    /// Start a cross-window drag from a workbench tab. `tab_id` is the DOM id
    /// of the dragged tab (`wb-tab-<panel id>`); any open document whose node
    /// id appears in it is the one being dragged, whatever the panel scheme.
    pub fn start_drag_from_tab(&mut self, tab_id: &str) -> bool {
        if let Some(panel) = self
            .session
            .local_tabs
            .peek()
            .iter()
            .find(|t| {
                let safe: String = t
                    .panel
                    .chars()
                    .map(|c| {
                        if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                            c
                        } else {
                            '-'
                        }
                    })
                    .collect();
                tab_id == format!("wb-tab-{safe}")
            })
            .map(|t| t.panel.clone())
        {
            return self.start_panel_drag(&panel, None);
        }
        let node = self
            .docs
            .open
            .peek()
            .iter()
            .map(|(id, _)| *id)
            .find(|id| tab_id.contains(&id.to_string()));
        match node {
            Some(node) => {
                self.start_drag(node);
                *self.session.own_drag.peek() == Some(node)
            }
            None => false,
        }
    }

    /// React to a message from another window of this session.
    pub async fn handle_message(mut self, msg: SessionMessage) {
        let msg = match msg {
            SessionMessage::Scoped { session, message }
                if session == *self.saved_sessions.active.peek() =>
            {
                *message
            }
            SessionMessage::Scoped { .. } => return,
            msg if *self.saved_sessions.active.peek() == super::DEFAULT_SESSION => msg,
            _ => return,
        };
        let me = self.session.window.peek().clone();
        if msg.sender().0.is_empty() || msg.sender().0.len() > 128 || !msg.sender().0.is_ascii() {
            return;
        }
        if msg.sender() == &me {
            return;
        }
        tracing::info!("session[{me}] recv {}", summary(&msg));
        let sender = msg.sender().clone();
        if self.session.peers.peek().len() < 128 && !self.session.peers.peek().contains(&sender) {
            self.session.peers.with_mut(|p| p.push(sender));
        }
        if self.handle_panel_message(&msg).await {
            return;
        }
        match msg {
            SessionMessage::Scoped { .. }
            | SessionMessage::WindowTabs { .. }
            | SessionMessage::Links { .. }
            | SessionMessage::FocusPanel { .. }
            | SessionMessage::MovePanel { .. }
            | SessionMessage::PanelDrag { .. }
            | SessionMessage::PanelMoved { .. }
            | SessionMessage::WindowLeft { .. } => (),
            SessionMessage::Presence { from, mut member } => {
                member.window = from.to_string();
                if let Some(p) = member.sharing.as_mut() {
                    if !p.valid() {
                        return;
                    }
                    p.updated = now_ms();
                }
                self.session.presence.with_mut(|members| {
                    members.retain(|m| m.window != member.window);
                    if members.len() < 128 {
                        members.push(member);
                    }
                });
            }
            SessionMessage::Welcome { .. } => {}
            SessionMessage::Hello { .. } => {
                self.publish_window_tabs();
                self.send(SessionMessage::Links {
                    from: me.clone(),
                    groups: self.links.groups.peek().clone(),
                    removed: Vec::new(),
                });
                self.send(SessionMessage::Welcome { from: me.clone() });
                // Tell the newcomer what we have open.
                let sources: Vec<_> = self
                    .sources
                    .open
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
                self.session.foreign_drag.set(Some(ForeignDrag {
                    from,
                    node,
                    source,
                    live: true,
                    transfer: None,
                }));
            }
            SessionMessage::DragEnded { from } => {
                // Keep the offer, but mark it as no longer a live drag.
                let pending = self.session.foreign_drag.peek().clone();
                if let Some(mut d) = pending {
                    if d.from == from && d.live {
                        d.live = false;
                        self.session.foreign_drag.set(Some(d));
                    }
                }
            }
            SessionMessage::Moved { node, from, to } => {
                // Our document landed in another window: close it here.
                if let Some(doc) = self.document(node).filter(|_| from == me) {
                    if doc.peek().dirty() {
                        self.set_status(t!(self, L, "move-unsaved"));
                    } else {
                        self.close_node(node);
                        self.set_status(t!(self, L, "moved-to-window", window = to.to_string()));
                    }
                }
                // Someone accepted the offer: withdraw it everywhere.
                if self.session.foreign_drag.peek().as_ref().map(|d| d.node.id) == Some(node) {
                    self.session.foreign_drag.set(None);
                }
            }
        }
    }

    /// Begin dragging one of our documents out (HTML5 `dragstart`).
    pub fn start_drag(&mut self, node: NodeId) {
        let Some((doc, source, dirty)) = self.document(node).and_then(|d| {
            let d = d.read();
            let src = self
                .sources
                .open
                .peek()
                .iter()
                .find(|s| s.descriptor.id == d.node.source)?
                .descriptor
                .clone();
            Some((d.node.clone(), src, d.dirty()))
        }) else {
            return;
        };
        // The session protocol transfers a source node, not unsaved text.
        // Refuse an offer that could discard local changes at the destination.
        if dirty {
            self.end_drag();
            self.set_status(t!(self, L, "move-unsaved"));
            return;
        }
        self.session.own_drag.set(Some(node));
        self.set_status(t!(self, L, "dragging", name = doc.native_key.clone()));
        self.send(SessionMessage::DragStarted {
            from: self.session.window.peek().clone(),
            node: doc,
            source,
        });
    }

    /// Decline an offer from another window.
    pub fn dismiss_drop(&mut self) {
        self.session.foreign_drag.set(None);
    }

    /// The drag ended without a drop elsewhere (`dragend`).
    pub fn end_drag(&mut self) {
        if self.session.own_drag.peek().is_some() {
            self.session.own_drag.set(None);
            self.send(SessionMessage::DragEnded {
                from: self.session.window.peek().clone(),
            });
        }
    }

    /// A foreign drag was dropped on this window: open the document here
    /// and tell the origin to close its copy.
    pub async fn accept_drop(mut self) -> Result<(), SourceError> {
        let Some(drag) = self.session.foreign_drag.peek().clone() else {
            return Ok(());
        };
        self.session.foreign_drag.set(None);
        if let Some(transfer) = drag.transfer {
            return self.accept_panel_drop(drag.from, transfer).await;
        }
        self.attach_source(drag.source).await?;
        self.open_node(drag.node.clone()).await?;
        self.send(SessionMessage::Moved {
            node: drag.node.id,
            from: drag.from,
            to: self.session.window.peek().clone(),
        });
        Ok(())
    }

    /// My presence record as the hub should see it now.
    pub fn my_presence(&self) -> crate::presence::Member {
        let active = self
            .docs
            .active
            .peek()
            .and_then(|n| self.document(n))
            .filter(|d| crate::preview::allowed_path(&d.peek().node.native_key))
            .map(|d| d.peek().node.native_key.clone());
        let sharing = self.shared_position();
        crate::presence::Member {
            window: self.session.window.peek().to_string(),
            name: self.settings.resolved.peek().user_name.clone(),
            active,
            line: sharing
                .as_ref()
                .and_then(|p| p.cursor.as_ref().map(|c| c.line)),
            sharing,
        }
    }

    /// Join the folder's presence room (called when a folder opens); a
    /// no-op without a hub. Re-joining replaces the link.
    pub fn join_presence(&mut self, room: &str) {
        let Some(join) = self.config.network.presence else {
            return;
        };
        let mut members = self.session.presence;
        let active = self.saved_sessions.active;
        let outer = active.peek().clone();
        let on_members = Callback::new(move |list: Vec<crate::presence::Member>| {
            if *active.peek() == outer {
                members.set(list);
            }
        });
        let room = format!("{}:{}", room, self.saved_sessions.active.peek());
        let link = join(room, self.my_presence(), on_members);
        self.session.presence_link.set(Some(link));
    }

    /// Tell the hub what this window looks at now.
    pub fn publish_presence(&self) {
        if self.config.network.presence.is_none() || self.session.presence_link.peek().is_none() {
            self.send(SessionMessage::Presence {
                from: self.session.window.peek().clone(),
                member: self.my_presence(),
            });
        }
        if let Some(link) = self.session.presence_link.peek().as_ref() {
            link.update(self.my_presence());
        }
    }

    /// Members other than this window.
    pub fn others(&self) -> Vec<crate::presence::Member> {
        let me = self.session.window.peek().to_string();
        let now = now_ms();
        let mut others: Vec<_> = self
            .session
            .presence
            .peek()
            .iter()
            .filter(|m| m.window != me)
            .cloned()
            .collect();
        for member in &mut others {
            if member.sharing.as_ref().is_some_and(|p| !p.fresh(now)) {
                member.sharing = None;
                member.line = None;
            }
        }
        let agents: Vec<_> = others
            .iter()
            .filter_map(|m| {
                let (name, cursor) = m.sharing.as_ref()?.agent.clone()?;
                Some(crate::presence::Member {
                    window: format!("{}:agent", m.window),
                    name,
                    active: Some(cursor.document.clone()),
                    line: Some(cursor.line),
                    sharing: Some(crate::sharing::Position {
                        cursor: Some(cursor),
                        pointer: None,
                        agent: None,
                        ..m.sharing.clone()?
                    }),
                })
            })
            .collect();
        others.extend(agents);
        others
    }
}

/// This window among the others: the bus, drag and drop, presence. (Milestone 18 phase 3c: the workspace's state, grouped by area.)
#[derive(Clone, Copy)]
pub struct SessionState {
    /// This host's current metadata catalogue.
    pub local_tabs: Signal<Vec<crate::window_panels::WindowTab>>,
    /// Live catalogues in other windows of this outer session.
    pub remote_tabs: Signal<Vec<crate::window_panels::WindowTabs>>,
    /// Receiving a focus request also raises the platform window.
    pub focus_requested: Signal<u64>,
    pub(crate) links_seen: Signal<Vec<crate::links::LinkGroup>>,
    pub(crate) catalogue_sent: Signal<u64>,
    pub(crate) requested_move: Signal<Option<(WindowId, String)>>,
    pub(crate) panel_offer: Signal<Option<String>>,
    /// This window's id in the session.
    pub window: Signal<WindowId>,
    /// Other windows we have heard from (diagnostic: shown in the status bar).
    pub peers: Signal<Vec<WindowId>>,
    /// A drag coming from another window, while it lasts.
    pub foreign_drag: Signal<Option<ForeignDrag>>,
    /// The node this window is currently dragging out, if any.
    pub own_drag: Signal<Option<NodeId>>,
    /// Who else is in the open folder (Milestone 8); this window included.
    pub presence: Signal<Vec<crate::presence::Member>>,
    /// Opt-in state, reset on every named-session switch.
    pub sharing: Signal<crate::sharing::Sharing>,
    pub(crate) presence_link: Signal<Option<Rc<dyn crate::presence::PresenceLink>>>,
    pub(crate) bus: Signal<Option<Rc<dyn SessionBus>>>,
}

impl SessionState {
    pub(super) fn new() -> Self {
        Self {
            local_tabs: Signal::new_in_scope(Vec::new(), ScopeId::ROOT),
            remote_tabs: Signal::new_in_scope(Vec::new(), ScopeId::ROOT),
            focus_requested: Signal::new_in_scope(0, ScopeId::ROOT),
            links_seen: Signal::new_in_scope(Vec::new(), ScopeId::ROOT),
            catalogue_sent: Signal::new_in_scope(0, ScopeId::ROOT),
            requested_move: Signal::new_in_scope(None, ScopeId::ROOT),
            panel_offer: Signal::new_in_scope(None, ScopeId::ROOT),
            window: Signal::new_in_scope(WindowId::fresh(), ScopeId::ROOT),
            peers: Signal::new_in_scope(Vec::new(), ScopeId::ROOT),
            foreign_drag: Signal::new_in_scope(None, ScopeId::ROOT),
            own_drag: Signal::new_in_scope(None, ScopeId::ROOT),
            presence: Signal::new_in_scope(Vec::new(), ScopeId::ROOT),
            sharing: Signal::new_in_scope(crate::sharing::Sharing::default(), ScopeId::ROOT),
            presence_link: Signal::new_in_scope(None, ScopeId::ROOT),
            bus: Signal::new_in_scope(None, ScopeId::ROOT),
        }
    }
}
