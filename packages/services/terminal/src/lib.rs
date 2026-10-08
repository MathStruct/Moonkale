//! # moonkale-terminal
//!
//! A terminal is a byte stream in each direction plus a resize signal. This
//! crate models that ([`Session`], [`TerminalBackend`]) without knowing where
//! the bytes come from: a local PTY (`moonkale-terminal-pty`, desktop and
//! server) or a websocket to the server (`moonkale_server::RemoteTerminal`, web).
//!
//! Milestone 3 ships the model and [`links`] (file paths in output). The VT
//! grid (`alacritty_terminal`) for a Rust-native renderer and search is
//! still a design note — xterm.js renders for now.

pub mod links;
pub mod relay;
pub mod session;

pub use relay::Relay;
pub use session::{
    output_channel, send_blocking, Output, Session, SessionId, SpawnTerminal, SpawnTerminalFuture,
    TerminalBackend, TerminalMessage, OUTPUT_CHUNKS,
};
