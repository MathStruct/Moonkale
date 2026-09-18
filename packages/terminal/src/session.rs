//! Sessions and the backend trait.

use futures_channel::mpsc;
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub u64);

impl SessionId {
    pub fn fresh() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

/// Output from the process, as raw bytes (VT sequences included).
pub type Output = mpsc::UnboundedReceiver<Vec<u8>>;

/// The wire format between a terminal view/client and its backend. Also
/// what goes over the websocket for remote terminals.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TerminalMessage {
    /// First message from a client: what to run and how big the view is.
    Open {
        cwd: Option<String>,
        cols: u16,
        rows: u16,
    },
    /// Keystrokes / pasted text, base64 so binary-safe in JSON.
    Input {
        data: String,
    },
    Resize {
        cols: u16,
        rows: u16,
    },
    /// Process output, base64.
    Output {
        data: String,
    },
    /// The process ended (or the backend failed).
    Exit {
        code: Option<i32>,
        message: Option<String>,
    },
}

/// Where the bytes go. Implementations: `PtyBackend` (local process),
/// `RemoteTerminal` (websocket). All methods are non-blocking; output is
/// delivered through the receiver handed out once by [`take_output`].
///
/// [`take_output`]: TerminalBackend::take_output
pub trait TerminalBackend {
    fn write(&self, data: &[u8]);
    fn resize(&self, cols: u16, rows: u16);
    /// The output stream; `None` after the first call.
    fn take_output(&mut self) -> Option<Output>;
    /// A short description for the tab ("bash", "remote").
    fn title(&self) -> String;
}

/// A running terminal the UI shows: id, title, and its backend.
pub struct Session {
    pub id: SessionId,
    pub title: String,
    pub cwd: Option<String>,
    pub backend: Box<dyn TerminalBackend>,
}

pub type SpawnTerminalFuture =
    Pin<Box<dyn Future<Output = Result<Box<dyn TerminalBackend>, String>>>>;
/// Installed by the platform: how to start a terminal (cwd, cols, rows).
pub type SpawnTerminal = fn(Option<String>, u16, u16) -> SpawnTerminalFuture;
