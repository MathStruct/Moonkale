//! Explicit graph-side structural proposals; renderer dragging remains layout-only.
use crate::L;
use dioxus::prelude::*;
use moonkale_code_view::graph_rename::{self, RenameError, RenameGuard};
use moonkale_core::{Direction, EdgeKind, Node, NodeKind, Query, SourceId, Value};
use moonkale_ext_api::{
    editor::TextChange,
    structural::{FunctionHandle, Operation, PlanError, PlanRequest, StructuralPlanner},
    t,
    workspace::grouped_edits::{self, FileSaveResult, FileSaveStatus, MultiDocumentEditProposal},
    workspace::proposals::{DocumentEditBase, DocumentEditProposal, ProposalConflict},
    Command, Workspace,
};
use std::rc::Rc;

#[derive(Clone, Default)]
struct Review {
    generation: u64,
    open: bool,
    busy: bool,
    file: Option<Node>,
    base: Option<Rc<DocumentEditBase>>,
    functions: Rc<Vec<FunctionHandle>>,
    target: Option<usize>,
    before: Option<usize>,
    moving: bool,
    renaming: bool,
    rename_error: Option<RenameError>,
    rename_guard: Option<Rc<RenameGuard>>,
    name: String,
    proposal: Option<ReviewProposal>,
    multi: Option<Rc<MultiDocumentEditProposal>>,
    group: Option<u64>,
    group_error: bool,
    saves: Vec<FileSaveResult>,
    conflict: bool,
    error: Option<PlanError>,
    applied: bool,
}
#[derive(Clone)]
enum ReviewProposal {
    Single(Rc<DocumentEditProposal>),
    Multi(Rc<MultiDocumentEditProposal>, usize),
}
impl ReviewProposal {
    fn doc(&self) -> &DocumentEditProposal {
        match self {
            Self::Single(doc) => doc,
            Self::Multi(multi, index) => &multi.files()[*index],
        }
    }
}
impl PartialEq for ReviewProposal {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Single(a), Self::Single(b)) => Rc::ptr_eq(a, b),
            (Self::Multi(a, i), Self::Multi(b, j)) => i == j && Rc::ptr_eq(a, b),
            _ => false,
        }
    }
}

#[derive(Clone, Copy)]
struct ReviewStore(Signal<Review>);

fn store(ws: Workspace) -> Signal<Review> {
    ws.shared_state(|| ReviewStore(Signal::new_in_scope(Review::default(), ScopeId::ROOT)))
        .0
}
fn current(state: Signal<Review>, generation: u64) -> bool {
    let state = state.peek();
    state.open && state.generation == generation
}
fn error(mut state: Signal<Review>, generation: u64, error: PlanError) {
    if current(state, generation) {
        state.with_mut(|state| {
            state.busy = false;
            state.error = Some(error);
        });
    }
}
fn proposal_error(error: ProposalConflict) -> PlanError {
    match error {
        ProposalConflict::UnsupportedSource => PlanError::ReadOnly,
        ProposalConflict::Closed | ProposalConflict::Changed => PlanError::Stale,
        ProposalConflict::LimitExceeded => PlanError::Limit,
        _ => PlanError::Invalid,
    }
}
fn number(node: &Node, key: &str) -> Option<usize> {
    match node.props.get(key) {
        Some(Value::Int(value)) => usize::try_from(*value).ok(),
        _ => None,
    }
}

async fn begin(ws: Workspace, node: Node, mut state: Signal<Review>) {
    let generation = state.peek().generation.wrapping_add(1);
    state.set(Review {
        generation,
        open: true,
        busy: true,
        ..Default::default()
    });
    let Some(planner) = ws.service::<StructuralPlanner>() else {
        error(state, generation, PlanError::Unavailable);
        return;
    };
    let source = ws.source(&node.source);
    let Some(source) = source else {
        error(state, generation, PlanError::Unavailable);
        return;
    };
    let result = source
        .query(Query::Neighbours {
            node: node.id,
            depth: 1,
            direction: Direction::In,
        })
        .await;
    if !current(state, generation) {
        return;
    }
    let Ok(result) = result else {
        error(state, generation, PlanError::Unavailable);
        return;
    };
    let edge = result
        .edges
        .iter()
        .find(|edge| edge.to == node.id && edge.kind == EdgeKind::Defines);
    let file = edge
        .and_then(|edge| {
            result
                .nodes
                .iter()
                .find(|file| file.id == edge.from && file.kind == NodeKind::File)
        })
        .cloned();
    let Some(file) = file else {
        error(state, generation, PlanError::Invalid);
        return;
    };
    let loaded = ws
        .load_node_guarded(file.clone(), || current(state, generation))
        .await;
    if !current(state, generation) {
        return;
    }
    if !matches!(loaded, Ok(true)) {
        error(state, generation, PlanError::Unavailable);
        return;
    }
    // Reconcile the newly attached source tab before keeping the requested review visible.
    let mut ws = ws;
    ws.dispatch(Command::ShowPanel("graph"));
    let base = match ws.document_edit_base(file.id) {
        Ok(base) => base,
        Err(e) => {
            error(state, generation, proposal_error(e));
            return;
        }
    };
    state.with_mut(|state| {
        state.file = Some(file.clone());
        state.base = Some(Rc::new(base.clone()));
    });
    let text = base.snapshot().text.clone();
    let response = (planner.0)(PlanRequest {
        text: text.clone(),
        operation: Operation::List,
    })
    .await;
    if !current(state, generation) {
        return;
    }
    if let Err(e) = base.validate(ws) {
        error(state, generation, proposal_error(e));
        return;
    }
    let response = match response {
        Ok(response) => response,
        Err(e) => {
            error(state, generation, e);
            return;
        }
    };
    // Old graph ranges may only preselect when the exact input provenance matches.
    let input = moonkale_core::NodeId::derive(&SourceId::new("index-input-v1"), &text).to_string();
    let matches_input = node.props.get("input") == Some(&Value::Text(input));
    let target = if matches_input {
        edge.and_then(|edge| {
            let proxy = Node {
                props: edge.props.clone(),
                ..node.clone()
            };
            let start = number(&proxy, "start_byte")?;
            let end = number(&proxy, "end_byte")?;
            response
                .functions
                .iter()
                .position(|function| function.start == start && function.end == end)
        })
    } else {
        None
    };
    let name = target
        .map(|index| response.functions[index].name.clone())
        .unwrap_or_default();
    state.with_mut(|state| {
        state.file = Some(file);
        state.base = Some(Rc::new(base));
        state.functions = Rc::new(response.functions);
        state.target = target;
        state.name = name;
        state.busy = false;
    });
}

async fn recompute(ws: Workspace, mut state: Signal<Review>) {
    let Some(file) = state.peek().file.clone() else {
        return;
    };
    let generation = state.peek().generation.wrapping_add(1);
    state.with_mut(|state| {
        state.generation = generation;
        state.busy = true;
        state.error = None;
        state.rename_error = None;
        state.rename_guard = None;
    });
    let Some(planner) = ws.service::<StructuralPlanner>() else {
        error(state, generation, PlanError::Unavailable);
        return;
    };
    let base = match ws.document_edit_base(file.id) {
        Ok(base) => base,
        Err(e) => {
            error(state, generation, proposal_error(e));
            return;
        }
    };
    let response = (planner.0)(PlanRequest {
        text: base.snapshot().text.clone(),
        operation: Operation::List,
    })
    .await;
    if !current(state, generation) {
        return;
    }
    if let Err(e) = base.validate(ws) {
        error(state, generation, proposal_error(e));
        return;
    }
    match response {
        Ok(response) => state.with_mut(|state| {
            state.base = Some(Rc::new(base));
            state.functions = Rc::new(response.functions);
            state.target = None;
            state.before = None;
            state.proposal = None;
            state.multi = None;
            state.group = None;
            state.saves.clear();
            state.group_error = false;
            state.conflict = false;
            state.applied = false;
            state.busy = false;
        }),
        Err(e) => error(state, generation, e),
    }
}

async fn preview(ws: Workspace, mut state: Signal<Review>) {
    let (Some(base), Some(target)) = (state.peek().base.clone(), state.peek().target) else {
        return;
    };
    let operation = {
        let state = state.peek();
        let Some(target) = state.functions.get(target).cloned() else {
            return;
        };
        if state.moving {
            Operation::Move {
                target,
                before: state
                    .before
                    .and_then(|index| state.functions.get(index).cloned()),
            }
        } else {
            Operation::DeclarationName {
                target,
                name: state.name.clone(),
            }
        }
    };
    let renaming = state.peek().renaming;
    let file = state.peek().file.clone();
    let generation = state.peek().generation.wrapping_add(1);
    state.with_mut(|state| {
        state.generation = generation;
        state.busy = true;
        state.error = None;
        state.rename_error = None;
        state.rename_guard = None;
        state.proposal = None;
        state.multi = None;
        state.group = None;
        state.saves.clear();
        state.group_error = false;
    });
    let Some(planner) = ws.service::<StructuralPlanner>() else {
        error(state, generation, PlanError::Unavailable);
        return;
    };
    let response = (planner.0)(PlanRequest {
        text: base.snapshot().text.clone(),
        operation,
    })
    .await;
    if !current(state, generation) {
        return;
    }
    if let Err(e) = base.validate(ws) {
        error(state, generation, proposal_error(e));
        return;
    }
    match response {
        Ok(response) => {
            if renaming {
                let name = state.peek().name.clone();
                let result = match (file, response.edits.first()) {
                    (Some(file), Some(edit)) => {
                        graph_rename::propose_multi_guarded(
                            ws,
                            &file,
                            &base,
                            edit.start,
                            &name,
                            || current(state, generation),
                        )
                        .await
                    }
                    _ => Err(RenameError::Invalid),
                };
                if !current(state, generation) {
                    return;
                }
                if let Err(e) = base.validate(ws) {
                    error(state, generation, proposal_error(e));
                    return;
                }
                state.with_mut(|state| {
                    state.busy = false;
                    match result {
                        Ok((proposal, guard)) => {
                            state.multi = Some(Rc::new(proposal));
                            state.rename_guard = Some(Rc::new(guard));
                        }
                        Err(e) => state.rename_error = Some(e),
                    }
                });
                return;
            }
            let changes = response
                .edits
                .into_iter()
                .map(|edit| TextChange {
                    range: edit.start..edit.end,
                    removed_text: edit.removed,
                    inserted_text: edit.inserted,
                })
                .collect();
            match base.propose(changes) {
                Ok(proposal) => state.with_mut(|state| {
                    state.proposal = Some(ReviewProposal::Single(Rc::new(proposal)));
                    state.busy = false;
                }),
                Err(_) => error(state, generation, PlanError::Invalid),
            }
        }
        Err(e) => error(state, generation, e),
    }
}

/// Experimental controls shown only when a planner service is installed.
#[component]
pub(crate) fn StructuralControls(ws: Workspace, selected: Option<Node>) -> Element {
    let mut state = store(ws);
    // Changing graph selection invalidates in-flight work but retains a completed review.
    let selection_key = selected.as_ref().map(|node| node.id);
    use_effect(use_reactive!(|(selection_key)| {
        let _ = selection_key;
        state.with_mut(|state| {
            state.generation = state.generation.wrapping_add(1);
            state.busy = false;
        });
    }));
    let available = ws.service::<StructuralPlanner>().is_some();
    let eligible = selected.as_ref().is_some_and(|node| {
        node.kind.is_definition()
            && node.props.get("lang") == Some(&Value::Text("rust".into()))
            && node.props.get("def_kind") == Some(&Value::Text("function".into()))
    });
    if !available {
        return rsx! {};
    }
    let review = state.read().clone();
    let live = review.file.as_ref().and_then(|file| ws.document(file.id));
    // Read both canonical text and revision so an edit/undo ABA also disables Apply.
    let live_text = live.map(|doc| doc.read().text.clone());
    let session = review
        .file
        .as_ref()
        .and_then(|file| ws.editor_session(file.id));
    let drift = review.base.as_ref().is_some_and(|base| {
        live_text.as_deref() != Some(base.snapshot().text.as_str())
            || session.is_none_or(|session| {
                session.read().snapshot().revision != base.snapshot().revision
            })
    });
    let resolver_drift = review
        .rename_guard
        .as_ref()
        .is_some_and(|guard| guard.validate(ws).is_err());
    let group_applied = review
        .group
        .and_then(|id| grouped_edits::is_applied(ws, id));
    let multi_drift = review
        .multi
        .as_ref()
        .is_some_and(|multi| multi.validate(ws).is_err());
    let conflict = review.conflict || (!review.applied && (drift || resolver_drift || multi_drift));
    rsx! {
        div { class: "mk-structural", "data-testid": "structural-controls", "data-selected-node": selected.as_ref().map(|node| node.id.to_string()).unwrap_or_default(),
            button { class: "mk-btn", disabled: !eligible || review.busy,
                onclick: move |_| { if let Some(node) = selected.clone() { spawn(begin(ws, node, state)); } },
                {t!(ws, L, "structural-edit")}
            }
            if review.open {
                section { class: "mk-structural-review", "aria-label": t!(ws, L, "structural-edit"),
                    if let Some(file) = &review.file { h3 { "{file.native_key}" } }
                    p { {t!(ws, L, "structural-warning")} }
                    if review.busy { p { role: "status", {t!(ws, L, "structural-busy")} } }
                    if let Some(error) = review.error {
                        p { role: "alert", {match error {
                            PlanError::Busy => t!(ws, L, "structural-workers-busy"),
                            PlanError::ReadOnly => t!(ws, L, "structural-read-only"),
                            PlanError::Unavailable => t!(ws, L, "structural-unavailable"),
                            PlanError::Limit => t!(ws, L, "structural-limit"),
                            PlanError::Syntax => t!(ws, L, "structural-syntax"),
                            PlanError::Stale => t!(ws, L, "structural-conflict"),
                            PlanError::Invalid => t!(ws, L, "structural-invalid"),
                        }} }
                    }
                    if let Some(error) = review.rename_error {
                        p { role: "alert", "data-testid": "structural-rename-error", {match error {
                            RenameError::MultiFile => t!(ws, L, "structural-rename-multi-file"),
                            RenameError::Unavailable => t!(ws, L, "structural-rename-unavailable"),
                            RenameError::Timeout => t!(ws, L, "structural-rename-timeout"),
                            RenameError::Stale => t!(ws, L, "structural-conflict"),
                            RenameError::Invalid => t!(ws, L, "structural-rename-invalid"),
                            RenameError::Limit => t!(ws, L, "structural-limit"),
                            RenameError::TargetsChanged => t!(ws, L, "structural-targets-changed"),
                        }} }
                    }
                    label { {t!(ws, L, "structural-function")}
                        select { "data-testid": "structural-function", value: review.target.map(|index| index.to_string()).unwrap_or_default(), disabled: review.busy,
                            onchange: move |event| { let target = event.value().parse::<usize>().ok(); state.with_mut(|state| {
                                state.target = target; if state.before == target { state.before = None; } state.name = target.and_then(|index| state.functions.get(index)).map(|function| function.name.clone()).unwrap_or_default();
                                state.proposal = None; state.multi = None; state.group = None; state.saves.clear(); state.group_error = false; state.rename_guard = None; state.rename_error = None; state.applied = false;
                            }); },
                            option { value: "", {t!(ws, L, "structural-choose")} }
                            for (index, function) in review.functions.iter().enumerate() {
                                option { value: "{index}", "{function.name} · {function.line}" }
                            }
                        }
                    }
                    label { {t!(ws, L, "structural-operation")}
                        select { "data-testid": "structural-operation", value: if review.moving { "move" } else if review.renaming { "rename" } else { "name" }, disabled: review.busy,
                            onchange: move |event| state.with_mut(|state| { state.moving = event.value() == "move"; state.renaming = event.value() == "rename"; state.rename_error = None; state.proposal = None; state.multi = None; state.group = None; state.saves.clear(); state.group_error = false; state.rename_guard = None; state.rename_error = None; state.applied = false; }),
                            option { value: "name", {t!(ws, L, "structural-name")} }
                            option { value: "move", {t!(ws, L, "structural-move")} }
                            option { value: "rename", {t!(ws, L, "structural-rename")} }
                        }
                    }
                    if review.moving {
                        label { {t!(ws, L, "structural-before")}
                            select { "data-testid": "structural-before", value: review.before.map(|index| index.to_string()).unwrap_or_default(), disabled: review.busy,
                                onchange: move |event| state.with_mut(|state| { state.before = event.value().parse().ok(); state.proposal = None; state.multi = None; state.group = None; state.saves.clear(); state.group_error = false; state.rename_guard = None; state.rename_error = None; state.applied = false; }),
                                option { value: "", {t!(ws, L, "structural-end")} }
                                for (index, function) in review.functions.iter().enumerate() {
                                    if Some(index) != review.target { option { value: "{index}", "{function.name} · {function.line}" } }
                                }
                            }
                        }
                    } else {
                        label { {if review.renaming { t!(ws, L, "structural-rename") } else { t!(ws, L, "structural-name") }}
                            input { "data-testid": "structural-name", value: "{review.name}", disabled: review.busy,
                                oninput: move |event| state.with_mut(|state| { state.name = event.value(); state.proposal = None; state.multi = None; state.group = None; state.saves.clear(); state.group_error = false; state.rename_guard = None; state.rename_error = None; state.applied = false; }) }
                        }
                    }
                    if conflict { p { role: "alert", {t!(ws, L, "structural-conflict")} } }
                    if review.applied { p { role: "status", {t!(ws, L, "structural-applied")} } }
                    button { class: "mk-btn", "data-testid": "structural-preview", disabled: review.busy || review.target.is_none() || conflict || review.applied,
                        onclick: move |_| { spawn(preview(ws, state)); }, {t!(ws, L, "structural-preview")} }
                    button { class: "mk-btn", "data-testid": "structural-apply", disabled: review.busy || (review.proposal.is_none() && review.multi.is_none()) || conflict || review.applied,
                        onclick: move |_| {
                            let multi = state.peek().multi.clone();
                            if let Some(multi) = multi {
                                let guard = state.peek().rename_guard.clone();
                                let generation = state.peek().generation;
                                state.with_mut(|state| { state.busy = true; state.group_error = false; });
                                dioxus::core::spawn_forever(async move {
                                    let result = multi.apply_guarded(ws, || current(state, generation) && guard.as_ref().is_none_or(|guard| guard.validate(ws).is_ok())).await;
                                    if !state.peek().open || !state.peek().multi.as_ref().is_some_and(|live| Rc::ptr_eq(live, &multi)) { return; }
                                    state.with_mut(|state| { state.busy = false; match result {
                                        Ok(group) => { state.group = Some(group); state.applied = true; state.conflict = false; },
                                        Err(_) => { state.conflict = true; state.group_error = true; },
                                    }});
                                });
                                return;
                            }
                            let proposal = state.peek().proposal.clone();
                            if let Some(proposal) = proposal {
                                let guard = state.peek().rename_guard.clone();
                                let result = guard.as_ref().map(|guard| guard.validate(ws)).transpose()
                                    .map_err(|_| ProposalConflict::Changed).and_then(|_| proposal.doc().apply(ws));
                                match result {
                                    Ok(_) => state.with_mut(|state| { state.applied = true; state.conflict = false; }),
                                    Err(e) => state.with_mut(|state| { state.conflict = true; state.error = Some(proposal_error(e)); }),
                                }
                            }
                        }, {t!(ws, L, "structural-apply")} }
                    if review.group_error { p { role: "alert", "data-testid": "structural-group-error", {t!(ws, L, "structural-group-blocked")} } }
                    if let Some(group) = review.group {
                        button { class: "mk-btn", "data-testid": "structural-undo", disabled: review.busy || group_applied != Some(true),
                            onclick: move |_| { state.with_mut(|s| { s.busy=true; s.group_error=false; }); dioxus::core::spawn_forever(async move {
                                let result = grouped_edits::restore(ws,group,false).await;
                                if state.peek().open && state.peek().group == Some(group) { state.with_mut(|s| {s.busy=false;s.group_error=result.is_err();}); }
                            }); }, {t!(ws,L,"structural-undo")} }
                        button { class: "mk-btn", "data-testid": "structural-redo", disabled: review.busy || group_applied != Some(false),
                            onclick: move |_| { state.with_mut(|s| { s.busy=true; s.group_error=false; }); dioxus::core::spawn_forever(async move {
                                let result = grouped_edits::restore(ws,group,true).await;
                                if state.peek().open && state.peek().group == Some(group) { state.with_mut(|s| {s.busy=false;s.group_error=result.is_err();}); }
                            }); }, {t!(ws,L,"structural-redo")} }
                        button { class: "mk-btn", "data-testid": "structural-save-all", disabled: review.busy || group_applied != Some(true),
                            onclick: move |_| { state.with_mut(|s| s.busy=true); dioxus::core::spawn_forever(async move {
                                let results = grouped_edits::save(ws,group).await;
                                if state.peek().open && state.peek().group == Some(group) { state.with_mut(|s| {s.busy=false;s.saves=results;}); }
                            }); }, {t!(ws,L,"structural-save-all")} }
                    }
                    for result in &review.saves {
                        p { "data-testid": "structural-save-result", "{ws.document(result.node).map(|doc| doc.read().node.native_key.clone()).unwrap_or_else(||result.node.to_string())}: ",
                            {match result.status {
                                FileSaveStatus::Saved => t!(ws,L,"structural-saved"),
                                FileSaveStatus::AlreadySaved => t!(ws,L,"structural-already-saved"),
                                FileSaveStatus::Changed => t!(ws,L,"structural-save-changed"),
                                FileSaveStatus::Closed => t!(ws,L,"structural-save-closed"),
                                FileSaveStatus::ReadOnly => t!(ws,L,"structural-save-read-only"),
                                FileSaveStatus::Conflict => t!(ws,L,"structural-save-conflict"),
                                FileSaveStatus::Failed => t!(ws,L,"structural-save-failed"),
                            }}
                        }
                    }
                    button { class: "mk-btn", "data-testid": "structural-recompute", disabled: review.busy || review.file.is_none(),
                        onclick: move |_| { spawn(recompute(ws, state)); }, {t!(ws, L, "structural-recompute")} }
                    button { class: "mk-btn", "data-testid": "structural-discard",
                        onclick: move |_| { let generation = state.peek().generation.wrapping_add(1); state.set(Review { generation, ..Default::default() }); }, {t!(ws, L, "structural-discard")} }
                    if let Some(multi) = &review.multi {
                        for (index, file) in multi.files().iter().enumerate() {
                            section { key: "{file.base().node}", "data-testid": "structural-file", "data-node": "{file.base().node}",
                                h4 { "{ws.document(file.base().node).map(|doc|doc.read().node.native_key.clone()).unwrap_or_else(||file.base().node.to_string())}" }
                                if !review.applied {
                                    if let Err(error) = file.validate(ws) {
                                        p { role: "alert", "data-testid": "structural-file-conflict", {match error {
                                            ProposalConflict::Closed => t!(ws,L,"structural-file-closed"),
                                            ProposalConflict::UnsupportedSource => t!(ws,L,"structural-file-read-only"),
                                            _ => t!(ws,L,"structural-file-changed"),
                                        }} }
                                    }
                                }
                                div { class: "mk-structural-columns",
                                    div { h4 { {t!(ws,L,"structural-original")} } SourcePreview { proposal: ReviewProposal::Multi(multi.clone(),index), proposed: false } }
                                    div { h4 { {t!(ws,L,"structural-proposed")} } SourcePreview { proposal: ReviewProposal::Multi(multi.clone(),index), proposed: true } }
                                    if conflict { div { h4 { {t!(ws,L,"structural-current")} } pre { "data-testid":"structural-current", "{ws.document(file.base().node).map(|doc|doc.read().text.clone()).unwrap_or_default()}" } } }
                                }
                            }
                        }
                    }
                    if let Some(proposal) = &review.proposal {
                        div { class: "mk-structural-columns",
                            div { h4 { {t!(ws, L, "structural-original")} } SourcePreview { proposal: proposal.clone(), proposed: false } }
                            div { h4 { {t!(ws, L, "structural-proposed")} } SourcePreview { proposal: proposal.clone(), proposed: true } }
                            if conflict { div { h4 { {t!(ws, L, "structural-current")} } pre { "data-testid": "structural-current", "{live_text.as_deref().unwrap_or_default()}" } } }
                        }
                    }
                }
            }
        }
    }
}

// Both previews use the exact validated byte ranges; source is never HTML.
fn preview_parts(text: &str, changes: &[TextChange], proposed: bool) -> Vec<(bool, String)> {
    let mut changes: Vec<_> = changes.iter().collect();
    changes.sort_by_key(|change| (change.range.start, change.range.end));
    let mut shift = 0isize;
    let mut cursor = 0;
    let mut parts = Vec::new();
    for change in changes {
        let range = if proposed {
            let start = change.range.start.checked_add_signed(shift).unwrap();
            shift += change.inserted_text.len() as isize - change.removed_text.len() as isize;
            start..start + change.inserted_text.len()
        } else {
            change.range.clone()
        };
        parts.push((false, text[cursor..range.start].to_string()));
        parts.push((true, text[range.clone()].to_string()));
        cursor = range.end;
    }
    parts.push((false, text[cursor..].to_string()));
    parts
}
#[component]
fn SourcePreview(proposal: ReviewProposal, proposed: bool) -> Element {
    let text = if proposed {
        proposal.doc().preview()
    } else {
        &proposal.doc().base().text
    };
    let parts = preview_parts(text, proposal.doc().changes(), proposed);
    rsx! { pre { "data-testid": if proposed { "structural-proposed" } else { "structural-original" },
        for (index, (changed, text)) in parts.into_iter().enumerate() {
            if changed { mark { key: "{index}", "{text}" } } else { span { key: "{index}", "{text}" } }
        }
    } }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn range_previews_preserve_unicode_crlf_and_separate_disjoint_edits() {
        let old = "😀old\r\nkeep\r\nold";
        let new = "😀新\r\nkeep\r\nlong";
        let changes = vec![
            TextChange {
                range: 4..7,
                removed_text: "old".into(),
                inserted_text: "新".into(),
            },
            TextChange {
                range: 15..18,
                removed_text: "old".into(),
                inserted_text: "long".into(),
            },
        ];
        for (text, proposed) in [(old, false), (new, true)] {
            let parts = preview_parts(text, &changes, proposed);
            assert_eq!(
                parts
                    .iter()
                    .map(|(_, text)| text.as_str())
                    .collect::<String>(),
                text
            );
            assert_eq!(parts.iter().filter(|(changed, _)| *changed).count(), 2);
            assert!(parts
                .iter()
                .any(|(changed, text)| !changed && text.contains("keep")));
        }
    }
}
