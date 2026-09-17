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
    pub fn fresh() -> Self {
        Self(uuid::Uuid::new_v4().simple().to_string())
    }
}

impl fmt::Display for WindowId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0[..self.0.len().min(8)])
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SessionMessage {
    /// A window came up and wants to know the session state.
    Hello {
        from: WindowId,
    },
    /// Reply to `Hello`: "I exist" (so the newcomer can count its peers).
    Welcome {
        from: WindowId,
    },
    /// Reply to `Hello` (and broadcast whenever a source is opened).
    SourceOpened {
        from: WindowId,
        descriptor: SourceDescriptor,
    },
    /// A document is being dragged out of `from`; other windows become
    /// drop targets. Carries everything needed to open it on the other side.
    DragStarted {
        from: WindowId,
        node: Node,
        source: SourceDescriptor,
    },
    DragEnded {
        from: WindowId,
    },
    /// `to` accepted the drop; `from` closes its copy.
    Moved {
        node: NodeId,
        from: WindowId,
        to: WindowId,
    },
}

impl SessionMessage {
    /// The window that *sent* this message (used to drop echoes of our own
    /// messages). For `Moved` that is the destination, not `from`.
    pub fn sender(&self) -> &WindowId {
        match self {
            SessionMessage::Hello { from }
            | SessionMessage::Welcome { from }
            | SessionMessage::SourceOpened { from, .. }
            | SessionMessage::DragStarted { from, .. }
            | SessionMessage::DragEnded { from } => from,
            SessionMessage::Moved { to, .. } => to,
        }
    }
}

/// Transport, implemented per platform: an in-process channel on desktop,
/// `BroadcastChannel` on web. `send` must not deliver back to the sender.
pub trait SessionBus {
    fn send(&self, msg: SessionMessage);
}
