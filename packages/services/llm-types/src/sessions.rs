//! Agent sessions that live on a server (Milestone 12): the wire types
//! between the Agent panel and `moonkale_server::agent_sessions`. Pure data, so the
//! panel (wasm), the client wrappers and the server share one definition.

use crate::{Class, Decision, LlmSettings, ToolOutcome};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One transcript entry — what the Agent panel draws.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SessionItem {
    User {
        text: String,
    },
    Assistant {
        text: String,
    },
    Tool {
        id: String,
        name: String,
        input: Value,
        class: Class,
        decision: Decision,
        outcome: Option<ToolOutcome>,
        summary: String,
    },
    Error {
        text: String,
    },
}

/// A tool call waiting for a client's answer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PendingApproval {
    pub call_id: String,
    pub name: String,
    pub input: Value,
    pub class: Class,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SessionSummary {
    /// Owning outer workspace; old conversations belong to Default.
    #[serde(default = "default_outer_session")]
    pub outer_session: String,
    pub id: String,
    pub folder: String,
    pub title: String,
    pub started: u64,
    pub running: bool,
    pub items: usize,
}

/// What `agent_events(since)` returns.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SessionState {
    /// Recent ephemeral display requests. Not replayed from saved transcripts.
    #[serde(default)]
    pub displays: Vec<DisplayEvent>,
    pub id: String,
    pub title: String,
    pub provider: String,
    pub running: bool,
    /// Items from `since` on; the client appends them.
    pub items: Vec<SessionItem>,
    pub total: usize,
    pub pending: Option<PendingApproval>,
}

/// What a client sends with a message: its resolved LLM settings and the
/// policy it would apply locally.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TurnSettings {
    /// Saved profile name captured when the person sends the turn.
    #[serde(default)]
    pub display_name: String,
    /// Owning outer workspace (missing on old clients means Default).
    #[serde(default)]
    pub outer_session: Option<String>,
    pub llm: LlmSettings,
    #[serde(default)]
    pub allow_writes: bool,
    #[serde(default)]
    pub denied_tools: Vec<String>,
}

/// Stable migration identity for legacy conversations.
pub fn default_outer_session() -> String {
    "default".into()
}

/// One bounded display request from a server-owned conversation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DisplayEvent {
    /// Trusted profile attribution from that turn, never from model content.
    #[serde(default)]
    pub agent: String,
    pub seq: u64,
    pub name: String,
    pub input: Value,
}
