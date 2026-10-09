//! Definition navigation validates the request before loading/opening its target.
use crate::{lsp::LspManager, native_diagnostics::file_uri, native_model::NativeModel, L};
use dioxus::prelude::*;
use futures_util::{
    future::{select, Either},
    FutureExt,
};
use moonkale_core::{Node, NodeId, NodeKind, Query};
use moonkale_ext_api::{
    editor::{DocumentRevision, Utf16Selection},
    t, Workspace,
};
use std::time::Duration;

#[derive(Clone, PartialEq, Eq)]
struct Request {
    sequence: u64,
    revision: DocumentRevision,
    selection: Utf16Selection,
    line: u32,
    col: u32,
}

pub(crate) fn use_definition(
    ws: Workspace,
    node: NodeId,
    manager: LspManager,
    model: Signal<NativeModel>,
) -> (Callback<()>, Callback<()>) {
    let doc = ws.document(node).expect("open document");
    let mut session = ws.editor_session(node).expect("open session");
    let origin = use_hook(move || doc.peek().node.clone());
    let mut request = use_signal(|| None::<Request>);
    let mut sequence = use_signal(|| 0u64);
    let current = use_memo(move || {
        let session = session.read();
        (
            session.snapshot().revision,
            session.snapshot().selection,
            *ws.docs.active.read(),
        )
    });
    use_effect(move || {
        let now = current();
        if request
            .peek()
            .as_ref()
            .is_some_and(|request| now != (request.revision, Some(request.selection), Some(node)))
        {
            request.set(None);
        }
    });
    let invoke = Callback::new(move |_: ()| {
        if *ws.docs.active.peek() != Some(node) {
            return;
        }
        let (revision, selection, line, col) = {
            let state = session.peek();
            let snapshot = state.snapshot();
            let selection = model.peek().selection(&snapshot.text);
            let caret = model.peek().engine.get_cursor_state().position;
            let col = snapshot
                .text
                .split('\n')
                .nth(caret.line)
                .unwrap_or_default()
                .trim_end_matches('\r')
                .chars()
                .take(caret.column)
                .map(char::len_utf16)
                .sum::<usize>() as u32;
            (snapshot.revision, selection, caret.line as u32, col)
        };
        session.with_mut(|session| {
            session.set_selection(revision, selection);
        });
        sequence.with_mut(|value| *value += 1);
        request.set(Some(Request {
            sequence: sequence(),
            revision,
            selection,
            line,
            col,
        }));
    });
    // The resource's dependency changes cancel both protocol and file-loading work.
    let _result = use_resource(move || {
        let pending = request();
        let now = current();
        let origin = origin.clone();
        async move {
            let Some(pending) = pending else {
                return;
            };
            if now != (pending.revision, Some(pending.selection), Some(node)) {
                return;
            }
            let sequence = pending.sequence;
            async {
                let expected = pending.clone();
                let current = move || {
                    let snapshot = session.peek();
                    let snapshot = snapshot.snapshot();
                    request.peek().as_ref() == Some(&expected)
                        && snapshot.revision == expected.revision
                        && snapshot.selection == Some(expected.selection)
                        && *ws.docs.active.peek() == Some(node)
                        && doc.peek().text == snapshot.text
                };
                let Some(root) = origin.source.as_str().strip_prefix("folder:") else {
                    if current() {
                        let mut ws = ws;
                        ws.set_status(t!(ws, L, "editor-definition-needs-lsp"));
                    }
                    return;
                };
                let uri = file_uri(root, &origin.native_key);
                let Some(lsp) = manager.peek_document_session(&uri) else {
                    if current() {
                        let mut ws = ws;
                        ws.set_status(t!(ws, L, "editor-definition-needs-lsp"));
                    }
                    return;
                };
                let response = select(
                    lsp.definition(&uri, pending.line, pending.col)
                        .boxed_local(),
                    futures_timer::Delay::new(Duration::from_secs(5)).boxed_local(),
                )
                .await;
                if !current() {
                    return;
                }
                let location = match response {
                    Either::Left((Ok(Some(location)), _)) => location,
                    Either::Left((Ok(None), _)) => {
                        let mut ws = ws;
                        ws.set_status(t!(ws, L, "editor-no-definition"));
                        return;
                    }
                    Either::Left((Err(error), _)) => {
                        let mut ws = ws;
                        ws.set_status(t!(ws, L, "editor-definition-failed", error = error));
                        return;
                    }
                    Either::Right(_) => {
                        let mut ws = ws;
                        ws.set_status(t!(ws, L, "editor-definition-timeout"));
                        return;
                    }
                };
                let Some(relative) = relative_file(root, &location.uri) else {
                    let mut ws = ws;
                    ws.set_status(t!(ws, L, "editor-definition-outside", uri = location.uri));
                    return;
                };
                let target = resolve_target(ws, &origin, &relative).await;
                if !current() {
                    return;
                }
                match target {
                    Ok(target) => {
                        if let Err(error) = ws
                            .reveal_guarded(target, location.line, location.col, current.clone())
                            .await
                        {
                            // A load error should not overwrite feedback for a newer request.
                            if current() {
                                let mut ws = ws;
                                ws.set_status(t!(
                                    ws,
                                    L,
                                    "editor-definition-failed",
                                    error = error.to_string()
                                ));
                            }
                        }
                    }
                    Err(error) => {
                        let mut ws = ws;
                        ws.set_status(t!(ws, L, "editor-definition-failed", error = error));
                    }
                }
            }
            .await;
            if request
                .peek()
                .as_ref()
                .is_some_and(|request| request.sequence == sequence)
            {
                request.set(None);
            }
        }
    });
    (invoke, Callback::new(move |_: ()| request.set(None)))
}

pub(crate) async fn resolve_target(
    ws: Workspace,
    origin: &Node,
    relative: &str,
) -> Result<Node, String> {
    if let Some(node) = ws.docs.open.peek().iter().find_map(|(_, document)| {
        let document = document.peek();
        (document.node.source == origin.source && document.node.native_key == relative)
            .then(|| document.node.clone())
    }) {
        return Ok(node);
    }
    let source = ws
        .sources
        .open
        .peek()
        .iter()
        .find(|source| source.descriptor.id == origin.source)
        .cloned()
        .ok_or("folder is closed")?;
    let mut parent = source.descriptor.root;
    let mut target = None;
    for part in relative.split('/') {
        let result = source
            .source
            .query(Query::Children(parent))
            .await
            .map_err(|error| error.to_string())?;
        let node = result
            .nodes
            .into_iter()
            .find(|node| node.label == part && node.source == origin.source)
            .ok_or("definition file not found")?;
        parent = node.id;
        target = Some(node);
    }
    target
        .filter(|node| {
            node.kind == NodeKind::File
                && !matches!(node.content, Some(moonkale_core::ContentRef::Blob { .. }))
        })
        .ok_or_else(|| "definition target is not a text file".into())
}

pub(crate) fn relative_file(root: &str, uri: &str) -> Option<String> {
    let encoded = uri.strip_prefix("file://")?;
    let encoded = encoded.strip_prefix("localhost").unwrap_or(encoded);
    if !encoded.starts_with('/') || encoded.contains(['?', '#']) {
        return None;
    }
    let mut bytes = Vec::new();
    let mut input = encoded.as_bytes().iter().copied();
    while let Some(byte) = input.next() {
        if byte == b'%' {
            let high = (input.next()? as char).to_digit(16)?;
            let low = (input.next()? as char).to_digit(16)?;
            bytes.push((high * 16 + low) as u8);
        } else {
            bytes.push(byte);
        }
    }
    let path = String::from_utf8(bytes).ok()?;
    let prefix = format!("{}/", root.trim_end_matches('/'));
    let relative = path.strip_prefix(&prefix)?;
    if relative
        .split('/')
        .any(|part| part.is_empty() || part == "." || part == ".." || part.contains(['\\', '\0']))
    {
        return None;
    }
    Some(relative.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn definition_uri_decoding_preserves_folder_boundaries() {
        assert_eq!(
            relative_file(
                "/tmp/my project",
                "file:///tmp/my%20project/src/a%23%C3%A9.rs"
            ),
            Some("src/a#é.rs".into())
        );
        assert_eq!(
            relative_file("/tmp/project", "file://localhost/tmp/project/a.rs"),
            Some("a.rs".into())
        );
        for uri in [
            "file:///tmp/project-other/a.rs",
            "file:///tmp/project/%2e%2e/a.rs",
            "file:///tmp/project/a%ZZ.rs",
            "file://other/tmp/project/a.rs",
            "https://example.org/a.rs",
            "file:///tmp/project/a.rs#fragment",
        ] {
            assert_eq!(relative_file("/tmp/project", uri), None, "{uri}");
        }
    }
}
