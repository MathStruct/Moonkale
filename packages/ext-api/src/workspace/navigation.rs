//! Bounded, window-local visits shared by documents and graph cameras.
use super::*;

#[derive(Clone, PartialEq)]
enum Visit {
    Document(NodeId),
    Camera(String, [f32; 7]),
}

/// Session navigation is separate from edit/undo history and is never persisted.
#[derive(Clone, Copy)]
pub struct NavigationState(Signal<Visits>);
#[derive(Default)]
struct Visits {
    entries: Vec<Visit>,
    cursor: usize,
    replaying: bool,
}
impl NavigationState {
    pub(super) fn new() -> Self {
        Self(Signal::new_in_scope(Visits::default(), ScopeId::ROOT))
    }
}
impl Workspace {
    fn visit(mut self, visit: Visit) {
        self.navigation.0.with_mut(|history| {
            if history.replaying || history.entries.get(history.cursor) == Some(&visit) {
                return;
            }
            history.entries.truncate(if history.entries.is_empty() {
                0
            } else {
                history.cursor + 1
            });
            history.entries.push(visit);
            if history.entries.len() > 256 {
                history.entries.remove(0);
            }
            history.cursor = history.entries.len() - 1;
        });
    }
    pub(super) fn visit_document(self, node: NodeId) {
        self.visit(Visit::Document(node));
    }

    /// Record a committed camera gesture, not every rendered frame.
    pub fn visit_camera(self, panel: &str, state: [f32; 7]) {
        if panel.len() <= 128 && state.iter().all(|x| x.is_finite()) && state[0] > 0.0 {
            self.visit(Visit::Camera(panel.into(), state));
        }
    }

    /// Move through visits without changing the browser's page history.
    pub fn navigate(mut self, forward: bool) {
        let target = self.navigation.0.with_mut(|history| {
            let next = if forward {
                history.cursor.checked_add(1)
            } else {
                history.cursor.checked_sub(1)
            }?;
            let target = history.entries.get(next)?.clone();
            history.cursor = next;
            history.replaying = true;
            Some(target)
        });
        match target {
            Some(Visit::Document(id)) => {
                if self.open_node_by_id(id).is_some() {
                    self.set_active_document(Some(id));
                }
            }
            Some(Visit::Camera(panel, state)) => {
                // Only host-created graph identities and finite numbers cross the adapter.
                let payload = serde_json::to_string(&(panel, state)).unwrap_or_default();
                let _ =
                    dioxus::document::eval(&format!("window.moonkaleRestoreCamera?.({payload});"));
            }
            None => {}
        }
        self.navigation
            .0
            .with_mut(|history| history.replaying = false);
    }
}
