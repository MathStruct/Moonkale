//! LSP coordinates stay UTF-16 until the decoration boundary.
use crate::lsp::LspManager;
use dioxus::prelude::*;
use moonkale_core::NodeId;
use moonkale_ext_api::Workspace;
use moonkale_lsp::Diagnostic;
use std::{cell::Cell, rc::Rc};

pub(crate) fn use_diagnostics(
    ws: Workspace,
    node: NodeId,
    manager: LspManager,
) -> Memo<Vec<Diagnostic>> {
    let doc = ws.document(node).expect("open document");
    let identity = use_hook(move || {
        let document = doc.peek();
        Some((
            document.node.language_hint()?.to_string(),
            document
                .node
                .source
                .as_str()
                .strip_prefix("folder:")?
                .to_string(),
            document.node.native_key.clone(),
        ))
    });
    let uri = identity
        .as_ref()
        .map(|(_, root, path)| file_uri(root, path));
    let active = use_hook(|| Rc::new(Cell::new(true)));
    let attached = use_hook(|| Rc::new(Cell::new(false)));
    let start_active = active.clone();
    let start_attached = attached.clone();
    let start_uri = uri.clone();
    use_hook(move || {
        if let Some((language, root, _)) = identity {
            dioxus::core::spawn_forever(async move {
                if let Some(session) = manager.ensure(ws, &language, &root).await {
                    if start_active.get() {
                        let document = doc.peek();
                        manager.open_document(
                            session,
                            start_uri.as_deref().unwrap(),
                            &language,
                            &root,
                            document.text.clone(),
                            document.saved.clone(),
                        );
                        start_attached.set(true);
                    }
                }
            });
        }
    });
    let update_uri = uri.clone();
    use_effect(move || {
        let document = doc.read();
        if let Some(uri) = &update_uri {
            manager.update_document(uri, &document.text, &document.saved);
        }
    });
    let close_uri = uri.clone();
    use_drop(move || {
        active.set(false);
        if attached.get() {
            if let Some(uri) = close_uri {
                manager.close_document(&uri);
            }
        }
    });
    use_memo(move || {
        uri.as_ref()
            .and_then(|uri| manager.diagnostics.read().get(uri).cloned())
            .unwrap_or_default()
    })
}

pub(crate) fn file_uri(root: &str, path: &str) -> String {
    let path = format!("{}/{path}", root.trim_end_matches('/'));
    let mut uri = String::from("file://");
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || b"/-._~:".contains(&byte) {
            uri.push(byte as char);
        } else {
            uri.push_str(&format!("%{byte:02X}"));
        }
    }
    uri
}

/// Scalar column for a UTF-16 column, clamped to the actual line.
pub(crate) fn scalar_column(text: &str, line: u32, column: u32) -> usize {
    let value = text
        .split('\n')
        .nth(line as usize)
        .unwrap_or_default()
        .trim_end_matches('\r');
    let mut units = 0;
    value
        .chars()
        .take_while(|ch| {
            units += ch.len_utf16() as u32;
            units <= column
        })
        .count()
}

#[cfg(test)]
fn contains(diagnostic: &Diagnostic, text: &str, line: usize, column: usize) -> bool {
    let start = (
        diagnostic.line as usize,
        scalar_column(text, diagnostic.line, diagnostic.col),
    );
    let end = (
        diagnostic.end_line as usize,
        scalar_column(text, diagnostic.end_line, diagnostic.end_col),
    );
    range_contains(start, end, (line, column))
}

#[cfg(test)]
pub(crate) fn range_contains(
    start: (usize, usize),
    end: (usize, usize),
    position: (usize, usize),
) -> bool {
    if end <= start {
        position == start
    } else {
        position >= start && position < end
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn coordinates_handle_unicode_crlf_multiline_and_empty_ranges() {
        let text = "a😀z\r\nnext\r\n";
        assert_eq!(scalar_column(text, 0, 3), 2);
        assert_eq!(scalar_column(text, 0, 2), 1);
        assert_eq!(scalar_column(text, 0, 99), 3);
        let mut diagnostic = Diagnostic {
            line: 0,
            col: 1,
            end_line: 1,
            end_col: 2,
            severity: "error",
            message: "test".into(),
        };
        assert!(contains(&diagnostic, text, 0, 1));
        assert!(contains(&diagnostic, text, 1, 1));
        assert!(!contains(&diagnostic, text, 1, 2));
        diagnostic.end_line = 0;
        diagnostic.end_col = 1;
        assert!(contains(&diagnostic, text, 0, 1));
        assert!(!contains(&diagnostic, text, 0, 2));
    }
    #[test]
    fn file_paths_are_uri_encoded() {
        assert_eq!(
            file_uri("/tmp/my project", "a#é.rs"),
            "file:///tmp/my%20project/a%23%C3%A9.rs"
        );
    }
}
