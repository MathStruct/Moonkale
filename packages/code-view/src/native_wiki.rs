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
    // Resolution depends on page identities, not source offsets or unrelated
    // typing. Retain the result while the same targets move within the note.
    let raw = use_memo(move || {
        let doc = doc.read();
        if doc.node.language_hint() == Some("markdown") {
            source_spans(&doc.text)
        } else {
            (Vec::new(), Vec::new())
        }
    });
    let from = use_memo(move || doc.read().node.clone());
    let targets = use_memo(move || {
        if from().language_hint() != Some("markdown") {
            return None;
        }
        let mut values: Vec<_> = raw
            .read()
            .0
            .iter()
            .map(|(_, _, target, _, _)| target.clone())
            .filter(|target| !target.is_empty())
            .collect();
        values.sort();
        values.dedup();
        Some(values)
    });
    let resolved = moonkale_ext_api::crash::use_resource(move || {
        let epoch = *ws.sources.graph_epoch.read();
        let from = from();
        let targets = targets();
        async move {
            let targets = targets?;
            let text = targets
                .iter()
                .map(|target| format!("[[{target}]]"))
                .collect::<String>();
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
        let spans_raw = raw.read();
        if from().language_hint() != Some("markdown") {
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
        spans_raw
            .0
            .iter()
            .zip(&spans_raw.1)
            .map(|((_, _, target, _, _), (start, end))| Mark {
                start: *start,
                end: *end,
                resolved: target.is_empty() || resolved_targets.contains(target),
                target: target.clone(),
            })
            .collect()
    });
    let follow = Callback::new(move |target: String| {
        let from = doc.peek().node.clone();
        let text = doc.peek().text.clone();
        let session = ws.editor_session(node).expect("open session");
        let revision = session.peek().snapshot().revision;
        let selection = session.peek().snapshot().selection;
        moonkale_ext_api::crash::spawn(async move {
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
    prefix.chars().count() - prefix.matches("\r\n").count()
}

fn source_spans(text: &str) -> (Vec<wiki::RawSpan>, Vec<(usize, usize)>) {
    let spans = wiki::spans(text);
    let mut previous_byte = 0;
    let mut previous_scalar = 0;
    let mut offset = |byte: usize| {
        let segment = &text[previous_byte..byte];
        previous_scalar += scalar_offset(segment, segment.len());
        previous_byte = byte;
        previous_scalar
    };
    let positions = spans
        .iter()
        .map(|(start, end, _, _, _)| (offset(*start), offset(*end)))
        .collect();
    (spans, positions)
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
    #[test]
    fn cached_spans_include_scalar_positions_when_byte_ranges_stay_equal() {
        let ascii = source_spans("abcd\r\n[[Note]]");
        let emoji = source_spans("😀\r\n[[Note]]");
        assert_eq!(ascii.0, emoji.0);
        assert_eq!(ascii.1, [(5, 13)]);
        assert_eq!(emoji.1, [(2, 10)]);
        assert!(ascii != emoji);
    }
}
