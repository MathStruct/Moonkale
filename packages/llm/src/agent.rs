//! The agent loop for the in-app assistant: prompt assembly from the current
//! `GraphView` + selection, streaming tool-use turns through `policy`,
//! and a transcript that is itself stored as nodes (so conversations are
//! part of the knowledge graph and can be linked from pages).
//!
//! External agents (e.g. an IDE agent, a Claude Code session) connect through
//! a small server in `api` exposing the same tools; see vault
//! `architecture/LLM and RAG.md` for the MCP question.
