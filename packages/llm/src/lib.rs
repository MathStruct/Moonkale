//! # moonkale-llm
//!
//! LLMs are *users* of Moonkale, not a feature bolted onto the side. This
//! crate gives them the same door humans use:
//!
//! - every `Command` flagged `llm_tool` becomes a tool definition,
//! - every open `Source` is reachable through `query`/`fetch`/`apply` tools
//!   (SQL, Cypher, TypeQL, structured graph walks),
//! - the index's `Search` (hybrid BM25 + vector) is a tool,
//! - embeddings are requested through the same provider config.
//!
//! and one gate they must pass through: [`policy`].
//!
//! ```text
//!   agent (in-app chat | external via MCP-like server | extension)
//!      │ tool calls
//!      ▼
//!   policy ─► command bus ─► extension / source
//!      │
//!      └─ audit log (every tool call, every query, visible in a panel)
//! ```
//!
//! Platform: providers are HTTP, so this works everywhere. On web the calls
//! go via the server (`api`) to keep keys off the client.

pub mod agent;
pub mod audit;
pub mod embed;
pub mod policy;
pub mod provider;
pub mod tools;
