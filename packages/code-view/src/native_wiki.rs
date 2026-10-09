//! Markdown link marks and guarded workspace navigation in engine scalar offsets.
use dioxus::prelude::*;
use futures_util::{
    future::{select, Either},
    FutureExt,
};
use moonkale_core::NodeId;
use moonkale_ext_api::{wiki, Workspace};
use std::time::Duration;

#[derive(Clone, PartialEq)]
pub(crate) struct Mark {
    pub start: usize,
    pub end: usize,
    pub target: String,
    pub resolved: bool,
}

pub(crate) fn use_wiki(ws: Workspace, node: NodeId) -> (Memo<Vec<Mark>>, Callback<String>) {
    let doc = ws.document(node).expect("open document");
    let resolved = use_resource(move || {
        let epoch = *ws.sources.graph_epoch.read();
        let doc = doc.read();
        let from = doc.node.clone();
        let text = (from.language_hint() == Some("markdown")).then(|| doc.text.clone());
        async move {
            let text = text?;
            futures_timer::Delay::new(Duration::from_millis(300)).await;
            let spans = match select(
                ws.wiki_spans(&from, &text).boxed_local(),
                futures_timer::Delay::new(Duration::from_secs(5)).boxed_local(),
            )
            .await
            {
                Either::Left((spans, _)) => spans,
                _ => return None,
            };
            Some((epoch, spans))
        }
    });
    let marks = use_memo(move || {
        let doc = doc.read();
        if doc.node.language_hint() != Some("markdown") {
            return Vec::new();
        }
        let result = resolved.read();
        let spans = result
            .as_ref()
            .and_then(|result| result.as_ref())
            .filter(|(epoch, _)| *epoch == *ws.sources.graph_epoch.read());
        let resolved_targets = spans
            .map(|(_, spans)| {
                spans
                    .iter()
                    .filter(|span| span.resolved)
                    .map(|span| span.target.clone())
                    .collect::<std::collections::HashSet<_>>()
            })
            .unwrap_or_default();
        let mut previous_byte = 0;
        let mut previous_scalar = 0;
        let mut offset = |byte: usize| {
            let segment = &doc.text[previous_byte..byte];
            previous_scalar += scalar_offset(segment, segment.len());
            previous_byte = byte;
            previous_scalar
        };
        wiki::spans(&doc.text)
            .into_iter()
            .map(|(start, end, target, _, _)| Mark {
                start: offset(start),
                end: offset(end),
                resolved: resolved_targets.contains(&target),
                target,
            })
            .collect()
    });
    let follow = Callback::new(move |target: String| {
        let from = doc.peek().node.clone();
        let text = doc.peek().text.clone();
        let session = ws.editor_session(node).expect("open session");
        let revision = session.peek().snapshot().revision;
        let selection = session.peek().snapshot().selection;
        spawn(async move {
            let current = || {
                doc.try_peek()
                    .is_ok_and(|doc| doc.node == from && doc.text == text)
                    && session.try_peek().is_ok_and(|session| {
                        session.snapshot().revision == revision
                            && session.snapshot().selection == selection
                    })
                    && *ws.docs.active.peek() == Some(node)
            };
            let _ = ws.follow_wiki_guarded(&from, &target, true, current).await;
        });
    });
    (marks, follow)
}

fn scalar_offset(text: &str, byte: usize) -> usize {
    let prefix = &text[..byte];
    prefix.chars().count()
        - prefix
            .as_bytes()
            .windows(2)
            .filter(|pair| *pair == b"\r\n")
            .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn marks_use_normalized_scalar_positions() {
        let text = "😀中\r\n[[Note#heading|alias]]";
        let (start, end, _, _, _) = wiki::spans(text).pop().unwrap();
        assert_eq!(scalar_offset(text, start), 3);
        assert_eq!(
            scalar_offset(text, end),
            text.replace("\r\n", "\n").chars().count()
        );
    }
}
