//! Session-owned panel links. Content stays in documents; groups carry identity only.
use crate::Workspace;
use dioxus::prelude::*;
use moonkale_core::{Node, NodeId, NodeKind, SourceError};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The concrete host panel containing an extension view.
#[derive(Clone, Debug, PartialEq)]
pub struct PanelOrigin(pub String);
/// Resolve a view's owning panel, including when the same document is shown twice.
pub fn panel_instance(fallback: String) -> String {
    try_consume_context::<PanelOrigin>()
        .map(|p| p.0)
        .unwrap_or(fallback)
}
/// Keep ordinary editor DOM identities stable and namespace additional views.
pub fn panel_element_id(prefix: &str, node: NodeId) -> String {
    let base = format!("{prefix}-{node}");
    match try_consume_context::<PanelOrigin>().filter(|p| p.0.starts_with("lead:")) {
        Some(p) => format!("{base}-{}", p.0.replace(':', "-")),
        None => base,
    }
}

/// A member's responsibility in a link group.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    /// Content that steers the group.
    Lead,
    /// Context that follows its lead.
    Follower,
}
/// The subject a follower tracks.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Follow {
    /// Follow the lead.
    Group,
    /// Keep a fixed subject.
    Pinned(Subject),
    /// Follow the active editor.
    Active,
}
/// A subject can be a document, row or flow block selection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Subject {
    /// Owning content node.
    pub node: NodeId,
    /// Selected model identities, when the content supports selection.
    #[serde(default)]
    pub selection: Vec<String>,
}
impl Subject {
    /// Start at an unselected content node.
    pub fn new(node: NodeId) -> Self {
        Self {
            node,
            selection: Vec::new(),
        }
    }
}
/// One lead and its context panels. Panel ids belong to the host, never content.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkGroup {
    /// Opaque group identity.
    pub id: String,
    /// One-based palette index (1..=8).
    pub colour: u8,
    /// Distinct visible number, including when palette colours repeat.
    pub number: u32,
    /// Panel id, responsibility and following mode.
    pub members: Vec<(String, Role, Follow)>,
    /// Current subject, independent of the active editor.
    pub subject: Subject,
    /// Bounded navigation within this group.
    pub history: Vec<Subject>,
    /// Current history position.
    pub cursor: usize,
}
impl LinkGroup {
    /// Circled palette markers, followed by distinct numbers after eight.
    pub fn marker(&self) -> String {
        if (1..=8).contains(&self.number) {
            ["①", "②", "③", "④", "⑤", "⑥", "⑦", "⑧"][self.number as usize - 1].into()
        } else {
            format!("({})", self.number)
        }
    }
    /// Remove a member. Closing a lead returns group-followers to close;
    /// pins stay as ungrouped members with their original colour.
    pub fn close(&mut self, panel: &str) -> Vec<String> {
        let lead = self
            .members
            .iter()
            .any(|(id, role, _)| id == panel && *role == Role::Lead);
        let mut closed = Vec::new();
        self.members.retain(|(id, _, follow)| {
            if id == panel {
                return false;
            }
            if lead && *follow == Follow::Group {
                closed.push(id.clone());
                return false;
            }
            true
        });
        closed
    }
}
/// Retained state of one graph instance, also when detached from a group.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LinkView {
    /// Camera snapshot; independent for every view.
    pub camera: Option<[f32; 7]>,
    /// Subject retained after unlinking.
    pub detached: Subject,
}
/// Reactive links for the current outer session.
#[derive(Clone, Copy)]
pub struct LinkState {
    /// Groups and detached pinned members.
    pub groups: Signal<Vec<LinkGroup>>,
    /// Graph instances, including detached views.
    pub views: Signal<BTreeMap<String, LinkView>>,
    /// Stable content views and their subject when detached.
    pub contents: Signal<BTreeMap<String, NodeId>>,
    /// Current table-result graphs, derived and never persisted.
    pub results: Signal<BTreeMap<NodeId, std::rc::Rc<moonkale_core::QueryResult>>>,
    /// Focused panel, including context panels.
    pub focused: Signal<Option<String>>,
}
impl LinkState {
    pub(crate) fn new() -> Self {
        Self {
            groups: Signal::new_in_scope(Vec::new(), ScopeId::ROOT),
            views: Signal::new_in_scope(BTreeMap::new(), ScopeId::ROOT),
            contents: Signal::new_in_scope(BTreeMap::new(), ScopeId::ROOT),
            results: Signal::new_in_scope(BTreeMap::new(), ScopeId::ROOT),
            focused: Signal::new_in_scope(None, ScopeId::ROOT),
        }
    }
}
impl Workspace {
    /// The conventional content panel identity used by the bundled editors.
    pub fn content_panel(&self, node: NodeId) -> String {
        let table = self
            .open_node_by_id(node)
            .is_some_and(|n| n.kind == NodeKind::Table);
        format!("{}:{node}", if table { "table" } else { "editor" })
    }
    /// Read a panel's subject reactively. Ungrouped panels follow the active editor.
    pub fn link_subject(&self, panel: &str) -> Option<Subject> {
        for group in self.links.groups.read().iter() {
            if let Some((_, _, follow)) = group.members.iter().find(|(id, _, _)| id == panel) {
                return match follow {
                    Follow::Pinned(subject) => Some(subject.clone()),
                    Follow::Group => Some(group.subject.clone()),
                    Follow::Active => (*self.docs.active.read()).map(Subject::new),
                };
            }
        }
        if let Some(node) = self.links.contents.read().get(panel) {
            return Some(Subject::new(*node));
        }
        if let Some(view) = self.links.views.read().get(panel) {
            return Some(view.detached.clone());
        }
        if let Some(node) = panel
            .strip_prefix("editor:")
            .or_else(|| panel.strip_prefix("table:"))
            .and_then(|s| s.parse().ok())
        {
            return Some(Subject::new(node));
        }
        (*self.docs.active.read()).map(Subject::new)
    }
    /// Content behind an instance, including detached or pinned views.
    pub fn linked_content(&self, panel: &str) -> Option<NodeId> {
        let fallback = self.links.contents.read().get(panel).copied()?;
        self.link_subject(panel).map(|s| s.node).or(Some(fallback))
    }
    /// Group owning a concrete panel.
    pub fn panel_group(&self, panel: &str) -> Option<LinkGroup> {
        self.links
            .groups
            .read()
            .iter()
            .find(|g| g.members.iter().any(|(id, _, _)| id == panel))
            .cloned()
    }
    /// Open content and create a local graph beside it.
    pub async fn open_with_context(mut self, node: Node) -> Result<String, SourceError> {
        let session = self.saved_sessions.active.peek().clone();
        let id = node.id;
        self.open_node(node).await?;
        if *self.saved_sessions.active.peek() != session {
            return Err(SourceError::NotFound);
        }

        let number = self
            .links
            .groups
            .peek()
            .iter()
            .map(|g| g.number)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        let group = uuid::Uuid::new_v4().to_string();
        self.links.contents.with_mut(|views| {
            views.insert(format!("lead:{group}"), id);
        });
        self.links.views.with_mut(|views| {
            views.insert(
                format!("graph:{group}"),
                LinkView {
                    camera: None,
                    detached: Subject::new(id),
                },
            );
        });
        self.links.groups.with_mut(|groups| {
            groups.push(LinkGroup {
                id: group.clone(),
                colour: ((number - 1) % 8 + 1) as u8,
                number,
                members: vec![
                    (format!("lead:{group}"), Role::Lead, Follow::Group),
                    (format!("graph:{group}"), Role::Follower, Follow::Group),
                ],
                subject: Subject::new(id),
                history: vec![Subject::new(id)],
                cursor: 0,
            })
        });
        Ok(group)
    }
    /// Detach a member without closing it.
    pub fn unlink_panel(mut self, panel: &str) {
        if let Some(subject) = self.link_subject(panel) {
            self.links.contents.with_mut(|views| {
                if let Some(node) = views.get_mut(panel) {
                    *node = subject.node;
                }
            });
            self.links.views.with_mut(|views| {
                if let Some(view) = views.get_mut(panel) {
                    view.detached = subject;
                }
            });
        }
        self.links.groups.with_mut(|groups| {
            for g in groups.iter_mut() {
                g.members.retain(|(id, _, _)| id != panel);
            }
            groups.retain(|g| !g.members.is_empty());
        });
    }
    /// Link an existing panel to a target's group. A bare target becomes a lead.
    pub fn link_panel(mut self, panel: &str, target: &str, pin: bool) {
        if panel == target {
            return;
        }
        let target_group = self.panel_group(target);
        if let Some(g) = &target_group {
            if g.members.iter().any(|(id, _, _)| id == panel) {
                let subject = g.subject.clone();
                self.links.groups.with_mut(|groups| {
                    if let Some(g) = groups
                        .iter_mut()
                        .find(|g| g.id == target_group.as_ref().unwrap().id)
                    {
                        if let Some((_, Role::Follower, follow)) =
                            g.members.iter_mut().find(|(id, _, _)| id == panel)
                        {
                            *follow = if pin {
                                Follow::Pinned(subject)
                            } else {
                                Follow::Group
                            };
                        }
                    }
                });
                return;
            }
        }
        let Some(subject) = self.link_subject(target) else {
            return;
        };
        let content = self.open_node_by_id(
            self.link_subject(panel)
                .map(|s| s.node)
                .unwrap_or(subject.node),
        );
        if panel.starts_with("editor:")
            || panel.starts_with("table:")
            || self.links.contents.peek().contains_key(panel)
        {
            if let Some(content) = content {
                self.links.contents.with_mut(|views| {
                    views.insert(panel.into(), content.id);
                });
            }
        }
        if target_group.is_none() && (target.starts_with("editor:") || target.starts_with("table:"))
        {
            self.links.contents.with_mut(|views| {
                views.insert(target.into(), subject.node);
            });
        }
        self.unlink_panel(panel);
        let target_id = target_group.map(|g| g.id);
        self.links.groups.with_mut(|groups| {
            let index = target_id
                .and_then(|id| groups.iter().position(|g| g.id == id))
                .unwrap_or_else(|| {
                    let number = groups
                        .iter()
                        .map(|g| g.number)
                        .max()
                        .unwrap_or(0)
                        .saturating_add(1);
                    groups.push(LinkGroup {
                        id: uuid::Uuid::new_v4().to_string(),
                        colour: ((number - 1) % 8 + 1) as u8,
                        number,
                        members: vec![(target.into(), Role::Lead, Follow::Group)],
                        subject: subject.clone(),
                        history: vec![subject.clone()],
                        cursor: 0,
                    });
                    groups.len() - 1
                });
            groups[index].members.push((
                panel.into(),
                Role::Follower,
                if pin {
                    Follow::Pinned(subject)
                } else {
                    Follow::Group
                },
            ));
        });
    }
    /// Close membership and the group followers of a closing lead.
    pub fn close_linked_panel(mut self, panel: &str) {
        let closed = self.links.groups.with_mut(|groups| {
            let closed: Vec<_> = groups.iter_mut().flat_map(|g| g.close(panel)).collect();
            groups.retain(|g| !g.members.is_empty());
            closed
        });
        self.links.contents.with_mut(|views| {
            views.remove(panel);
            for id in &closed {
                views.remove(id);
            }
        });
        self.links.views.with_mut(|views| {
            views.remove(panel);
            for id in &closed {
                views.remove(id);
            }
        });
        self.shell.closed_panels.with_mut(|all| all.extend(closed));
    }
    /// Close one content view and its followers, retaining other views of
    /// the same document. Returns false for ordinary contributed panels.
    pub fn close_content_panel(mut self, panel: &str) -> bool {
        let Some(node) = self.linked_content(panel) else {
            return false;
        };
        self.close_linked_panel(panel);
        let remaining: Vec<_> = self.links.contents.peek().keys().cloned().collect();
        if !remaining
            .iter()
            .any(|p| self.linked_content(p) == Some(node))
        {
            self.close_node(node);
        }
        if self.links.focused.peek().as_deref() == Some(panel) {
            self.links.focused.set(None);
        }
        true
    }
    /// Synchronize selection without navigating or stealing keyboard focus.
    pub fn select_linked(mut self, panel: &str, selection: Vec<String>) {
        if selection.len() > 4096 || selection.iter().any(|s| s.len() > 256) {
            return;
        }
        if !self.links.groups.peek().iter().any(|g| {
            g.members
                .iter()
                .any(|(id, _, f)| id == panel && *f == Follow::Group)
                && g.subject.selection != selection
        }) {
            return;
        }
        self.links.groups.with_mut(|groups| {
            for g in groups {
                if g.members
                    .iter()
                    .any(|(id, _, f)| id == panel && *f == Follow::Group)
                    && g.subject.selection != selection
                {
                    g.subject.selection = selection.clone();
                }
            }
        });
    }
    /// Capture the originating group before asynchronous navigation.
    pub fn link_origin(&self, node: NodeId) -> Option<String> {
        let focused = self.links.focused.peek();
        self.links
            .groups
            .peek()
            .iter()
            .find(|g| {
                g.subject.node == node
                    && focused
                        .as_ref()
                        .is_some_and(|p| g.members.iter().any(|(id, _, _)| id == p))
            })
            .map(|g| g.id.clone())
    }
    /// Commit successful navigation to its originating group only.
    pub fn move_group_lead(self, group: &str, to: NodeId) {
        self.move_group_subject(group, Subject::new(to));
    }
    /// Record a group navigation, including row or block selection.
    pub fn move_group_subject(mut self, group: &str, subject: Subject) {
        let lead = self.links.groups.with_mut(|groups| {
            if let Some(g) = groups.iter_mut().find(|g| g.id == group) {
                if g.history.get(g.cursor) != Some(&subject) {
                    g.history.truncate(g.cursor.saturating_add(1));
                    g.history.push(subject.clone());
                    if g.history.len() > 256 {
                        g.history.remove(0);
                    }
                    g.cursor = g.history.len() - 1;
                }
                g.subject = subject;
                return g
                    .members
                    .iter()
                    .find(|(_, role, _)| *role == Role::Lead)
                    .map(|(panel, _, _)| panel.clone());
            }
            None
        });
        if let Some(lead) = lead {
            self.show_panel(&lead);
        }
    }
    /// Reveal a selected row/block through its lead and record Back's target.
    pub fn steer_linked_selection(self, panel: &str, selection: Vec<String>) {
        if selection.len() > 4096 || selection.iter().any(|s| s.len() > 256) {
            return;
        }
        if let (Some(group), Some(mut subject)) =
            (self.panel_group(panel), self.link_subject(panel))
        {
            subject.selection = selection;
            self.move_group_subject(&group.id, subject);
        }
    }
    /// Transfer the focused lead after synchronous navigation.
    pub fn move_linked_lead(self, from: NodeId, to: NodeId) {
        if let Some(group) = self.link_origin(from) {
            self.move_group_lead(&group, to);
        }
    }
    /// Steer one group from its graph; other groups retain their subjects.
    pub async fn steer_linked(self, panel: &str, node: Node) -> Result<(), SourceError> {
        let group = self.panel_group(panel).map(|g| g.id);
        let session = self.saved_sessions.active.peek().clone();
        let to = node.id;
        self.open_node(node).await?;
        if *self.saved_sessions.active.peek() == session {
            if let Some(group) = group {
                self.move_group_lead(&group, to);
            }
        }
        Ok(())
    }
    /// Back/forward within the focused group. Returns false for ordinary panels.
    pub fn navigate_linked(mut self, forward: bool) -> bool {
        let focused = self
            .links
            .focused
            .peek()
            .clone()
            .or_else(|| (*self.docs.active.peek()).map(|n| self.content_panel(n)));
        let Some(group) = focused.and_then(|p| self.panel_group(&p)) else {
            return false;
        };
        let next = if forward {
            group.cursor.checked_add(1)
        } else {
            group.cursor.checked_sub(1)
        };
        let Some((cursor, subject)) =
            next.and_then(|i| group.history.get(i).map(|s| (i, s.clone())))
        else {
            return true;
        };
        let node = subject.node;
        if self.open_node_by_id(node).is_none() {
            return true;
        }
        self.links.groups.with_mut(|groups| {
            if let Some(g) = groups.iter_mut().find(|g| g.id == group.id) {
                g.subject = subject;
                g.cursor = cursor;
            }
        });
        self.set_active_document(Some(node));
        if let Some((lead, _, _)) = group
            .members
            .iter()
            .find(|(_, role, _)| *role == Role::Lead)
        {
            self.show_panel(lead);
        }
        true
    }
}
