//! Output that outlives a view (#12): a session's output stream can be taken
//! only once, but the view showing it is unmounted and mounted again when its
//! panel is docked elsewhere. The relay sits between the two — one pump per
//! session feeds it, each mounted view subscribes, gets the recent output to
//! repaint from, then the live stream.

use futures_channel::mpsc;
use std::collections::VecDeque;

/// How much output a newly mounted view is repainted from.
pub const REPLAY_BYTES: usize = 256 * 1024;

#[derive(Default)]
pub struct Relay {
    recent: VecDeque<u8>,
    sink: Option<mpsc::UnboundedSender<Vec<u8>>>,
}

impl Relay {
    pub fn new() -> Self {
        Self::default()
    }

    /// A chunk from the process: kept for the next subscriber and passed to
    /// the current one, if it is still listening.
    pub fn push(&mut self, chunk: &[u8]) {
        self.recent.extend(chunk);
        if self.recent.len() > REPLAY_BYTES {
            let mut cut = self.recent.len() - REPLAY_BYTES;
            // Start the replay at a line, not inside an escape sequence.
            while cut < self.recent.len() && self.recent[cut - 1] != b'\n' {
                cut += 1;
            }
            self.recent.drain(..cut);
        }
        if let Some(sink) = &self.sink {
            if sink.unbounded_send(chunk.to_vec()).is_err() {
                self.sink = None;
            }
        }
    }

    /// A view mounts: the output so far, and the stream from now on. The
    /// previous subscriber's stream ends.
    pub fn subscribe(&mut self) -> (Vec<u8>, mpsc::UnboundedReceiver<Vec<u8>>) {
        let (tx, rx) = mpsc::unbounded();
        self.sink = Some(tx);
        (self.recent.iter().copied().collect(), rx)
    }

    /// The process ended: pass `marker` on, and end the live stream.
    pub fn end(&mut self, marker: &[u8]) {
        self.push(marker);
        self.sink = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_channel::mpsc::TryRecvError;

    #[test]
    fn a_second_subscriber_is_repainted_and_then_live() {
        let mut r = Relay::new();
        r.push(b"one\r\n");
        let (replay, mut first) = r.subscribe();
        assert_eq!(replay, b"one\r\n");
        r.push(b"two\r\n");
        assert_eq!(first.try_recv().unwrap(), b"two\r\n");
        // The panel is docked elsewhere: a new view subscribes.
        let (replay, mut second) = r.subscribe();
        assert_eq!(replay, b"one\r\ntwo\r\n");
        r.push(b"three");
        assert_eq!(second.try_recv().unwrap(), b"three");
        // The first view's stream has ended.
        assert!(matches!(first.try_recv(), Err(TryRecvError::Closed)));
        r.end(b"[exit]");
        assert_eq!(second.try_recv().unwrap(), b"[exit]");
        assert!(matches!(second.try_recv(), Err(TryRecvError::Closed)));
    }

    #[test]
    fn the_replay_is_bounded_and_starts_at_a_line() {
        let mut r = Relay::new();
        let line = [b'x'; 99].iter().chain(b"\n").copied().collect::<Vec<u8>>();
        for _ in 0..(REPLAY_BYTES / 100 + 50) {
            r.push(&line);
        }
        let (replay, _) = r.subscribe();
        assert!(replay.len() <= REPLAY_BYTES);
        assert_eq!(replay.len() % 100, 0, "replay starts at a line");
    }
}
