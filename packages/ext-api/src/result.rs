//! Table result actions and row identities shared by independent views.
use dioxus::prelude::*;
use moonkale_core::{NodeId, Value};
use std::rc::Rc;

/// Host renderer for enabled extensions' result commands.
#[derive(Clone)]
pub struct ResultActions(pub Rc<dyn Fn(crate::Workspace, String) -> Element>);

/// Place inside a result's overflow menu. Empty in standalone hosts.
#[component]
pub fn ResultMenu(ws: crate::Workspace, panel: String) -> Element {
    match try_consume_context::<ResultActions>() {
        Some(actions) => (actions.0)(ws, panel),
        None => VNode::empty(),
    }
}

/// A snapshot row token (including position to distinguish identical rows),
/// followed by any authoritative graph identities. Tokens never become paths.
/// Changed/evicted rows cannot select a different row at the old position.
pub fn row_selection(owner: NodeId, index: usize, row: &[Value]) -> Vec<String> {
    let bytes = serde_json::to_vec(&(owner, index, row)).unwrap_or_default();
    let mut ids = vec![format!(
        "row:{owner}:{index}:{}",
        NodeId::from_content(&bytes)
    )];
    ids.extend(
        row.iter()
            .filter_map(|v| match v {
                Value::Ref { node, .. } => Some(node.to_string()),
                _ => None,
            })
            .take(4095),
    );
    ids
}

/// Resolve a snapshot selection without scanning/re-hashing a whole result.
/// Both the owner and current row hash must match before exposing the index.
pub fn selected_row(
    owner: NodeId,
    table: &moonkale_core::Table,
    selection: &[String],
) -> Option<usize> {
    let prefix = format!("row:{owner}:");
    for token in selection.iter().take(4096) {
        let Some(index) = token
            .strip_prefix(&prefix)
            .and_then(|s| s.split_once(':'))
            .and_then(|(i, _)| i.parse::<usize>().ok())
        else {
            continue;
        };
        if let Some(row) = table.rows.get(index) {
            if row_selection(owner, index, row).first() == Some(token) {
                return Some(index);
            }
        }
    }
    None
}
