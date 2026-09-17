//! # moonkale-terminal
//!
//! A terminal is a byte stream in each direction plus a resize signal. This
//! crate models `Session { write(bytes), resize(cols, rows), output: Stream }`
//! over a `trait Backend`, and keeps a VT-parsed *grid* (via
//! `alacritty_terminal`) so that the UI can be rendered by Rust and so that
//! terminal output can be *searched and linked* (a file:line in a compiler
//! error becomes a clickable edge into the graph — see [`links`]).
//!
//! Backends:
//! - local PTY: `moonkale-terminal-pty` (desktop only),
//! - remote: websocket to `api`, which owns the PTY (web, mobile, and
//!   "terminal on the server" on desktop).
//!
//! The *view* (xterm.js or a Rust canvas renderer) is in
//! `packages/editors/terminal`.

pub mod backend;
pub mod grid;
pub mod links;
pub mod session;
