//! The History panel (Milestone 8): the workspace's entity log — every
//! create, rename, delete, edit (by user or agent) and git checkpoint —
//! newest first, filterable to the active document, with "text at this
//! point" views that replay the log.
//!
//! Its own crate since Milestone 18 phase 4.2 (it was `ui/src/history.rs`):
//! nothing in the shell is privileged, and a distribution may leave it out.

use dioxus::prelude::*;
use moonkale_core::{EventId, EventKind, NodeId};
use moonkale_ext_api::prelude::*;
use moonkale_ext_api::{i18n::Locales, t, Command};

/// This crate's strings (spec 030): English, German, Chinese.
pub(crate) static L: moonkale_ext_api::i18n::Locales = &[
    ("en", include_str!("../locales/en.ftl")),
    ("de", include_str!("../locales/de.ftl")),
    ("zh-CN", include_str!("../locales/zh-CN.ftl")),
];

const CSS: Asset = asset!("/assets/history.css");

pub const PANEL_ID: &str = "history";
const VIEW_PREFIX: &str = "history-view:";

#[derive(Clone, Copy)]
pub struct HistoryState {
    /// Open "text at" views: (event, node, key).
    views: Signal<Vec<(EventId, NodeId, String)>>,
}

impl PartialEq for HistoryState {
    fn eq(&self, o: &Self) -> bool {
        self.views == o.views
    }
}

pub struct HistoryExtension {
    state: HistoryState,
}

impl Default for HistoryExtension {
    fn default() -> Self {
        Self::new()
    }
}

impl HistoryExtension {
    pub fn new() -> Self {
        Self {
            state: HistoryState {
                views: Signal::new_in_scope(Vec::new(), ScopeId::ROOT),
            },
        }
    }
}

impl Extension for HistoryExtension {
    fn manifest(&self) -> Manifest {
        Manifest::optional(
            "dev.moonkale.history",
            "History",
            "The workspace's entity log: who changed what, and any file as it was after any change.",
        )
    }

    fn panels(&self, ws: Workspace) -> Vec<PanelContribution> {
        let mut out =
            vec![
                PanelContribution::new(PANEL_ID, t!(ws, L, "history-title"), PanelHome::Side)
                    .closable(true)
                    .activity(
                        Activity::new("history", 50, t!(ws, L, "history-title")).phone_secondary(),
                    ),
            ];
        for (ev, _, key) in self.state.views.read().iter() {
            out.push(
                PanelContribution::new(
                    format!("{VIEW_PREFIX}{ev}"),
                    format!("{} @ {}", key.rsplit('/').next().unwrap_or(key), ev.short()),
                    PanelHome::Main,
                )
                .closable(true),
            );
        }
        out
    }

    fn render(&self, panel_id: &str, ws: Workspace) -> Element {
        let state = self.state;
        if let Some(id) = panel_id.strip_prefix(VIEW_PREFIX) {
            let event = id.to_string();
            return rsx! { TextAtPanel { ws, state, event } };
        }
        rsx! { HistoryPanel { ws, state } }
    }

    fn on_panel_closed(&self, panel_id: &str, _ws: Workspace) {
        if let Some(id) = panel_id.strip_prefix(VIEW_PREFIX) {
            let id = id.to_string();
            let mut views = self.state.views;
            views.with_mut(|v| v.retain(|(e, _, _)| e.to_string() != id));
        }
    }

    fn commands(&self, ws: Workspace) -> Vec<CommandContribution> {
        vec![CommandContribution::new(
            "history.show",
            t!(ws, L, "history-show"),
        )]
    }

    fn locales(&self) -> Locales {
        L
    }

    fn run_command(&self, id: &str, mut ws: Workspace) {
        if id == "history.show" {
            ws.dispatch(Command::ShowPanel(PANEL_ID));
        }
    }
}

fn when(ws: Workspace, at_ms: u64, now: u64) -> String {
    let d = now.saturating_sub(at_ms) / 1000;
    if d < 60 {
        t!(ws, L, "ago-seconds", n = d)
    } else if d < 3600 {
        t!(ws, L, "ago-minutes", n = d / 60)
    } else if d < 86_400 {
        t!(ws, L, "ago-hours", n = d / 3600)
    } else {
        t!(ws, L, "ago-days", n = d / 86_400)
    }
}

/// An event in one line, in the user's language (`Event::summary` is the
/// English form `core` keeps for logs and tests).
fn summary(ws: Workspace, e: &moonkale_core::Event) -> String {
    match &e.kind {
        EventKind::Add { node, .. } => t!(ws, L, "event-added", name = node.native_key.clone()),
        EventKind::Remove { .. } => t!(ws, L, "event-removed"),
        EventKind::Rename {
            from_key, to_key, ..
        } => {
            t!(
                ws,
                L,
                "event-renamed",
                from = from_key.clone(),
                to = to_key.clone()
            )
        }
        EventKind::Content { patch, .. } => {
            let moonkale_core::TextPatch::Replace(s) = patch;
            let ins: usize = s.iter().map(|x| x.text.chars().count()).sum();
            let del: usize = s.iter().map(|x| x.end.saturating_sub(x.start)).sum();
            t!(ws, L, "event-edited", ins = ins, del = del)
        }
        EventKind::Checkpoint { commit, message } => {
            let head: String = commit.chars().take(7).collect();
            t!(
                ws,
                L,
                "event-commit",
                commit = head,
                message = message.clone()
            )
        }
        EventKind::Snapshot { live, folded, .. } => {
            t!(
                ws,
                L,
                "event-snapshot",
                nodes = live.len(),
                folded = *folded
            )
        }
    }
}

#[component]
fn HistoryPanel(ws: Workspace, state: HistoryState) -> Element {
    let mut ws = ws;
    let mut state = state;
    let mut only_active = use_signal(|| false);
    let log = ws.history.log.read().clone();
    let active = *ws.docs.active.read();
    let now = moonkale_ext_api::workspace::now_ms();
    let folded = log.fold(None);
    let key_of = |node: NodeId| -> String {
        folded
            .live
            .get(&node)
            .map(|n| n.native_key.clone())
            .or_else(|| {
                log.events().iter().rev().find_map(|e| match &e.kind {
                    EventKind::Add { node: n, .. } if n.id == node => Some(n.native_key.clone()),
                    EventKind::Rename { to, to_key, .. } if *to == node => Some(to_key.clone()),
                    _ => None,
                })
            })
            .unwrap_or_else(|| node.to_string())
    };
    let events: Vec<_> = if *only_active.read() {
        match active {
            Some(n) => log.for_node(n).into_iter().cloned().collect(),
            None => Vec::new(),
        }
    } else {
        log.events().to_vec()
    };
    let count = events.len();
    let checkpoints = log
        .events()
        .iter()
        .filter(|e| matches!(e.kind, EventKind::Checkpoint { .. }))
        .count();
    let kb = log.to_jsonl().len() / 1024;
    const KEEP: usize = 200;
    let foldable = log.len().saturating_sub(KEEP);

    rsx! {
        moonkale_ext_api::Stylesheet { href: CSS }
        div { class: "mk-history",
            div { class: "mk-history-head",
                span { class: "mk-muted", {t!(ws, L, "history-head", events = log.len(), checkpoints = checkpoints, kb = kb)} }
                span { class: "mk-history-spacer" }
                if foldable > 0 {
                    button { class: "mk-btn mk-btn-mini", title: t!(ws, L, "history-compact-title", keep = KEEP),
                        onclick: move |_| { let n = ws.compact_history(KEEP); ws.set_status(t!(ws, L, "history-folded", n = n)); },
                        {t!(ws, L, "history-compact", n = foldable)} }
                }
                label { class: "mk-history-check", title: t!(ws, L, "history-active-title"),
                    input { r#type: "checkbox", checked: only_active(), onchange: move |e| only_active.set(e.checked()) }
                    {t!(ws, L, "history-active")}
                }
            }
            if log.is_empty() {
                p { class: "mk-muted", {t!(ws, L, "history-empty")} }
            }
            if !log.is_empty() && count == 0 {
                p { class: "mk-muted", {t!(ws, L, "history-none-active")} }
            }
            ul { class: "mk-history-list", "data-count": "{count}",
                for e in events.iter().rev() {
                    {
                        let is_checkpoint = matches!(e.kind, EventKind::Checkpoint { .. });
                        let node = e.node();
                        let key = e.key.clone().or_else(|| node.map(key_of)).unwrap_or_default();
                        let has_text = node.is_some_and(|n| log.text_at(n, Some(e.id)).is_some());
                        let (ev_id, actor, at) = (e.id, e.actor.clone(), e.at);
                        let who = actor.split(':').nth(1).unwrap_or(&actor).to_string();
                        let summary = summary(ws, e);
                        let key2 = key.clone();
                        rsx! {
                            li { key: "{e.id}", class: if is_checkpoint { "mk-history-event mk-history-checkpoint" } else { "mk-history-event" },
                                "data-kind": match &e.kind { EventKind::Add { .. } => "add", EventKind::Remove { .. } => "remove", EventKind::Rename { .. } => "rename", EventKind::Content { .. } => "content", EventKind::Checkpoint { .. } => "checkpoint", EventKind::Snapshot { .. } => "snapshot" },
                                div { class: "mk-history-line",
                                    span { class: "mk-history-when", "{when(ws, at, now)}" }
                                    span { class: "mk-history-actor", title: "{actor}", "{who}" }
                                    span { class: "mk-history-summary", "{summary}" }
                                }
                                if !key.is_empty() {
                                    div { class: "mk-history-key",
                                        span { class: "mk-muted", "{key}" }
                                        if has_text {
                                            button { class: "mk-btn mk-btn-mini", title: "Open the text as it was after this event", onclick: move |_| {
                                                let Some(n) = node else { return };
                                                if !state.views.peek().iter().any(|(x, _, _)| *x == ev_id) {
                                                    state.views.with_mut(|v| v.push((ev_id, n, key2.clone())));
                                                }
                                                ws.dispatch(Command::ShowPanel(Box::leak(format!("{VIEW_PREFIX}{ev_id}").into_boxed_str())));
                                            }, {t!(ws, L, "history-text")} }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn TextAtPanel(ws: Workspace, state: HistoryState, event: String) -> Element {
    let view = state
        .views
        .read()
        .iter()
        .find(|(e, _, _)| e.to_string() == event)
        .cloned();
    let Some((ev, node, key)) = view else {
        return rsx! { div { class: "mk-history-view", p { class: "mk-muted", {t!(ws, L, "history-closed")} } } };
    };
    let log = ws.history.log.read();
    let text = log.text_at(node, Some(ev));
    let current = ws.document(node).map(|d| d.peek().text.clone());
    let same = current.as_ref().is_some_and(|c| Some(c) == text.as_ref());
    let mut ws = ws;
    rsx! {
        moonkale_ext_api::Stylesheet { href: CSS }
        div { class: "mk-history-view",
            div { class: "mk-history-view-head",
                span { "{key}" }
                span { class: "mk-muted", {t!(ws, L, "history-as-of", event = ev.short())} }
                if same { span { class: "mk-muted", {t!(ws, L, "history-identical")} } }
                span { class: "mk-history-spacer" }
                if text.is_some() && !same {
                    button { class: "mk-btn mk-btn-mini", title: t!(ws, L, "history-restore-title"), onclick: move |_| {
                        spawn(async move {
                            if let Err(e) = ws.restore_text_at(node, ev).await { ws.set_status(t!(ws, L, "history-restore-failed", error = e.to_string())); }
                        });
                    }, {t!(ws, L, "history-restore")} }
                }
            }
            match text {
                Some(t) => rsx! { pre { class: "mk-history-text", "{t}" } },
                None => rsx! { p { class: "mk-muted", {t!(ws, L, "history-no-content")} } },
            }
        }
    }
}
