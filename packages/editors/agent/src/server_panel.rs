//! The Agent panel when the sources live on a server (Milestone 12): the
//! turn runs there (`moonkale_server::agent_sessions`) and keeps running with no
//! client attached; this panel sends, polls the transcript, answers
//! approvals, and lists the folder's sessions so a later client — the
//! phone — picks up where things stand.

use crate::L;
use dioxus::prelude::*;
use moonkale_core::SourceFamily;
use moonkale_ext_api::{t, AgentSessions, Workspace};
use moonkale_llm::sessions::{PendingApproval, SessionItem, SessionSummary, TurnSettings};
use moonkale_llm::ToolOutcome;

const CSS: Asset = asset!("/assets/agent.css");

/// Per-window state of the server mode; survives panel remounts.
#[derive(Clone, Copy)]
struct ServerChat {
    session: Signal<Option<String>>,
    items: Signal<Vec<SessionItem>>,
    running: Signal<bool>,
    pending: Signal<Option<PendingApproval>>,
    provider: Signal<String>,
    /// The saved agent the next send runs (Milestone 15).
    profile: Signal<String>,
    sessions: Signal<Vec<SessionSummary>>,
    /// Bumped by the poller to keep one loop per session.
    poll_gen: Signal<u64>,
    /// Bumped by `send` so the poller restarts for the same session.
    restart: Signal<u64>,
}

fn folder_of(ws: Workspace) -> Option<String> {
    ws.sources
        .open
        .peek()
        .iter()
        .find(|s| s.descriptor.family == SourceFamily::Folder)
        .and_then(|s| {
            s.descriptor
                .id
                .as_str()
                .strip_prefix("folder:")
                .map(str::to_string)
        })
}

#[component]
pub fn ServerAgentPanel(ws: Workspace, api: AgentSessions) -> Element {
    let chat = use_hook(|| ServerChat {
        session: Signal::new_in_scope(None, ScopeId::ROOT),
        items: Signal::new_in_scope(Vec::new(), ScopeId::ROOT),
        running: Signal::new_in_scope(false, ScopeId::ROOT),
        pending: Signal::new_in_scope(None, ScopeId::ROOT),
        provider: Signal::new_in_scope("server".into(), ScopeId::ROOT),
        profile: Signal::new_in_scope(
            ws.settings.resolved.peek().agent.default.clone(),
            ScopeId::ROOT,
        ),
        sessions: Signal::new_in_scope(Vec::new(), ScopeId::ROOT),
        poll_gen: Signal::new_in_scope(0, ScopeId::ROOT),
        restart: Signal::new_in_scope(0, ScopeId::ROOT),
    });
    let ServerChat {
        mut session,
        mut items,
        mut running,
        mut pending,
        mut provider,
        mut profile,
        mut sessions,
        mut poll_gen,
        mut restart,
    } = chat;
    let mut input = use_signal(String::new);
    let mut ws = ws;

    // The folder's sessions, when the folder changes.
    let folder = use_memo(move || {
        ws.sources
            .open
            .read()
            .iter()
            .find(|s| s.descriptor.family == SourceFamily::Folder)
            .and_then(|s| {
                s.descriptor
                    .id
                    .as_str()
                    .strip_prefix("folder:")
                    .map(str::to_string)
            })
    });
    use_effect(move || {
        let Some(f) = folder() else {
            return;
        };
        spawn(async move {
            if let Ok(list) = (api.list)(f).await {
                // A turn still running when we arrive: show it.
                if session.peek().is_none() {
                    if let Some(live) = list.iter().find(|s| s.running) {
                        session.set(Some(live.id.clone()));
                    }
                }
                sessions.set(list);
            }
        });
    });

    // Poll the open session while it runs (and once when it is opened).
    use_effect(move || {
        let _ = restart();
        let Some(id) = session() else {
            return;
        };
        let gen = *poll_gen.peek() + 1;
        poll_gen.set(gen);
        items.set(Vec::new());
        pending.set(None);
        spawn(async move {
            let mut since = 0usize;
            loop {
                if *poll_gen.peek() != gen {
                    return;
                }
                match (api.events)(id.clone(), since).await {
                    Ok(state) => {
                        if !state.items.is_empty() {
                            items.with_mut(|v| {
                                // A streamed assistant bubble arrives whole each time:
                                // replace the trailing one rather than append.
                                v.truncate(since);
                                v.extend(state.items.iter().cloned());
                            });
                        }
                        // Re-read the last item next time (it may still be streaming).
                        since = state.total.saturating_sub(1);
                        if !state.provider.is_empty() {
                            provider.set(state.provider.clone());
                        }
                        running.set(state.running);
                        pending.set(state.pending.clone());
                        if !state.running {
                            // Final fetch of the whole tail, then stop.
                            if let Ok(fin) = (api.events)(id.clone(), 0).await {
                                items.set(fin.items);
                            }
                            return;
                        }
                    }
                    Err(e) => {
                        ws.set_status(t!(ws, L, "agent-session-error", error = e.to_string()));
                        running.set(false);
                        return;
                    }
                }
                crate::sleep_ms(700).await;
            }
        });
    });

    let mut send = move |_| {
        let text = input.peek().trim().to_string();
        if text.is_empty() || *running.peek() {
            return;
        }
        let Some(f) = folder_of(ws) else {
            ws.set_status(t!(ws, L, "agent-open-folder"));
            return;
        };
        input.set(String::new());
        let settings = {
            let s = ws.settings.resolved.peek();
            TurnSettings {
                llm: s.agent(&profile.peek()).llm.clone(),
                allow_writes: s.policy.allow_writes,
                denied_tools: s.policy.denied_tools.clone(),
            }
        };
        let current = session.peek().clone();
        items.with_mut(|v| v.push(SessionItem::User { text: text.clone() }));
        running.set(true);
        spawn(async move {
            match (api.send)(current.clone(), f.clone(), text, settings).await {
                Ok(id) => {
                    session.set(Some(id));
                    restart += 1;
                    if let Ok(list) = (api.list)(f).await {
                        sessions.set(list);
                    }
                }
                Err(e) => {
                    items.with_mut(|v| v.push(SessionItem::Error { text: e }));
                    running.set(false);
                }
            }
        });
    };

    let answer = move |allow: bool| {
        let (Some(id), Some(p)) = (session.peek().clone(), pending.peek().clone()) else {
            return;
        };
        spawn(async move {
            if let Err(e) = (api.approve)(id, p.call_id, allow).await {
                ws.set_status(t!(ws, L, "agent-approval-error", error = e.to_string()));
            }
        });
    };

    let list = sessions();
    let current = session();
    let agents: Vec<String> = ws
        .settings
        .resolved
        .read()
        .agents
        .iter()
        .map(|a| a.name.clone())
        .collect();
    let profile_now = profile();
    rsx! {
        moonkale_ext_api::Stylesheet { href: CSS }
        div { class: "mk-agent mk-agent-server",
            div { class: "mk-agent-toolbar",
                span { class: "mk-agent-provider", title: t!(ws, L, "agent-server-title"), {t!(ws, L, "agent-on-the-server", provider = provider())} }
                span { class: "mk-agent-spacer" }
                select { class: "mk-agent-sessions", title: t!(ws, L, "agent-server-sessions"),
                    value: "{current.clone().unwrap_or_default()}",
                    onchange: move |e| { let v = e.value(); if v.is_empty() { session.set(None); items.set(Vec::new()); pending.set(None); running.set(false); } else { session.set(Some(v)); } },
                    option { value: "", selected: current.is_none(), {t!(ws, L, "agent-new-session")} }
                    for s in list.iter() {
                        option { key: "{s.id}", value: "{s.id}", selected: current.as_deref() == Some(s.id.as_str()), if s.running { "● " } "{s.title}" }
                    }
                }
                button { class: "mk-btn", disabled: running(), onclick: move |_| { session.set(None); items.set(Vec::new()); pending.set(None); }, {t!(ws, L, "agent-new")} }
                select { class: "mk-agent-profile", title: t!(ws, L, "agent-profile-next"),
                    value: "{profile_now}",
                    onchange: move |e| profile.set(e.value()),
                    for name in agents.iter() {
                        option { key: "{name}", value: "{name}", selected: *name == profile_now, "{name}" }
                    }
                }
            }
            div { class: "mk-agent-log",
                if items().is_empty() {
                    p { class: "mk-muted mk-agent-hint",
                        {t!(ws, L, "agent-server-hint")}
                    }
                }
                for (i, item) in items().into_iter().enumerate() {
                    {
                        match item {
                            SessionItem::User { text } => rsx! { div { key: "{i}", class: "mk-agent-msg mk-agent-user", "{text}" } },
                            SessionItem::Assistant { text } => rsx! { div { key: "{i}", class: "mk-agent-msg mk-agent-assistant", "{text}" } },
                            SessionItem::Error { text } => rsx! { div { key: "{i}", class: "mk-agent-msg mk-agent-error", "{text}" } },
                            SessionItem::Tool { name, input, class, decision, outcome, summary, .. } => rsx! {
                                div { key: "{i}", class: "mk-agent-tool",
                                    div { class: "mk-agent-tool-head",
                                        span { class: "mk-agent-tool-name", "{name}" }
                                        span { class: "mk-agent-badge", "{class:?}" }
                                        span { class: "mk-agent-badge", "{decision:?}" }
                                        match outcome {
                                            None => rsx! { span { class: "mk-muted", "running…" } },
                                            Some(ToolOutcome::Ran { ok: true }) => rsx! { span { class: "mk-agent-ok", "ok" } },
                                            Some(ToolOutcome::Ran { ok: false }) => rsx! { span { class: "mk-agent-err", "failed" } },
                                            Some(ToolOutcome::Denied) => rsx! { span { class: "mk-agent-err", "denied by policy" } },
                                            Some(ToolOutcome::Declined) => rsx! { span { class: "mk-agent-err", "declined" } },
                                        }
                                    }
                                    code { class: "mk-agent-tool-input", "{input}" }
                                    if !summary.is_empty() { div { class: "mk-agent-tool-summary", "{summary}" } }
                                }
                            },
                        }
                    }
                }
                if let Some(p) = pending() {
                    div { class: "mk-agent-approval",
                        div { class: "mk-agent-approval-title",
                            {t!(ws, L, "agent-wants")}
                            code { "{p.name}" }
                            {t!(ws, L, "agent-waiting", class = format!("{:?}", p.class))}
                        }
                        code { class: "mk-agent-tool-input", "{p.input}" }
                        div { class: "mk-agent-approval-actions",
                            button { class: "mk-btn mk-btn-on", onclick: move |_| answer(true), {t!(ws, L, "agent-allow")} }
                            button { class: "mk-btn", onclick: move |_| answer(false), {t!(ws, L, "agent-deny")} }
                        }
                    }
                }
                if running() {
                    p { class: "mk-muted mk-agent-running", "running on the server…" }
                }
            }
            div { class: "mk-agent-compose",
                textarea {
                    id: crate::panel::INPUT_ID,
                    class: "mk-agent-input",
                    rows: 2,
                    placeholder: t!(ws, L, "agent-ask-server"),
                    value: "{input}",
                    oninput: move |e| input.set(e.value()),
                    onkeydown: move |e: KeyboardEvent| {
                        // Not while an input method composes (#19).
                        if e.key() == Key::Enter && !e.modifiers().shift() && !e.is_composing() {
                            e.prevent_default();
                            send(());
                        }
                    },
                }
                button { class: "mk-btn", disabled: running(), onclick: move |_| send(()), if running() { "…" } else { {t!(ws, L, "agent-send")} } }
            }
        }
    }
}
