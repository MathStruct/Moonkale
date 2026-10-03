//! Remote folders (Milestone 11): a folder on another machine, reached
//! through the platform's SSH session (`moonkale-remote` on the desktop).
//! The workspace knows only this: how to start a session, what phase it is
//! in, and how to end it. The `ssh` process itself shows up as a terminal
//! tab so every prompt (passphrase, password, host key) is the user's to
//! answer; Moonkale never sees a credential.

use moonkale_core::SourceId;
use moonkale_terminal::TerminalBackend;
use std::sync::Arc;

/// Where a session is, for the status bar and the menus.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemotePhase {
    /// Starting `ssh`.
    Connecting,
    /// `ssh` is waiting for the user (the line it printed).
    Prompt(String),
    /// The server binary is being copied to the host (first time per version).
    Uploading,
    /// The server is starting on the host.
    Starting,
    /// Connected; the folder is open.
    Ready,
    /// It failed, with the reason.
    Failed(String),
    /// The session ended.
    Closed,
}

impl RemotePhase {
    /// Whether the phase is an end (ready, failed, closed).
    pub fn is_final(&self) -> bool {
        matches!(self, Self::Ready | Self::Failed(_) | Self::Closed)
    }

    /// What the status bar shows for this phase.
    pub fn label(&self) -> String {
        match self {
            Self::Connecting => "connecting…".into(),
            Self::Prompt(_) => "waiting for you in the terminal".into(),
            Self::Uploading => "uploading the server…".into(),
            Self::Starting => "starting the server…".into(),
            Self::Ready => "connected".into(),
            Self::Failed(e) => format!("failed: {e}"),
            Self::Closed => "closed".into(),
        }
    }
}

/// A running session the workspace can end.
pub trait RemoteSession: Send + Sync {
    /// End the SSH session (and the remote server).
    fn close(&self);
}

/// Where phase changes go (called from any thread).
pub type PhaseSink = Box<dyn Fn(RemotePhase) + Send + Sync>;

/// What a started session hands back: the terminal showing `ssh` and the
/// handle that ends it.
pub type Opened = (Box<dyn TerminalBackend>, Arc<dyn RemoteSession>);

/// Start a session to `host` for `path`: [`Opened`], or why it could not start.
pub type OpenRemote = fn(String, String, PhaseSink) -> Result<Opened, String>;

/// What the platform hands the workspace for remote folders.
#[derive(Clone, Copy)]
pub struct RemoteHosts {
    /// Open `host:path` over SSH.
    pub open: OpenRemote,
    /// Host aliases to offer (`~/.ssh/config`).
    pub hosts: fn() -> Vec<String>,
    /// A session to open when the window starts (`moonkale --ssh host:path`,
    /// `MOONKALE_SSH`), if any.
    pub at_start: fn() -> Option<(String, String)>,
}

/// The one session a window has.
#[derive(Clone)]
pub struct RemoteState {
    /// The host as typed after `ssh`.
    pub host: String,
    /// The folder on the host.
    pub path: String,
    /// Where the session is.
    pub phase: RemotePhase,
    /// The session, to close it.
    pub session: Arc<dyn RemoteSession>,
    /// Sources opened through the session (closed with it).
    pub sources: Vec<SourceId>,
}

impl RemoteState {
    /// `user@host:path` — the host's last word (the spec may carry `ssh`
    /// options and variables before it).
    pub fn label(&self) -> String {
        let host = self.host.split_whitespace().last().unwrap_or(&self.host);
        format!("{host}:{}", self.path)
    }
}

impl PartialEq for RemoteState {
    fn eq(&self, o: &Self) -> bool {
        self.host == o.host
            && self.path == o.path
            && self.phase == o.phase
            && self.sources == o.sources
            && Arc::ptr_eq(&self.session, &o.session)
    }
}
