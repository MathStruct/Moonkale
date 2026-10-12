//! Stage all rename targets before one synchronous, undoable Workspace commit.
use crate::{
    lsp::LspManager,
    native_completion::byte_position,
    native_definition::{relative_file, resolve_target},
};
use dioxus::prelude::*;
use moonkale_core::{Node, Version};
use moonkale_ext_api::{
    editor::{EditBatch, EditorSnapshot, RevisionedDocument, TextChange, Utf16Selection},
    Document, Workspace,
};
use moonkale_lsp::{TextEdit, WorkspaceEdit};

pub(crate) fn edit_batch(base: &EditorSnapshot, edits: &[TextEdit]) -> Result<EditBatch, String> {
    let mut changes = Vec::new();
    let has_crlf = base.text.contains("\r\n");
    let starts = std::iter::once(0)
        .chain(base.text.match_indices('\n').map(|(byte, _)| byte + 1))
        .collect::<Vec<_>>();
    // Resolve sorted endpoints by advancing within each line, rather than
    // rescanning a long line for every occurrence of a renamed symbol.
    let mut positions = std::collections::BTreeMap::new();
    for edit in edits {
        positions.insert((edit.line, edit.col), 0);
        positions.insert((edit.end_line, edit.end_col), 0);
    }
    let (mut previous_line, mut previous_col, mut previous_byte) = (None, 0, 0);
    for (&(line, col), byte) in &mut positions {
        let start = *starts.get(line as usize).ok_or("line outside document")?;
        let end = starts
            .get(line as usize + 1)
            .map(|end| end - 1)
            .unwrap_or(base.text.len());
        if previous_line != Some(line) {
            previous_byte = start;
            previous_col = 0;
        }
        previous_byte += byte_position(&base.text[previous_byte..end], 0, col - previous_col)?;
        *byte = previous_byte;
        previous_line = Some(line);
        previous_col = col;
    }
    for edit in edits {
        let start = positions[&(edit.line, edit.col)];
        let end = positions[&(edit.end_line, edit.end_col)];
        if start > end {
            return Err("reversed edit".into());
        }
        let inserted_text = if has_crlf {
            edit.new_text.replace("\r\n", "\n").replace('\n', "\r\n")
        } else {
            edit.new_text.clone()
        };
        changes.push(TextChange {
            range: start..end,
            removed_text: base.text[start..end].into(),
            inserted_text,
        });
    }
    changes.sort_by_key(|change| (change.range.start, change.range.end));
    for pair in changes.windows(2) {
        if pair[0].range.end > pair[1].range.start
            || (pair[0].range.is_empty()
                && pair[1].range.is_empty()
                && pair[0].range.start == pair[1].range.start)
        {
            return Err("overlapping edits".into());
        }
    }
    changes.retain(|change| change.removed_text != change.inserted_text);
    let (mut previous_byte, mut units) = (0, 0u32);
    let positions = changes
        .iter()
        .map(|change| {
            units += base.text[previous_byte..change.range.start]
                .encode_utf16()
                .count() as u32;
            let start = units;
            units += change.removed_text.encode_utf16().count() as u32;
            previous_byte = change.range.end;
            (
                start,
                units,
                change.inserted_text.encode_utf16().count() as u32,
            )
        })
        .collect::<Vec<_>>();
    let rebase = |point: u32| {
        let mut shift = 0i64;
        for &(start, end, inserted) in &positions {
            if point < start {
                break;
            }
            if point <= end {
                return (i64::from(start) + shift + i64::from(inserted)) as u32;
            }
            shift += i64::from(inserted) - i64::from(end - start);
        }
        (i64::from(point) + shift) as u32
    };
    let selection = base.selection.map(|selection| Utf16Selection {
        anchor: rebase(selection.anchor),
        head: rebase(selection.head),
    });
    Ok(EditBatch {
        node: base.node,
        base_revision: base.revision,
        changes,
        selection,
    })
}

struct Staged {
    node: Node,
    version: Version,
    base: RevisionedDocument,
    next: RevisionedDocument,
    stored: Option<moonkale_ext_api::editor::EditorSnapshot>,
    open: Option<Signal<Document>>,
}

/// Closed targets are loaded into unsaved documents, never written to disk here.
/// Every range and every live base is checked before mutating any document.
pub(crate) async fn apply_guarded(
    mut ws: Workspace,
    origin: &Node,
    manager: LspManager,
    edit: &WorkspaceEdit,
    current: impl Fn() -> bool,
) -> Result<usize, String> {
    let root = origin
        .source
        .as_str()
        .strip_prefix("folder:")
        .ok_or("folder not open")?;
    let source = ws
        .sources
        .open
        .peek()
        .iter()
        .find(|source| source.descriptor.id == origin.source)
        .map(|source| source.source.clone())
        .ok_or("folder not open")?;
    let mut staged = Vec::new();
    for (uri, edits) in &edit.changes {
        if !current() {
            return Ok(0);
        }
        let relative =
            relative_file(root, uri).ok_or_else(|| format!("{uri} is outside the folder"))?;
        let node = resolve_target(ws, origin, &relative).await?;
        if staged.iter().any(|entry: &Staged| entry.node.id == node.id) {
            return Err("duplicate target URI".into());
        }
        let open = ws
            .docs
            .open
            .peek()
            .iter()
            .find(|(id, _)| *id == node.id)
            .map(|(_, doc)| *doc);
        let (text, version) = if let Some(doc) = open {
            let doc = doc.peek();
            (doc.text.clone(), doc.version)
        } else {
            source
                .fetch_text(node.id)
                .await
                .map_err(|error| error.to_string())?
        };
        if !current() {
            return Ok(0);
        }
        let mut base = ws
            .docs
            .editor_sessions
            .peek()
            .get(&node.id)
            .map(|session| session.peek().clone())
            .unwrap_or_else(|| RevisionedDocument::new(node.id, text.clone()));
        let stored = ws
            .docs
            .editor_sessions
            .peek()
            .get(&node.id)
            .map(|session| session.peek().snapshot().clone());
        base.replace_from_workspace(text)
            .map_err(|error| format!("{error:?}"))?;
        let batch = edit_batch(base.snapshot(), edits)?;
        if batch.changes.is_empty() {
            continue;
        }
        let mut next = base.clone();
        next.apply_in_place(&batch)
            .map_err(|error| format!("{error:?}"))?;
        staged.push(Staged {
            node,
            version,
            base,
            next,
            stored,
            open,
        });
    }
    if !current() {
        return Ok(0);
    }
    for (uri, version) in &edit.versions {
        if manager.peek_document_version(uri) != Some(*version) {
            return Err("server document version is stale".into());
        }
    }
    // There are no awaits between these checks and the complete in-memory commit.
    for entry in &staged {
        let live = ws
            .docs
            .open
            .peek()
            .iter()
            .find(|(id, _)| *id == entry.node.id)
            .map(|(_, doc)| *doc);
        if live != entry.open
            || live.is_some_and(|doc| doc.peek().text != entry.base.snapshot().text)
        {
            return Err("target changed; retry rename".into());
        }
        if ws
            .docs
            .editor_sessions
            .peek()
            .get(&entry.node.id)
            .map(|session| session.peek().snapshot().clone())
            != entry.stored
        {
            return Err("target revision changed; retry rename".into());
        }
    }
    let lsp = manager
        .peek_document_session(&crate::native_diagnostics::file_uri(
            root,
            &origin.native_key,
        ))
        .ok_or("language server unavailable")?;
    let count = staged.len();
    for entry in staged {
        let changed_node = entry.node.clone();
        let text = entry.next.snapshot().text.clone();
        if let Some(mut doc) = entry.open {
            doc.write().text = text;
        } else {
            let mut document = Document::new(
                entry.node.clone(),
                entry.base.snapshot().text.clone(),
                entry.version,
            );
            document.text = text;
            let doc = Signal::new_in_scope(document, ScopeId::ROOT);
            ws.docs
                .open
                .with_mut(|open| open.push((entry.node.id, doc)));
        }
        let existing = ws.docs.editor_sessions.peek().get(&entry.node.id).copied();
        if let Some(mut session) = existing {
            session.set(entry.next);
        } else {
            let session = Signal::new_in_scope(entry.next, ScopeId::ROOT);
            ws.docs
                .editor_sessions
                .with_mut(|sessions| sessions.insert(entry.node.id, session));
        }
        manager.retain_workspace_document(
            ws,
            &changed_node,
            lsp.clone(),
            root,
            origin.language_hint().unwrap_or("plaintext"),
        );
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use moonkale_core::{NodeId, SourceId};
    fn edit(col: u32, end: u32, text: &str) -> TextEdit {
        TextEdit {
            line: 0,
            col,
            end_line: 0,
            end_col: end,
            new_text: text.into(),
        }
    }
    #[test]
    fn rename_batches_preserve_crlf_selection_and_grouped_history() {
        let node = NodeId::derive(&SourceId::new("test"), "test.rs");
        let mut doc = RevisionedDocument::new(node, "😀old old\r\n".into());
        doc.set_selection(
            doc.snapshot().revision,
            Utf16Selection { anchor: 4, head: 9 },
        );
        let batch = edit_batch(
            doc.snapshot(),
            &[edit(2, 5, "longer"), edit(6, 9, "longer")],
        )
        .unwrap();
        let next = doc.apply(&batch).unwrap();
        assert_eq!(next.text, "😀longer longer\r\n");
        assert_eq!(
            next.selection,
            Some(Utf16Selection {
                anchor: 8,
                head: 15
            })
        );
        assert_eq!(doc.undo().unwrap().unwrap().text, "😀old old\r\n");
        assert_eq!(doc.redo().unwrap().unwrap().text, next.text);
        let batch = edit_batch(doc.snapshot(), &[edit(2, 8, "a\nb")]).unwrap();
        assert_eq!(batch.changes[0].inserted_text, "a\r\nb");
    }

    #[test]
    fn many_unicode_edits_rebase_selection_in_canonical_crlf_text() {
        let node = NodeId::derive(&SourceId::new("test"), "test.rs");
        let text = "😀old\r\n".repeat(1000);
        let mut doc = RevisionedDocument::new(node, text);
        let end = doc.utf16_len();
        doc.set_selection(
            doc.snapshot().revision,
            Utf16Selection {
                anchor: end,
                head: 2,
            },
        );
        let edits: Vec<_> = (0..1000)
            .map(|line| TextEdit {
                line,
                col: 2,
                end_line: line,
                end_col: 5,
                new_text: "λ".into(),
            })
            .collect();
        let batch = edit_batch(doc.snapshot(), &edits).unwrap();
        doc.apply_in_place(&batch).unwrap();
        assert_eq!(doc.snapshot().text, "😀λ\r\n".repeat(1000));
        assert_eq!(
            doc.snapshot().selection,
            Some(Utf16Selection {
                anchor: 5000,
                head: 3
            })
        );
        assert_eq!(doc.undo().unwrap().unwrap().text, "😀old\r\n".repeat(1000));
    }
    #[test]
    fn many_occurrences_in_one_long_line_map_without_prefix_rescans() {
        let node = NodeId::derive(&SourceId::new("test"), "test.rs");
        let mut doc = RevisionedDocument::new(
            node,
            format!("{}{}", "x".repeat(3_000_000), "😀old ".repeat(1000)),
        );
        let edits: Vec<_> = (0..1000)
            .map(|index| edit(3_000_002 + index * 6, 3_000_005 + index * 6, "λ"))
            .collect();
        let batch = edit_batch(doc.snapshot(), &edits).unwrap();
        doc.apply_in_place(&batch).unwrap();
        assert_eq!(
            doc.snapshot().text,
            format!("{}{}", "x".repeat(3_000_000), "😀λ ".repeat(1000))
        );
    }
    #[test]
    fn rejects_surrogate_splits_out_of_bounds_reversed_and_overlapping_edits() {
        let node = NodeId::derive(&SourceId::new("test"), "test.rs");
        let doc = RevisionedDocument::new(node, "😀old\r\n".into());
        for edits in [
            vec![edit(1, 2, "x")],
            vec![edit(2, 99, "x")],
            vec![edit(5, 2, "x")],
            vec![edit(2, 5, "a"), edit(3, 5, "b")],
            vec![edit(2, 2, "a"), edit(2, 2, "b")],
        ] {
            assert!(edit_batch(doc.snapshot(), &edits).is_err());
        }
    }
}
