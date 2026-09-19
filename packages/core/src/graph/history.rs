//! The entity log (Milestone 8; [[ADR-0012 Two histories]]): an append-only
//! record of what changed in a workspace's graph, keyed by node id rather
//! than by path. Git keeps the history of *text in files*; this log keeps
//! the history of *entities* — creations, removals, renames, content
//! patches from any actor (user, agent, extension) — and the checkpoints
//! that tie the two together (a commit hash).
//!
//! State is a fold over the log: [`EntityLog::fold`] gives the live nodes
//! and tombstones, [`EntityLog::text_at`] replays content patches to show a
//! node's text as it was after any event. Persistence is one JSON object
//! per line ([`EntityLog::to_jsonl`] / [`from_jsonl`]), small enough to
//! rewrite whole on every append.

use crate::graph::Node;
use crate::source::TextPatch;
use crate::NodeId;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

/// Time-ordered id: milliseconds since the epoch plus a random tail, so ids
/// sort by time even across replicas (uuid v7 shape, our own encoding).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct EventId(pub u128);

impl EventId {
    pub fn new(at_ms: u64) -> Self {
        let rand: u64 = uuid::Uuid::new_v4().as_u128() as u64;
        EventId(((at_ms as u128) << 64) | rand as u128)
    }

    pub fn at_ms(&self) -> u64 {
        (self.0 >> 64) as u64
    }

    /// Short display form (`0192f3…`).
    pub fn short(&self) -> String {
        let s = format!("{:032x}", self.0);
        s[s.len() - 8..].to_string()
    }
}

impl std::fmt::Display for EventId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:032x}", self.0)
    }
}

/// Who did it. Free-form, `user:<name>`, `agent:<model>`, `ext:<id>`.
pub type Actor = String;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EventKind {
    /// A node came into existence (created here, or first seen). `text` is
    /// the initial content of a text node, so replay has a base.
    Add {
        node: Node,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
    },
    /// Tombstone.
    Remove { node: NodeId },
    /// The node's key changed; ids derive from keys, so it gets a new id.
    Rename {
        from: NodeId,
        to: NodeId,
        from_key: String,
        to_key: String,
    },
    /// Content changed by `patch`; `chars_after` lets a reader validate
    /// replay. `base` is the text *before* the patch, carried by the first
    /// content event of a node the log never saw added (files that existed
    /// before the log did), so replay has something to start from.
    Content {
        node: NodeId,
        patch: TextPatch,
        chars_after: usize,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        base: Option<String>,
    },
    /// A git commit captured the state up to here.
    Checkpoint { commit: String, message: String },
    /// The folded state at this point (Milestone 9): what compaction
    /// leaves in place of older events. `texts` are the replayable
    /// contents the log knew, so `text_at` can start here.
    Snapshot {
        live: Vec<Node>,
        texts: BTreeMap<NodeId, String>,
        /// How many events this snapshot replaced.
        folded: usize,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub id: EventId,
    /// Milliseconds since the Unix epoch.
    pub at: u64,
    pub actor: Actor,
    /// The node's key (path) at the time, for display without a lookup.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// Provenance: the event this one undoes, merges or was caused by.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cause: Option<EventId>,
    #[serde(flatten)]
    pub kind: EventKind,
}

impl Event {
    pub fn new(at: u64, actor: impl Into<Actor>, kind: EventKind) -> Self {
        let key = match &kind {
            EventKind::Add { node, .. } => Some(node.native_key.clone()),
            EventKind::Rename { to_key, .. } => Some(to_key.clone()),
            _ => None,
        };
        Self {
            id: EventId::new(at),
            at,
            actor: actor.into(),
            key,
            cause: None,
            kind,
        }
    }

    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// The node this event is about (the *new* id for a rename).
    pub fn node(&self) -> Option<NodeId> {
        match &self.kind {
            EventKind::Add { node, .. } => Some(node.id),
            EventKind::Remove { node } | EventKind::Content { node, .. } => Some(*node),
            EventKind::Rename { to, .. } => Some(*to),
            EventKind::Checkpoint { .. } | EventKind::Snapshot { .. } => None,
        }
    }

    pub fn summary(&self) -> String {
        match &self.kind {
            EventKind::Add { node, .. } => format!("added {}", node.native_key),
            EventKind::Remove { .. } => "removed".into(),
            EventKind::Rename {
                from_key, to_key, ..
            } => format!("renamed {from_key} → {to_key}"),
            EventKind::Content { patch, .. } => {
                let TextPatch::Replace(s) = patch;
                let ins: usize = s.iter().map(|x| x.text.chars().count()).sum();
                let del: usize = s.iter().map(|x| x.end.saturating_sub(x.start)).sum();
                format!("edited (+{ins} −{del})")
            }
            EventKind::Checkpoint { commit, message } => {
                format!("commit {} {}", &commit[..commit.len().min(7)], message)
            }
            EventKind::Snapshot { live, folded, .. } => {
                format!("snapshot of {} nodes ({folded} events folded)", live.len())
            }
        }
    }
}

/// The folded state: what exists now, what was removed, and where each
/// node's key went.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    pub live: BTreeMap<NodeId, Node>,
    pub tombstones: BTreeMap<NodeId, EventId>,
    /// Old id → new id for renames.
    pub renamed: HashMap<NodeId, NodeId>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EntityLog {
    events: Vec<Event>,
}

impl EntityLog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn events(&self) -> &[Event] {
        &self.events
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Append, keeping the log ordered by id (a replica's older event lands
    /// where it belongs).
    pub fn append(&mut self, event: Event) -> EventId {
        let id = event.id;
        let pos = self.events.partition_point(|e| e.id <= id);
        self.events.insert(pos, event);
        id
    }

    /// Merge another log (set union by id).
    pub fn merge(&mut self, other: &EntityLog) -> usize {
        let mut added = 0;
        for e in &other.events {
            if !self.events.iter().any(|x| x.id == e.id) {
                self.append(e.clone());
                added += 1;
            }
        }
        added
    }

    /// Events touching `node`, following renames backwards.
    pub fn for_node(&self, node: NodeId) -> Vec<&Event> {
        let mut ids = vec![node];
        // Walk rename chains: the node's earlier ids.
        loop {
            let last = *ids.last().unwrap();
            match self.events.iter().find_map(|e| match &e.kind {
                EventKind::Rename { from, to, .. } if *to == last => Some(*from),
                _ => None,
            }) {
                Some(prev) if !ids.contains(&prev) => ids.push(prev),
                _ => break,
            }
        }
        self.events
            .iter()
            .filter(|e| e.node().is_some_and(|n| ids.contains(&n)))
            .collect()
    }

    /// Fold up to (and including) `until`, or the whole log.
    pub fn fold(&self, until: Option<EventId>) -> State {
        let mut st = State::default();
        for e in &self.events {
            if until.is_some_and(|u| e.id > u) {
                break;
            }
            match &e.kind {
                EventKind::Add { node, .. } => {
                    st.tombstones.remove(&node.id);
                    st.live.insert(node.id, node.clone());
                }
                EventKind::Remove { node } => {
                    st.live.remove(node);
                    st.tombstones.insert(*node, e.id);
                }
                EventKind::Rename {
                    from, to, to_key, ..
                } => {
                    if let Some(mut n) = st.live.remove(from) {
                        n.id = *to;
                        n.native_key = to_key.clone();
                        n.label = to_key.rsplit('/').next().unwrap_or(to_key).to_string();
                        st.live.insert(*to, n);
                    }
                    st.renamed.insert(*from, *to);
                }
                EventKind::Snapshot { live, .. } => {
                    st.live.clear();
                    for n in live {
                        st.live.insert(n.id, n.clone());
                    }
                }
                EventKind::Content { .. } | EventKind::Checkpoint { .. } => {}
            }
        }
        st
    }

    /// Fold every event older than the last `keep` into one snapshot,
    /// keeping checkpoints (they are the seam to git). Returns how many
    /// events were folded. `text_at` answers stay the same for every node
    /// the log had a base for.
    pub fn compact(&mut self, keep: usize, at: u64, actor: impl Into<Actor>) -> usize {
        if self.events.len() <= keep {
            return 0;
        }
        let cut = self.events.len() - keep;
        let boundary = self.events[cut - 1].id;
        let st = self.fold(Some(boundary));
        let mut texts = BTreeMap::new();
        for id in st.live.keys() {
            if let Some(t) = self.text_at(*id, Some(boundary)) {
                texts.insert(*id, t);
            }
        }
        let (old, rest): (Vec<Event>, Vec<Event>) = std::mem::take(&mut self.events)
            .into_iter()
            .partition(|e| e.id <= boundary);
        let checkpoints: Vec<Event> = old
            .iter()
            .filter(|e| matches!(e.kind, EventKind::Checkpoint { .. }))
            .cloned()
            .collect();
        let folded = old.len() - checkpoints.len();
        let mut snapshot = Event::new(
            at,
            actor,
            EventKind::Snapshot {
                live: st.live.into_values().collect(),
                texts,
                folded,
            },
        );
        // The snapshot sits exactly at the boundary so ordering is preserved.
        snapshot.id = EventId(boundary.0 + 1);
        self.events = checkpoints;
        self.events.push(snapshot);
        self.events.extend(rest);
        self.events.sort_by_key(|e| e.id);
        folded
    }

    /// The text of `node` after event `until` (or now), replayed from the
    /// last base (`Add` with text) through the content patches, across
    /// renames. `None` when the log has no base for the node.
    pub fn text_at(&self, node: NodeId, until: Option<EventId>) -> Option<String> {
        let chain = self.for_node(node);
        let mut text: Option<String> = None;
        // Start from the latest snapshot at or before `until` that knows the node.
        for e in self.events.iter().rev() {
            if until.is_some_and(|u| e.id > u) {
                continue;
            }
            if let EventKind::Snapshot { texts, .. } = &e.kind {
                if let Some(t) = texts.get(&node) {
                    text = Some(t.clone());
                }
                break;
            }
        }
        let snapshot_id = self
            .events
            .iter()
            .rev()
            .find(|e| {
                matches!(e.kind, EventKind::Snapshot { .. }) && !until.is_some_and(|u| e.id > u)
            })
            .map(|e| e.id);
        for e in chain {
            if until.is_some_and(|u| e.id > u) {
                break;
            }
            if snapshot_id.is_some_and(|sid| e.id <= sid) {
                continue;
            }
            match &e.kind {
                EventKind::Add { text: Some(t), .. } => text = Some(t.clone()),
                EventKind::Content { patch, base, .. } => {
                    if text.is_none() {
                        text = base.clone();
                    }
                    if let Some(t) = &text {
                        text = patch.apply(t).ok();
                    }
                }
                EventKind::Remove { .. } => text = None,
                _ => {}
            }
        }
        text
    }

    /// One JSON object per line.
    pub fn to_jsonl(&self) -> String {
        let mut out = String::new();
        for e in &self.events {
            if let Ok(line) = serde_json::to_string(e) {
                out.push_str(&line);
                out.push('\n');
            }
        }
        out
    }

    /// Tolerant: bad lines are skipped.
    pub fn from_jsonl(text: &str) -> Self {
        let mut log = Self::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Ok(e) = serde_json::from_str::<Event>(line) {
                log.append(e);
            }
        }
        log
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{NodeKind, Version};
    use crate::source::Splice;
    use crate::SourceId;

    fn node(key: &str) -> Node {
        let src = SourceId::new("folder:/x");
        Node {
            id: NodeId::derive(&src, key),
            source: src,
            kind: NodeKind::File,
            label: key.rsplit('/').next().unwrap().into(),
            native_key: key.into(),
            content: None,
            version: Version::default(),
        }
    }

    #[test]
    fn fold_replay_rename_and_round_trip() {
        let mut log = EntityLog::new();
        let a = node("a.md");
        log.append(Event::new(
            1,
            "user:t",
            EventKind::Add {
                node: a.clone(),
                text: Some("hello".into()),
            },
        ));
        let e2 = log.append(Event::new(
            2,
            "user:t",
            EventKind::Content {
                node: a.id,
                patch: TextPatch::Replace(vec![Splice {
                    start: 5,
                    end: 5,
                    text: " world".into(),
                }]),
                chars_after: 11,
                base: None,
            },
        ));
        let b = node("b.md");
        log.append(Event::new(
            3,
            "user:t",
            EventKind::Rename {
                from: a.id,
                to: b.id,
                from_key: "a.md".into(),
                to_key: "b.md".into(),
            },
        ));
        log.append(Event::new(
            4,
            "agent:mock",
            EventKind::Content {
                node: b.id,
                patch: TextPatch::Replace(vec![Splice {
                    start: 0,
                    end: 5,
                    text: "HELLO".into(),
                }]),
                chars_after: 11,
                base: None,
            },
        ));
        log.append(Event::new(
            5,
            "user:t",
            EventKind::Checkpoint {
                commit: "abc1234".into(),
                message: "m".into(),
            },
        ));

        let st = log.fold(None);
        assert!(st.live.contains_key(&b.id) && !st.live.contains_key(&a.id));
        assert_eq!(st.live[&b.id].native_key, "b.md");
        assert_eq!(st.renamed[&a.id], b.id);
        assert_eq!(log.text_at(b.id, None).as_deref(), Some("HELLO world"));
        assert_eq!(log.text_at(b.id, Some(e2)).as_deref(), Some("hello world"));
        assert_eq!(log.for_node(b.id).len(), 4);

        log.append(Event::new(6, "user:t", EventKind::Remove { node: b.id }));
        let st = log.fold(None);
        assert!(st.live.is_empty() && st.tombstones.contains_key(&b.id));
        assert_eq!(log.text_at(b.id, None), None);

        let text = log.to_jsonl();
        let back = EntityLog::from_jsonl(&text);
        assert_eq!(back, log);
        assert_eq!(back.events()[4].summary(), "commit abc1234 m");
    }

    #[test]
    fn content_with_a_base_replays_files_the_log_never_saw_added() {
        let mut log = EntityLog::new();
        let a = node("old.md");
        let e = log.append(
            Event::new(
                1,
                "u",
                EventKind::Content {
                    node: a.id,
                    patch: TextPatch::Replace(vec![Splice {
                        start: 3,
                        end: 3,
                        text: "!".into(),
                    }]),
                    chars_after: 4,
                    base: Some("abc".into()),
                },
            )
            .with_key("old.md"),
        );
        assert_eq!(log.text_at(a.id, Some(e)).as_deref(), Some("abc!"));
        assert_eq!(log.events()[0].key.as_deref(), Some("old.md"));
    }

    #[test]
    fn compaction_keeps_answers_and_checkpoints() {
        let mut log = EntityLog::new();
        let a = node("a.md");
        log.append(Event::new(
            1,
            "u",
            EventKind::Add {
                node: a.clone(),
                text: Some("v1".into()),
            },
        ));
        log.append(Event::new(
            2,
            "u",
            EventKind::Checkpoint {
                commit: "c1".into(),
                message: "one".into(),
            },
        ));
        log.append(Event::new(
            3,
            "u",
            EventKind::Content {
                node: a.id,
                patch: TextPatch::Replace(vec![Splice {
                    start: 1,
                    end: 2,
                    text: "2".into(),
                }]),
                chars_after: 2,
                base: None,
            },
        ));
        let b = node("b.md");
        log.append(Event::new(
            4,
            "u",
            EventKind::Add {
                node: b.clone(),
                text: Some("bee".into()),
            },
        ));
        log.append(Event::new(
            5,
            "u",
            EventKind::Content {
                node: a.id,
                patch: TextPatch::Replace(vec![Splice {
                    start: 1,
                    end: 2,
                    text: "3".into(),
                }]),
                chars_after: 2,
                base: None,
            },
        ));
        let before_a = log.text_at(a.id, None);
        let before_b = log.text_at(b.id, None);
        let folded = log.compact(1, 10, "u");
        assert_eq!(folded, 3, "add, content, add folded; the checkpoint stays");
        let kinds: Vec<&str> = log
            .events()
            .iter()
            .map(|e| match &e.kind {
                EventKind::Checkpoint { .. } => "checkpoint",
                EventKind::Snapshot { .. } => "snapshot",
                EventKind::Content { .. } => "content",
                _ => "other",
            })
            .collect();
        assert_eq!(kinds, ["checkpoint", "snapshot", "content"]);
        assert_eq!(log.text_at(a.id, None), before_a);
        assert_eq!(log.text_at(b.id, None), before_b);
        assert_eq!(log.fold(None).live.len(), 2);
        let back = EntityLog::from_jsonl(&log.to_jsonl());
        assert_eq!(back, log);
        assert_eq!(log.compact(10, 11, "u"), 0);
    }

    #[test]
    fn append_keeps_time_order_and_merge_dedups() {
        let mut log = EntityLog::new();
        let late = Event::new(
            10,
            "u",
            EventKind::Checkpoint {
                commit: "b".into(),
                message: String::new(),
            },
        );
        let early = Event::new(
            5,
            "u",
            EventKind::Checkpoint {
                commit: "a".into(),
                message: String::new(),
            },
        );
        log.append(late.clone());
        log.append(early.clone());
        assert_eq!(log.events()[0].id, early.id);
        let mut other = EntityLog::new();
        other.append(late);
        other.append(Event::new(
            7,
            "u",
            EventKind::Checkpoint {
                commit: "c".into(),
                message: String::new(),
            },
        ));
        assert_eq!(log.merge(&other), 1);
        assert_eq!(log.len(), 3);
    }
}
