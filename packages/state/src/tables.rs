//! The tables of Moonkale's state and how their keys are laid out — the
//! schema the comparison (phase 5) measures and the migration fills. The
//! record *types* stay with their owners (`ext-api` settings, `core` events,
//! the agent's sessions); each implements [`crate::Record`] for its table
//! when it moves here.
//!
//! | table | key | value | today in |
//! |---|---|---|---|
//! | [`SETTINGS`] | `str(scope)` — `"user"`, `"folder"` + `str(folder id)`, later `"project"` + `str(id)` | the settings file of that scope | `settings.json` (user, per platform), `.moonkale/settings.json` |
//! | [`LAYOUT`] | `str(folder or project id)` | layout, open and active documents | `.moonkale/settings.json` |
//! | [`AGENT_SESSIONS`] | `str(folder id) · str("local") · str(session id)`: the Agent panel's session, whole; `str(folder id) · str("server") · str(session id)`: a server session's head (title, started), `· u64(n)` its `n`th transcript item | agent sessions, in the store of the folder's host | here since phase 5.15 (was `.moonkale/agent-sessions/`, imported once) |
//! | [`EVENTS`] | `str(folder id) · u128(event id)` | one entity-log event, compaction snapshots included (they are events) — in the store of the machine that hosts the folder | here since phase 5.10 (was `.moonkale/history.jsonl`, imported once) |
//! | [`SNAPSHOTS`] | — | reserved: snapshots turned out to be events ([`EVENTS`]) | — |
//! | [`PROJECTS`] | `str(project id)` | a project ([[Projects and Sources]]) | — (not built) |
//! | [`INDEX`] | `str(source id) · str(content hash) · str(model)` | derived data (symbols, chunks, embeddings) | memory only |
//!
//! Secrets are never stored here.

pub const SETTINGS: &str = "settings";
pub const LAYOUT: &str = "layout";
pub const AGENT_SESSIONS: &str = "agent_sessions";
pub const EVENTS: &str = "events";
pub const SNAPSHOTS: &str = "snapshots";
pub const PROJECTS: &str = "projects";
pub const INDEX: &str = "index";

/// Every table, for tools that list or export the store.
pub const ALL: &[&str] = &[
    SETTINGS,
    LAYOUT,
    AGENT_SESSIONS,
    EVENTS,
    SNAPSHOTS,
    PROJECTS,
    INDEX,
];
