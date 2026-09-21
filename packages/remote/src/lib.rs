//! # moonkale-remote (Milestone 11)
//!
//! `SshSession::open(host, path)` turns a folder on another machine into a
//! Moonkale server the desktop talks to — Zed's model:
//!
//! 1. One `ssh` process, run under a PTY so every prompt (passphrase,
//!    password, unknown host key) is visible and answered by the user, is the
//!    **control master** (`-M -S <socket>`) and carries the port forward
//!    (`-L 127.0.0.1:<local>:127.0.0.1:<remote>`). It runs a small shell
//!    script on the host that reports the architecture, waits for the
//!    server binary if it is missing, reads the session token from the
//!    terminal with echo off, and `exec`s `moonkale-server --token-stdin`.
//! 2. When the script says the binary is missing, a second `ssh -S <socket>`
//!    (multiplexed: no second prompt) streams it from this machine into
//!    `~/.local/share/moonkale/server/<version>/` on the host.
//! 3. When the server answers on the forwarded port, `api::client::connect`
//!    makes it this desktop's server; closing the session kills the master,
//!    which ends the forward and the remote server.
//!
//! Nothing here reads, stores or forwards a credential: `ssh` does the
//! authenticating with `~/.ssh`, the agent and `known_hosts` exactly as in a
//! terminal. The token exists for the session only. See the vault's
//! "Remote and Server Modes" page for the threat model.

mod session;
mod tee;

pub use session::{looks_like_prompt, server_binary, Phase, SshSession, SshTarget, VERSION};
pub use tee::TeeBackend;
