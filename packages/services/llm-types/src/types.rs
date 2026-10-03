//! Provider-neutral message model. Close to Anthropic's shape (content
//! blocks with tool use / tool result) because it is the most general; the
//! OpenAI translation lives in `openai.rs`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Content {
    Text {
        text: String,
    },
    /// The model asks for a tool; `id` correlates the result.
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    /// The host's answer to a `ToolUse` (sent back as a user message).
    ToolResult {
        id: String,
        output: String,
        is_error: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: Vec<Content>,
}

impl Message {
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: vec![Content::Text { text: text.into() }],
        }
    }
    pub fn assistant(content: Vec<Content>) -> Self {
        Self {
            role: Role::Assistant,
            content,
        }
    }
    pub fn tool_results(results: Vec<Content>) -> Self {
        Self {
            role: Role::User,
            content: results,
        }
    }
    /// The concatenated text blocks.
    pub fn text(&self) -> String {
        self.content
            .iter()
            .filter_map(|c| match c {
                Content::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }
}

/// Resolved LLM settings (Milestone 5): what a platform needs to build a
/// provider, minus the secret itself (named, resolved where the provider
/// runs). Defined here so `api` and the platforms can pass it around
/// without depending on `ext-api`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LlmSettings {
    /// `anthropic` | `openai` | `ollama` | `mock`.
    pub provider: String,
    /// Empty = the provider's default model.
    pub model: String,
    /// OpenAI-compatible endpoint or Ollama host; empty = provider default.
    pub base_url: String,
    pub embed_model: Option<String>,
    /// Name of the secret holding the API key.
    pub secret: String,
    /// Provider-specific options (Milestone 12): for `claude-code`,
    /// `permission_mode` and `allowed_tools`.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub options: std::collections::BTreeMap<String, String>,
}

/// A tool the model may call; `input_schema` is JSON Schema.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Request {
    /// `None` = the provider's configured default.
    pub model: Option<String>,
    /// Where a turn runs, for providers that act on a folder (`claude-code`
    /// spawns the CLI there); the server refuses paths outside its root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    pub system: String,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolDef>,
    pub max_tokens: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
    #[default]
    Other,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

/// What a streamed completion yields, in order. A provider must end every
/// stream with exactly one `Done` or `Error`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Event {
    TextDelta {
        text: String,
    },
    /// A complete tool call (providers stream the JSON; we deliver it whole).
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    Done {
        stop: StopReason,
        usage: Usage,
    },
    Error {
        message: String,
    },
}
