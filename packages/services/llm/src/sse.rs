//! Server-sent events, the streaming format both Anthropic and OpenAI use.
//! A tolerant incremental parser: feed bytes, take complete events.

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SseEvent {
    pub event: Option<String>,
    pub data: String,
}

/// The longest event kept (#14): a server that never sends the blank line
/// ending an event cannot grow the buffer past this; what is beyond is
/// dropped (the event is lost, the stream goes on).
pub const MAX_EVENT: usize = 8 << 20;

/// The most text a streamed tool call may accumulate (arguments, partial
/// JSON): an `editor.replace` with a whole large file still fits.
pub const MAX_TOOL_INPUT: usize = 16 << 20;

/// The most tool calls one response may hold.
pub const MAX_TOOL_CALLS: usize = 64;

/// Append `more` to `s` unless that passes `limit`.
pub fn push_capped(s: &mut String, more: &str, limit: usize) {
    if s.len() + more.len() <= limit {
        s.push_str(more);
    }
}

#[derive(Default)]
pub struct SseParser {
    buf: String,
}

impl SseParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a chunk and return every event completed by it.
    pub fn feed(&mut self, chunk: &str) -> Vec<SseEvent> {
        self.buf.push_str(chunk);
        let mut out = Vec::new();
        // Events are separated by a blank line.
        while let Some(pos) = self.buf.find("\n\n") {
            let block = self.buf[..pos].to_string();
            self.buf.drain(..pos + 2);
            if let Some(ev) = parse_block(&block) {
                out.push(ev);
            }
        }
        if self.buf.len() > MAX_EVENT {
            self.buf.clear();
        }
        out
    }

    /// Whatever remains at end of stream (an event without a trailing blank line).
    pub fn finish(&mut self) -> Option<SseEvent> {
        let rest = std::mem::take(&mut self.buf);
        parse_block(&rest)
    }
}

fn parse_block(block: &str) -> Option<SseEvent> {
    let mut ev = SseEvent::default();
    let mut any = false;
    for line in block.lines() {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.starts_with(':') || line.is_empty() {
            continue;
        }
        let (field, value) = match line.split_once(':') {
            Some((f, v)) => (f, v.strip_prefix(' ').unwrap_or(v)),
            None => (line, ""),
        };
        match field {
            "event" => ev.event = Some(value.to_string()),
            "data" => {
                if !ev.data.is_empty() {
                    ev.data.push('\n');
                }
                ev.data.push_str(value);
                any = true;
            }
            _ => {}
        }
    }
    (any || ev.event.is_some()).then_some(ev)
}

#[cfg(test)]
mod tests {
    #[test]
    fn an_event_without_end_cannot_grow_the_buffer() {
        let mut p = super::SseParser::new();
        let chunk = "x".repeat(1 << 20);
        for _ in 0..20 {
            assert!(p.feed(&chunk).is_empty());
        }
        assert!(p.buf.len() <= super::MAX_EVENT);
        // The stream goes on afterwards.
        let ev = p.feed("\n\ndata: ok\n\n");
        assert_eq!(ev.last().map(|e| e.data.as_str()), Some("ok"));
    }

    use super::*;

    #[test]
    fn splits_events_across_chunks() {
        let mut p = SseParser::new();
        let a = p.feed("event: ping\ndata: {\"a\":1}\n\nevent: x\ndata: par");
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].event.as_deref(), Some("ping"));
        assert_eq!(a[0].data, "{\"a\":1}");
        let b = p.feed("tial\n\n");
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].data, "partial");
        assert_eq!(p.finish(), None);
    }

    #[test]
    fn data_only_and_done() {
        let mut p = SseParser::new();
        let evs = p.feed("data: {\"x\":1}\n\ndata: [DONE]\n\n");
        assert_eq!(evs.len(), 2);
        assert_eq!(evs[1].data, "[DONE]");
    }
}
