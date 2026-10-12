//! F2 rename prompt, bounded LSP request and guarded Workspace edits.
use crate::{
    lsp::LspManager, native_completion::byte_position, native_diagnostics::file_uri,
    native_model::NativeModel, native_workspace_edit::apply_guarded, L,
};
use dioxus::prelude::*;
use futures_util::{
    future::{select, Either},
    FutureExt,
};
use moonkale_core::NodeId;
use moonkale_ext_api::{
    editor::{DocumentRevision, Utf16Selection},
    t, Workspace,
};
use std::time::Duration;

#[derive(Clone, PartialEq)]
struct Request {
    serial: u64,
    revision: DocumentRevision,
    selection: Utf16Selection,
    line: u32,
    col: u32,
    word: String,
    name: Option<String>,
    documents: Vec<(NodeId, String, Option<DocumentRevision>)>,
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) struct Rename {
    request: Signal<Option<Request>>,
    value: Signal<String>,
    pub invoke: Callback<()>,
    pub cancel: Callback<()>,
    pub dismiss: Callback<()>,
    commit: Callback<()>,
}

pub(crate) fn use_rename(
    ws: Workspace,
    node: NodeId,
    manager: LspManager,
    model: Signal<NativeModel>,
    mut focus: Signal<u64>,
) -> Rename {
    let doc = ws.document(node).expect("open document");
    let mut session = ws.editor_session(node).expect("open session");
    let origin = use_hook(move || doc.peek().node.clone());
    let mut request = use_signal(|| None::<Request>);
    let mut value = use_signal(String::new);
    let mut serial = use_signal(|| 0u64);
    let valid = use_memo(move || {
        let pending = request.read();
        let Some(pending) = pending.as_ref() else {
            return false;
        };
        let snapshot = session.read();
        let snapshot = snapshot.snapshot();
        if *ws.docs.active.read() != Some(node)
            || snapshot.revision != pending.revision
            || snapshot.selection != Some(pending.selection)
        {
            return false;
        }
        if !ws
            .sources
            .open
            .read()
            .iter()
            .any(|source| source.descriptor.id == doc.read().node.source)
        {
            return false;
        }
        let open = ws.docs.open.read();
        let sessions = ws.docs.editor_sessions.read();
        pending.documents.iter().all(|(id, text, revision)| {
            open.iter()
                .find(|(live_id, _)| live_id == id)
                .is_some_and(|(_, doc)| doc.read().text == *text)
                && sessions
                    .get(id)
                    .map(|session| session.read().snapshot().revision)
                    .unwrap_or_default()
                    == revision.unwrap_or_default()
        })
    });
    use_effect(move || {
        if !valid() && request.peek().is_some() {
            request.set(None);
        }
    });
    let invoke = Callback::new(move |_: ()| {
        if *ws.docs.active.peek() != Some(node) {
            return;
        }
        let node_data = doc.peek().node.clone();
        let available = node_data
            .source
            .as_str()
            .strip_prefix("folder:")
            .is_some_and(|root| {
                manager
                    .peek_document_session(&file_uri(root, &node_data.native_key))
                    .is_some()
            });
        if !available {
            let mut ws = ws;
            ws.set_status(t!(ws, L, "editor-rename-needs-lsp"));
            return;
        }
        let snapshot = session.peek().snapshot().clone();
        if doc.peek().text != snapshot.text {
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
        let word = word_at(&snapshot.text, caret.line as u32, col).unwrap_or_default();
        let selection = model.peek().selection(&snapshot.text);
        session.with_mut(|session| {
            session.set_selection(snapshot.revision, selection);
        });
        let documents = ws
            .docs
            .open
            .peek()
            .iter()
            .map(|(id, doc)| {
                (
                    *id,
                    doc.peek().text.clone(),
                    ws.docs
                        .editor_sessions
                        .peek()
                        .get(id)
                        .map(|session| session.peek().snapshot().revision),
                )
            })
            .collect();
        serial.with_mut(|value| *value += 1);
        value.set(word.clone());
        request.set(Some(Request {
            serial: serial(),
            revision: snapshot.revision,
            selection,
            line: caret.line as u32,
            col,
            word,
            name: None,
            documents,
        }));
    });
    let dismiss = Callback::new(move |_: ()| request.set(None));
    let cancel = Callback::new(move |_: ()| {
        let active = request.peek().is_some();
        dismiss.call(());
        if active && *ws.docs.active.peek() == Some(node) {
            focus.with_mut(|value| *value += 1);
        }
    });
    let commit = Callback::new(move |_: ()| {
        let name = value.peek().trim().to_string();
        if name.is_empty() || !valid() {
            return;
        }
        let mut pending = request.peek().clone().expect("valid request");
        if pending.name.is_some() {
            return;
        }
        if name == pending.word {
            cancel.call(());
            return;
        }
        pending.name = Some(name);
        request.set(Some(pending));
        focus.with_mut(|value| *value += 1);
    });
    let _result = use_resource(move || {
        let pending = request();
        let valid_now = valid();
        let origin = origin.clone();
        async move {
            let Some(pending) = pending.filter(|pending| pending.name.is_some()) else {
                return;
            };
            if !valid_now {
                return;
            }
            let expected = pending.clone();
            let source_id = origin.source.clone();
            let current = move || {
                ws.sources
                    .open
                    .peek()
                    .iter()
                    .any(|source| source.descriptor.id == source_id)
                    && *valid.peek()
                    && request.peek().as_ref() == Some(&expected)
                    && *ws.docs.active.peek() == Some(node)
                    && session.peek().snapshot().revision == expected.revision
                    && session.peek().snapshot().selection == Some(expected.selection)
                    && expected.documents.iter().all(|(id, text, revision)| {
                        ws.docs
                            .open
                            .peek()
                            .iter()
                            .find(|(live_id, _)| live_id == id)
                            .is_some_and(|(_, doc)| doc.peek().text == *text)
                            && ws
                                .docs
                                .editor_sessions
                                .peek()
                                .get(id)
                                .map(|session| session.peek().snapshot().revision)
                                .unwrap_or_default()
                                == revision.unwrap_or_default()
                    })
            };
            let outcome = async {
                let root = origin
                    .source
                    .as_str()
                    .strip_prefix("folder:")
                    .ok_or("folder not open")?;
                let uri = file_uri(root, &origin.native_key);
                manager.sync_workspace_documents(ws);
                let lsp = manager
                    .peek_document_session(&uri)
                    .ok_or("language server unavailable")?;
                let edit = match select(
                    lsp.rename(
                        &uri,
                        pending.line,
                        pending.col,
                        pending.name.as_deref().unwrap(),
                    )
                    .boxed_local(),
                    futures_timer::Delay::new(Duration::from_secs(5)).boxed_local(),
                )
                .await
                {
                    Either::Left((result, _)) => result?,
                    Either::Right(_) => return Err(t!(ws, L, "editor-rename-timeout")),
                };
                if !current() {
                    return Ok(None);
                }
                let count = apply_guarded(ws, &origin, manager, &edit, current.clone()).await?;
                Ok(if count == 0 && !current() {
                    None
                } else {
                    Some(count)
                })
            }
            .await;
            // Successful edits invalidate the original revision, so use serial for cleanup.
            if request
                .peek()
                .as_ref()
                .is_some_and(|request| request.serial == pending.serial)
            {
                match outcome {
                    Ok(Some(count)) => {
                        let mut ws = ws;
                        ws.set_status(if count == 0 {
                            t!(ws, L, "editor-nothing-to-rename")
                        } else {
                            t!(ws, L, "editor-applied", n = count)
                        });
                    }
                    Err(error) if current() => {
                        let mut ws = ws;
                        ws.set_status(t!(ws, L, "editor-edit-failed", error = error));
                    }
                    _ => {}
                }
                request.set(None);
                if *ws.docs.active.peek() == Some(node) {
                    focus.with_mut(|value| *value += 1);
                }
            }
        }
    });
    Rename {
        request,
        value,
        invoke,
        cancel,
        dismiss,
        commit,
    }
}

fn word_at(text: &str, line: u32, col: u32) -> Result<String, &'static str> {
    let caret = byte_position(text, line, col)?;
    let identifier = |ch: char| ch.is_alphanumeric() || ch == '_';
    let start = text[..caret]
        .char_indices()
        .rev()
        .take_while(|(_, ch)| identifier(*ch))
        .last()
        .map(|(byte, _)| byte)
        .unwrap_or(caret);
    let end = caret
        + text[caret..]
            .chars()
            .take_while(|ch| identifier(*ch))
            .map(char::len_utf8)
            .sum::<usize>();
    Ok(text[start..end].into())
}

#[component]
pub(crate) fn RenamePrompt(ws: Workspace, mut rename: Rename) -> Element {
    let Some(pending) = (rename.request)() else {
        return rsx! {};
    };
    let busy = pending.name.is_some();
    let label = t!(ws, L, "editor-rename-prompt", word = pending.word);
    rsx! {
        div { class: "mk-editor-bar mk-editor-rename mk-native-rename-prompt", role: "dialog", aria_label: "{label}", aria_busy: "{busy}",
            onkeydown: move |event| {
                event.stop_propagation();
                if event.key() == Key::Escape { event.prevent_default(); rename.cancel.call(()); }
                if event.key() == Key::Enter { event.prevent_default(); rename.commit.call(()); }
            },
            span { "{label}" }
            input { class: "mk-input", aria_label: "{label}", value: "{rename.value}", disabled: busy,
                onmounted: move |event| { spawn(async move { let _ = event.data().set_focus(true).await; }); },
                oninput: move |event| rename.value.set(event.value()),
            }
            button { class: "mk-btn mk-native-rename-confirm", disabled: busy || rename.value.read().trim().is_empty(), onclick: move |_| rename.commit.call(()), {t!(ws, L, "editor-rename")} }
            button { class: "mk-btn", onclick: move |_| rename.cancel.call(()), {t!(ws, L, "editor-cancel")} }
        }
    }
}
