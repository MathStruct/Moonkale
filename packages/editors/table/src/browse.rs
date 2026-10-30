//! Bounded page retention and query controls; values never become SQL text.
use crate::{
    edit::{Editing, RowsContext},
    grid, L,
};
use dioxus::prelude::*;
use moonkale_core::{RowKey, RowPage, Table, TableFilter, TablePredicate, TableQuery, Value};
use moonkale_ext_api::{t, Workspace};
use std::rc::Rc;

pub const MAX_ROWS: usize = 4000;
pub const MAX_CACHE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_PIN_BYTES: usize = 4 * 1024 * 1024;

fn row_bytes(row: &moonkale_core::RowSnapshot) -> usize {
    serde_json::to_vec(row)
        .expect("row snapshot serialization")
        .len()
}

/// Check before a write. A source trigger can enlarge the returned row by at
/// most one bounded page (4 MiB), so pinned snapshots have an 8 MiB hard bound.
pub fn pin_fits(page: &RowPage, pinned: &[RowKey], candidate: &moonkale_core::RowSnapshot) -> bool {
    let others: usize = page
        .rows
        .iter()
        .filter(|r| r.key != candidate.key && pinned.contains(&r.key))
        .map(row_bytes)
        .sum();
    others.saturating_add(row_bytes(candidate)) <= MAX_PIN_BYTES
}

/// Drop old unedited snapshots in a single pass. Edited rows remain until
/// refresh; their independent budget leaves space for the next source page.
fn trim(
    rows: &mut Vec<moonkale_core::RowSnapshot>,
    pinned: &[RowKey],
    row_cap: usize,
    byte_cap: usize,
) -> usize {
    let sizes: Vec<_> = rows.iter().map(row_bytes).collect();
    let mut bytes: usize = sizes.iter().sum();
    let mut excess = rows.len().saturating_sub(row_cap);
    let mut i = 0;
    let mut removed = 0;
    rows.retain(|r| {
        let size = sizes[i];
        i += 1;
        if (excess > 0 || bytes > byte_cap) && !pinned.contains(&r.key) {
            excess = excess.saturating_sub(1);
            bytes = bytes.saturating_sub(size);
            removed += 1;
            false
        } else {
            true
        }
    });
    removed
}
pub fn table(page: &RowPage) -> Table {
    Table {
        columns: page.columns.clone(),
        rows: page
            .rows
            .iter()
            .map(|r| page.columns.iter().map(|c| r.values[c].clone()).collect())
            .collect(),
        truncated: page.truncated,
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct Browse {
    pub query: Signal<TableQuery>,
    pub busy: Signal<bool>,
    pub error: Signal<String>,
    pub generation: Signal<u64>,
    pub skipped: Signal<usize>,
    pub reposition: Signal<Option<f64>>,
}
impl Browse {
    pub fn reset(mut self, mut query: TableQuery) {
        query.after = None;
        self.query.set(query);
    }
    pub fn next(mut self, editing: Editing) {
        if *self.busy.peek() || *editing.busy.peek() || editing.active.peek().is_some() {
            return;
        }
        let next = editing.page.peek().next.clone();
        if let Some(next) = next {
            let mut query = self.query.peek().clone();
            query.after = Some(next);
            self.query.set(query);
        }
    }
}

pub async fn load(
    ws: Workspace,
    context: RowsContext,
    mut editing: Editing,
    mut browse: Browse,
    query: TableQuery,
    generation: u64,
) {
    let outer = ws.saved_sessions.active.peek().clone();
    let result = context
        .handle
        .query_rows(context.table.clone(), query.clone())
        .await;
    if *browse.generation.peek() != generation {
        return;
    }
    if *ws.saved_sessions.active.peek() != outer
        || !ws
            .source(&context.source)
            .is_some_and(|s| std::sync::Arc::ptr_eq(&s, &context.handle))
    {
        browse.busy.set(false);
        browse.error.set(t!(ws, L, "table-edit-source-changed"));
        return;
    }
    browse.busy.set(false);
    match result {
        Ok(mut next) if next.refusal.is_none() => {
            // Old snapshots cannot be merged with a different column layout.
            // Refresh remounts the grid with fresh metadata and cell contexts.
            if next.columns != context.page.columns {
                browse.error.set(t!(ws, L, "table-schema-changed"));
                return;
            }
            if query.after.is_some() {
                let old = editing.page.peek().clone();
                let mut rows = old.rows.clone();
                let mut seen: std::collections::HashSet<_> = rows
                    .iter()
                    .map(|r| serde_json::to_string(&r.key).expect("row key serialization"))
                    .collect();
                for row in next.rows {
                    if seen.insert(serde_json::to_string(&row.key).expect("row key serialization"))
                    {
                        rows.push(row);
                    }
                }
                // Edited rows stay visible until explicit refresh, including rows
                // now outside the filter. Up to 1000 are pinned by the edit guard.
                let removed = trim(&mut rows, &editing.pinned.peek(), MAX_ROWS, MAX_CACHE_BYTES);
                let previous = *browse.skipped.peek();
                browse.skipped.set(previous + removed);
                if removed != 0 {
                    browse.reposition.set(Some(removed as f64 * grid::ROW_H));
                }
                editing.undo.with_mut(|undo| {
                    undo.retain_mut(|u| {
                        if let Some(i) = rows.iter().position(|r| r.key == u.after.key) {
                            u.row = i;
                            true
                        } else {
                            false
                        }
                    });
                });
                next.rows = rows;
            } else {
                browse.skipped.set(0);
                browse.reposition.set(Some(f64::INFINITY));
                editing.undo.set(Vec::new());
                editing.pinned.set(Vec::new());
            }
            editing.page.set(Rc::new(next));
        }
        Ok(page) => browse.error.set(page.refusal.unwrap_or_default()),
        Err(e) => browse.error.set(e.to_string()),
    }
}

pub fn sort(mut browse: Browse, column: String, secondary: bool) {
    let mut q = browse.query.peek().clone();
    let existing = q.sort.iter().find(|s| s.column == column).cloned();
    if !secondary {
        q.sort.retain(|s| s.column == column);
    }
    q.sort.retain(|s| s.column != column);
    if let Some(s) = existing {
        if !s.descending {
            q.sort.push(moonkale_core::TableSort {
                column,
                descending: true,
            });
        }
    } else {
        if q.sort.len() >= 8 {
            q.sort.remove(0);
        }
        q.sort.push(moonkale_core::TableSort {
            column,
            descending: false,
        });
    }
    q.after = None;
    browse.query.set(q);
}

#[component]
pub fn FilterMenu(
    ws: Workspace,
    column: String,
    sample: Value,
    browse: Browse,
    disabled: bool,
) -> Element {
    let default_operator = if matches!(sample, Value::Text(_)) {
        "contains"
    } else {
        "eq"
    };
    let mut operator = use_signal(|| default_operator.to_string());
    let mut value = use_signal(String::new);
    let mut invalid = use_signal(|| false);
    let apply_column = column.clone();
    let apply = move |_| {
        let text = value.peek().clone();
        let p = match operator.peek().as_str() {
            "null" => Some(TablePredicate::IsNull),
            "contains" => Some(TablePredicate::Contains(text)),
            "prefix" => Some(TablePredicate::Prefix(text)),
            op => {
                let v = match &sample {
                    Value::Int(_) => text.parse().ok().map(Value::Int),
                    Value::Float(_) => text
                        .parse::<f64>()
                        .ok()
                        .filter(|n| n.is_finite())
                        .map(Value::Float),
                    Value::Bool(_) => text.parse().ok().map(Value::Bool),
                    Value::DateTime(_) => {
                        moonkale_core::DateTimeCell::parse(&text).map(Value::DateTime)
                    }
                    _ => Some(Value::Text(text)),
                };
                v.map(|v| match op {
                    "lt" => TablePredicate::Lt(v),
                    "le" => TablePredicate::Le(v),
                    "gt" => TablePredicate::Gt(v),
                    "ge" => TablePredicate::Ge(v),
                    _ => TablePredicate::Eq(v),
                })
            }
        };
        invalid.set(p.is_none());
        if let Some(predicate) = p {
            let mut q = browse.query.peek().clone();
            q.filters.retain(|f| f.column != apply_column);
            q.filters.push(TableFilter {
                column: apply_column.clone(),
                predicate,
            });
            browse.reset(q);
        }
    };
    rsx! {
        details { class: "mk-column-menu", onclick: move |e| e.stop_propagation(), onkeydown: move |e| { e.stop_propagation(); if e.key()==Key::Escape { let _=document::eval("const d=document.activeElement.closest('details');if(d){d.open=false;d.querySelector('summary').focus()}"); } },
            summary { title: t!(ws,L,"table-column-filter",column=column.clone()), "aria-label": t!(ws,L,"table-column-filter",column=column.clone()), "⋯" }
            div { class: "mk-column-popover",
                select { class: "mk-filter-op", "aria-label": t!(ws,L,"table-filter-operator"), value: "{operator}", disabled,
                    onchange: move |e| operator.set(e.value()),
                    for (id,label) in [("eq",t!(ws,L,"table-op-eq")),("lt",t!(ws,L,"table-op-lt")),("le",t!(ws,L,"table-op-le")),("gt",t!(ws,L,"table-op-gt")),("ge",t!(ws,L,"table-op-ge")),("contains",t!(ws,L,"table-op-contains")),("prefix",t!(ws,L,"table-op-prefix")),("null",t!(ws,L,"table-op-null"))] {
                        option { value: id, "{label}" }
                    }
                }
                input { class: "mk-filter-value", "aria-label": t!(ws,L,"table-filter-value"), value: "{value}", disabled, oninput: move |e| value.set(e.value()) }
                button { class: "mk-filter-apply", disabled, onclick: apply, {t!(ws,L,"table-filter-apply")} }
                button { class: "mk-filter-clear", disabled, onclick: move |_| { let mut q=browse.query.peek().clone(); q.filters.retain(|f| f.column!=column); browse.reset(q); }, {t!(ws,L,"table-filter-clear")} }
                if invalid() { span { role: "alert", {t!(ws,L,"table-edit-invalid")} } }
            }
        }
    }
}

pub fn outside_filter(page: &RowPage, query: &TableQuery, pinned: &[RowKey]) -> bool {
    page.rows.iter().any(|r| {
        pinned.contains(&r.key)
            && query.filters.iter().any(|f| {
                !grid::matches(
                    r.values.get(&f.column).unwrap_or(&Value::Null),
                    &f.predicate,
                )
            })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use moonkale_core::{RowSnapshot, Version};
    use std::collections::BTreeMap;
    fn row(id: i64, bytes: usize) -> RowSnapshot {
        RowSnapshot {
            key: BTreeMap::from([("id".into(), Value::Int(id))]),
            values: BTreeMap::from([("text".into(), Value::Text("x".repeat(bytes)))]),
            stored: Default::default(),
            version: Version(1),
        }
    }
    #[test]
    fn byte_eviction_retains_edited_rows_and_keeps_a_suffix() {
        let mut rows = vec![row(0, 1000), row(1, 1000), row(2, 1000), row(3, 1000)];
        let pinned = vec![rows[0].key.clone()];
        let two_rows = row_bytes(&rows[0]) + row_bytes(&rows[3]);
        assert_eq!(trim(&mut rows, &pinned, 4, two_rows), 2);
        assert_eq!(
            rows.iter().map(|r| r.key["id"].clone()).collect::<Vec<_>>(),
            vec![Value::Int(0), Value::Int(3)]
        );
        assert!(rows.iter().map(row_bytes).sum::<usize>() <= two_rows);
    }
    #[test]
    fn pinned_byte_budget_checks_new_rows_and_larger_edits() {
        let large = row(0, MAX_PIN_BYTES / 2 + 100);
        let other = row(1, MAX_PIN_BYTES / 2 + 100);
        let page = RowPage {
            rows: vec![large.clone()],
            ..Default::default()
        };
        assert!(!pin_fits(&page, std::slice::from_ref(&large.key), &other));
        assert!(pin_fits(
            &page,
            std::slice::from_ref(&large.key),
            &row(0, 10)
        ));
        assert!(!pin_fits(&page, &[], &row(2, MAX_PIN_BYTES)));
    }
}
