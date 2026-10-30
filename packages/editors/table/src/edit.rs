//! Direct, version-checked person edits. Drafts and conflicts stay at the cell;
//! undo is a new checked write, never a database snapshot rollback.
use crate::L;
use dioxus::prelude::*;
use moonkale_core::{RowBatch, RowChange, RowOp, RowPage, RowSnapshot, SourceId, Value};
use moonkale_ext_api::{t, Workspace};
use std::{collections::BTreeMap, rc::Rc};

#[derive(Clone)]
pub(crate) struct RowsContext {
    pub source: SourceId,
    pub handle: std::sync::Arc<dyn moonkale_core::Source>,
    pub table: String,
    pub page: Rc<RowPage>,
}
impl PartialEq for RowsContext {
    fn eq(&self, other: &Self) -> bool {
        self.source == other.source
            && std::sync::Arc::ptr_eq(&self.handle, &other.handle)
            && self.table == other.table
            && Rc::ptr_eq(&self.page, &other.page)
    }
}
#[derive(Clone)]
pub(crate) struct SharedRow(pub Rc<RowSnapshot>);
impl PartialEq for SharedRow {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}
#[derive(Clone)]
pub(crate) struct Undo {
    pub row: usize,
    pub column: String,
    pub before: Value,
    pub after: RowSnapshot,
}
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct Editing {
    pub page: Signal<Rc<RowPage>>,
    pub pinned: Signal<Vec<moonkale_core::RowKey>>,
    pub undo: Signal<Vec<Undo>>,
    pub start: Signal<Option<(usize, usize)>>,
    pub busy: Signal<bool>,
    pub active: Signal<Option<(usize, usize)>>,
}
#[derive(Clone)]
struct Draft {
    text: String,
    base: RowSnapshot,
    error: String,
    current: Option<RowSnapshot>,
    conflicted: bool,
    busy: bool,
}

struct BusyGuard(Signal<bool>);
impl Drop for BusyGuard {
    fn drop(&mut self) {
        if let Ok(mut busy) = self.0.try_write() {
            *busy = false;
        }
    }
}

fn value_bytes(value: &Value) -> usize {
    match value {
        Value::Text(s) => s.len(),
        Value::Vector(v) => v.as_slice().len().saturating_mul(8),
        Value::Ref { label, .. } => label.len(),
        _ => 32,
    }
}
fn undo_bytes(entry: &Undo) -> usize {
    value_bytes(&entry.before)
        + entry
            .after
            .key
            .iter()
            .map(|(key, value)| key.len() + value_bytes(value))
            .sum::<usize>()
}

fn parse(text: &str, original: &Value) -> Option<Value> {
    if text.len() > 512 * 1024 {
        return None;
    }
    Some(match original {
        Value::Int(_) => Value::Int(text.parse().ok()?),
        Value::Float(_) => {
            let v: f64 = text.parse().ok()?;
            if !v.is_finite() {
                return None;
            }
            Value::Float(v)
        }
        Value::Bool(_) => Value::Bool(text.parse().ok()?),
        Value::DateTime(_) => Value::DateTime(moonkale_core::DateTimeCell::parse(text)?),
        Value::Vector(_) => Value::Vector(moonkale_core::NumericVector::new(
            text.trim_matches(['[', ']'])
                .split(',')
                .filter(|s| !s.trim().is_empty())
                .map(|s| s.trim().parse())
                .collect::<Result<Vec<f64>, _>>()
                .ok()?,
        )?),
        Value::Null if text == "NULL" => Value::Null,
        Value::Null | Value::Text(_) => Value::Text(text.into()),
        _ => return None,
    })
}
fn op(table: &str, base: &RowSnapshot, column: &str, value: Value) -> RowOp {
    RowOp {
        table: table.into(),
        key: base.key.clone(),
        expected: Some(base.version),
        change: RowChange::Update(BTreeMap::from([(column.into(), value)])),
    }
}
fn valid(
    ws: Workspace,
    outer: &str,
    source: &SourceId,
    handle: &std::sync::Arc<dyn moonkale_core::Source>,
) -> bool {
    *ws.saved_sessions.active.peek() == outer
        && ws
            .source(source)
            .is_some_and(|s| std::sync::Arc::ptr_eq(&s, handle))
}

#[component]
pub(crate) fn Cell(
    ws: Workspace,
    context: RowsContext,
    snapshot: SharedRow,
    mut editing: Editing,
    blocked: bool,
    row: usize,
    column: usize,
    onfinish: EventHandler<()>,
    onmove: EventHandler<(usize, usize, crate::grid::Move)>,
    onreload: EventHandler<()>,
) -> Element {
    let name = context.page.columns[column].clone();
    let snapshot = snapshot.0;
    // Observe page updates in the parent once per row, not a full row clone
    // for every visible cell and every draft keystroke.
    let value = snapshot.values.get(&name).cloned().unwrap_or(Value::Null);
    let mut draft = use_signal(|| None::<Draft>);
    moonkale_ext_api::crash::use_effect(move || {
        let closed = draft.read().is_none();
        let active = *editing.active.peek();
        if closed && active == Some((row, column)) {
            editing.active.set(None);
        }
    });
    let mut begin = {
        let value = value.clone();
        let snapshot = snapshot.clone();
        move || {
            if !crate::browse::pin_fits(&editing.page.peek(), &editing.pinned.peek(), &snapshot) {
                ws.set_status(t!(ws, L, "table-edit-cap"));
                return;
            }
            if draft.peek().is_none()
                && !blocked
                && !*editing.busy.peek()
                && editing.pinned.peek().len() < 1000
            {
                editing.active.set(Some((row, column)));
                draft.set(Some(Draft {
                    text: match &value {
                        Value::DateTime(d) => d.source_text().unwrap_or_else(|| value.to_string()),
                        _ => value.to_string(),
                    },
                    base: snapshot.as_ref().clone(),
                    error: String::new(),
                    current: None,
                    conflicted: false,
                    busy: false,
                }));
            }
        }
    };
    moonkale_ext_api::crash::use_effect({
        let mut begin = begin.clone();
        move || {
            if *editing.start.read() == Some((row, column)) {
                begin();
                editing.start.set(None);
            }
            if *editing.active.read() != Some((row, column)) && draft.peek().is_some() {
                draft.set(None);
            }
        }
    });
    let save = {
        let name = name.clone();
        let context = context.clone();
        move |next: Option<crate::grid::Move>| {
            let Some(d) = draft.peek().clone() else {
                return;
            };
            if d.busy || d.conflicted || *editing.busy.peek() {
                return;
            }
            let original = d.base.values.get(&name).cloned().unwrap_or(Value::Null);
            let Some(value) = parse(&d.text, &original) else {
                draft.with_mut(|d| {
                    if let Some(d) = d {
                        d.error = t!(ws, L, "table-edit-invalid");
                    }
                });
                return;
            };
            let mut projected = d.base.clone();
            projected.values.insert(name.clone(), value.clone());
            projected.stored.remove(&name);
            if !crate::browse::pin_fits(&editing.page.peek(), &editing.pinned.peek(), &projected) {
                draft.with_mut(|d| {
                    if let Some(d) = d {
                        d.error = t!(ws, L, "table-edit-cap");
                    }
                });
                return;
            }
            let source = context.handle.clone();
            let outer = ws.saved_sessions.active.peek().clone();
            if !valid(ws, &outer, &context.source, &source) {
                draft.with_mut(|d| {
                    if let Some(d) = d {
                        d.error = t!(ws, L, "table-edit-source-changed");
                    }
                });
                return;
            }
            let context = context.clone();
            let name = name.clone();
            draft.with_mut(|d| {
                if let Some(d) = d {
                    d.busy = true;
                }
            });
            editing.busy.set(true);
            let guard = BusyGuard(editing.busy);
            moonkale_ext_api::crash::spawn(async move {
                let guard = guard;
                if !valid(ws, &outer, &context.source, &source) {
                    return;
                }
                let result = source
                    .apply_rows(RowBatch {
                        ops: vec![op(&context.table, &d.base, &name, value)],
                    })
                    .await;
                if !valid(ws, &outer, &context.source, &source) {
                    return;
                }
                match result {
                    Ok(rows) => {
                        drop(guard);
                        if let Some(Some(after)) = rows.into_iter().next() {
                            editing.undo.with_mut(|u| {
                                if u.len() >= 64 {
                                    u.remove(0);
                                }
                                u.push(Undo {
                                    row,
                                    column: name.clone(),
                                    before: d.base.stored.get(&name).cloned().unwrap_or(original),
                                    after: RowSnapshot {
                                        key: after.key.clone(),
                                        values: Default::default(),
                                        stored: Default::default(),
                                        version: after.version,
                                    },
                                });
                                while u.iter().map(undo_bytes).sum::<usize>() > 8 * 1024 * 1024 {
                                    u.remove(0);
                                }
                            });
                            editing.pinned.with_mut(|keys| {
                                if !keys.contains(&after.key) {
                                    keys.push(after.key.clone());
                                }
                            });
                            editing.page.with_mut(|p| Rc::make_mut(p).rows[row] = after);
                            draft.set(None);
                            if next.is_none() {
                                onfinish.call(());
                            }
                            if let Some(movement) = next {
                                onmove.call((row, column, movement));
                            }
                        }
                    }
                    Err(error) => {
                        let current =
                            if matches!(error, moonkale_core::SourceError::Conflict { .. }) {
                                source
                                    .query_rows(context.table.clone(), key_query(&d.base.key))
                                    .await
                                    .ok()
                                    .and_then(|p| p.rows.into_iter().find(|r| r.key == d.base.key))
                            } else {
                                None
                            };
                        if !valid(ws, &outer, &context.source, &source) {
                            return;
                        }
                        draft.with_mut(|d| {
                            if let Some(d) = d {
                                d.busy = false;
                                d.error = error.to_string();
                                d.conflicted =
                                    matches!(error, moonkale_core::SourceError::Conflict { .. });
                                d.current = current;
                            }
                        });
                    }
                }
            });
        }
    };
    let display = match &value {
        Value::DateTime(d) => d.display(&ws.lang()),
        _ => value.to_string(),
    };
    rsx! {
        if let Some(d) = draft() {
            div { class: "mk-cell-editor", onclick: |e| e.stop_propagation(), ondoubleclick: |e| e.stop_propagation(),
                input { class: "mk-cell-input", aria_label: t!(ws,L,"table-edit-value"), value: "{d.text}", disabled: d.busy, maxlength: 524288,
                    r#type: if matches!(value, Value::Int(_) | Value::Float(_)) { "number" } else { "text" }, step: "any",
                    onmounted: move |e| { moonkale_ext_api::crash::spawn(async move { let _ = e.data().set_focus(true).await; }); },
                    oninput: move |e| draft.with_mut(|d| { if let Some(d) = d { d.text=e.value(); } }),
                    onkeydown: { let mut save=save.clone(); move |e: KeyboardEvent| {
                        e.stop_propagation();
                        if e.is_composing() { return; }
                        match e.key() {
                            Key::Escape if !d.busy => { e.prevent_default(); draft.set(None); onfinish.call(()); }
                            Key::Enter => { e.prevent_default(); save(None); }
                            Key::Tab => { e.prevent_default(); save(Some(if e.modifiers().shift() { crate::grid::Move::Previous } else { crate::grid::Move::Next })); }
                            _ => {}
                        }
                    } },
                }
                button { class: "mk-cell-save", disabled: d.busy || d.conflicted, onclick: { let mut save=save.clone(); move |_| save(None) }, {t!(ws,L,"table-edit-save")} }
                button { disabled: d.busy, onclick: move |_| { draft.set(None); onfinish.call(()); }, {t!(ws,L,"table-edit-cancel")} }
                if !d.error.is_empty() { span { class: "mk-cell-refusal", role: "alert", "{d.error}" } }
                if d.conflicted {
                    { let current = d.current.clone(); rsx! {
                    div { class: "mk-cell-conflict", role: "alert",
                        p { {t!(ws,L,"table-edit-conflict")} }
                        pre { {t!(ws,L,"table-edit-base")} ": {d.base.values.get(&name).unwrap_or(&Value::Null)}" }
                        pre { {t!(ws,L,"table-edit-mine")} ": {d.text}" }
                        pre { {t!(ws,L,"table-edit-current")} ": " {current.as_ref().map(|r|r.values.get(&name).unwrap_or(&Value::Null).to_string()).unwrap_or_else(||t!(ws,L,"table-edit-deleted"))} }
                        button { class: "mk-cell-keep-mine", disabled: current.is_none(), onclick: { let current=current.clone(); move |_| {
                            draft.with_mut(|d| { if let Some(d) = d { if let Some(current)=&current { d.base=current.clone(); } d.current=None; d.conflicted=false; d.error.clear(); } });
                        } }, {t!(ws,L,"table-edit-keep-mine")} }
                        button { class: "mk-cell-keep-current", onclick: { let current=current.clone(); move |_| {
                            if let Some(current)=&current { editing.page.with_mut(|p| Rc::make_mut(p).rows[row]=current.clone()); } else { onreload.call(()); } draft.set(None); onfinish.call(());
                        } }, {t!(ws,L,"table-edit-keep-current")} }
                        button { class: "mk-cell-reopen", onclick: { let context=context.clone(); move |_| {
                            let context=context.clone(); let key=d.base.key.clone();
                            moonkale_ext_api::crash::spawn(async move {
                                let source=context.handle.clone();
                                {
                                    let outer=ws.saved_sessions.active.peek().clone();
                                    if !valid(ws,&outer,&context.source,&source) { return; }
                                    if let Ok(page)=source.query_rows(context.table.clone(),key_query(&key)).await {
                                        if !valid(ws,&outer,&context.source,&source) { return; }
                                        let current=page.rows.into_iter().find(|r|r.key==key);
                                        draft.with_mut(|d| { if let Some(d)=d { d.current=current; } });
                                    }
                                }
                            });
                        } }, {t!(ws,L,"table-edit-reopen")} }
                    }
                    } }
                }
            }
        } else {
            button { class: "mk-cell-display", title: t!(ws,L,"table-edit-start"), aria_label: "{display}", onclick: move |_| begin(),
                if let Value::Vector(v)=&value { svg { class: "mk-cell-vector", view_box: "0 0 100 20", width: "100", height: "20", "aria-label": "{display}", polyline { points: crate::grid::sparkline(v), fill: "none", stroke: "currentColor", stroke_width: "1.5" } } } else { "{display}" }
            }
        }
    }
}

pub(crate) async fn undo(
    ws: Workspace,
    context: RowsContext,
    mut editing: Editing,
) -> Result<(), String> {
    if *editing.busy.peek() {
        return Err("row undo is already running".into());
    }
    let entry = editing
        .undo
        .peek()
        .last()
        .cloned()
        .ok_or("no row edit to undo")?;
    let source = context.handle.clone();
    let outer = ws.saved_sessions.active.peek().clone();
    if !valid(ws, &outer, &context.source, &source) {
        return Err(t!(ws, L, "table-edit-source-changed"));
    }
    editing.busy.set(true);
    let result = source
        .apply_rows(RowBatch {
            ops: vec![op(
                &context.table,
                &entry.after,
                &entry.column,
                entry.before,
            )],
        })
        .await;
    editing.busy.set(false);
    if !valid(ws, &outer, &context.source, &source) {
        return Err("session or source changed during row undo".into());
    }
    let result = result.map_err(|e| e.to_string())?;
    if let Some(Some(snapshot)) = result.into_iter().next() {
        editing
            .page
            .with_mut(|p| Rc::make_mut(p).rows[entry.row] = snapshot);
        editing.undo.with_mut(|u| {
            u.pop();
        });
    }
    Ok(())
}

fn key_query(key: &moonkale_core::RowKey) -> moonkale_core::TableQuery {
    moonkale_core::TableQuery {
        filters: key
            .iter()
            .map(|(column, value)| moonkale_core::TableFilter {
                column: column.clone(),
                predicate: moonkale_core::TablePredicate::Eq(value.clone()),
            })
            .collect(),
        limit: 1,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn typed_drafts_preserve_text_and_reject_invalid_numbers() {
        assert_eq!(
            parse("NULL", &Value::Text(String::new())),
            Some(Value::Text("NULL".into()))
        );
        assert_eq!(parse("42", &Value::Int(0)), Some(Value::Int(42)));
        assert_eq!(parse("x", &Value::Int(0)), None);
        assert_eq!(parse("NaN", &Value::Float(0.)), None);
        assert!(matches!(
            parse(
                "2026-10-30",
                &Value::DateTime(moonkale_core::DateTimeCell::parse("2026-01-01").unwrap())
            ),
            Some(Value::DateTime(_))
        ));
    }
}
