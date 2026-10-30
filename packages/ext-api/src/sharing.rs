//! Ephemeral, opt-in position sharing. No buffer or input bodies cross this boundary.
use serde::{Deserialize, Serialize};

/// Publication cadence and receiver expiry, in milliseconds.
pub const INTERVAL_MS: u64 = 50;
/// Positions disappear after a suspended/disconnected publisher stops refreshing them.
pub const STALE_MS: u64 = 15_000;

/// A caret and selection in an already-open document. Offsets are UTF-16.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cursor {
    /// Source identity; identical relative paths in other sources never match.
    pub source: String,
    /// Relative source key, never document text.
    pub document: String,
    /// Zero-based line.
    pub line: u32,
    /// Zero-based UTF-16 column.
    pub column: u32,
    /// Absolute selection start.
    pub anchor: u32,
    /// Absolute caret end.
    pub head: u32,
}

/// Pointer in a named graph's world space, independent of viewport zoom.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pointer {
    /// Semantic graph/canvas identity.
    pub view: String,
    /// World coordinates.
    pub x: f64,
    /// World coordinates.
    pub y: f64,
}

/// Shared metadata only; absent unless this participant explicitly enabled sharing.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Position {
    /// Publisher clock, refreshed by heartbeats. Hub replaces it with receipt time.
    pub updated: u64,
    /// Human caret, independently optional.
    pub cursor: Option<Cursor>,
    /// Graph pointer, independently optional.
    pub pointer: Option<Pointer>,
    /// Agent's current approved reveal/read position, independently opt-in with cursor sharing.
    pub agent: Option<(String, Cursor)>,
}
impl Position {
    /// Bound labels and coordinates before sending or accepting metadata.
    pub fn valid(&self) -> bool {
        let cursor = |c: &Cursor| {
            c.source.len() <= 4096 && c.document.len() <= 4096 && !c.document.contains('\0')
        };
        self.cursor.as_ref().is_none_or(cursor)
            && self
                .agent
                .as_ref()
                .is_none_or(|(name, c)| name.len() <= 128 && cursor(c))
            && self.pointer.as_ref().is_none_or(|p| {
                p.view.len() <= 4096
                    && p.x.is_finite()
                    && p.y.is_finite()
                    && p.x.abs() <= 1e9
                    && p.y.abs() <= 1e9
            })
    }
    /// Ignore stale values even while a transport remains connected.
    pub fn fresh(&self, now: u64) -> bool {
        self.valid() && now.saturating_sub(self.updated) < STALE_MS
    }
}

/// Per-window, per-session switches; intentionally never persisted.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sharing {
    /// Share caret and selections.
    pub cursor: bool,
    /// Share graph pointer.
    pub pointer: bool,
    /// Share the demonstrated agent participant independently of the human caret.
    pub agent_enabled: bool,
    /// Most recent pointer sample (last write wins).
    pub position: Option<Pointer>,
    /// Agent read/reveal sample.
    pub agent: Option<(String, Cursor)>,
    /// Participant currently followed.
    pub following: Option<String>,
    /// Last admitted follow read (participant, source, relative key).
    pub follow_request: Option<(String, String, String)>,
    /// Follow reads are limited to one per second and 32 per explicit follow.
    pub follow_opened: u32,
    /// Last follow read admission clock.
    pub follow_read_at: u64,
    /// Last published state, for coalescing and heartbeats.
    pub last: Option<Position>,
    /// Publication clock.
    pub sent: u64,
}
impl Sharing {
    /// At most twenty publications per second; heartbeat only while sharing.
    pub fn due(&self, position: &Position, now: u64) -> bool {
        now.saturating_sub(self.sent) >= INTERVAL_MS
            && (self.last.as_ref().is_none_or(|last| {
                last.cursor != position.cursor
                    || last.pointer != position.pointer
                    || last.agent != position.agent
            }) || now.saturating_sub(self.sent) >= 5_000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sharing_is_off_by_default() {
        let s = Sharing::default();
        assert!(!s.cursor && !s.pointer && !s.agent_enabled && s.following.is_none());
    }
    #[test]
    fn coalesces_last_write_at_twenty_hz_and_heartbeats() {
        let mut s = Sharing {
            sent: 100,
            last: Some(Position::default()),
            ..Sharing::default()
        };
        let p = Position {
            pointer: Some(Pointer {
                view: "graph".into(),
                x: 4.,
                y: 5.,
            }),
            ..Position::default()
        };
        assert!(!s.due(&p, 149));
        assert!(s.due(&p, 150));
        s.last = Some(p.clone());
        s.sent = 150;
        assert!(!s.due(&p, 200));
        assert!(s.due(&p, 5150));
    }
    #[test]
    fn expires_and_rejects_nonfinite_coordinates() {
        let mut p = Position {
            updated: 100,
            ..Position::default()
        };
        assert!(p.fresh(15_099));
        assert!(!p.fresh(15_100));
        p.pointer = Some(Pointer {
            view: "graph".into(),
            x: f64::NAN,
            y: 0.,
        });
        assert!(!p.fresh(100));
    }
}
