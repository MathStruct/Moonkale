//! Code actions and reference lists share guarded request and document lifetimes.
use crate::{
    edit::utf16_offset_to_line_col,
    lsp::LspManager,
    native_definition::{relative_file, resolve_target},
    native_diagnostics::file_uri,
    native_model::NativeModel,
    native_workspace_edit::apply_guarded,
    L,
};
use dioxus::prelude::*;
use futures_util::{
    future::{select, Either},
    FutureExt,
};
use moonkale_core::{Node, NodeId};
use moonkale_ext_api::{
    editor::{DocumentRevision, RevisionedDocument, Utf16Selection},
    t, Workspace,
};
use moonkale_lsp::{CodeAction, Location};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
    time::Duration,
};

#[derive(Clone, PartialEq)]
struct Request {
    serial: u64,
    revision: DocumentRevision,
    selection: Utf16Selection,
    start: (u32, u32),
    end: (u32, u32),
    caret: (u32, u32),
    documents: Vec<(NodeId, String, DocumentRevision)>,
}
#[derive(Clone, PartialEq)]
enum Task {
    Actions,
    References,
    Apply(CodeAction),
    Reveal(Location),
}
#[derive(Clone, PartialEq)]
struct Operation {
    request: Request,
    task: Task,
}
#[derive(Clone, PartialEq)]
enum List {
    Actions(Vec<CodeAction>),
    References(Vec<Location>),
}
impl List {
    fn length(&self) -> usize {
        match self {
            Self::Actions(items) => items.len(),
            Self::References(items) => items.len(),
        }
    }
    fn enabled(&self, index: usize, root: &str) -> bool {
        match self {
            Self::Actions(items) => items.get(index).is_some_and(|item| item.disabled.is_none()),
            Self::References(items) => items
                .get(index)
                .is_some_and(|item| relative_file(root, &item.uri).is_some()),
        }
    }
}
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct Tools {
    operation: Signal<Option<Operation>>,
    list: Signal<Option<List>>,
    selected: Signal<usize>,
    pub actions: Callback<()>,
    pub references: Callback<()>,
    pub cancel: Callback<()>,
    pub dismiss: Callback<()>,
    choose: Callback<(u64, usize)>,
}

fn current(
    ws: Workspace,
    node: NodeId,
    session: Signal<RevisionedDocument>,
    origin: &Node,
    operation: Signal<Option<Operation>>,
    expected: &Operation,
) -> bool {
    let snapshot = session.peek();
    let snapshot = snapshot.snapshot();
    operation.peek().as_ref() == Some(expected)
        && *ws.docs.active.peek() == Some(node)
        && snapshot.revision == expected.request.revision
        && snapshot.selection == Some(expected.request.selection)
        && ws
            .sources
            .open
            .peek()
            .iter()
            .any(|source| source.descriptor.id == origin.source)
        && expected
            .request
            .documents
            .iter()
            .all(|(id, text, revision)| {
                ws.docs
                    .open
                    .peek()
                    .iter()
                    .find(|(live, _)| live == id)
                    .is_some_and(|(_, doc)| doc.peek().text == *text)
                    && ws
                        .docs
                        .editor_sessions
                        .peek()
                        .get(id)
                        .map(|session| session.peek().snapshot().revision)
                        .unwrap_or_default()
                        == *revision
            })
}

pub(crate) fn use_tools(
    ws: Workspace,
    node: NodeId,
    manager: LspManager,
    model: Signal<NativeModel>,
    mut focus: Signal<u64>,
) -> Tools {
    let doc = ws.document(node).expect("open document");
    let mut session = ws.editor_session(node).expect("open session");
    let origin = use_hook(move || doc.peek().node.clone());
    let mut operation = use_signal(|| None::<Operation>);
    let mut list = use_signal(|| None::<List>);
    let mut selected = use_signal(|| 0usize);
    let mut serial = use_signal(|| 0u64);
    let valid_origin = origin.clone();
    let valid = use_memo(move || {
        let pending = operation.read();
        let Some(pending) = pending.as_ref() else {
            return false;
        };
        // Subscribe to canonical changes; the async request itself uses peek reads.
        let _ = session.read();
        let _ = ws.docs.active.read();
        let _ = ws.sources.open.read();
        let open = ws.docs.open.read();
        for (_, doc) in open.iter() {
            let _ = doc.read();
        }
        let sessions = ws.docs.editor_sessions.read();
        for session in sessions.values() {
            let _ = session.read();
        }
        current(ws, node, session, &valid_origin, operation, pending)
    });
    use_effect(move || {
        if !valid() && operation.peek().is_some() {
            operation.set(None);
            list.set(None);
        }
    });
    let start = Callback::new(move |references: bool| {
        if *ws.docs.active.peek() != Some(node) {
            return;
        }
        let doc = doc.peek();
        let snapshot = session.peek().snapshot().clone();
        if doc.text != snapshot.text {
            return;
        }
        let available = doc
            .node
            .source
            .as_str()
            .strip_prefix("folder:")
            .is_some_and(|root| {
                manager
                    .peek_document_session(&file_uri(root, &doc.node.native_key))
                    .is_some()
            });
        if !available {
            let mut ws = ws;
            ws.set_status(t!(ws, L, "editor-tools-needs-lsp"));
            return;
        }
        let selection = model.peek().selection(&snapshot.text);
        let Some((start, end, caret)) = positions(&snapshot.text, selection) else {
            return;
        };
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
                        .map(|session| session.peek().snapshot().revision)
                        .unwrap_or_default(),
                )
            })
            .collect();
        serial.with_mut(|serial| *serial += 1);
        selected.set(0);
        list.set(None);
        operation.set(Some(Operation {
            request: Request {
                serial: serial(),
                revision: snapshot.revision,
                selection,
                start,
                end,
                caret,
                documents,
            },
            task: if references {
                Task::References
            } else {
                Task::Actions
            },
        }));
    });
    let dismiss = Callback::new(move |_: ()| {
        operation.set(None);
        list.set(None);
    });
    let cancel = Callback::new(move |_: ()| {
        let active = operation.peek().is_some();
        dismiss.call(());
        if active && *ws.docs.active.peek() == Some(node) {
            focus.with_mut(|focus| *focus += 1);
        }
    });
    let choose = Callback::new(move |(serial, index): (u64, usize)| {
        if !valid() {
            return;
        }
        let Some(mut pending) = operation.peek().clone() else {
            return;
        };
        if pending.request.serial != serial {
            return;
        }
        let task = match list.peek().as_ref() {
            Some(List::Actions(items)) => items
                .get(index)
                .filter(|item| item.disabled.is_none())
                .cloned()
                .map(Task::Apply),
            Some(List::References(items)) => items.get(index).cloned().map(Task::Reveal),
            _ => None,
        };
        if let Some(task) = task {
            pending.task = task;
            list.set(None);
            operation.set(Some(pending));
            focus.with_mut(|focus| *focus += 1);
        }
    });
    let request_origin = origin.clone();
    let _result = use_resource(move || {
        let pending = operation();
        let valid_now = valid();
        let origin = request_origin.clone();
        async move {
            let Some(pending) = pending else {
                return;
            };
            if !valid_now {
                return;
            }
            let expected = pending.clone();
            let guard_origin = origin.clone();
            let guard = move || current(ws, node, session, &guard_origin, operation, &expected);
            let root = origin
                .source
                .as_str()
                .strip_prefix("folder:")
                .unwrap_or_default();
            let outcome: Result<Option<List>, String> = async {
                manager.sync_workspace_documents(ws);
                let lsp = manager
                    .peek_document_session(&file_uri(root, &origin.native_key))
                    .ok_or("language server unavailable")?;
                match &pending.task {
                    Task::Actions => {
                        let uri = file_uri(root, &origin.native_key);
                        let diagnostics = manager
                            .diagnostics
                            .peek()
                            .get(&uri)
                            .cloned()
                            .unwrap_or_default()
                            .into_iter()
                            .filter(|diagnostic| {
                                (diagnostic.line, diagnostic.col) <= pending.request.end
                                    && (diagnostic.end_line, diagnostic.end_col)
                                        >= pending.request.start
                            })
                            .collect::<Vec<_>>();
                        let request = lsp.code_actions_with_diagnostics(
                            &uri,
                            pending.request.start.0,
                            pending.request.start.1,
                            pending.request.end.0,
                            pending.request.end.1,
                            &diagnostics,
                        );
                        let response = select(
                            request.boxed_local(),
                            futures_timer::Delay::new(Duration::from_secs(5)).boxed_local(),
                        )
                        .await;
                        match response {
                            Either::Left((result, _)) => Ok(Some(List::Actions(result?))),
                            Either::Right(_) => Err(t!(ws, L, "editor-tools-timeout")),
                        }
                    }
                    Task::References => {
                        match select(
                            lsp.references(
                                &file_uri(root, &origin.native_key),
                                pending.request.caret.0,
                                pending.request.caret.1,
                            )
                            .boxed_local(),
                            futures_timer::Delay::new(Duration::from_secs(5)).boxed_local(),
                        )
                        .await
                        {
                            Either::Left((result, _)) => {
                                let mut seen = HashSet::new();
                                Ok(Some(List::References(
                                    result?
                                        .into_iter()
                                        .filter(|location| {
                                            seen.insert((
                                                location.uri.clone(),
                                                location.line,
                                                location.col,
                                            ))
                                        })
                                        .collect(),
                                )))
                            }
                            Either::Right(_) => Err(t!(ws, L, "editor-tools-timeout")),
                        }
                    }
                    Task::Apply(action) => {
                        let edit = match select(
                            lsp.resolve_code_action(action).boxed_local(),
                            futures_timer::Delay::new(Duration::from_secs(5)).boxed_local(),
                        )
                        .await
                        {
                            Either::Left((result, _)) => result?,
                            Either::Right(_) => return Err(t!(ws, L, "editor-tools-timeout")),
                        };
                        if !guard() {
                            return Ok(None);
                        }
                        let count =
                            apply_guarded(ws, &origin, manager, &edit, guard.clone()).await?;
                        if count > 0 || guard() {
                            let mut ws = ws;
                            ws.set_status(t!(ws, L, "editor-applied", n = count));
                        }
                        Ok(None)
                    }
                    Task::Reveal(location) => {
                        let relative = relative_file(root, &location.uri)
                            .ok_or_else(|| t!(ws, L, "editor-reference-outside"))?;
                        let target = resolve_target(ws, &origin, &relative).await?;
                        if guard() {
                            ws.reveal_guarded(target, location.line, location.col, guard.clone())
                                .await
                                .map_err(|error| error.to_string())?;
                        }
                        Ok(None)
                    }
                }
            }
            .await;
            if operation.peek().as_ref() != Some(&pending) {
                return;
            }
            match outcome {
                Ok(Some(items)) if guard() => {
                    selected.set(
                        (0..items.length())
                            .find(|index| items.enabled(*index, root))
                            .unwrap_or(0),
                    );
                    let mut ws = ws;
                    ws.set_status(match &items {
                        List::Actions(items) if items.is_empty() => t!(ws, L, "editor-no-actions"),
                        List::Actions(_) => t!(ws, L, "editor-actions"),
                        List::References(items) => t!(ws, L, "editor-references", n = items.len()),
                    });
                    list.set(Some(items));
                }
                other => {
                    if let Err(error) = other {
                        if guard() {
                            let mut ws = ws;
                            ws.set_status(t!(ws, L, "editor-edit-failed", error = error));
                        }
                    }
                    operation.set(None);
                    list.set(None);
                    if *ws.docs.active.peek() == Some(node) {
                        focus.with_mut(|focus| *focus += 1);
                    }
                }
            }
        }
    });
    Tools {
        operation,
        list,
        selected,
        actions: Callback::new(move |_| start.call(false)),
        references: Callback::new(move |_| start.call(true)),
        cancel,
        dismiss,
        choose,
    }
}

type RequestPositions = ((u32, u32), (u32, u32), (u32, u32));

fn positions(text: &str, selection: Utf16Selection) -> Option<RequestPositions> {
    Some((
        utf16_offset_to_line_col(text, selection.anchor.min(selection.head) as usize)?,
        utf16_offset_to_line_col(text, selection.anchor.max(selection.head) as usize)?,
        utf16_offset_to_line_col(text, selection.head as usize)?,
    ))
}

#[component]
pub(crate) fn ToolsMenu(ws: Workspace, node: NodeId, mut tools: Tools) -> Element {
    let root = ws
        .document(node)
        .map(|doc| {
            doc.peek()
                .node
                .source
                .as_str()
                .strip_prefix("folder:")
                .unwrap_or_default()
                .to_string()
        })
        .unwrap_or_default();
    let nodes = use_hook(|| Rc::new(RefCell::new(HashMap::<usize, Rc<MountedData>>::new())));
    let focus_nodes = nodes.clone();
    use_effect(move || {
        let active = tools.operation.read().is_some();
        let selected = (tools.selected)();
        let ready = tools.list.read().is_some();
        if !active || !ready {
            focus_nodes.borrow_mut().clear();
            return;
        }
        if let Some(element) = focus_nodes.borrow().get(&selected).cloned() {
            spawn(async move {
                let _ = element.set_focus(true).await;
                let _ = element
                    .scroll_to_with_options(ScrollToOptions {
                        behavior: ScrollBehavior::Instant,
                        vertical: ScrollLogicalPosition::Nearest,
                        horizontal: ScrollLogicalPosition::Nearest,
                    })
                    .await;
            });
        }
    });
    let Some(operation) = (tools.operation)() else {
        return rsx! {};
    };
    let list = (tools.list)();
    let references = matches!(operation.task, Task::References | Task::Reveal(_));
    let label = if references {
        t!(ws, L, "editor-references-shortcut")
    } else {
        t!(ws, L, "editor-actions-shortcut")
    };
    let rows: Vec<_> = match &list {
        Some(List::Actions(items)) => items
            .iter()
            .map(|item| {
                (
                    item.title.clone(),
                    item.disabled.clone().unwrap_or_else(|| item.kind.clone()),
                    item.disabled.is_some(),
                )
            })
            .collect(),
        Some(List::References(items)) => items
            .iter()
            .map(|item| {
                let relative = relative_file(&root, &item.uri);
                (
                    format!(
                        "{}:{}:{}",
                        relative.as_deref().unwrap_or(&item.uri),
                        u64::from(item.line) + 1,
                        u64::from(item.col) + 1
                    ),
                    item.uri.clone(),
                    relative.is_none(),
                )
            })
            .collect(),
        None => Vec::new(),
    };
    let enabled: Vec<_> = rows
        .iter()
        .enumerate()
        .filter_map(|(index, (_, _, disabled))| (!disabled).then_some(index))
        .collect();
    let row_count = rows.len();
    let serial = operation.request.serial;
    rsx! {
        div { class: if references {"mk-editor-bar mk-native-tools mk-native-references"} else {"mk-editor-bar mk-native-tools mk-native-actions"}, role:"region", aria_label:"{label}", aria_busy: "{list.is_none()}", "data-count":"{row_count}",
            onkeydown: move |event| {
                event.stop_propagation();
                match event.key() {
                    Key::Escape => { event.prevent_default(); tools.cancel.call(()); }
                    Key::ArrowDown | Key::ArrowUp | Key::Home | Key::End if !enabled.is_empty() => {
                        event.prevent_default();
                        let index = enabled.iter().position(|index| *index == (tools.selected)()).unwrap_or(0);
                        let index = match event.key() { Key::Home=>0,Key::End=>enabled.len()-1,Key::ArrowUp=>(index+enabled.len()-1)%enabled.len(),_=>(index+1)%enabled.len() };
                        tools.selected.set(enabled[index]);
                    }
                    _ => {}
                }
            },
            div { class:"mk-native-tools-header", span { "{label}" } button { class:"mk-btn", onclick:move |_| tools.cancel.call(()), {t!(ws,L,"editor-cancel")} } }
            if list.is_none() { span { {t!(ws,L,"editor-tools-loading")} } }
            else if row_count == 0 { span { if references { {t!(ws,L,"editor-references",n=0)} } else { {t!(ws,L,"editor-no-actions")} } } }
            for (index,(label,detail,disabled)) in rows.into_iter().enumerate() {
                button { key:"{serial}-{index}", class:if index==(tools.selected)() {"mk-native-tool-item mk-native-tool-selected"} else {"mk-native-tool-item"}, title:"{detail}", disabled, tabindex:if index==(tools.selected)() {"0"} else {"-1"},
                    onmounted:{let nodes=nodes.clone(); move |event| { let element=event.data(); nodes.borrow_mut().insert(index,element.clone()); if index==(tools.selected)() && !disabled { spawn(async move {let _=element.set_focus(true).await;});} }},
                    onclick:move |_| tools.choose.call((serial,index)), "{label}"
                    if disabled { span { class:"mk-muted", " · {detail}" } }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reversed_unicode_crlf_selection_sends_sorted_range_and_head_caret() {
        let (start, end, caret) = positions(
            "😀old\r\nnext\r\n",
            Utf16Selection {
                anchor: 11,
                head: 2,
            },
        )
        .unwrap();
        assert_eq!(start, (0, 2));
        assert_eq!(end, (1, 4));
        assert_eq!(caret, (0, 2));
        assert!(positions(
            "x",
            Utf16Selection {
                anchor: 99,
                head: 0
            }
        )
        .is_none());
    }
}
