//! The Agent panel in local mode (Milestone 15: sessions). Any number of
//! sessions live at once; each has its own transcript, agent, provider —
//! chosen among the saved agents of Settings → Agents — busy flag and
//! pending approval. A turn runs in a root-owned task, so closing the panel
//! or switching sessions does not stop it. Finished turns are written to
//! `.moonkale/agent-sessions/local/<id>.json`; the folder's saved sessions
//! are listed in the session select and restored on pick.

use crate::host::{PendingApproval, WorkspaceHost};
use crate::transcript;
use dioxus::prelude::*;
use moonkale_core::{SourceFamily, SourceId};
use moonkale_ext_api::settings::LlmSettings;
use moonkale_ext_api::{Extension, Workspace};
use moonkale_llm::{Agent, AgentEvent, Class, Decision, Message, Provider, ToolOutcome};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

/// The compose box, focused by the `agent.focus` command.
pub const INPUT_ID: &str = "mk-agent-input";

const CSS: Asset = asset!("/assets/agent.css");

/// Where a folder's local sessions are kept.
pub const SESSIONS_DIR: &str = ".moonkale/agent-sessions/local";

/// What the panel shows, in order.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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

/// One conversation (Milestone 15). Signals are root-owned: the session
/// outlives the panel.
#[derive(Clone)]
pub struct Session {
    pub id: String,
    pub created: u64,
    pub title: Signal<String>,
    /// The saved agent this session runs (Settings → Agents).
    pub profile: Signal<String>,
    pub items: Signal<Vec<Item>>,
    pub agent: Signal<Option<Rc<RefCell<Agent>>>>,
    pub provider_label: Signal<String>,
    pub busy: Signal<bool>,
    pub pending: Signal<Option<PendingApproval>>,
    pub cited: Signal<Vec<String>>,
    /// Mirror of the agent's audit log: the agent is mutably borrowed
    /// while an exchange runs, so the panel never reads it directly.
    pub audit: Signal<Vec<moonkale_llm::AuditEntry>>,
    /// The settings the provider was built from (reconnect on change).
    pub connected_for: Signal<Option<LlmSettings>>,
    /// The folder the session belongs to (where it is saved).
    pub folder: Signal<Option<SourceId>>,
}

impl PartialEq for Session {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

/// A session saved in the folder, not loaded yet.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedSummary {
    pub id: String,
    pub title: String,
    pub profile: String,
    pub created: u64,
}

/// The file shape under `SESSIONS_DIR`.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct SavedSession {
    id: String,
    title: String,
    profile: String,
    created: u64,
    messages: Vec<Message>,
    items: Vec<Item>,
    #[serde(default)]
    cited: Vec<String>,
}

/// Per-window state, provided at the root once; survives panel remounts.
#[derive(Clone, Copy)]
pub struct Chats {
    pub sessions: Signal<Vec<Session>>,
    pub current: Signal<Option<String>>,
    pub saved: Signal<Vec<SavedSummary>>,
    /// Which folder `saved` was listed for.
    pub saved_for: Signal<Option<SourceId>>,
    pub show_activity: Signal<bool>,
}

impl Chats {
    fn get() -> Self {
        use_hook(|| match try_consume_context::<Chats>() {
            Some(c) => c,
            None => dioxus::core::provide_root_context(Chats {
                sessions: Signal::new_in_scope(Vec::new(), ScopeId::ROOT),
                current: Signal::new_in_scope(None, ScopeId::ROOT),
                saved: Signal::new_in_scope(Vec::new(), ScopeId::ROOT),
                saved_for: Signal::new_in_scope(None, ScopeId::ROOT),
                show_activity: Signal::new_in_scope(false, ScopeId::ROOT),
            }),
        })
    }

    pub fn current_session(&self) -> Option<Session> {
        let id = self.current.peek().clone()?;
        self.sessions.peek().iter().find(|s| s.id == id).cloned()
    }

    fn session(&self, id: &str) -> Option<Session> {
        self.sessions.peek().iter().find(|s| s.id == id).cloned()
    }
}

fn folder_of(ws: Workspace) -> Option<moonkale_ext_api::SourceHandle> {
    ws.sources
        .peek()
        .iter()
        .find(|s| s.descriptor.family == SourceFamily::Folder)
        .cloned()
}

fn folder_path(ws: Workspace) -> Option<String> {
    folder_of(ws).and_then(|s| {
        s.descriptor
            .id
            .as_str()
            .strip_prefix("folder:")
            .map(str::to_string)
    })
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

/// A fresh session running the saved agent `profile`.
fn new_session(ws: Workspace, mut chats: Chats, profile: String) -> Session {
    let s = Session {
        id: uuid::Uuid::new_v4().simple().to_string(),
        created: moonkale_ext_api::workspace::now_ms(),
        title: Signal::new_in_scope("new session".into(), ScopeId::ROOT),
        profile: Signal::new_in_scope(profile, ScopeId::ROOT),
        items: Signal::new_in_scope(Vec::new(), ScopeId::ROOT),
        agent: Signal::new_in_scope(None, ScopeId::ROOT),
        provider_label: Signal::new_in_scope("connecting…".into(), ScopeId::ROOT),
        busy: Signal::new_in_scope(false, ScopeId::ROOT),
        pending: Signal::new_in_scope(None, ScopeId::ROOT),
        cited: Signal::new_in_scope(Vec::new(), ScopeId::ROOT),
        audit: Signal::new_in_scope(Vec::new(), ScopeId::ROOT),
        connected_for: Signal::new_in_scope(None, ScopeId::ROOT),
        folder: Signal::new_in_scope(
            folder_of(ws).map(|f| f.descriptor.id.clone()),
            ScopeId::ROOT,
        ),
    };
    chats.sessions.with_mut(|v| v.push(s.clone()));
    chats.current.set(Some(s.id.clone()));
    connect(ws, s.clone());
    s
}

/// Build (or swap) the session's provider from its profile's settings.
fn connect(ws: Workspace, s: Session) {
    let Session {
        mut agent,
        mut provider_label,
        mut connected_for,
        profile,
        busy,
        ..
    } = s;
    if *busy.peek() {
        return;
    }
    let (llm, known) = {
        let settings = ws.settings.peek();
        let name = profile.peek().clone();
        let known = settings.agents.iter().any(|a| a.name == name);
        (settings.agent(&name).llm.clone(), known)
    };
    if connected_for.peek().as_ref() == Some(&llm) && agent.peek().is_some() {
        return;
    }
    connected_for.set(Some(llm.clone()));
    let Some(make) = ws.llm() else {
        provider_label.set("no LLM provider on this platform".into());
        return;
    };
    provider_label.set(format!("connecting {}…", llm.provider));
    dioxus::core::spawn_forever(async move {
        match make(llm).await {
            Ok(p) => {
                let p: Arc<dyn Provider> = p;
                let note = if known {
                    ""
                } else {
                    " (profile missing → Default)"
                };
                provider_label.set(format!("{} · {}{note}", p.name(), p.model()));
                let existing = agent.peek().clone();
                match existing {
                    Some(a) => a.borrow_mut().provider = p,
                    None => agent.set(Some(Rc::new(RefCell::new(Agent::new(p, String::new()))))),
                }
            }
            Err(e) => provider_label.set(format!("provider error: {e}")),
        }
    });
}

/// One exchange in a root-owned task: the session's input is closed while
/// it runs; other sessions are untouched. The agent is mutably borrowed for
/// the whole exchange on purpose: `busy` keeps every other path off it.
#[allow(clippy::await_holding_refcell_ref)]
fn run_turn(ws: Workspace, s: Session, text: String) {
    let Session {
        mut items,
        agent,
        mut busy,
        pending,
        cited,
        mut audit,
        mut title,
        ..
    } = s.clone();
    let Some(a) = agent.peek().clone() else {
        return;
    };
    if title.peek().as_str() == "new session" {
        title.set(text.chars().take(60).collect());
    }
    items.with_mut(|v| v.push(Item::User(text.clone())));
    busy.set(true);
    dioxus::core::spawn_forever(async move {
        // The system prompt, the policy and the tool list follow the workspace.
        {
            let mut g = a.borrow_mut();
            g.system = system_prompt(ws);
            // Where a turn acts (Milestone 12): the first open folder.
            g.cwd = folder_path(ws);
            g.tools = moonkale_llm::builtin_tools();
            g.tools.extend(crate::host::wasm_tools(ws));
            let (ps, ext) = {
                let s = ws.settings.peek();
                (s.policy.clone(), s.extensions.clone())
            };
            // Permissions removed in the Extensions panel deny the tools.
            let manifest = crate::extension::AgentExtension.manifest();
            let mut denied = ps.denied_tools;
            if !ext.has(&manifest, "write-files") {
                denied.extend(["editor.replace".to_string(), "file.create".to_string()]);
            }
            if !ext.has(&manifest, "run-commands") {
                denied.push("terminal.run".to_string());
            }
            g.policy = moonkale_llm::Policy {
                mutating: if ps.allow_writes {
                    moonkale_llm::Decision::Allow
                } else {
                    moonkale_llm::Decision::Ask
                },
                destructive: moonkale_llm::Decision::Ask,
                denied_tools: denied,
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
        let mut guard = a.borrow_mut();
        guard.send(text, &host, &mut on_event).await;
        audit.set(guard.audit.entries.clone());
        drop(guard);
        busy.set(false);
        persist(ws, s).await;
    });
}

/// Write the session's snapshot into its folder (silently skipped without
/// a folder or an agent).
async fn persist(ws: Workspace, s: Session) {
    // A session started before a folder was open belongs to the first one.
    if s.folder.peek().is_none() {
        let mut folder = s.folder;
        folder.set(folder_of(ws).map(|f| f.descriptor.id.clone()));
    }
    let Some(folder) = s.folder.peek().clone() else {
        return;
    };
    let Some(a) = s.agent.peek().clone() else {
        return;
    };
    let file = {
        let g = a.borrow();
        SavedSession {
            id: s.id.clone(),
            title: s.title.peek().clone(),
            profile: s.profile.peek().clone(),
            created: s.created,
            messages: g.messages.clone(),
            items: s.items.peek().clone(),
            cited: s.cited.peek().clone(),
        }
    };
    let Ok(text) = serde_json::to_string_pretty(&file) else {
        return;
    };
    let rel = format!("{SESSIONS_DIR}/{}.json", s.id);
    if let Err(e) = ws.write_text_at(&folder, &rel, &text).await {
        let mut ws = ws;
        ws.set_status(format!("Agent session not saved: {e}"));
    }
}

/// List the folder's saved sessions (newest first).
async fn load_saved(ws: Workspace, chats: Chats, folder: SourceId) {
    let mut list = Vec::new();
    for node in ws.list_at(&folder, SESSIONS_DIR).await {
        if !node.native_key.ends_with(".json") {
            continue;
        }
        let Some(text) = ws.read_text_at(&folder, &node.native_key).await else {
            continue;
        };
        if let Ok(f) = serde_json::from_str::<SavedSession>(&text) {
            list.push(SavedSummary {
                id: f.id,
                title: f.title,
                profile: f.profile,
                created: f.created,
            });
        }
    }
    list.sort_by_key(|s| std::cmp::Reverse(s.created));
    let mut chats = chats;
    chats.saved.set(list);
    chats.saved_for.set(Some(folder));
}

/// Bring a saved session back as a live one.
async fn restore(ws: Workspace, chats: Chats, folder: SourceId, id: String) {
    let rel = format!("{SESSIONS_DIR}/{id}.json");
    let Some(text) = ws.read_text_at(&folder, &rel).await else {
        let mut ws = ws;
        ws.set_status("That session's file is gone");
        return;
    };
    let Ok(f) = serde_json::from_str::<SavedSession>(&text) else {
        let mut ws = ws;
        ws.set_status("That session's file could not be read");
        return;
    };
    let s = Session {
        id: f.id,
        created: f.created,
        title: Signal::new_in_scope(f.title, ScopeId::ROOT),
        profile: Signal::new_in_scope(f.profile, ScopeId::ROOT),
        items: Signal::new_in_scope(f.items, ScopeId::ROOT),
        agent: Signal::new_in_scope(None, ScopeId::ROOT),
        provider_label: Signal::new_in_scope("connecting…".into(), ScopeId::ROOT),
        busy: Signal::new_in_scope(false, ScopeId::ROOT),
        pending: Signal::new_in_scope(None, ScopeId::ROOT),
        cited: Signal::new_in_scope(f.cited, ScopeId::ROOT),
        audit: Signal::new_in_scope(Vec::new(), ScopeId::ROOT),
        connected_for: Signal::new_in_scope(None, ScopeId::ROOT),
        folder: Signal::new_in_scope(Some(folder), ScopeId::ROOT),
    };
    let messages = f.messages;
    let mut chats = chats;
    chats.sessions.with_mut(|v| v.push(s.clone()));
    chats.current.set(Some(s.id.clone()));
    // Connect, then put the saved messages into the agent once it exists.
    connect(ws, s.clone());
    let agent = s.agent;
    dioxus::core::spawn_forever(async move {
        for _ in 0..200 {
            if let Some(a) = agent.peek().clone() {
                a.borrow_mut().messages = messages;
                return;
            }
            crate::sleep_ms(50).await;
        }
    });
}

#[component]
pub fn AgentPanel(ws: Workspace) -> Element {
    let chats = Chats::get();
    let Chats {
        mut sessions,
        mut current,
        saved,
        mut saved_for,
        mut show_activity,
    } = chats;
    let mut input = use_signal(String::new);

    // The folder's saved sessions, when the folder changes.
    let folder = use_memo(move || {
        ws.sources
            .read()
            .iter()
            .find(|s| s.descriptor.family == SourceFamily::Folder)
            .map(|f| f.descriptor.id.clone())
    });
    use_effect(move || {
        let f = folder();
        if *saved_for.peek() == f {
            return;
        }
        match f {
            Some(f) => spawn(load_saved(ws, chats, f)),
            None => {
                saved_for.set(None);
                let mut chats = chats;
                chats.saved.set(Vec::new());
                spawn(async {})
            }
        };
    });

    // One session at least; reconnect the current one when the settings
    // its profile resolves to change (a model edited in Settings).
    use_effect(move || {
        let _ = ws.settings.read().agents.len();
        let _ = ws.settings.read().llm.clone();
        let _ = ws.settings.read().agents.clone();
        if sessions.peek().is_empty() {
            let profile = ws.settings.peek().agent.default.clone();
            new_session(ws, chats, profile);
            return;
        }
        if let Some(s) = chats.current_session() {
            connect(ws, s);
        }
    });

    // Reactive reads: the session list and the current id drive the render.
    let current_id = current();
    let session = {
        let list = sessions.read();
        current_id
            .as_ref()
            .and_then(|id| list.iter().find(|s| &s.id == id).cloned())
            .or_else(|| list.first().cloned())
    };
    let Some(session) = session else {
        return rsx! { div { class: "mk-agent", p { class: "mk-muted", "starting…" } } };
    };
    let Session {
        id: session_id,
        mut items,
        agent,
        provider_label,
        busy,
        pending,
        mut cited,
        mut audit,
        mut profile,
        ..
    } = session.clone();

    let send_session = session.clone();
    let send = Callback::new(move |_: ()| {
        let text = input.peek().trim().to_string();
        if text.is_empty() || *busy.peek() || agent.peek().is_none() {
            return;
        }
        input.set(String::new());
        run_turn(ws, send_session.clone(), text);
    });

    let save = move |_| {
        let Some(a) = agent.peek().clone() else {
            return;
        };
        spawn(async move {
            let Some(folder) = folder_of(ws) else {
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

    // Session select: live sessions, then the folder's saved ones not loaded.
    let live: Vec<(String, String, bool)> = sessions
        .read()
        .iter()
        .map(|s| (s.id.clone(), s.title.read().clone(), *s.busy.read()))
        .collect();
    let stored: Vec<SavedSummary> = saved
        .read()
        .iter()
        .filter(|s| !live.iter().any(|(id, _, _)| *id == s.id))
        .cloned()
        .collect();
    let running = live.iter().filter(|(_, _, b)| *b).count();
    let agents: Vec<String> = ws
        .settings
        .read()
        .agents
        .iter()
        .map(|a| a.name.clone())
        .collect();
    let profile_now = profile();
    let sid = session_id.clone();
    let mut pick_session = move |v: String| {
        if v.is_empty() {
            return;
        }
        if let Some(rest) = v.strip_prefix("saved:") {
            let id = rest.to_string();
            if let Some(f) = folder_of(ws) {
                spawn(restore(ws, chats, f.descriptor.id.clone(), id));
            }
        } else if chats.session(&v).is_some() {
            current.set(Some(v));
        }
    };
    let close_session = move |_| {
        let id = sid.clone();
        if let Some(s) = chats.session(&id) {
            if *s.busy.peek() {
                return;
            }
        }
        sessions.with_mut(|v| v.retain(|s| s.id != id));
        let next = sessions.peek().last().map(|s| s.id.clone());
        current.set(next);
    };

    let pending_now = pending();
    let audit_entries = audit();

    rsx! {
        moonkale_ext_api::Stylesheet { href: CSS }
        div { class: "mk-agent", "data-session": "{session_id}",
            div { class: "mk-agent-toolbar",
                select { class: "mk-agent-sessions", title: "Sessions: live ones, then the folder's saved ones",
                    value: "{session_id}",
                    onchange: move |e| pick_session(e.value()),
                    for (id, title, b) in live.iter() {
                        option { key: "{id}", value: "{id}", selected: *id == session_id, if *b { "● " } "{title}" }
                    }
                    if !stored.is_empty() {
                        optgroup { label: "saved in this folder",
                            for s in stored.iter() {
                                option { key: "saved:{s.id}", value: "saved:{s.id}", "{s.title} · {s.profile}" }
                            }
                        }
                    }
                }
                button { class: "mk-btn mk-agent-new", title: "A new session (the current one keeps running)", onclick: move |_| { let profile = ws.settings.peek().agent.default.clone(); new_session(ws, chats, profile); }, "New" }
                select { class: "mk-agent-profile", title: "Which saved agent this session runs (Settings → Agents)",
                    value: "{profile_now}",
                    disabled: busy(),
                    onchange: {
                        let s = session.clone();
                        move |e| { profile.set(e.value()); connect(ws, s.clone()); }
                    },
                    for name in agents.iter() {
                        option { key: "{name}", value: "{name}", selected: *name == profile_now, "{name}" }
                    }
                    if !agents.contains(&profile_now) {
                        option { value: "{profile_now}", selected: true, "{profile_now} (missing)" }
                    }
                }
                span { class: "mk-agent-provider", title: "Provider · model", "{provider_label}" }
                if running > 1 { span { class: "mk-agent-running-count", title: "Sessions with a turn in flight", "{running} running" } }
                span { class: "mk-agent-spacer" }
                button { class: if show_activity() { "mk-btn mk-btn-on" } else { "mk-btn" }, onclick: move |_| show_activity.toggle(), title: "Every tool call with its policy decision", "Activity" }
                button { class: "mk-btn", disabled: items().is_empty() || busy(), onclick: save, title: "Save this conversation as a markdown page in the folder (.moonkale/chats/)", "Save" }
                button { class: "mk-btn", disabled: items().is_empty() || busy(), onclick: clear, "Clear" }
                button { class: "mk-btn mk-agent-close", disabled: busy() || live.len() <= 1, onclick: close_session, title: "Close this session (it stays saved in the folder)", "×" }
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
                        "Ask about the open folder or databases. The agent can list sources, browse the graph, read files, run read-only queries and search. Writes ask for your approval. Sessions run side by side: New starts another while this one works; finished turns are saved in the folder."
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
                    id: INPUT_ID,
                    class: "mk-agent-input",
                    rows: 2,
                    placeholder: if busy() { "This session is working — New starts another one meanwhile" } else if agent().is_some() { "Ask the agent… (Enter to send, Shift+Enter for a new line)" } else { "Connecting to the provider…" },
                    disabled: agent().is_none(),
                    value: "{input}",
                    oninput: move |e| input.set(e.value()),
                    onkeydown: move |e: KeyboardEvent| {
                        if e.key() == Key::Enter && !e.modifiers().shift() {
                            e.prevent_default();
                            send.call(());
                        }
                    },
                }
                button { class: "mk-btn", disabled: busy() || agent().is_none(), onclick: move |_| send.call(()), if busy() { "…" } else { "Send" } }
            }
        }
    }
}
