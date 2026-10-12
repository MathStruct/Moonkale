//! LSP coordinates stay UTF-16 until the decoration boundary.
use crate::lsp::LspManager;
use dioxus::prelude::*;
use moonkale_core::NodeId;
use moonkale_ext_api::Workspace;
use moonkale_lsp::Diagnostic;

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
    let attached = crate::lsp::use_document(
        ws,
        doc,
        manager,
        identity.map(|(language, root, _)| (language, root, uri.clone().unwrap())),
    );
    let update_uri = uri.clone();
    use_effect(move || {
        let document = doc.read();
        let _ = attached.read();
        if let Some(uri) = &update_uri {
            manager.update_document(uri, &document.text, &document.saved);
        }
    });
    use_memo(move || {
        uri.as_ref()
            .and_then(|uri| manager.diagnostics.read().get(uri).cloned())
            .unwrap_or_default()
    })
}

pub(crate) fn file_uri(root: &str, path: &str) -> String {
    crate::uri::file_uri(root, path)
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
            raw: None,
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
