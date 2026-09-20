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
    /// An entry in the activity bar / phone bar (spec 009); `None` for
    /// document panels and panels reached another way.
    pub activity: Option<Activity>,
}

/// An activity-bar entry for a static panel (spec 009). One registry drives
/// the desktop rail and the phone's bottom bar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Activity {
    /// Icon name the shell resolves to an inline SVG: `files`, `search`,
    /// `links`, `git`, `history`, `agent`, `terminal`, `graph`, `settings`,
    /// `table`, `flow`, `image`, `puzzle` (unknown names get a generic dot).
    pub icon: &'static str,
    /// Position; built-ins use 10, 20, …, extensions 100+.
    pub order: u16,
    /// A count shown as a bubble (changed files, pending approvals), `0` = none.
    pub badge: u32,
    /// Short label under the icon (phone tiles, rail tooltip).
    pub label: String,
    /// On the phone, keep this entry in the **More** sheet rather than the
    /// bar (side panels people open rarely on a phone: Links, Git, History).
    pub phone_secondary: bool,
}

impl Activity {
    pub fn new(icon: &'static str, order: u16, label: impl Into<String>) -> Self {
        Self {
            icon,
            order,
            badge: 0,
            label: label.into(),
            phone_secondary: false,
        }
    }
    pub fn badge(mut self, n: u32) -> Self {
        self.badge = n;
        self
    }
    pub fn phone_secondary(mut self) -> Self {
        self.phone_secondary = true;
        self
    }
}
