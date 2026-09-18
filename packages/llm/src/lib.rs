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
pub mod config;
pub mod mock;
pub mod policy;
pub mod provider;
pub mod sse;
pub mod tools;
pub mod types;

#[cfg(all(feature = "http", not(target_arch = "wasm32")))]
pub mod anthropic;
#[cfg(all(feature = "http", not(target_arch = "wasm32")))]
pub mod openai;

pub use agent::{Agent, AgentEvent, ToolHost, ToolOutcome};
pub use audit::{AuditEntry, AuditLog};
pub use config::{Config, ProviderKind};
pub use mock::MockProvider;
pub use policy::{Class, Decision, Policy};
pub use provider::{BoxFuture, EventStream, Provider};
pub use tools::{builtin_tools, ToolCall};
pub use types::{Content, Event, Message, Request, Role, StopReason, ToolDef, Usage};
