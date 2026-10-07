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

/// Output from the process, as raw bytes (VT sequences included). Bounded
/// (#14): a producer waits while [`OUTPUT_CHUNKS`] chunks are unread, so a
/// consumer that stalls (a client that stopped reading its websocket) holds
/// the process back instead of filling memory.
pub type Output = mpsc::Receiver<Vec<u8>>;

/// How many chunks an [`Output`] holds unread (a PTY reads 8 KiB at a time).
pub const OUTPUT_CHUNKS: usize = 64;

/// A bounded output channel.
pub fn output_channel() -> (mpsc::Sender<Vec<u8>>, Output) {
    mpsc::channel(OUTPUT_CHUNKS)
}

/// Send from a thread, waiting while the channel is full. False when the
/// receiver is gone.
pub fn send_blocking(tx: &mut mpsc::Sender<Vec<u8>>, mut chunk: Vec<u8>) -> bool {
    loop {
        match tx.try_send(chunk) {
            Ok(()) => return true,
            Err(e) if e.is_full() => {
                chunk = e.into_inner();
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            Err(_) => return false,
        }
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    /// #14: a producer waits while the consumer lags, instead of queueing.
    #[test]
    fn a_full_output_holds_the_producer_back() {
        let (mut tx, mut rx) = output_channel();
        // futures' bounded channel takes `buffer + senders` messages.
        let mut sent = 0;
        while tx.try_send(vec![0u8; 8192]).is_ok() {
            sent += 1;
        }
        assert!((OUTPUT_CHUNKS..=OUTPUT_CHUNKS + 2).contains(&sent), "{sent}");
        let done = Arc::new(AtomicBool::new(false));
        let d = done.clone();
        let producer = std::thread::spawn(move || {
            assert!(send_blocking(&mut tx, b"more".to_vec()));
            d.store(true, Ordering::SeqCst);
        });
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert!(!done.load(Ordering::SeqCst), "the producer must wait");
        rx.try_recv().unwrap();
        producer.join().unwrap();
        assert!(done.load(Ordering::SeqCst));
        // A gone consumer ends the producer.
        let (mut tx, rx) = output_channel();
        drop(rx);
        assert!(!send_blocking(&mut tx, b"x".to_vec()));
    }
}
