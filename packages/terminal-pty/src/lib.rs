//! # moonkale-terminal-pty  (desktop + server only)
//!
//! `PtyBackend` over `portable-pty`: spawns the user's shell (or a command)
//! in a pseudo-terminal, streams its output on a reader thread, forwards
//! input and resizes. On the server the same backend serves remote sessions
//! for web clients — dev-server only until the security list in the vault's
//! Platform Matrix is done (P-20).

#[cfg(not(target_arch = "wasm32"))]
mod pty;
#[cfg(not(target_arch = "wasm32"))]
pub use pty::PtyBackend;
