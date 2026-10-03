//! Server-sent events, the streaming format both Anthropic and OpenAI use.
//! A tolerant incremental parser: feed bytes, take complete events.

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SseEvent {
    pub event: Option<String>,
    pub data: String,
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
