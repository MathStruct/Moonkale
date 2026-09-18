//! # moonkale-editor-agent
//!
//! The in-app assistant (Milestone 4). One `Agent` per window, kept in ROOT
//! signals so docking the panel elsewhere keeps the conversation. The panel
//! renders `AgentEvent`s as they stream; tool calls run through
//! [`host::WorkspaceHost`], which executes them against the open sources and
//! asks the user when policy says `Ask`.
//!
//! Vault: `architecture/LLM and RAG.md`, `milestones/Milestone 4 - Agents.md`.

mod extension;
pub mod host;
mod panel;
mod transcript;

pub use extension::{AgentExtension, PANEL_ID};
