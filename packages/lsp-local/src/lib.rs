//! # moonkale-lsp-local  (desktop + server only)
//!
//! Separated from `moonkale-lsp` because spawning processes is a platform
//! capability, not a protocol concern.
//!
//! - [`StdioTransport`]: a child process speaking JSON-RPC over stdin/stdout
//!   with `Content-Length` framing; plain `std` threads, no runtime needed.
//! - [`discover`]: which binary serves which language, and where it is.
//!
//! Supervision (restart on crash, resource caps) is still a design note.

#[cfg(not(target_arch = "wasm32"))]
pub mod discover;
#[cfg(not(target_arch = "wasm32"))]
pub mod stdio;

#[cfg(not(target_arch = "wasm32"))]
pub use stdio::StdioTransport;
