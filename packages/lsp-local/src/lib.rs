//! # moonkale-lsp-local  (desktop + server only)
//!
//! Separated from `moonkale-lsp` because spawning processes is a
//! platform capability, not a protocol concern. Provides:
//!
//! - `StdioTransport`: a child process with JSON-RPC over stdin/stdout,
//! - [`discover`]: find servers on `PATH` / in toolchains (`rust-analyzer`,
//!   `julia --project -e 'using LanguageServer…'`, `gopls`, `ucm` for Unison,
//!   `lake serve` for Lean) with per-language install hints,
//! - [`supervise`]: restart on crash with backoff, resource limits,
//! - a server-side mode used by `api` to offer LSP to web clients over a
//!   websocket (one process per (user, language, workspace)).

pub mod discover;
pub mod stdio;
pub mod supervise;
