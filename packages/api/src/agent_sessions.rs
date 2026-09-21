//! Agent sessions that live on the server (Milestone 12, Prompt20): a turn
//! runs to completion whether or not a page, a desktop or a phone is
//! watching, and any client that connects later reads the transcript as
//! it stands. One record per session, in memory and appended to
//! `<folder>/.moonkale/agent-sessions/<id>.jsonl`.
//!
//! The turn runs `moonkale_llm::Agent::send` on a thread of its own (the
//! host futures are not `Send`) with a **server tool host**: the read-only
//! tools the MCP endpoint already implements over the registry; anything
//! mutating waits for an approval from *some* client (`agent_approve`),
//! `policy.allow_writes` auto-approves, and ten minutes without an answer
//! is a decline. Claude Code as the provider brings its own tools, so for
//! it the host is never asked.

use dioxus::prelude::*;

pub use moonkale_llm::sessions::{
    PendingApproval, SessionItem, SessionState, SessionSummary, TurnSettings,
};

#[cfg(feature = "server")]
mod server {
    use super::*;
    use moonkale_llm::{Agent, AgentEvent, Class, Decision, ToolCall, ToolHost};
    use std::collections::HashMap;
    use std::sync::mpsc as std_mpsc;
    use std::sync::{Arc, Mutex, OnceLock};

    pub struct Record {
        pub summary: SessionSummary,
        pub provider_label: String,
        pub items: Vec<SessionItem>,
        pub messages: Vec<moonkale_llm::Message>,
        pub pending: Option<(PendingApproval, std_mpsc::Sender<bool>)>,
    }

    type Store = Mutex<HashMap<String, Arc<Mutex<Record>>>>;
    static SESSIONS: OnceLock<Store> = OnceLock::new();

    pub fn store() -> &'static Store {
        SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
    }

    fn now() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    fn log_path(folder: &str, id: &str) -> std::path::PathBuf {
        std::path::Path::new(folder)
            .join(".moonkale/agent-sessions")
            .join(format!("{id}.jsonl"))
    }

    fn append_log(folder: &str, id: &str, item: &SessionItem) {
        let path = log_path(folder, id);
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            use std::io::Write;
            let _ = writeln!(f, "{}", serde_json::to_string(item).unwrap_or_default());
        }
    }

    /// Sessions of a folder from disk (after a restart) into the store.
    pub fn load_folder(folder: &str) {
        let dir = std::path::Path::new(folder).join(".moonkale/agent-sessions");
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return;
        };
        let mut store = store().lock().unwrap();
        for e in entries.flatten() {
            let path = e.path();
            let Some(id) = path
                .file_stem()
                .and_then(|s| s.to_str())
                .map(str::to_string)
            else {
                continue;
            };
            if store.contains_key(&id) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let items: Vec<SessionItem> = text
                .lines()
                .filter_map(|l| serde_json::from_str(l).ok())
                .collect();
            let title = items
                .iter()
                .find_map(|i| match i {
                    SessionItem::User { text } => Some(text.chars().take(60).collect()),
                    _ => None,
                })
                .unwrap_or_else(|| "chat".into());
            let started = e
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let messages = rebuild_messages(&items);
            store.insert(
                id.clone(),
                Arc::new(Mutex::new(Record {
                    summary: SessionSummary {
                        id,
                        folder: folder.to_string(),
                        title,
                        started,
                        running: false,
                        items: items.len(),
                    },
                    provider_label: String::new(),
                    items,
                    messages,
                    pending: None,
                })),
            );
        }
    }

    /// The model-facing history from the transcript (tool exchanges are
    /// folded into text: enough for the next turn's context).
    fn rebuild_messages(items: &[SessionItem]) -> Vec<moonkale_llm::Message> {
        let mut out = Vec::new();
        for i in items {
            match i {
                SessionItem::User { text } => out.push(moonkale_llm::Message::user(text.clone())),
                SessionItem::Assistant { text } if !text.is_empty() => {
                    out.push(moonkale_llm::Message::assistant(vec![
                        moonkale_llm::Content::Text { text: text.clone() },
                    ]))
                }
                _ => {}
            }
        }
        out
    }

    pub fn new_record(folder: &str, first: &str) -> (String, Arc<Mutex<Record>>) {
        let id = uuid::Uuid::new_v4().simple().to_string();
        let rec = Arc::new(Mutex::new(Record {
            summary: SessionSummary {
                id: id.clone(),
                folder: folder.to_string(),
                title: first.chars().take(60).collect(),
                started: now(),
                running: false,
                items: 0,
            },
            provider_label: String::new(),
            items: Vec::new(),
            messages: Vec::new(),
            pending: None,
        }));
        store().lock().unwrap().insert(id.clone(), rec.clone());
        (id, rec)
    }

    fn push(rec: &Arc<Mutex<Record>>, item: SessionItem) {
        let mut r = rec.lock().unwrap();
        let (folder, id) = (r.summary.folder.clone(), r.summary.id.clone());
        r.items.push(item.clone());
        r.summary.items = r.items.len();
        drop(r);
        append_log(&folder, &id, &item);
    }

    /// The server's tool host: read-only tools over the registry; writes
    /// need an approval from a client.
    struct ServerHost {
        rec: Arc<Mutex<Record>>,
        allow_writes: bool,
    }

    impl ToolHost for ServerHost {
        fn call(&self, call: ToolCall) -> moonkale_llm::agent::HostFuture<Result<String, String>> {
            Box::pin(async move {
                match call.name.as_str() {
                    "workspace.list_sources" | "graph.query" | "graph.fetch" | "index.search"
                    | "source.text_query" => crate::mcp::run(call).await,
                    other => Err(format!(
                        "{other} is not available in a server session (read-only tools only; edits need a desktop or the Claude Code provider)"
                    )),
                }
            })
        }

        fn approve(&self, call: ToolCall, class: Class) -> moonkale_llm::agent::HostFuture<bool> {
            let rec = self.rec.clone();
            let allow = self.allow_writes;
            Box::pin(async move {
                if allow {
                    return true;
                }
                let (tx, rx) = std_mpsc::channel::<bool>();
                rec.lock().unwrap().pending = Some((
                    PendingApproval {
                        call_id: call.id.clone(),
                        name: call.name.clone(),
                        input: call.input.clone(),
                        class,
                    },
                    tx,
                ));
                // Poll the std channel from the async loop (no client may ever answer).
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(600);
                let answer = loop {
                    match rx.try_recv() {
                        Ok(v) => break v,
                        Err(std_mpsc::TryRecvError::Disconnected) => break false,
                        Err(std_mpsc::TryRecvError::Empty) => {
                            if std::time::Instant::now() > deadline {
                                break false;
                            }
                            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                        }
                    }
                };
                rec.lock().unwrap().pending = None;
                answer
            })
        }
    }

    /// Run one turn on its own thread; returns at once.
    pub fn start_turn(rec: Arc<Mutex<Record>>, text: String, settings: TurnSettings) {
        push(&rec, SessionItem::User { text: text.clone() });
        {
            let mut r = rec.lock().unwrap();
            r.summary.running = true;
        }
        std::thread::Builder::new()
            .name("moonkale-agent-session".into())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("tokio");
                let local = tokio::task::LocalSet::new();
                local.block_on(&rt, run_turn(rec, text, settings));
            })
            .ok();
    }

    async fn run_turn(rec: Arc<Mutex<Record>>, text: String, settings: TurnSettings) {
        let Some(provider) = crate::llm::provider_for(&settings.llm) else {
            push(
                &rec,
                SessionItem::Error {
                    text: "no provider for these settings on the server".into(),
                },
            );
            rec.lock().unwrap().summary.running = false;
            return;
        };
        let (folder, messages) = {
            let r = rec.lock().unwrap();
            (r.summary.folder.clone(), r.messages.clone())
        };
        {
            let mut r = rec.lock().unwrap();
            r.provider_label = format!("{} · {}", provider.name(), provider.model());
        }
        let mut agent = Agent::new(provider, server_system_prompt(&folder));
        agent.messages = messages;
        agent.cwd = Some(folder.clone());
        agent.policy.mutating = if settings.allow_writes {
            Decision::Allow
        } else {
            Decision::Ask
        };
        agent.policy.denied_tools = settings.denied_tools.clone();
        let host = ServerHost {
            rec: rec.clone(),
            allow_writes: settings.allow_writes,
        };
        let rec2 = rec.clone();
        let mut on_event = |ev: AgentEvent| match ev {
            AgentEvent::TextDelta(t) => {
                let mut r = rec2.lock().unwrap();
                match r.items.last_mut() {
                    Some(SessionItem::Assistant { text }) => text.push_str(&t),
                    _ => {
                        r.items.push(SessionItem::Assistant { text: t });
                        r.summary.items = r.items.len();
                    }
                }
            }
            AgentEvent::ToolCall {
                id,
                name,
                input,
                class,
                decision,
            } => push(
                &rec2,
                SessionItem::Tool {
                    id,
                    name,
                    input,
                    class,
                    decision,
                    outcome: None,
                    summary: String::new(),
                },
            ),
            AgentEvent::ToolResult {
                id,
                outcome,
                summary,
            } => {
                let mut r = rec2.lock().unwrap();
                if let Some(SessionItem::Tool {
                    outcome: o,
                    summary: s,
                    ..
                }) = r
                    .items
                    .iter_mut()
                    .rev()
                    .find(|i| matches!(i, SessionItem::Tool { id: tid, .. } if *tid == id))
                {
                    *o = Some(outcome);
                    *s = summary;
                }
            }
            AgentEvent::TurnDone { .. } => {
                // The assistant text so far is final: log it, start a new bubble later.
                let r = rec2.lock().unwrap();
                if let Some(SessionItem::Assistant { text }) = r.items.last() {
                    let item = SessionItem::Assistant { text: text.clone() };
                    let (folder, id) = (r.summary.folder.clone(), r.summary.id.clone());
                    drop(r);
                    append_log(&folder, &id, &item);
                }
            }
            AgentEvent::Finished => {}
            AgentEvent::Error(e) => push(&rec2, SessionItem::Error { text: e }),
        };
        // The user text is already in the transcript; `send` pushes it into the history itself.
        agent.send(text, &host, &mut on_event).await;
        let mut r = rec.lock().unwrap();
        r.messages = agent.messages.clone();
        r.summary.running = false;
        r.pending = None;
        // Drop an empty trailing bubble.
        if matches!(r.items.last(), Some(SessionItem::Assistant { text }) if text.is_empty()) {
            r.items.pop();
            r.summary.items = r.items.len();
        }
    }

    fn server_system_prompt(folder: &str) -> String {
        format!(
            "You are Moonkale's assistant, running on the server for the folder {folder}. \
             Tools: workspace.list_sources, graph.query, graph.fetch, index.search, source.text_query (read-only). \
             Answer concisely; cite file paths."
        )
    }
}

/// Sessions of `folder` (blank = the root), newest first.
#[post("/api/agent/list")]
pub async fn agent_list(folder: String) -> Result<Vec<SessionSummary>, ServerFnError> {
    let folder =
        crate::state::jail_dir(Some(&folder)).map_err(|e| ServerFnError::new(e.to_string()))?;
    server::load_folder(&folder);
    let mut out: Vec<SessionSummary> = server::store()
        .lock()
        .unwrap()
        .values()
        .map(|r| r.lock().unwrap().summary.clone())
        .filter(|s| s.folder == folder)
        .collect();
    out.sort_by_key(|s| std::cmp::Reverse(s.started));
    Ok(out)
}

/// Send a message: into `session`, or a new one for `folder`. Returns the
/// session id at once; the turn runs on the server.
#[post("/api/agent/send")]
pub async fn agent_send(
    session: Option<String>,
    folder: String,
    text: String,
    settings: TurnSettings,
) -> Result<String, ServerFnError> {
    let folder =
        crate::state::jail_dir(Some(&folder)).map_err(|e| ServerFnError::new(e.to_string()))?;
    let (id, rec) = match session.filter(|s| !s.is_empty()) {
        Some(id) => {
            let rec = server::store()
                .lock()
                .unwrap()
                .get(&id)
                .cloned()
                .ok_or_else(|| ServerFnError::new(format!("no session {id}")))?;
            if rec.lock().unwrap().summary.running {
                return Err(ServerFnError::new("this session is still running a turn"));
            }
            (id, rec)
        }
        None => server::new_record(&folder, &text),
    };
    tracing::info!(target: "moonkale::audit", "agent session {id}: turn started ({} chars)", text.len());
    server::start_turn(rec, text, settings);
    Ok(id)
}

/// The transcript from `since` on, and whether a turn is running.
#[post("/api/agent/events")]
pub async fn agent_events(session: String, since: usize) -> Result<SessionState, ServerFnError> {
    let rec = server::store()
        .lock()
        .unwrap()
        .get(&session)
        .cloned()
        .ok_or_else(|| ServerFnError::new(format!("no session {session}")))?;
    let r = rec.lock().unwrap();
    Ok(SessionState {
        id: r.summary.id.clone(),
        title: r.summary.title.clone(),
        provider: r.provider_label.clone(),
        running: r.summary.running,
        items: r.items.iter().skip(since).cloned().collect(),
        total: r.items.len(),
        pending: r.pending.as_ref().map(|(p, _)| p.clone()),
    })
}

/// Answer a pending approval.
#[post("/api/agent/approve")]
pub async fn agent_approve(
    session: String,
    call_id: String,
    allow: bool,
) -> Result<(), ServerFnError> {
    let rec = server::store()
        .lock()
        .unwrap()
        .get(&session)
        .cloned()
        .ok_or_else(|| ServerFnError::new(format!("no session {session}")))?;
    let mut r = rec.lock().unwrap();
    match r.pending.take() {
        Some((p, tx)) if p.call_id == call_id => {
            tracing::info!(target: "moonkale::audit", "agent session {session}: {} {}", if allow { "allowed" } else { "declined" }, p.name);
            let _ = tx.send(allow);
            Ok(())
        }
        other => {
            r.pending = other;
            Err(ServerFnError::new("no such pending approval"))
        }
    }
}
