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
    preview: PlainTail,
    private: bool,
    paused: bool,
}

impl Relay {
    pub fn new() -> Self {
        Self::default()
    }

    /// A chunk from the process: kept for the next subscriber and passed to
    /// the current one, if it is still listening.
    pub fn push(&mut self, chunk: &[u8]) {
        self.preview.push(chunk, !self.private && !self.paused);
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

    /// Update the sharing gate; discard the tail at both boundaries so private
    /// output cannot reappear after a prompt or a pause ends. Owner replay is separate.
    pub fn set_private(&mut self, private: bool) {
        if self.private != private {
            self.preview.bytes.clear();
        }
        self.private = private;
    }

    pub fn toggle_sharing(&mut self) {
        self.paused = !self.paused;
        self.preview.bytes.clear();
    }

    pub fn sharing_paused(&self) -> bool {
        self.paused
    }

    /// A bounded, non-consuming tail for static overview schematics.
    pub fn preview(&self) -> String {
        if self.private || self.paused {
            String::new()
        } else {
            self.preview.text()
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

// Strip controls as bytes arrive: parsing state survives chunk and tail boundaries.
// Replay/live output remains byte-for-byte intact for the actual terminal.
#[derive(Default)]
struct PlainTail {
    bytes: VecDeque<u8>,
    state: EscapeState,
}

#[derive(Default)]
enum EscapeState {
    #[default]
    Text,
    Escape,
    Intermediate,
    Csi,
    String,
    StringEscape,
}

impl PlainTail {
    fn push(&mut self, chunk: &[u8], retain: bool) {
        use EscapeState::*;
        for &byte in chunk {
            self.state = match (&self.state, byte) {
                (StringEscape, b'\\') | (String | StringEscape, 7) => Text,
                (String | StringEscape, 0x1b) => StringEscape,
                (String | StringEscape, _) => String,
                (_, 0x1b) => Escape,
                (Escape, b'[') => Csi,
                (Escape, b']' | b'P' | b'X' | b'^' | b'_') => String,
                (Escape | Intermediate, 0x20..=0x2f) => Intermediate,
                (Escape | Intermediate, _) | (Csi, 0x40..=0x7e) => Text,
                (Csi, _) => Csi,
                (Text, _) => {
                    if retain && (byte >= 0x20 && byte != 0x7f || matches!(byte, b'\n' | b'\t')) {
                        self.bytes.push_back(byte);
                        if self.bytes.len() > 2048 {
                            self.bytes.pop_front();
                        }
                    }
                    Text
                }
            };
        }
    }

    fn text(&self) -> String {
        let bytes: Vec<_> = self.bytes.iter().copied().collect();
        // A bounded byte tail may begin partway through a UTF-8 character.
        let start = bytes
            .iter()
            .position(|b| b & 0xc0 != 0x80)
            .unwrap_or(bytes.len());
        String::from_utf8_lossy(&bytes[start..])
            .chars()
            .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_channel::mpsc::TryRecvError;

    #[test]
    fn escape_strings_remain_stripped_across_privacy_and_pause_boundaries() {
        let mut relay = Relay::new();
        relay.push(b"\x1b]metadata");
        relay.set_private(true);
        relay.push(b"private");
        relay.set_private(false);
        relay.push(b"continued\x07safe");
        assert_eq!(relay.preview(), "safe");
        relay.push(b"\x1b[31");
        relay.toggle_sharing();
        relay.push(b"mignored\x1b]hidden");
        relay.toggle_sharing();
        relay.push(b"continued\x07text");
        assert_eq!(relay.preview(), "text");
    }

    #[test]
    fn private_and_paused_output_never_enters_the_shared_tail() {
        let mut relay = Relay::new();
        relay.push(b"public");
        relay.set_private(true);
        relay.push(b"private");
        assert_eq!(relay.preview(), "");
        relay.set_private(false);
        assert_eq!(relay.preview(), "");
        relay.push(b"public again");
        relay.toggle_sharing();
        relay.push(b"paused");
        assert_eq!(relay.preview(), "");
        relay.toggle_sharing();
        assert_eq!(relay.preview(), "");
        relay.push(b"new public");
        assert_eq!(relay.preview(), "new public");
        assert_eq!(
            relay.subscribe().0,
            b"publicprivatepublic againpausednew public"
        );
    }

    #[test]
    fn overview_tail_is_bounded_and_does_not_steal_live_output() {
        let mut relay = Relay::new();
        let (_, mut stream) = relay.subscribe();
        let output = "x".repeat(4096);
        relay.push(output.as_bytes());
        assert_eq!(relay.preview().len(), 2048);
        assert_eq!(stream.try_recv().unwrap(), output.as_bytes());
        relay.push(b"tail");
        assert!(relay.preview().ends_with("tail"));
        assert_eq!(stream.try_recv().unwrap(), b"tail");
    }

    #[test]
    fn overview_strips_controls_without_changing_replay() {
        let mut relay = Relay::new();
        let raw = b"\x1b[31mred\x1b[0m\r\n\x1b]0;window title\x07\x1b]8;;https://example.test\x1b\\link\x1b]8;;\x1b\\\x1bPprivate\x1b\\\x1b(B\x00\x08\x7f\tend";
        for byte in raw {
            relay.push(&[*byte]); // Deliberately split every escape sequence.
        }
        assert_eq!(relay.preview(), "red\nlink\tend");
        assert_eq!(relay.subscribe().0, raw);
    }

    #[test]
    fn overview_tail_never_starts_inside_an_escape_or_utf8_character() {
        let mut relay = Relay::new();
        relay.push(b"\x1b]0;");
        relay.push(&vec![b'x'; 4096]);
        assert_eq!(relay.preview(), "");
        relay.push(b"\x1b");
        relay.push(b"\\");
        relay.push("中".repeat(1000).as_bytes());
        let preview = relay.preview();
        assert!(preview.len() <= 2048);
        assert!(preview.chars().all(|c| c == '中'));
        relay.push(b"\x1b[31");
        assert_eq!(relay.preview(), preview);
        relay.push(b"mend");
        assert!(relay.preview().ends_with("end"));
    }

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
