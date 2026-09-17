//! Contribution points. Milestone 1 has one: panels.

use moonkale_core::NodeId;

/// Which workbench tile a panel first appears in. Mirrors the tile ids the
/// shell's default layout uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelHome {
    Side,
    Main,
    Bottom,
    Right,
}

impl PanelHome {
    pub fn tile_id(self) -> &'static str {
        match self {
            PanelHome::Side => "side",
            PanelHome::Main => "main",
            PanelHome::Bottom => "bottom",
            PanelHome::Right => "right",
        }
    }
}

/// A dockable panel. `id` must be unique across all extensions; extensions
/// that open one panel per node suffix the node id (`"editor:<uuid>"`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PanelContribution {
    pub id: String,
    pub title: String,
    pub home: PanelHome,
    pub closable: bool,
    /// Shown as a dot on the tab (unsaved changes).
    pub dirty: bool,
    /// The node this panel edits, if any. The shell brings the panel of the
    /// workspace's active node to the front.
    pub node: Option<NodeId>,
}
