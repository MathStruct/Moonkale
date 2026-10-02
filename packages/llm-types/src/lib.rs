//! # moonkale-llm-types
//!
//! The *data* of Moonkale's agents: messages and events ([`types`]), the
//! [`Provider`](provider::Provider) trait, saved-session records
//! ([`sessions`]), and the policy's classes and decisions. No provider, no
//! HTTP, no process: split out of `moonkale-llm` in Milestone 18 phase 3c so
//! the extension contract (`moonkale-ext-api`) can name these types without
//! depending on the agent engine. `moonkale-llm` re-exports everything here,
//! so `moonkale_llm::…` paths are unchanged.

pub mod provider;
pub mod sessions;
pub mod types;

pub use provider::{BoxFuture, EventStream, Provider, ProviderStatus};
pub use types::{Content, Event, LlmSettings, Message, Request, Role, StopReason, ToolDef, Usage};

use serde::{Deserialize, Serialize};

/// How risky a tool call is (the policy's classification).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Class {
    ReadOnly,
    Mutating,
    Destructive,
}

/// What the policy decided for a tool call.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Decision {
    Allow,
    Ask,
    Deny,
}

/// What became of a tool call.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ToolOutcome {
    Ran { ok: bool },
    Denied,
    Declined,
}
