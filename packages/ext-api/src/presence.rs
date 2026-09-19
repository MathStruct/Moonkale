//! Presence (Milestone 8, P-34): who else has this folder open, and what
//! they are looking at. A room per folder id on the server hub; the client
//! sends its own state and receives the full member list on every change.

use serde::{Deserialize, Serialize};
use std::rc::Rc;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Member {
    /// The window's id (a user may have several).
    pub window: String,
    pub name: String,
    /// Relative key of the active document, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<String>,
    /// Cursor line (0-based) in the active document (Milestone 9).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
}

impl Member {
    /// Two letters for a badge.
    pub fn initials(&self) -> String {
        let mut it = self
            .name
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty());
        let first = it.next().unwrap_or("?");
        match it.next() {
            Some(second) => format!(
                "{}{}",
                first.chars().next().unwrap_or('?').to_ascii_uppercase(),
                second.chars().next().unwrap_or('?').to_ascii_uppercase()
            ),
            None => first
                .chars()
                .take(2)
                .collect::<String>()
                .to_ascii_uppercase(),
        }
    }
}

/// Wire messages, both directions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PresenceMessage {
    /// Client → hub: join `room` as `member` (first message).
    Join { room: String, member: Member },
    /// Client → hub: my state changed.
    Update { member: Member },
    /// Hub → clients: everyone in the room (including the receiver).
    Members { members: Vec<Member> },
}

/// A live connection to the hub; dropping it leaves the room.
pub trait PresenceLink {
    fn update(&self, member: Member);
}

/// How a platform joins a room: `room`, the member to announce, and where
/// member lists go. `None` on platforms without a hub.
pub type JoinPresence =
    fn(String, Member, dioxus::prelude::Callback<Vec<Member>>) -> Rc<dyn PresenceLink>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initials() {
        let m = |n: &str| Member {
            window: "w".into(),
            name: n.into(),
            active: None,
            line: None,
        };
        assert_eq!(m("Daniel Boigk").initials(), "DB");
        assert_eq!(m("daniel").initials(), "DA");
        assert_eq!(m("").initials(), "?");
    }
}
