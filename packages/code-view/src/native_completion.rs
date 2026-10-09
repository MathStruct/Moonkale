//! Completion requests and atomic edits in Workspace coordinates.
use crate::{lsp::LspManager, native_diagnostics::file_uri, native_model::NativeModel};
use dioxus::prelude::*;
use futures_util::{
    future::{select, Either},
    FutureExt,
};
use moonkale_core::NodeId;
use moonkale_ext_api::{
    editor::{DocumentRevision, EditBatch, EditorSnapshot, TextChange, Utf16Selection},
    Workspace,
};
use moonkale_lsp::{CompletionItem, TextEdit};
use std::{ops::Range, time::Duration};

#[derive(Clone, PartialEq, Eq)]
struct Request {
    serial: u64,
    revision: DocumentRevision,
    selection: Utf16Selection,
    line: u32,
    col: u32,
    wiki: Option<WikiContext>,
}
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Menu {
    request: Request,
    pub items: Vec<CompletionItem>,
}

pub(crate) fn use_completion(
    ws: Workspace,
    node: NodeId,
    manager: LspManager,
    mut model: Signal<NativeModel>,
    mut first_row: Signal<usize>,
    mut focus: Signal<u64>,
    oncommit: Callback<()>,
) -> (
    Memo<Option<Menu>>,
    Callback<()>,
    Callback<usize>,
    Callback<()>,
) {
    let mut doc = ws.document(node).expect("open document");
    let mut session = ws.editor_session(node).expect("open session");
    let uri = use_hook(move || {
        let doc = doc.peek();
        Some(file_uri(
            doc.node.source.as_str().strip_prefix("folder:")?,
            &doc.node.native_key,
        ))
    });
    let mut request = use_signal(|| None::<Request>);
    let mut serial = use_signal(|| 0u64);
    let current = use_memo(move || {
        let session = session.read();
        (session.snapshot().revision, session.snapshot().selection)
    });
    use_effect(move || {
        let now = current();
        let stale = request
            .peek()
            .as_ref()
            .is_some_and(|request| now != (request.revision, Some(request.selection)));
        if stale {
            request.set(None);
        }
    });
    let request_uri = uri.clone();
    let start = Callback::new(move |_: ()| {
        let snapshot = session.peek();
        let snapshot = snapshot.snapshot();
        let selection = model.peek().selection(&snapshot.text);
        if selection.anchor != selection.head || doc.peek().text != snapshot.text {
            request.set(None);
            return;
        }
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
        let wiki = (doc.peek().node.language_hint() == Some("markdown"))
            .then(|| wiki_context(&snapshot.text, caret.line as u32, col))
            .flatten();
        if wiki.is_none()
            && request_uri
                .as_ref()
                .and_then(|uri| manager.document_session(uri))
                .is_none()
        {
            return;
        }
        serial.with_mut(|value| *value += 1);
        request.set(Some(Request {
            serial: serial(),
            revision: snapshot.revision,
            selection,
            line: caret.line as u32,
            col,
            wiki,
        }));
    });
    let result = use_resource(move || {
        let request = request();
        let _ = ws.sources.graph_epoch.read();
        let current = current();
        let uri = uri.clone();
        let lsp = uri.as_ref().and_then(|uri| manager.document_session(uri));
        async move {
            let request = request?;
            if current != (request.revision, Some(request.selection)) {
                return None;
            }
            if let Some(context) = &request.wiki {
                futures_timer::Delay::new(Duration::from_millis(150)).await;
                let candidates = match select(
                    ws.wiki_candidates(&context.query, 12).boxed_local(),
                    futures_timer::Delay::new(Duration::from_secs(5)).boxed_local(),
                )
                .await
                {
                    Either::Left((candidates, _)) => candidates,
                    _ => return None,
                };
                let items = candidates
                    .into_iter()
                    .map(|candidate| CompletionItem {
                        label: candidate.target.clone(),
                        kind: "".into(),
                        detail: Some(candidate.key),
                        insert: None,
                        sort: None,
                        filter: None,
                        additional_edits: vec![],
                        text_edit: Some(TextEdit {
                            line: request.line,
                            col: context.start_col,
                            end_line: request.line,
                            end_col: context.end_col,
                            new_text: format!("{}{}", candidate.target, context.close),
                        }),
                    })
                    .collect::<Vec<_>>();
                return (!items.is_empty()).then_some(Menu { request, items });
            }
            let lsp = lsp?;
            let uri = uri?;
            futures_timer::Delay::new(Duration::from_millis(150)).await;
            let response = select(
                lsp.completion(&uri, request.line, request.col)
                    .boxed_local(),
                futures_timer::Delay::new(Duration::from_secs(5)).boxed_local(),
            )
            .await;
            let mut items = match response {
                Either::Left((Ok(items), _)) => items,
                _ => return None,
            };
            if items.is_empty() {
                return None;
            }
            let prefix = prefix(&doc.peek().text, request.line, request.col)
                .unwrap_or_default()
                .to_lowercase();
            items.retain(|item| {
                item.filter
                    .as_deref()
                    .unwrap_or(&item.label)
                    .to_lowercase()
                    .starts_with(&prefix)
            });
            items.sort_by(|a, b| {
                a.sort
                    .as_ref()
                    .unwrap_or(&a.label)
                    .cmp(b.sort.as_ref().unwrap_or(&b.label))
            });
            if items.is_empty() {
                None
            } else {
                Some(Menu { request, items })
            }
        }
    });
    let menu = use_memo(move || {
        if *result.state().read() != UseResourceState::Ready {
            return None;
        }
        result
            .read()
            .as_ref()
            .and_then(|value| value.as_ref())
            .filter(|menu| {
                request().as_ref() == Some(&menu.request)
                    && (menu.request.wiki.is_none() || *ws.docs.active.read() == Some(node))
                    && current() == (menu.request.revision, Some(menu.request.selection))
            })
            .cloned()
    });
    let close = Callback::new(move |_: ()| request.set(None));
    let accept = Callback::new(move |index: usize| {
        let Some(menu) = menu() else {
            return;
        };
        let Some(item) = menu.items.get(index) else {
            return;
        };
        let base = session.peek().snapshot().clone();
        if base.revision != menu.request.revision
            || base.selection != Some(menu.request.selection)
            || doc.peek().text != base.text
        {
            request.set(None);
            return;
        }
        if let Ok(batch) = completion_batch(&base, item, menu.request.line, menu.request.col) {
            if let Ok(snapshot) = session.with_mut(|session| session.apply(&batch)) {
                model.with_mut(|model| model.replace(&snapshot));
                doc.write().text = snapshot.text;
                let caret = model.peek().engine.get_cursor_state().position;
                let row = model
                    .peek()
                    .engine
                    .editor()
                    .logical_position_to_visual(caret.line, caret.column)
                    .map(|(row, _)| row)
                    .unwrap_or(0);
                first_row.set(row.saturating_sub(3));
                oncommit.call(());
                focus.with_mut(|value| *value += 1);
            }
        }
        request.set(None);
    });
    (menu, start, accept, close)
}

#[derive(Clone, PartialEq, Eq)]
struct WikiContext {
    query: String,
    start_col: u32,
    end_col: u32,
    close: String,
}

// Complete only the page part of an unfinished link on the caret's line.
fn wiki_context(text: &str, line: u32, col: u32) -> Option<WikiContext> {
    let caret = byte_position(text, line, col).ok()?;
    let start = text[..caret].rfind("[[")? + 2;
    let query = &text[start..caret];
    if query.contains(['\n', '\r', '[', ']', '#', '|']) {
        return None;
    }
    let line_start = text[..start].rfind('\n').map_or(0, |byte| byte + 1);
    let tail = text[caret..].split(['\n', '\r', '[']).next()?;
    let (end, close) = if let Some(closing) = tail.find("]]") {
        if let Some(part) = tail[..closing].find(['#', '|']) {
            (caret + part, "")
        } else {
            (caret + closing + 2, "]]")
        }
    } else {
        (caret + usize::from(tail.starts_with(']')), "]]")
    };
    Some(WikiContext {
        query: query.into(),
        start_col: text[line_start..start].encode_utf16().count() as u32,
        end_col: text[line_start..end].encode_utf16().count() as u32,
        close: close.into(),
    })
}

pub(crate) fn byte_position(text: &str, line: u32, col: u32) -> Result<usize, &'static str> {
    let mut start = 0;
    for (index, value) in text.split('\n').enumerate() {
        if index == line as usize {
            let mut units = 0;
            for (byte, ch) in value.trim_end_matches('\r').char_indices() {
                if units == col {
                    return Ok(start + byte);
                }
                units += ch.len_utf16() as u32;
                if units > col {
                    return Err("position splits a surrogate pair");
                }
            }
            return if units == col {
                Ok(start + value.trim_end_matches('\r').len())
            } else {
                Err("column outside line")
            };
        }
        start += value.len() + 1;
    }
    Err("line outside document")
}
fn prefix_range(text: &str, line: u32, col: u32) -> Result<Range<usize>, &'static str> {
    let end = byte_position(text, line, col)?;
    let start = text[..end]
        .char_indices()
        .rev()
        .take_while(|(_, ch)| ch.is_alphanumeric() || *ch == '_')
        .last()
        .map(|(byte, _)| byte)
        .unwrap_or(end);
    Ok(start..end)
}
fn prefix(text: &str, line: u32, col: u32) -> Result<&str, &'static str> {
    Ok(&text[prefix_range(text, line, col)?])
}

pub(crate) fn completion_batch(
    base: &EditorSnapshot,
    item: &CompletionItem,
    line: u32,
    col: u32,
) -> Result<EditBatch, &'static str> {
    let normalize = |value: &str| {
        if base.text.contains("\r\n") {
            value.replace("\r\n", "\n").replace('\n', "\r\n")
        } else {
            value.to_string()
        }
    };
    let change = |edit: &TextEdit| -> Result<TextChange, &'static str> {
        let range = byte_position(&base.text, edit.line, edit.col)?
            ..byte_position(&base.text, edit.end_line, edit.end_col)?;
        if range.start > range.end {
            return Err("reversed edit");
        }
        Ok(TextChange {
            removed_text: base.text[range.clone()].to_string(),
            range,
            inserted_text: normalize(&edit.new_text),
        })
    };
    let primary = if let Some(edit) = &item.text_edit {
        change(edit)?
    } else {
        let range = prefix_range(&base.text, line, col)?;
        TextChange {
            removed_text: base.text[range.clone()].to_string(),
            range,
            inserted_text: normalize(item.insert.as_deref().unwrap_or(&item.label)),
        }
    };
    let mut caret = primary.range.start as i64 + primary.inserted_text.len() as i64;
    let mut changes = vec![primary.clone()];
    for edit in &item.additional_edits {
        let change = change(edit)?;
        if change.range.end <= primary.range.start {
            caret += change.inserted_text.len() as i64 - change.range.len() as i64;
        }
        changes.push(change);
    }
    changes.sort_by_key(|change| (change.range.start, change.range.end));
    for pair in changes.windows(2) {
        if pair[0].range.end > pair[1].range.start
            || (pair[0].range.is_empty()
                && pair[1].range.is_empty()
                && pair[0].range.start == pair[1].range.start)
        {
            return Err("overlapping edits");
        }
    }
    let mut text = base.text.clone();
    for change in changes.iter().rev() {
        text.replace_range(change.range.clone(), &change.inserted_text);
    }
    if caret < 0 || !text.is_char_boundary(caret as usize) {
        return Err("invalid resulting caret");
    }
    let head = text[..caret as usize].encode_utf16().count() as u32;
    Ok(EditBatch {
        node: base.node,
        base_revision: base.revision,
        changes,
        selection: Some(Utf16Selection { anchor: head, head }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wiki_completion_replaces_unicode_suffix_and_closers_atomically() {
        use moonkale_core::SourceId;
        use moonkale_ext_api::editor::RevisionedDocument;
        for (text, col, expected) in [
            ("😀 [[No]]\r\n", 7, "😀 [[Note]]\r\n"),
            ("😀 [[No]\r\n", 7, "😀 [[Note]]\r\n"),
            ("😀 [[No suffix\r\n", 7, "😀 [[Note]] suffix\r\n"),
            (
                "😀 [[No#Heading|alias]]\r\n",
                7,
                "😀 [[Note#Heading|alias]]\r\n",
            ),
        ] {
            let context = wiki_context(text, 0, col).unwrap();
            let mut item = item();
            item.text_edit = Some(TextEdit {
                line: 0,
                col: context.start_col,
                end_line: 0,
                end_col: context.end_col,
                new_text: format!("Note{}", context.close),
            });
            let node = NodeId::derive(&SourceId::new("test"), "note.md");
            let mut document = RevisionedDocument::new(node, text.into());
            let batch = completion_batch(document.snapshot(), &item, 0, col).unwrap();
            assert_eq!(document.apply(&batch).unwrap().text, expected);
            assert_eq!(document.undo().unwrap().unwrap().text, text);
        }
        assert!(wiki_context("[[Note]] next", 0, 13).is_none());
        assert!(wiki_context("[[Note#head", 0, 11).is_none());
        assert!(wiki_context("[[Note\r\nnext", 1, 4).is_none());
    }
    use moonkale_core::SourceId;
    use moonkale_ext_api::editor::RevisionedDocument;
    fn item() -> CompletionItem {
        CompletionItem {
            label: "println!".into(),
            kind: "function".into(),
            detail: None,
            insert: None,
            sort: None,
            filter: None,
            text_edit: None,
            additional_edits: vec![],
        }
    }
    #[test]
    fn completion_and_import_are_one_crlf_unicode_history_entry() {
        let original = "😀\r\n    pri\r\n";
        let node = NodeId::derive(&SourceId::new("test"), "test.rs");
        let mut document = RevisionedDocument::new(node, original.into());
        let mut item = item();
        item.additional_edits.push(TextEdit {
            line: 0,
            col: 0,
            end_line: 0,
            end_col: 0,
            new_text: "use thing;\n".into(),
        });
        let batch = completion_batch(document.snapshot(), &item, 1, 7).unwrap();
        assert_eq!(batch.changes.len(), 2);
        let snapshot = document.apply(&batch).unwrap();
        assert_eq!(snapshot.text, "use thing;\r\n😀\r\n    println!\r\n");
        assert_eq!(
            snapshot.selection.unwrap().head,
            snapshot
                .text
                .trim_end_matches("\r\n")
                .encode_utf16()
                .count() as u32
        );
        assert_eq!(document.undo().unwrap().unwrap().text, original);
        assert!(document.undo().unwrap().is_none());
        assert_eq!(document.redo().unwrap().unwrap().text, snapshot.text);
    }
    #[test]
    fn server_range_overrides_prefix_and_invalid_edits_are_rejected() {
        let node = NodeId::derive(&SourceId::new("test"), "test.rs");
        let document = RevisionedDocument::new(node, "😀word\r\n".into());
        let mut item = item();
        item.text_edit = Some(TextEdit {
            line: 0,
            col: 2,
            end_line: 0,
            end_col: 6,
            new_text: "new".into(),
        });
        let batch = completion_batch(document.snapshot(), &item, 0, 4).unwrap();
        assert_eq!(batch.changes[0].removed_text, "word");
        item.additional_edits.push(TextEdit {
            line: 0,
            col: 3,
            end_line: 0,
            end_col: 5,
            new_text: "overlap".into(),
        });
        assert!(completion_batch(document.snapshot(), &item, 0, 4).is_err());
        item.additional_edits.clear();
        item.text_edit.as_mut().unwrap().col = 1;
        assert!(completion_batch(document.snapshot(), &item, 0, 4).is_err());
        item.text_edit.as_mut().unwrap().line = 99;
        assert!(completion_batch(document.snapshot(), &item, 0, 4).is_err());
    }
}
