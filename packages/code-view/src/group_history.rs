//! Persistent CodeMirror history coordination, keyed by an open document session.
use dioxus::prelude::*;
use moonkale_core::NodeId;
use moonkale_ext_api::{workspace::grouped_edits, Workspace};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, serde::Deserialize)]
pub struct Stamp {
    pub epoch: u64,
    pub sequence: u64,
    #[serde(default)]
    pub workspace: bool,
}
#[derive(Clone)]
pub(crate) struct Binding {
    pub key: String,
    pub stamp: Rc<Cell<Stamp>>,
    pub held: Rc<RefCell<Option<String>>>,
}
impl Binding {
    pub(crate) fn standalone() -> Self {
        Self {
            key: String::new(),
            stamp: Rc::new(Cell::new(Stamp::default())),
            held: Rc::new(RefCell::new(None)),
        }
    }
}
static NEXT: AtomicU64 = AtomicU64::new(1);

pub(crate) fn install(ws: Workspace, node: NodeId) -> Binding {
    let state = ws
        .editor_view_state_with_cleanup(
            node,
            || Binding {
                key: format!("cm-session-{}", NEXT.fetch_add(1, Ordering::Relaxed)),
                stamp: Rc::new(Cell::new(Stamp::default())),
                held: Rc::new(RefCell::new(None)),
            },
            |binding| {
                let eval = document::eval(
                    "const key=await dioxus.recv(); window.moonkale?.codemirror?.dropSession(key);",
                );
                let _ = eval.send(&binding.key);
            },
        )
        .expect("open document");
    let binding = state.peek().clone();
    let bridge = binding.clone();
    let doc = ws.document(node).expect("open document");
    grouped_edits::register_view(
        ws,
        node,
        "codemirror",
        Some(Rc::new(move |mut request| {
            let bridge = bridge.clone();
            Box::pin(async move {
                if request.phase == "prepare" {
                    *bridge.held.borrow_mut() = Some(request.token.clone());
                }
                let mut stamp = bridge.stamp.get();
                if request.phase == "commit" {
                    stamp.epoch = stamp.epoch.saturating_add(1);
                    stamp.sequence = 0;
                    bridge.stamp.set(stamp);
                }
                if request.phase == "cancel" {
                    if ws.document(node) != Some(doc) {
                        return false;
                    }
                    let text = doc.peek().text.clone();
                    let owns_hold = bridge.held.borrow().as_deref() == Some(&request.token);
                    if owns_hold {
                        *bridge.held.borrow_mut() = None;
                    }
                    if owns_hold && text != request.text {
                        stamp.epoch = stamp.epoch.saturating_add(1);
                        stamp.sequence = 0;
                        bridge.stamp.set(stamp);
                    }
                    request.text = text;
                }
                let message = serde_json::json!({ "key":bridge.key, "request":{
                    "phase":request.phase, "direction":request.direction, "group":request.group,
                    "token":request.token, "text":request.text, "epoch":stamp.epoch, "sequence":stamp.sequence,
                }});
                let mut eval = document::eval("const m=await dioxus.recv(); dioxus.send(window.moonkale?.codemirror?.groupSession(m.key,m.request) ?? false);");
                let ready = if eval.send(message).is_err() {
                    false
                } else {
                    eval.recv::<bool>().await.unwrap_or(false)
                };
                // Refusal did not hold the JS view. Its queued user input must
                // keep the same epoch; cancellation must not advance it.
                if request.phase == "prepare"
                    && !ready
                    && bridge.held.borrow().as_deref() == Some(&request.token)
                {
                    *bridge.held.borrow_mut() = None;
                }
                ready
            })
        })),
    );
    binding
}

pub(crate) fn restore(ws: Workspace, group: u64, redo: bool) {
    dioxus::core::spawn_forever(async move {
        if let Err(error) = grouped_edits::restore(ws, group, redo).await {
            let mut ws = ws;
            let file = error
                .node
                .and_then(|id| ws.document(id))
                .map(|doc| doc.peek().node.native_key.clone())
                .unwrap_or_default();
            ws.set_status(moonkale_ext_api::t!(
                ws,
                crate::L,
                "editor-group-history-blocked",
                file = file
            ));
        }
    });
}
