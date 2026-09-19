//! The panel: a tab strip of sessions and one xterm mount per session.

use base64::Engine;
use dioxus::document::{self, Eval};
use dioxus::prelude::*;
use futures_util::StreamExt;
use moonkale_ext_api::{Command, Workspace};
use moonkale_terminal::{links, Session, SessionId};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::rc::Rc;

const CSS: Asset = asset!("/assets/terminal.css");
const XTERM_JS: Asset = asset!("/assets/xterm.js");
const XTERM_CSS: Asset = asset!("/assets/xterm.css");

/// Sessions shared by the extension and every mounted panel instance.
#[derive(Clone, Copy)]
pub struct Sessions {
    pub list: Signal<Vec<Rc<RefCell<Session>>>>,
    pub active: Signal<Option<SessionId>>,
    /// Bumped by "Trace → Graph": the visible session sends its text.
    pub trace_tick: Signal<u64>,
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
            active: Signal::new_in_scope(None, ScopeId::ROOT),
            trace_tick: Signal::new_in_scope(0, ScopeId::ROOT),
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
    if (!e.ctrlKey && !e.metaKey) return;
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

async fn start_session(mut ws: Workspace, mut sessions: Sessions, cwd: Option<String>) {
    let Some(spawn_fn) = ws.spawn_terminal() else {
        ws.set_status("Terminals are not available on this platform");
        return;
    };
    match spawn_fn(cwd.clone(), 80, 24).await {
        Ok(backend) => {
            let id = SessionId::fresh();
            let title = backend.title();
            let session = Session {
                id,
                title,
                cwd,
                backend,
            };
            sessions
                .list
                .with_mut(|l| l.push(Rc::new(RefCell::new(session))));
            sessions.active.set(Some(id));
        }
        Err(e) => ws.set_status(format!("Could not start a terminal: {e}")),
    }
}

#[component]
pub fn TerminalPanel(ws: Workspace, sessions: Sessions) -> Element {
    let mut sessions = sessions;
    // View → New Terminal / "New terminal here".
    use_effect(move || {
        let (_, cmd) = *ws.commands.read();
        if cmd == Some(Command::NewTerminal) {
            let cwd = ws.terminal_cwd.peek().clone();
            spawn(start_session(ws, sessions, cwd));
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
                                span { class: "mk-term-close", title: "Close", onclick: move |e| {
                                    e.stop_propagation();
                                    sessions.list.with_mut(|l| l.retain(|s| s.borrow().id != id));
                                    if sessions.active.peek().as_ref() == Some(&id) {
                                        let next = sessions.list.peek().last().map(|s| s.borrow().id);
                                        sessions.active.set(next);
                                    }
                                }, "✕" }
                            }
                        }
                    }
                }
                button { class: "mk-term-tab mk-term-new", title: "New terminal", disabled: !available,
                    onclick: move |_| { ws.terminal_cwd.set(None); spawn(start_session(ws, sessions, None)); },
                    "+"
                }
                if active.is_some() {
                    button { class: "mk-term-tab mk-term-trace", title: "Parse the stack traces / compiler errors in this terminal and draw them in the Graph panel",
                        onclick: move |_| { let mut t = sessions.trace_tick; t += 1; },
                        "Trace → Graph"
                    }
                }
            }
            div { class: "mk-term-body",
                if !available {
                    p { class: "mk-term-empty", "Terminals are not available on this platform." }
                } else if list.is_empty() {
                    p { class: "mk-term-empty", "No terminal yet — press + or use View → New Terminal." }
                }
                for s in list.iter() {
                    {
                        let id = s.borrow().id;
                        rsx! {
                            SessionView { key: "{id.0}", ws, session: s.clone(), visible: active == Some(id), trace_tick: sessions.trace_tick }
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
            let output = session.borrow_mut().backend.take_output();
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
                                ws.set_status(
                                    "No stack trace or compiler error found in this terminal",
                                );
                            } else {
                                // The newest trace is the interesting one.
                                let n = traces.len();
                                for t in traces {
                                    let unique = ws.next_unique();
                                    ws.add_source(std::sync::Arc::new(
                                        moonkale_trace::TraceSource::new(&t, unique),
                                    ));
                                }
                                ws.set_status(format!(
                                    "Drew {n} trace(s); see the Graph panel's source picker"
                                ));
                            }
                        }
                        Err(dioxus::document::EvalError::Serialization(_)) => continue,
                        Err(_) => break,
                    }
                }
            });
            // Backend → JS: process output, base64.
            if let Some(mut out) = output {
                let ev_out = ev;
                spawn(async move {
                    while let Some(chunk) = out.next().await {
                        let data = base64::engine::general_purpose::STANDARD.encode(&chunk);
                        if ev_out.send(ToJs::Output { data: &data }).is_err() {
                            break;
                        }
                    }
                    let _ = ev_out.send(ToJs::Output {
                        data: &base64::engine::general_purpose::STANDARD
                            .encode(b"\r\n[process exited]\r\n"),
                    });
                });
            }
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
