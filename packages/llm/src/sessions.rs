//! Agent sessions that live on a server (Milestone 12): the wire types
//! between the Agent panel and `api::agent_sessions`. Pure data, so the
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
    pub llm: LlmSettings,
    #[serde(default)]
    pub allow_writes: bool,
    #[serde(default)]
    pub denied_tools: Vec<String>,
}
