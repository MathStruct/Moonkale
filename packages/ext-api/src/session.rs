//! The session bus — how the windows (desktop) and tabs (web) of **one
//! user** talk to each other. It is deliberately shaped like the future
//! multi-user presence channel (see vault `architecture/Collaboration.md`):
//! a window is just a peer with zero latency, and a user on another machine
//! is a peer behind the server. Same messages, different transport.
//!
//! Milestone 1 uses it for two things: a new window learns which sources are
//! open, and a document can be dragged from one window into another.

use moonkale_core::{Node, NodeId, SourceDescriptor};
use serde::{Deserialize, Serialize};
use std::fmt;

/// One window / tab. Random per instance; never persisted.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WindowId(pub String);

impl WindowId {
    /// A new random id for this window.
    pub fn fresh() -> Self {
        Self(uuid::Uuid::new_v4().simple().to_string())
    }
}

impl fmt::Display for WindowId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0[..self.0.len().min(8)])
    }
}

/// What the windows of one session tell each other (the session bus).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SessionMessage {
    /// Message confined to a named outer session. One envelope maximum.
    Scoped {
        /// Opaque outer-session identity.
        session: String,
        /// Existing window protocol.
        message: Box<SessionMessage>,
    },
    /// Bounded catalogue of this window's open panels, refreshed while alive.
    WindowTabs {
        /// Publisher.
        from: WindowId,
        /// Metadata only, never previews or document bodies.
        tabs: Vec<crate::window_panels::WindowTab>,
    },
    /// Changed groups and tombstones, with local panel ownership left untouched.
    Links {
        /// Publisher.
        from: WindowId,
        /// Changed groups only.
        groups: Vec<crate::links::LinkGroup>,
        /// Removed group identities.
        removed: Vec<String>,
    },
    /// Activate a panel in its owning window.
    FocusPanel {
        /// Requesting window.
        from: WindowId,
        /// Owning window.
        to: WindowId,
        /// Advertised panel identity.
        panel: String,
    },
    /// Ask an owning window to offer one of its panels for transfer.
    MovePanel {
        /// Requesting window.
        from: WindowId,
        /// Owning window.
        to: WindowId,
        /// Advertised panel identity.
        panel: String,
    },
    /// Transfer one instance while retaining its group membership.
    PanelDrag {
        /// Offering window.
        from: WindowId,
        /// Optional requesting destination (overview moves).
        to: Option<WindowId>,
        /// Source-backed panel metadata only.
        transfer: Box<crate::window_panels::PanelTransfer>,
    },
    /// A panel offer was accepted; only the origin relinquishes it.
    PanelMoved {
        /// Origin window.
        from: WindowId,
        /// Destination window (sender).
        to: WindowId,
        /// Moved panel.
        panel: String,
    },
    /// The window leaves this outer session.
    WindowLeft {
        /// Departing window.
        from: WindowId,
    },
    /// Ephemeral opt-in presence over the local desktop bus.
    Presence {
        /// Sending window.
        from: WindowId,
        /// Metadata only.
        member: crate::presence::Member,
    },
    /// A window came up and wants to know the session state.
    Hello {
        /// The window that sent it.
        from: WindowId,
    },
    /// Reply to `Hello`: "I exist" (so the newcomer can count its peers).
    Welcome {
        /// The window that sent it.
        from: WindowId,
    },
    /// Reply to `Hello` (and broadcast whenever a source is opened).
    SourceOpened {
        /// The window that opened it.
        from: WindowId,
        /// The source, to attach it by descriptor.
        descriptor: SourceDescriptor,
    },
    /// A document is being dragged out of `from`; other windows become
    /// drop targets. Carries everything needed to open it on the other side.
    DragStarted {
        /// The window the drag started in.
        from: WindowId,
        /// The dragged document's node.
        node: Node,
        /// Its source.
        source: SourceDescriptor,
    },
    /// The drag ended without a drop.
    DragEnded {
        /// The window the drag started in.
        from: WindowId,
    },
    /// `to` accepted the drop; `from` closes its copy.
    Moved {
        /// The moved document.
        node: NodeId,
        /// The window it left.
        from: WindowId,
        /// The window it moved to.
        to: WindowId,
    },
}

impl SessionMessage {
    /// Parse the browser envelope through the existing bounded, fuzzed session JSON boundary.
    pub fn parse_wire(text: &str) -> Result<Self, &'static str> {
        #[derive(Deserialize)]
        struct Envelope {
            message: SessionMessage,
        }
        let envelope: Envelope = moonkale_state::session_bundle::parse(text)?;
        Ok(envelope.message)
    }

    /// The window that *sent* this message (used to drop echoes of our own
    /// messages). For `Moved` that is the destination, not `from`.
    pub fn sender(&self) -> &WindowId {
        match self {
            SessionMessage::Scoped { message, .. } => message.sender(),
            SessionMessage::WindowTabs { from, .. }
            | SessionMessage::Links { from, .. }
            | SessionMessage::FocusPanel { from, .. }
            | SessionMessage::MovePanel { from, .. }
            | SessionMessage::PanelDrag { from, .. }
            | SessionMessage::WindowLeft { from }
            | SessionMessage::Hello { from }
            | SessionMessage::Presence { from, .. }
            | SessionMessage::Welcome { from }
            | SessionMessage::SourceOpened { from, .. }
            | SessionMessage::DragStarted { from, .. }
            | SessionMessage::DragEnded { from } => from,
            SessionMessage::Moved { to, .. } | SessionMessage::PanelMoved { to, .. } => to,
        }
    }
}

/// Transport, implemented per platform: an in-process channel on desktop,
/// `BroadcastChannel` on web. `send` must not deliver back to the sender.
pub trait SessionBus {
    /// Send a message to every other window of the session.
    fn send(&self, msg: SessionMessage);
}
