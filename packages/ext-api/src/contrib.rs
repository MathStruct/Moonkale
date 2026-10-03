//! Contribution points. Milestone 1 has one: panels.

use moonkale_core::NodeId;

/// Which workbench tile a panel first appears in. Mirrors the tile ids the
/// shell's default layout uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelHome {
    /// The side bar (Explorer, Search, …).
    Side,
    /// The editor area (documents).
    Main,
    /// The bottom panel (terminals, problems).
    Bottom,
    /// The right-hand panel (the Agent).
    Right,
}

impl PanelHome {
    /// The id of the shell's layout tile this home maps to.
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
///
/// Built with [`PanelContribution::new`] and the builder methods (the struct
/// is `#[non_exhaustive]` since `lib-v1`, so new fields are not breaking):
///
/// ```
/// use moonkale_ext_api::{Activity, PanelContribution, PanelHome};
/// let p = PanelContribution::new("git", "Changes", PanelHome::Side)
///     .closable(true)
///     .activity(Activity::new("git", 40, "Git"));
/// assert!(p.closable && p.activity.is_some() && p.node.is_none());
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct PanelContribution {
    /// Unique across extensions; one-per-node panels suffix the node id.
    pub id: String,
    /// The tab's text.
    pub title: String,
    /// The tile it opens in the first time.
    pub home: PanelHome,
    /// Whether the tab has a close button.
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

impl PanelContribution {
    /// A panel that is not closable, not dirty, edits no node and has no
    /// activity-bar entry.
    pub fn new(id: impl Into<String>, title: impl Into<String>, home: PanelHome) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            home,
            closable: false,
            dirty: false,
            node: None,
            activity: None,
        }
    }
    /// Whether the tab has a close button.
    pub fn closable(mut self, closable: bool) -> Self {
        self.closable = closable;
        self
    }
    /// Whether the tab shows the unsaved-changes dot.
    pub fn dirty(mut self, dirty: bool) -> Self {
        self.dirty = dirty;
        self
    }
    /// The node this panel edits (`NodeId` or `Option<NodeId>`).
    pub fn node(mut self, node: impl Into<Option<NodeId>>) -> Self {
        self.node = node.into();
        self
    }
    /// Its activity-bar entry (`Activity` or `Option<Activity>`).
    pub fn activity(mut self, activity: impl Into<Option<Activity>>) -> Self {
        self.activity = activity.into();
        self
    }
}

/// An activity-bar entry for a static panel (spec 009). One registry drives
/// the desktop rail and the phone's bottom bar.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
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
    /// An entry with icon name, position and label; no badge, on the phone's bar.
    pub fn new(icon: &'static str, order: u16, label: impl Into<String>) -> Self {
        Self {
            icon,
            order,
            badge: 0,
            label: label.into(),
            phone_secondary: false,
        }
    }
    /// Show `n` as a bubble (`0` = none).
    pub fn badge(mut self, n: u32) -> Self {
        self.badge = n;
        self
    }
    /// On the phone, list it in the **More** sheet instead of the bar.
    pub fn phone_secondary(mut self) -> Self {
        self.phone_secondary = true;
        self
    }
}

/// A mark an extension puts on a file (Milestone 18 phase 3c): a letter on
/// the Explorer row and on the document's tab, a CSS class for its colour and
/// a tooltip. The shell draws marks without knowing who set them; git's
/// status letters are the first. Keyed by the file's native key in
/// `Workspace::contrib.file_marks`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct FileMark {
    /// The letter (git: `M`, `A`, `D`, `R`, `?`, `U`).
    pub letter: char,
    /// A CSS class for its colour (`mk-vcs-modified`, …).
    pub class: &'static str,
    /// The tooltip ("modified").
    pub title: &'static str,
}

impl FileMark {
    /// A mark from its letter, CSS class and tooltip.
    pub fn new(letter: char, class: &'static str, title: &'static str) -> Self {
        Self {
            letter,
            class,
            title,
        }
    }
}
