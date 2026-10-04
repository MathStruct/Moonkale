//! The panel: a tab strip of sessions and one xterm mount per session.

use crate::L;
use base64::Engine;
use dioxus::document::{self, Eval};
use dioxus::prelude::*;
use futures_util::StreamExt;
use moonkale_ext_api::{t, Command, Workspace};
use moonkale_terminal::{links, Relay, Session, SessionId};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

const CSS: Asset = asset!("/assets/terminal.css");
const XTERM_JS: Asset = asset!("/assets/xterm.js");
const XTERM_CSS: Asset = asset!("/assets/xterm.css");

/// Sessions shared by the extension and every mounted panel instance.
#[derive(Clone, Copy)]
pub struct Sessions {
    pub list: Signal<Vec<Rc<RefCell<Session>>>>,
    /// Each session's output, kept for whichever view mounts next (#12).
    relays: Signal<HashMap<SessionId, Rc<RefCell<Relay>>>>,
    pub active: Signal<Option<SessionId>>,
    /// Bumped by "Trace → Graph": the visible session sends its text.
    pub trace_tick: Signal<u64>,
    /// The last command handled: a remounted panel must not start the last
    /// New Terminal again (#12).
    handled: Signal<u64>,
}

impl PartialEq for Sessions {
    fn eq(&self, o: &Self) -> bool {
        self.list == o.list && self.active == o.active
    }
}

impl Sessions {
    pub fn new() -> Self {
        Self {
            list: Signal::new_in_scope(Vec::new(), ScopeId::ROOT),
            relays: Signal::new_in_scope(HashMap::new(), ScopeId::ROOT),
            active: Signal::new_in_scope(None, ScopeId::ROOT),
            trace_tick: Signal::new_in_scope(0, ScopeId::ROOT),
            handled: Signal::new_in_scope(0, ScopeId::ROOT),
        }
    }
}

impl Sessions {
    /// Add a running session: its output is pumped into a relay for as long
    /// as the session lives — not for as long as a view of it is mounted
    /// (#12: docking the panel elsewhere ended the output for good).
    fn add(mut self, session: Session) {
        let id = session.id;
        let session = Rc::new(RefCell::new(session));
        let relay = Rc::new(RefCell::new(Relay::new()));
        let output = session.borrow_mut().backend.take_output();
        if let Some(mut out) = output {
            let relay = relay.clone();
            // Ends when the session is closed: dropping the backend closes the stream.
            dioxus::core::spawn_forever(async move {
                while let Some(chunk) = out.next().await {
                    relay.borrow_mut().push(&chunk);
                }
                relay.borrow_mut().end(b"\r\n[process exited]\r\n");
            });
        }
        self.relays.with_mut(|m| {
            m.insert(id, relay);
        });
        self.list.with_mut(|l| l.push(session));
        self.active.set(Some(id));
    }

    fn close(mut self, id: SessionId) {
        self.list.with_mut(|l| l.retain(|s| s.borrow().id != id));
        self.relays.with_mut(|m| {
            m.remove(&id);
        });
        if self.active.peek().as_ref() == Some(&id) {
            let next = self.list.peek().last().map(|s| s.borrow().id);
            self.active.set(next);
        }
    }
}

impl Default for Sessions {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum ToJs<'a> {
    Output {
        data: &'a str,
    },
    Focus,
    /// Ask for the whole buffer (reply: `text`).
    Text,
    Destroy,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum FromJs {
    Ready { cols: u16, rows: u16 },
    Input { data: String },
    Resize { cols: u16, rows: u16 },
    Link { lines: Vec<String> },
    Text { text: String },
}

const SCRIPT: &str = r#"
await dioxus.recv();
const el = document.getElementById(ID);
if (!el) { return; }
while (!(window.moonkale && window.moonkale.xterm)) { await new Promise((r) => setTimeout(r, 20)); }
const x = window.moonkale.xterm;
const size = x.mount(el, {
    onData: (data) => dioxus.send({ kind: "input", data }),
    onResize: (cols, rows) => dioxus.send({ kind: "resize", cols, rows }),
});
el.addEventListener("click", (e) => {
    // Cmd+click on a Mac, Ctrl+click elsewhere (spec 027).
    if (!(/Mac|iPhone|iPad/.test(navigator.platform) ? e.metaKey : e.ctrlKey)) return;
    const lines = x.lineAt(el, e.clientY);
    if (lines) dioxus.send({ kind: "link", lines });
});
dioxus.send({ kind: "ready", cols: size.cols, rows: size.rows });
for (;;) {
    const msg = await dioxus.recv();
    if (msg.kind === "output") x.write(el, msg.data);
    else if (msg.kind === "focus") x.focus(el);
    else if (msg.kind === "text") dioxus.send({ kind: "text", text: x.allText(el) });
    else if (msg.kind === "destroy") { x.destroy(el); break; }
}
"#;

async fn start_session(mut ws: Workspace, sessions: Sessions, cwd: Option<String>) {
    let Some(spawn_fn) = ws.spawn_terminal() else {
        ws.set_status(t!(ws, L, "terminal-unavailable"));
        return;
    };
    match spawn_fn(cwd.clone(), 80, 24).await {
        Ok(backend) => {
            let id = SessionId::fresh();
            let title = backend.title();
            sessions.add(Session {
                id,
                title,
                cwd,
                backend,
            });
        }
        Err(e) => ws.set_status(t!(ws, L, "terminal-failed", error = e.to_string())),
    }
}

#[component]
pub fn TerminalPanel(ws: Workspace, sessions: Sessions) -> Element {
    let mut sessions = sessions;
    // View → New Terminal / "New terminal here".
    use_effect(move || {
        let (seq, cmd) = *ws.shell.commands.read();
        let mut handled = sessions.handled;
        if seq <= *handled.peek() {
            return;
        }
        handled.set(seq);
        // The frame resolves `NewTerminal` to an implementation (Milestone 12).
        if cmd == Some(Command::NewTerminalIn("xterm")) {
            let cwd = ws.processes.terminal_cwd.peek().clone();
            spawn(start_session(ws, sessions, cwd));
        }
    });
    // Sessions started elsewhere — the `ssh` of a remote folder (Milestone
    // 11) — become tabs here, so their prompts can be answered.
    use_effect(move || {
        let mut ws = ws;
        let pending: Vec<Session> = ws
            .processes
            .adopt_terminals
            .read()
            .iter()
            .filter_map(|s| s.borrow_mut().take())
            .collect();
        if pending.is_empty() {
            return;
        }
        ws.processes.adopt_terminals.with_mut(|v| v.clear());
        for session in pending {
            sessions.add(session);
        }
    });
    let list = sessions.list.read().clone();
    let active = *sessions.active.read();
    let available = ws.spawn_terminal().is_some();

    rsx! {
        moonkale_ext_api::Stylesheet { href: XTERM_CSS }
        moonkale_ext_api::Stylesheet { href: CSS }
        document::Script { src: XTERM_JS, defer: true }
        div { class: "mk-term",
            div { class: "mk-term-tabs",
                for s in list.iter() {
                    {
                        let (id, title, cwd) = { let s = s.borrow(); (s.id, s.title.clone(), s.cwd.clone()) };
                        rsx! {
                            button { key: "{id.0}",
                                class: if active == Some(id) { "mk-term-tab mk-term-tab-active" } else { "mk-term-tab" },
                                title: cwd.unwrap_or_default(),
                                onclick: move |_| sessions.active.set(Some(id)),
                                "{title}"
                                span { class: "mk-term-close", title: t!(ws, L, "terminal-close"), onclick: move |e| {
                                    e.stop_propagation();
                                    sessions.close(id);
                                }, "✕" }
                            }
                        }
                    }
                }
                button { class: "mk-term-tab mk-term-new", title: t!(ws, L, "terminal-new"), disabled: !available,
                    onclick: move |_| { ws.processes.terminal_cwd.set(None); spawn(start_session(ws, sessions, None)); },
                    "+"
                }
                if active.is_some() {
                    button { class: "mk-term-tab mk-term-trace", title: t!(ws, L, "terminal-trace-title"),
                        onclick: move |_| { let mut t = sessions.trace_tick; t += 1; },
                        {t!(ws, L, "terminal-trace")}
                    }
                }
            }
            div { class: "mk-term-body",
                if !available {
                    p { class: "mk-term-empty", "Terminals are not available on this platform." }
                } else if list.is_empty() {
                    p { class: "mk-term-empty", {t!(ws, L, "terminal-empty")} }
                }
                for s in list.iter() {
                    {
                        let id = s.borrow().id;
                        let relay = sessions.relays.peek().get(&id).cloned();
                        rsx! {
                            if let Some(relay) = relay {
                                SessionView { key: "{id.0}", ws, session: s.clone(), relay, visible: active == Some(id), trace_tick: sessions.trace_tick }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone)]
struct SessionViewProps {
    ws: Workspace,
    session: Rc<RefCell<Session>>,
    relay: Rc<RefCell<Relay>>,
    visible: bool,
    trace_tick: Signal<u64>,
}

impl PartialEq for SessionViewProps {
    fn eq(&self, o: &Self) -> bool {
        Rc::ptr_eq(&self.session, &o.session) && self.visible == o.visible
    }
}

#[component]
fn SessionView(props: SessionViewProps) -> Element {
    let SessionViewProps {
        ws,
        session,
        relay,
        visible,
        trace_tick,
    } = props;
    let id = session.borrow().id;
    let element_id = format!("mk-term-{}", id.0);
    let mut eval: Signal<Option<Eval>> = use_signal(|| None);
    let mount = {
        let session = session.clone();
        let element_id = element_id.clone();
        move |_| {
            if eval.peek().is_some() {
                return;
            }
            let script = SCRIPT.replace("ID", &serde_json::to_string(&element_id).unwrap());
            let ev = document::eval(&script);
            let mut rx = ev;
            // What this session printed so far (a remounted view repaints
            // from it), then the live output.
            let (replay, mut live) = relay.borrow_mut().subscribe();
            let session_in = session.clone();
            // JS → Rust: keystrokes, resizes, links.
            spawn(async move {
                loop {
                    match rx.recv::<FromJs>().await {
                        Ok(FromJs::Ready { cols, rows }) | Ok(FromJs::Resize { cols, rows }) => {
                            session_in.borrow().backend.resize(cols, rows)
                        }
                        Ok(FromJs::Input { data }) => {
                            if let Ok(bytes) =
                                base64::engine::general_purpose::STANDARD.decode(data)
                            {
                                session_in.borrow().backend.write(&bytes);
                            }
                        }
                        Ok(FromJs::Link { lines }) => {
                            // The clicked row first, then its neighbours.
                            if let Some(link) = lines.iter().flat_map(|l| links::find(l)).next() {
                                spawn(open_link(ws, link.path));
                            }
                        }
                        Ok(FromJs::Text { text }) => {
                            let mut ws = ws;
                            let traces = moonkale_trace::parse(&text);
                            if traces.is_empty() {
                                ws.set_status(t!(ws, L, "terminal-no-trace"));
                            } else {
                                // The newest trace is the interesting one.
                                let n = traces.len();
                                for t in traces {
                                    let unique = ws.next_unique();
                                    ws.add_source(std::sync::Arc::new(
                                        moonkale_trace::TraceSource::new(&t, unique),
                                    ));
                                }
                                ws.set_status(t!(ws, L, "terminal-drew", n = n));
                            }
                        }
                        Err(dioxus::document::EvalError::Serialization(_)) => continue,
                        Err(_) => break,
                    }
                }
            });
            // Backend → JS: process output, base64.
            let ev_out = ev;
            spawn(async move {
                let send = |chunk: &[u8]| {
                    let data = base64::engine::general_purpose::STANDARD.encode(chunk);
                    ev_out.send(ToJs::Output { data: &data }).is_ok()
                };
                if !replay.is_empty() && !send(&replay) {
                    return;
                }
                while let Some(chunk) = live.next().await {
                    if !send(&chunk) {
                        break;
                    }
                }
            });
            let _ = ev.send(serde_json::json!({ "kind": "init" }));
            eval.set(Some(ev));
        }
    };

    // "Trace → Graph" pressed: the visible session answers.
    {
        let mut seen = use_signal(|| 0u64);
        use_effect(move || {
            let tick = *trace_tick.read();
            if tick == *seen.peek() || !visible {
                return;
            }
            seen.set(tick);
            if let Some(ev) = eval.peek().as_ref() {
                let _ = ev.send(ToJs::Text);
            }
        });
    }

    use_drop(move || {
        if let Some(ev) = eval.peek().as_ref() {
            let _ = ev.send(ToJs::Destroy);
        }
    });

    rsx! {
        div {
            id: "{element_id}",
            class: "mk-term-session",
            class: if !visible { "mk-term-hidden" },
            onmounted: mount,
            onclick: move |_| { if let Some(ev) = eval.peek().as_ref() { let _ = ev.send(ToJs::Focus); } },
        }
    }
}

/// Ctrl+click on `path:line`: open the file through the workspace.
async fn open_link(ws: Workspace, path: String) {
    let _ = ws.open_relative_path(&path).await;
}
