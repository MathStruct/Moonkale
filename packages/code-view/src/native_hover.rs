//! Debounced hover requests; decorations never own document state.
use crate::{lsp::LspManager, native_diagnostics::file_uri};
use dioxus::prelude::*;
use futures_util::{
    future::{select, Either},
    FutureExt,
};
use moonkale_core::NodeId;
use moonkale_ext_api::Workspace;
use std::time::Duration;

#[derive(Clone)]
struct HoverResult {
    position: (usize, usize),
    revision: moonkale_ext_api::editor::DocumentRevision,
    text: String,
}

pub(crate) fn use_hover(
    ws: Workspace,
    node: NodeId,
    manager: LspManager,
    target: Signal<Option<(usize, usize)>>,
) -> Memo<Option<String>> {
    let doc = ws.document(node).expect("open document");
    let session = ws.editor_session(node).expect("open session");
    let revision = use_memo(move || session.read().snapshot().revision);
    let uri = use_hook(move || {
        let document = doc.peek();
        Some(file_uri(
            document.node.source.as_str().strip_prefix("folder:")?,
            &document.node.native_key,
        ))
    });
    let result = use_resource(move || {
        let position = target();
        let revision = revision();
        let uri = uri.clone();
        let lsp = uri.as_ref().and_then(|uri| manager.document_session(uri));
        async move {
            let position = position?;
            let uri = uri?;
            let lsp = lsp?;
            // Use canonical text for UTF-16, excluding CR in CRLF line endings.
            let column = doc
                .peek()
                .text
                .split('\n')
                .nth(position.0)?
                .trim_end_matches('\r')
                .chars()
                .take(position.1)
                .map(char::len_utf16)
                .sum::<usize>() as u32;
            futures_timer::Delay::new(Duration::from_millis(250)).await;
            let response = select(
                lsp.hover(&uri, position.0 as u32, column).boxed_local(),
                futures_timer::Delay::new(Duration::from_secs(5)).boxed_local(),
            )
            .await;
            let text = match response {
                Either::Left((Ok(Some(text)), _)) => text,
                _ => return None,
            };
            if text.trim().is_empty() {
                return None;
            }
            Some(HoverResult {
                position,
                revision,
                text,
            })
        }
    });
    use_memo(move || {
        if *result.state().read() != UseResourceState::Ready {
            return None;
        }
        result
            .read()
            .as_ref()
            .and_then(|result| result.as_ref())
            .filter(|result| Some(result.position) == target() && result.revision == revision())
            .map(|result| result.text.clone())
    })
}
