//! # moonkale-terminal-pty  (desktop + server only)
//!
//! `PtyBackend` using `portable-pty`: spawns the user's shell (or a command),
//! forwards resize, and streams output. On the server (`api`) the same
//! backend serves remote sessions for web/mobile clients with per-user
//! isolation (working directory jail + resource limits; see vault
//! `architecture/Platform Matrix.md` for the security discussion).

pub mod pty;
pub mod shell;
