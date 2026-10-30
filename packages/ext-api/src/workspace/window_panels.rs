//! Cross-window links and tab routing. All traffic uses the outer-session envelope.
use super::*;
use crate::links::LinkGroup;
use crate::window_panels::{PanelTransfer, WindowTab, WindowTabs};
use crate::{t, L};

impl Workspace {
    /// Publish only changed group identities; received changes do not echo.
    pub fn publish_link_changes(mut self) {
        let next = self.links.groups.peek().clone();
        let previous = self.session.links_seen.peek().clone();
        if previous == next {
            return;
        }
        let groups = next
            .iter()
            .filter(|g| !previous.contains(g))
            .cloned()
            .collect();
        let removed = previous
            .iter()
            .filter(|g| !next.iter().any(|n| n.id == g.id))
            .map(|g| g.id.clone())
            .collect();
        self.session.links_seen.set(next);
        self.send(SessionMessage::Links {
            from: self.session.window.peek().clone(),
            groups,
            removed,
        });
    }

    /// Update the host catalogue, omitting private paths and all content previews.
    pub fn set_window_tabs(mut self, tabs: Vec<WindowTab>) {
        let tabs: Vec<_> = tabs
            .into_iter()
            .filter(|t| t.valid() && (t.path.is_empty() || crate::preview::allowed_path(&t.path)))
            .take(512)
            .collect();
        if *self.session.local_tabs.peek() != tabs {
            self.session.local_tabs.set(tabs);
            self.publish_window_tabs();
        }
    }

    pub(super) fn publish_window_tabs(&self) {
        self.send(SessionMessage::WindowTabs {
            from: self.session.window.peek().clone(),
            tabs: self.session.local_tabs.peek().clone(),
        });
    }

    /// Heartbeat and expiry share the shell's existing publication clock.
    pub fn window_tick(mut self) {
        let now = now_ms();
        if now.saturating_sub(*self.session.catalogue_sent.peek()) < 2000 {
            return;
        }
        self.session.catalogue_sent.set(now);
        self.publish_window_tabs();
        if self
            .session
            .remote_tabs
            .peek()
            .iter()
            .any(|p| now.saturating_sub(p.received) >= 8000)
        {
            self.session
                .remote_tabs
                .with_mut(|peers| peers.retain(|p| now.saturating_sub(p.received) < 8000));
        }
    }

    /// Ask a peer to activate its actual tab. A browser may refuse OS-level focus.
    pub fn focus_window_panel(&self, window: WindowId, panel: String) {
        self.send(SessionMessage::FocusPanel {
            from: self.session.window.peek().clone(),
            to: window,
            panel,
        });
    }
    /// Request an offer from the origin; the origin checks dirty state again.
    pub fn move_window_panel(&mut self, window: WindowId, panel: String) {
        self.session
            .requested_move
            .set(Some((window.clone(), panel.clone())));
        self.send(SessionMessage::MovePanel {
            from: self.session.window.peek().clone(),
            to: window,
            panel,
        });
    }

    fn receive_groups(mut self, groups: &[LinkGroup], removed: &[String]) {
        let closed: Vec<_> = self
            .links
            .groups
            .peek()
            .iter()
            .filter(|old| {
                let next = groups.iter().find(|g| g.id == old.id);
                removed.contains(&old.id)
                    || next.is_some_and(|g| {
                        old.members.iter().any(|(p, r, _)| {
                            *r == crate::links::Role::Lead
                                && !g.members.iter().any(|(n, _, _)| n == p)
                        })
                    })
            })
            .flat_map(|g| {
                g.members
                    .iter()
                    .filter(|(_, r, f)| {
                        *r == crate::links::Role::Follower && *f == crate::links::Follow::Group
                    })
                    .map(|(p, _, _)| p.clone())
            })
            .collect();
        self.links.groups.with_mut(|all| {
            all.retain(|g| !removed.contains(&g.id));
            for group in groups {
                if let Some(old) = all.iter_mut().find(|g| g.id == group.id) {
                    *old = group.clone();
                } else if all.len() < 256 {
                    all.push(group.clone());
                }
            }
        });
        // Preserve pending local changes to unrelated groups; suppress only this input.
        self.session.links_seen.with_mut(|seen| {
            seen.retain(|g| !removed.contains(&g.id));
            for group in groups {
                if let Some(old) = seen.iter_mut().find(|g| g.id == group.id) {
                    *old = group.clone();
                } else {
                    seen.push(group.clone());
                }
            }
        });
        // Closing a lead in another window closes only the followers removed there.
        for panel in closed {
            self.links.views.with_mut(|v| {
                v.remove(&panel);
            });
            self.links.contents.with_mut(|v| {
                v.remove(&panel);
            });
            self.shell.closed_panels.with_mut(|p| {
                p.insert(panel);
            });
        }
    }

    pub(super) async fn handle_panel_message(mut self, msg: &SessionMessage) -> bool {
        let me = self.session.window.peek().clone();
        match msg {
            SessionMessage::WindowTabs { from, tabs } => {
                if tabs.len() > 512 || !tabs.iter().all(WindowTab::valid) {
                    return true;
                }
                self.session.remote_tabs.with_mut(|peers| {
                    peers.retain(|p| p.window != *from);
                    if peers.len() < 64 {
                        peers.push(WindowTabs {
                            window: from.clone(),
                            tabs: tabs.clone(),
                            received: now_ms(),
                        });
                    }
                });
            }
            SessionMessage::WindowLeft { from } => {
                self.session
                    .remote_tabs
                    .with_mut(|peers| peers.retain(|p| p.window != *from));
                self.session
                    .peers
                    .with_mut(|peers| peers.retain(|p| p != from));
            }
            SessionMessage::Links {
                groups, removed, ..
            } => {
                if crate::window_panels::valid_groups(groups, removed) {
                    self.receive_groups(groups, removed);
                    self.hydrate_link_leads(groups).await;
                }
            }
            SessionMessage::FocusPanel { to, panel, .. } if *to == me => {
                if self
                    .session
                    .local_tabs
                    .peek()
                    .iter()
                    .any(|t| t.panel == *panel)
                {
                    if self
                        .session
                        .local_tabs
                        .peek()
                        .iter()
                        .any(|t| t.panel == *panel && !t.path.is_empty())
                    {
                        if let Some(subject) = self.link_subject(panel) {
                            self.set_active_document(Some(subject.node));
                        }
                    }
                    self.links.focused.set(Some(panel.clone()));
                    self.show_panel(panel);
                    self.session
                        .focus_requested
                        .with_mut(|n| *n = n.wrapping_add(1));
                }
            }
            SessionMessage::MovePanel { from, to, panel } if *to == me => {
                self.start_panel_drag(panel, Some(from.clone()));
            }
            SessionMessage::PanelDrag { from, to, transfer }
                if to.as_ref().is_none_or(|t| *t == me) =>
            {
                if !transfer.valid() {
                    return true;
                }
                if to.is_some()
                    && self.session.requested_move.peek().as_ref()
                        != Some(&(from.clone(), transfer.panel.clone()))
                {
                    return true;
                }
                self.session.requested_move.set(None);
                self.session.foreign_drag.set(Some(ForeignDrag {
                    from: from.clone(),
                    node: transfer.node.clone(),
                    source: transfer.source.clone(),
                    live: to.is_none(),
                    transfer: Some(*transfer.clone()),
                }));
                // Overview's explicit move request is already a destination action.
                if to.is_some() {
                    let _ = self.accept_drop().await;
                }
            }
            SessionMessage::PanelMoved { from, panel, .. } if *from == me => {
                if self.session.panel_offer.peek().as_deref() != Some(panel) {
                    return true;
                }
                self.session.panel_offer.set(None);
                self.session.own_drag.set(None);
                // Transfer ownership, never close membership or the remote followers.
                let node = self.link_subject(panel).map(|s| s.node);
                if !panel.starts_with("graph:") && self.document_opt_dirty(node) {
                    self.set_status(t!(self, L, "move-unsaved"));
                    return true;
                }
                self.links.views.with_mut(|v| {
                    v.remove(panel);
                });
                self.links.contents.with_mut(|v| {
                    v.remove(panel);
                });
                if let Some(node) = node.filter(|_| !panel.starts_with("graph:")) {
                    if !self
                        .links
                        .contents
                        .peek()
                        .keys()
                        .any(|p| self.link_subject(p).is_some_and(|s| s.node == node))
                    {
                        super::grouped_edits::forget_document(self, node);
                        self.docs
                            .open
                            .with_mut(|docs| docs.retain(|(id, _)| *id != node));
                        self.docs
                            .views
                            .with_mut(|docs| docs.retain(|n| n.id != node));
                        self.docs.editor_sessions.with_mut(|docs| {
                            docs.remove(&node);
                        });
                        if *self.docs.active.peek() == Some(node) {
                            let next = self.docs.open.peek().last().map(|(id, _)| *id);
                            self.set_active_document(next);
                        }
                    }
                }
                self.shell.closed_panels.with_mut(|p| {
                    p.insert(panel.clone());
                });
                self.set_status(t!(
                    self,
                    L,
                    "moved-to-window",
                    window = msg.sender().to_string()
                ));
            }
            SessionMessage::PanelMoved { panel, .. } => {
                if self
                    .session
                    .foreign_drag
                    .peek()
                    .as_ref()
                    .and_then(|d| d.transfer.as_ref())
                    .is_some_and(|t| t.panel == *panel)
                {
                    self.session.foreign_drag.set(None);
                }
            }
            SessionMessage::FocusPanel { .. }
            | SessionMessage::MovePanel { .. }
            | SessionMessage::PanelDrag { .. } => (),
            _ => return false,
        }
        true
    }

    async fn hydrate_link_leads(self, groups: &[LinkGroup]) {
        let outer = self.saved_sessions.active.peek().clone();
        for group in groups {
            if !group.members.iter().any(|(p, r, _)| {
                *r == crate::links::Role::Lead && self.links.contents.peek().contains_key(p)
            }) || self.document(group.subject.node).is_some()
            {
                continue;
            }
            let current = || {
                *self.saved_sessions.active.peek() == outer
                    && self
                        .links
                        .groups
                        .peek()
                        .iter()
                        .any(|g| g.id == group.id && g.subject == group.subject)
            };
            let sources: Vec<_> = self
                .sources
                .open
                .peek()
                .iter()
                .take(64)
                .map(|s| s.source.clone())
                .collect();
            for source in sources {
                if !current() {
                    return;
                }
                if let Ok(result) = source.query(Query::Node(group.subject.node)).await {
                    if let Some(node) = result.nodes.into_iter().find(|n| {
                        n.id == group.subject.node && crate::preview::allowed_path(&n.native_key)
                    }) {
                        if current() {
                            let _ = self.load_node_guarded(node, current).await;
                        }
                        break;
                    }
                }
            }
        }
    }

    fn document_opt_dirty(&self, node: Option<NodeId>) -> bool {
        node.and_then(|n| self.document(n))
            .is_some_and(|d| d.peek().dirty())
    }

    /// Offer an owned source-backed panel, preserving its group and graph view.
    pub fn start_panel_drag(mut self, panel: &str, to: Option<WindowId>) -> bool {
        let Some(tab) = self
            .session
            .local_tabs
            .peek()
            .iter()
            .find(|t| t.panel == panel)
            .cloned()
        else {
            return false;
        };
        if !tab.movable
            || tab.dirty
            || (!panel.starts_with("graph:")
                && self.document_opt_dirty(self.link_subject(panel).map(|s| s.node)))
        {
            self.set_status(t!(self, L, "move-unsaved"));
            return false;
        }
        let Some(subject) = self.link_subject(panel) else {
            return false;
        };
        let Some(node) = self.open_node_by_id(subject.node).or_else(|| {
            self.links
                .views
                .peek()
                .get(panel)
                .and_then(|view| self.open_node_by_id(view.detached.node))
        }) else {
            return false;
        };
        let Some(source) = self
            .sources
            .open
            .peek()
            .iter()
            .find(|s| s.descriptor.id == node.source)
            .map(|s| s.descriptor.clone())
        else {
            return false;
        };
        let view = self.links.views.peek().get(panel).cloned();
        let transfer = PanelTransfer {
            content_instance: self.links.contents.peek().contains_key(panel),
            panel: panel.into(),
            node: node.clone(),
            source,
            group: self.panel_group(panel),
            view,
        };
        if !transfer.valid() {
            return false;
        }
        self.session.panel_offer.set(Some(panel.into()));
        self.session.own_drag.set(Some(node.id));
        self.send(SessionMessage::PanelDrag {
            from: self.session.window.peek().clone(),
            to,
            transfer: Box::new(transfer),
        });
        true
    }

    pub(super) async fn accept_panel_drop(
        mut self,
        from: WindowId,
        transfer: PanelTransfer,
    ) -> Result<(), SourceError> {
        let outer = self.saved_sessions.active.peek().clone();
        self.attach_source(transfer.source).await?;
        if *self.saved_sessions.active.peek() != outer {
            return Err(SourceError::NotFound);
        }
        let node = transfer.node.id;
        let graph_node = transfer.node.clone();
        // Load content without taking focus; a graph does not need to open an editor.
        if transfer.view.is_none() {
            self.load_node_guarded(transfer.node, || {
                *self.saved_sessions.active.peek() == outer
            })
            .await?;
        }
        if *self.saved_sessions.active.peek() != outer {
            return Err(SourceError::NotFound);
        }
        let content_instance = transfer.content_instance || transfer.group.is_some();
        if let Some(group) = transfer.group {
            self.receive_groups(&[group], &[]);
        }
        if let Some(view) = transfer.view {
            if self.open_node_by_id(node).is_none() {
                self.docs.views.with_mut(|v| v.push(graph_node));
            }
            self.links.views.with_mut(|v| {
                v.insert(transfer.panel.clone(), view);
            });
        } else {
            if content_instance {
                self.links.contents.with_mut(|v| {
                    v.insert(transfer.panel.clone(), node);
                });
            }
            self.set_active_document(Some(node));
        }
        self.shell.closed_panels.with_mut(|p| {
            p.remove(&transfer.panel);
        });
        self.show_panel(&transfer.panel);
        self.send(SessionMessage::PanelMoved {
            from,
            to: self.session.window.peek().clone(),
            panel: transfer.panel,
        });
        Ok(())
    }
}
