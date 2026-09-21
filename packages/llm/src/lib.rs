//! # moonkale-llm
//!
//! LLMs are *users* of Moonkale, not a feature bolted onto the side. This
//! crate gives them the same door humans use:
//!
//! - every open `Source` is reachable through `graph.query` / `graph.fetch` /
//!   `source.text_query` tools (SQL, Cypher, structured graph walks),
//! - the index's search is a tool,
//! - embeddings are requested through the same provider config,
//!
//! and one gate they must pass through: [`policy`].
//!
//! ```text
//!   agent panel ─► agent loop ─► provider (HTTP on desktop, websocket relay on web)
//!                     │ tool calls
//!                     ▼
//!                  policy ─► ToolHost (the workspace) ─► sources
//!                     │
//!                     └─ audit log (every call, every decision; shown in the panel)
//! ```
//!
//! Platform: everything but the HTTP providers compiles to wasm. Providers
//! live behind the `http` feature; the web build talks to `api`'s relay,
//! which runs the same providers on the server so keys never reach the
//! browser.
//!
//! Vault: `architecture/LLM and RAG.md`, `milestones/Milestone 4 - Agents.md`.

pub mod agent;
pub mod audit;
#[cfg(feature = "claude-code")]
pub mod claude_code;
pub mod config;
pub mod mock;
pub mod policy;
pub mod provider;
pub mod sessions;
pub mod sse;
pub mod tools;
pub mod types;

#[cfg(all(feature = "http", not(target_arch = "wasm32")))]
pub mod anthropic;
#[cfg(all(feature = "http", not(target_arch = "wasm32")))]
pub mod openai;
#[cfg(not(target_arch = "wasm32"))]
pub mod secrets;

pub use agent::{Agent, AgentEvent, ToolHost, ToolOutcome};
pub use audit::{AuditEntry, AuditLog};
pub use config::{Config, ProviderKind};
pub use mock::MockProvider;
pub use policy::{Class, Decision, Policy};
pub use provider::{BoxFuture, EventStream, Provider};
pub use tools::{builtin_tools, ToolCall};
pub use types::{Content, Event, LlmSettings, Message, Request, Role, StopReason, ToolDef, Usage};

/// The HTTP client every provider uses: a connect timeout so an unreachable
/// host fails in seconds instead of hanging a search or a chat forever.
/// Streams (chat completions) have no total timeout; single round trips
/// (embeddings) set one per request.
#[cfg(feature = "http")]
pub fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}
