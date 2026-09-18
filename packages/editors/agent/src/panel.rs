use crate::host::{PendingApproval, WorkspaceHost};
use crate::transcript;
use dioxus::prelude::*;
use moonkale_core::SourceFamily;
use moonkale_ext_api::Workspace;
use moonkale_llm::{Agent, AgentEvent, Class, Decision, Provider, ToolOutcome};
use serde_json::Value;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

const CSS: Asset = asset!("/assets/agent.css");

/// What the panel shows, in order.
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    User(String),
    Assistant(String),
    Tool {
        id: String,
        name: String,
        input: Value,
        class: Class,
        decision: Decision,
        outcome: Option<ToolOutcome>,
        summary: String,
    },
    Error(String),
}

/// Conversation state, per window; survives panel remounts.
#[derive(Clone)]
pub struct Chat {
    pub items: Signal<Vec<Item>>,
    pub agent: Signal<Option<Rc<RefCell<Agent>>>>,
    pub provider_label: Signal<String>,
    pub busy: Signal<bool>,
    pub pending: Signal<Option<PendingApproval>>,
    pub cited: Signal<Vec<String>>,
    pub show_activity: Signal<bool>,
    /// Mirror of the agent's audit log: the agent is mutably borrowed
    /// while an exchange runs, so the panel never reads it directly.
    pub audit: Signal<Vec<moonkale_llm::AuditEntry>>,
}

impl Chat {
    fn get(ws: Workspace) -> Self {
        let _ = ws;
        use_hook(|| Self {
            items: Signal::new_in_scope(Vec::new(), ScopeId::ROOT),
            agent: Signal::new_in_scope(None, ScopeId::ROOT),
            provider_label: Signal::new_in_scope("connecting…".into(), ScopeId::ROOT),
            busy: Signal::new_in_scope(false, ScopeId::ROOT),
            pending: Signal::new_in_scope(None, ScopeId::ROOT),
            cited: Signal::new_in_scope(Vec::new(), ScopeId::ROOT),
            show_activity: Signal::new_in_scope(false, ScopeId::ROOT),
            audit: Signal::new_in_scope(Vec::new(), ScopeId::ROOT),
        })
    }
}

fn system_prompt(ws: Workspace) -> String {
    let mut s = String::from(
        "You are the assistant inside Moonkale, a graph-native code and knowledge editor. \
         The user's open sources (folders, indexes, databases) are reachable through tools. \
         Start with workspace.list_sources when you need ids. Prefer index.search to find \
         things in files, graph.query to browse structure, graph.fetch to read, and \
         source.text_query for SQL/Cypher. Be concise; cite file paths.\n",
    );
    let sources = ws.sources.peek();
    if !sources.is_empty() {
        s.push_str("\nOpen sources:\n");
        for h in sources.iter() {
            s.push_str(&format!(
                "- {} ({:?}) id={}\n",
                h.descriptor.display_name, h.descriptor.family, h.descriptor.id
            ));
        }
    }
    if let Some(active) = *ws.active.peek() {
        if let Some(doc) = ws.document(active) {
            let d = doc.peek();
            s.push_str(&format!(
                "\nThe user's active document is `{}` (source {}, node id {}).\n",
                d.node.native_key, d.node.source, d.node.id
            ));
        }
    }
    s
}

// The agent is mutably borrowed for a whole exchange on purpose: `busy`
// keeps every other path (save, clear, render) off it meanwhile.
#[allow(clippy::await_holding_refcell_ref)]
#[component]
pub fn AgentPanel(ws: Workspace) -> Element {
    let chat = Chat::get(ws);
    let Chat {
        mut items,
        mut agent,
        mut provider_label,
        mut busy,
        pending,
        mut cited,
        mut show_activity,
        mut audit,
    } = chat.clone();
    let mut input = use_signal(String::new);

    // Connect the provider from the resolved settings; reconnect when the
    // LLM settings change (the conversation is kept, the provider swapped).
    let mut connected_for: Signal<Option<moonkale_llm::LlmSettings>> = use_signal(|| None);
    use_effect(move || {
        let llm = ws.settings.read().llm.clone();
        if connected_for.peek().as_ref() == Some(&llm) {
            return;
        }
        if *busy.peek() {
            return;
        }
        connected_for.set(Some(llm.clone()));
        let Some(make) = ws.llm() else {
            provider_label.set("no LLM provider on this platform".into());
            return;
        };
        provider_label.set(format!("connecting {}…", llm.provider));
        spawn(async move {
            match make(llm).await {
                Ok(p) => {
                    let p: Arc<dyn Provider> = p;
                    provider_label.set(format!("{} · {}", p.name(), p.model()));
                    let existing = agent.peek().clone();
                    match existing {
                        Some(a) => a.borrow_mut().provider = p,
                        None => {
                            agent.set(Some(Rc::new(RefCell::new(Agent::new(p, String::new())))))
                        }
                    }
                }
                Err(e) => provider_label.set(format!("provider error: {e}")),
            }
        });
    });

    let mut send = move |_| {
        let text = input.peek().trim().to_string();
        if text.is_empty() || *busy.peek() {
            return;
        }
        let Some(a) = agent.peek().clone() else {
            return;
        };
        input.set(String::new());
        items.with_mut(|v| v.push(Item::User(text.clone())));
        busy.set(true);
        spawn(async move {
            // The system prompt and the policy follow the workspace.
            {
                let mut g = a.borrow_mut();
                g.system = system_prompt(ws);
                let ps = ws.settings.peek().policy.clone();
                g.policy = moonkale_llm::Policy {
                    mutating: if ps.allow_writes {
                        moonkale_llm::Decision::Allow
                    } else {
                        moonkale_llm::Decision::Ask
                    },
                    destructive: moonkale_llm::Decision::Ask,
                    denied_tools: ps.denied_tools,
                };
            }
            let host = WorkspaceHost { ws, pending, cited };
            let mut on_event = |ev: AgentEvent| match ev {
                AgentEvent::TextDelta(t) => items.with_mut(|v| match v.last_mut() {
                    Some(Item::Assistant(s)) => s.push_str(&t),
                    _ => v.push(Item::Assistant(t)),
                }),
                AgentEvent::ToolCall {
                    id,
                    name,
                    input,
                    class,
                    decision,
                } => items.with_mut(|v| {
                    v.push(Item::Tool {
                        id,
                        name,
                        input,
                        class,
                        decision,
                        outcome: None,
                        summary: String::new(),
                    })
                }),
                AgentEvent::ToolResult {
                    id,
                    outcome,
                    summary,
                } => items.with_mut(|v| {
                    if let Some(Item::Tool {
                        outcome: o,
                        summary: s,
                        ..
                    }) = v
                        .iter_mut()
                        .rev()
                        .find(|i| matches!(i, Item::Tool { id: tid, .. } if *tid == id))
                    {
                        *o = Some(outcome);
                        *s = summary;
                    }
                }),
                AgentEvent::TurnDone { .. } => {
                    // Start a fresh assistant bubble for text after tool calls.
                    items.with_mut(|v| {
                        if matches!(v.last(), Some(Item::Assistant(_))) {
                            v.push(Item::Assistant(String::new()));
                        }
                    })
                }
                AgentEvent::Finished => items.with_mut(|v| {
                    if matches!(v.last(), Some(Item::Assistant(s)) if s.is_empty()) {
                        v.pop();
                    }
                }),
                AgentEvent::Error(e) => items.with_mut(|v| v.push(Item::Error(e))),
            };
            // `send` needs `&mut Agent` across awaits; the RefCell borrow is
            // held for the whole exchange (nothing else touches the agent
            // while `busy`).
            let mut guard = a.borrow_mut();
            guard.send(text, &host, &mut on_event).await;
            audit.set(guard.audit.entries.clone());
            drop(guard);
            busy.set(false);
        });
    };

    let save = move |_| {
        let Some(a) = agent.peek().clone() else {
            return;
        };
        spawn(async move {
            let folder = ws
                .sources
                .peek()
                .iter()
                .find(|s| s.descriptor.family == SourceFamily::Folder)
                .cloned();
            let Some(folder) = folder else {
                let mut ws = ws;
                ws.set_status("Open a folder to save the transcript into");
                return;
            };
            let (text, title) = {
                let g = a.borrow();
                let first = g
                    .messages
                    .iter()
                    .find(|m| m.role == moonkale_llm::Role::User)
                    .map(|m| m.text())
                    .unwrap_or_else(|| "chat".into());
                let title: String = first.chars().take(60).collect();
                let label = provider_label.peek().clone();
                (
                    transcript::render(&title, &label, &g.messages, &g.audit, &cited.peek()),
                    title,
                )
            };
            let slug: String = title
                .chars()
                .map(|c| {
                    if c.is_alphanumeric() {
                        c.to_ascii_lowercase()
                    } else {
                        '-'
                    }
                })
                .collect::<String>()
                .trim_matches('-')
                .chars()
                .take(40)
                .collect();
            let tag = uuid::Uuid::new_v4().simple().to_string();
            let name = format!(".moonkale/chats/{slug}-{}.md", &tag[..8]);
            match ws
                .create_text(&folder.descriptor.id, folder.descriptor.root, &name, &text)
                .await
            {
                Ok(node) => {
                    let _ = ws.open_node(node).await;
                }
                Err(e) => {
                    let mut ws = ws;
                    ws.set_status(format!("Could not save transcript: {e}"));
                }
            }
        });
    };

    let clear = move |_| {
        if let Some(a) = agent.peek().clone() {
            let mut g = a.borrow_mut();
            g.messages.clear();
            g.audit = Default::default();
        }
        items.set(Vec::new());
        cited.set(Vec::new());
        audit.set(Vec::new());
    };

    let pending_now = pending();
    let audit_entries = audit();

    rsx! {
        document::Stylesheet { href: CSS }
        div { class: "mk-agent",
            div { class: "mk-agent-toolbar",
                span { class: "mk-agent-provider", title: "Provider · model", "{provider_label}" }
                span { class: "mk-agent-spacer" }
                button { class: if show_activity() { "mk-btn mk-btn-on" } else { "mk-btn" }, onclick: move |_| show_activity.toggle(), title: "Every tool call with its policy decision", "Activity" }
                button { class: "mk-btn", disabled: items().is_empty() || busy(), onclick: save, title: "Save this conversation as a markdown page in the folder (.moonkale/chats/)", "Save" }
                button { class: "mk-btn", disabled: items().is_empty() || busy(), onclick: clear, "Clear" }
            }
            if show_activity() {
                div { class: "mk-agent-activity",
                    if audit_entries.is_empty() { p { class: "mk-muted", "No tool calls yet." } }
                    for e in audit_entries {
                        div { key: "{e.seq}", class: "mk-agent-audit",
                            span { class: "mk-agent-audit-tool", "{e.tool}" }
                            span { class: "mk-agent-badge", "{e.class:?}" }
                            span { class: "mk-agent-badge", "{e.decision:?}" }
                            span { class: if e.ok { "mk-agent-ok" } else { "mk-agent-err" }, if e.ok { "ok" } else { "failed" } }
                            span { class: "mk-muted", " {e.millis} ms" }
                            div { class: "mk-agent-audit-summary", "{e.summary}" }
                        }
                    }
                }
            }
            div { class: "mk-agent-log",
                if items().is_empty() {
                    p { class: "mk-muted mk-agent-hint",
                        "Ask about the open folder or databases. The agent can list sources, browse the graph, read files, run read-only queries and search. Writes ask for your approval."
                    }
                }
                for (i, item) in items().into_iter().enumerate() {
                    {
                        match item {
                            Item::User(t) => rsx! { div { key: "{i}", class: "mk-agent-msg mk-agent-user", "{t}" } },
                            Item::Assistant(t) => rsx! { div { key: "{i}", class: "mk-agent-msg mk-agent-assistant", "{t}" } },
                            Item::Error(t) => rsx! { div { key: "{i}", class: "mk-agent-msg mk-agent-error", "{t}" } },
                            Item::Tool { name, input, class, decision, outcome, summary, .. } => rsx! {
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
                if let Some(p) = pending_now {
                    div { class: "mk-agent-approval",
                        div { class: "mk-agent-approval-title",
                            "The agent wants to run "
                            code { "{p.call.name}" }
                            " ("
                            "{p.class:?}"
                            ")"
                        }
                        if p.call.name == "editor.replace" {
                            div { class: "mk-agent-diff",
                                pre { class: "mk-agent-diff-old", "{p.call.str(\"old\").unwrap_or_default()}" }
                                pre { class: "mk-agent-diff-new", "{p.call.str(\"new\").unwrap_or_default()}" }
                            }
                        } else if p.call.name == "terminal.run" {
                            pre { class: "mk-agent-diff-cmd", "$ {p.call.str(\"command\").unwrap_or_default()}" }
                        } else {
                            code { class: "mk-agent-tool-input", "{p.call.input}" }
                        }
                        div { class: "mk-agent-approval-actions",
                            button { class: "mk-btn mk-btn-on", onclick: { let p = p.clone(); move |_| { if let Some(tx) = p.reply.borrow_mut().take() { let _ = tx.send(true); } } }, "Allow" }
                            button { class: "mk-btn", onclick: { let p = p.clone(); move |_| { if let Some(tx) = p.reply.borrow_mut().take() { let _ = tx.send(false); } } }, "Deny" }
                        }
                    }
                }
            }
            div { class: "mk-agent-compose",
                textarea {
                    class: "mk-agent-input",
                    rows: 2,
                    placeholder: if agent().is_some() { "Ask the agent… (Enter to send, Shift+Enter for a new line)" } else { "Connecting to the provider…" },
                    disabled: agent().is_none(),
                    value: "{input}",
                    oninput: move |e| input.set(e.value()),
                    onkeydown: move |e: KeyboardEvent| {
                        if e.key() == Key::Enter && !e.modifiers().shift() {
                            e.prevent_default();
                            send(());
                        }
                    },
                }
                button { class: "mk-btn", disabled: busy() || agent().is_none(), onclick: move |_| send(()), if busy() { "…" } else { "Send" } }
            }
        }
    }
}
