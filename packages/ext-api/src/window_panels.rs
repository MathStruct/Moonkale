//! Bounded metadata exchanged by windows of one outer session. Never buffer bodies.
use crate::links::{LinkGroup, LinkView};
use crate::WindowId;
use moonkale_core::{Node, SourceDescriptor};
use serde::{Deserialize, Serialize};

/// A host tab advertised in another window's overview.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WindowTab {
    /// Stable local panel identity.
    pub panel: String,
    /// Escaped display title.
    pub title: String,
    /// Relative path, empty for non-document panels.
    pub path: String,
    /// Host panel category.
    pub category: String,
    /// Group identity, when linked.
    pub group: Option<String>,
    /// Source-backed instance supported by the transfer protocol.
    #[serde(default)]
    pub movable: bool,
    /// Unsaved documents cannot move windows.
    pub dirty: bool,
}
impl WindowTab {
    /// Validate before retaining any catalogue from a peer.
    pub fn valid(&self) -> bool {
        self.panel.len() <= 256
            && self.title.len() <= 512
            && self.path.len() <= 4096
            && self.category.len() <= 64
            && self.group.as_ref().is_none_or(|g| g.len() <= 128)
    }
}
/// One live peer's latest catalogue. Receipt time is local, never trusted from the wire.
#[derive(Clone, Debug, PartialEq)]
pub struct WindowTabs {
    /// Publishing window.
    pub window: WindowId,
    /// Retained bounded metadata.
    pub tabs: Vec<WindowTab>,
    /// Local receipt clock.
    pub received: u64,
}
/// A single panel instance offered for a move, without its unsaved text.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PanelTransfer {
    /// Whether the source owns a content alias, including a detached instance.
    #[serde(default)]
    pub content_instance: bool,
    /// Offered panel identity.
    pub panel: String,
    /// Authorized source node to read at the destination.
    pub node: Node,
    /// Source descriptor, as for ordinary document moves.
    pub source: SourceDescriptor,
    /// Membership includes the remote members; it does not instantiate them.
    pub group: Option<LinkGroup>,
    /// Graph camera and detached subject, for graph instances.
    pub view: Option<LinkView>,
}

/// Reuse the session import's group, member, selection and camera limits.
pub(crate) fn valid_groups(groups: &[LinkGroup], removed: &[String]) -> bool {
    if removed.len() > 256 || removed.iter().any(|id| id.len() > 128) {
        return false;
    }
    let saved = crate::workspace::SavedWorkspace {
        id: String::new(),
        name: String::new(),
        touched: 0,
        sources: Vec::new(),
        documents: Vec::new(),
        active: None,
        settings: crate::SettingsFile::new(),
        layout: Default::default(),
        archived: false,
        link_groups: groups.to_vec(),
        link_views: Default::default(),
        link_contents: Default::default(),
    };
    saved.validate_links().is_ok()
}
impl PanelTransfer {
    /// Reject malformed membership, source identities or camera samples before attaching.
    pub fn valid(&self) -> bool {
        if self.panel.is_empty()
            || self.panel.len() > 128
            || self.node.native_key.len() > 4096
            || self.node.source != self.source.id
            || self.source.id.as_str().len() > 4096
            || !crate::preview::allowed_path(&self.node.native_key)
        {
            return false;
        }
        if let Some(group) = &self.group {
            if !valid_groups(std::slice::from_ref(group), &[])
                || !group.members.iter().any(|(p, _, _)| p == &self.panel)
            {
                return false;
            }
        }
        if let Some(view) = &self.view {
            if !self.panel.starts_with("graph:")
                || view.detached.selection.len() > 4096
                || view.detached.selection.iter().any(|id| id.len() > 256)
                || view
                    .camera
                    .is_some_and(|c| c.iter().any(|v| !v.is_finite()) || c[0] <= 0. || c[6] <= 0.)
            {
                return false;
            }
        }
        true
    }
}
