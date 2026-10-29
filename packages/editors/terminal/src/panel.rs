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
    pub frame: Signal<u64>,
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
            frame: Signal::new_in_scope(0, ScopeId::ROOT),
        }
    }
}

impl Sessions {
    /// Add a running session: its output is pumped into a relay for as long
    /// as the session lives — not for as long as a view of it is mounted
    /// (#12: docking the panel elsewhere ended the output for good).
    pub(crate) fn preview(self) -> Option<String> {
        let active = (*self.active.peek())?;
        let relay = self.relays.peek().get(&active)?.clone();
        let private = self
            .list
            .peek()
            .iter()
            .find(|s| s.borrow().id == active)?
            .borrow()
            .backend
            .private_input();
        relay.borrow_mut().set_private(private);
        let text = relay.borrow().preview();
        Some(text)
    }
    fn add(mut self, session: Session) {
        let id = session.id;
        let session = Rc::new(RefCell::new(session));
        let relay = Rc::new(RefCell::new(Relay::new()));
        let output = session.borrow_mut().backend.take_output();
        if let Some(mut out) = output {
            let relay = relay.clone();
            let weak = Rc::downgrade(&session);
            // Ends when the session is closed: dropping the backend closes the stream.
            moonkale_ext_api::crash::spawn_forever(async move {
                while let Some(chunk) = out.next().await {
                    let Some(session) = weak.upgrade() else {
                        break;
                    };
                    let private = session.borrow().backend.private_input();
                    let mut relay = relay.borrow_mut();
                    relay.set_private(private);
                    relay.push(&chunk);
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

    pub fn toggle_sharing(mut self) {
        if let Some(relay) = self
            .active
            .peek()
            .and_then(|id| self.relays.peek().get(&id).cloned())
        {
            relay.borrow_mut().toggle_sharing();
        }
        self.frame += 1;
    }

    pub fn private(self) -> bool {
        self.active
            .peek()
            .and_then(|id| {
                self.list
                    .peek()
                    .iter()
                    .find(|s| s.borrow().id == id)
                    .cloned()
            })
            .is_some_and(|s| s.borrow().backend.private_input())
    }

    pub fn paused(self) -> bool {
        self.active
            .peek()
            .and_then(|id| self.relays.peek().get(&id).cloned())
            .is_some_and(|r| r.borrow().sharing_paused())
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

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum FromJs {
    Tick,
    Paste { data: String },
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
const timer = setInterval(() => dioxus.send({kind: "tick"}), 100);
el.addEventListener("paste", (e) => {
    e.preventDefault(); e.stopImmediatePropagation();
    const bytes = new TextEncoder().encode((e.clipboardData?.getData("text/plain") || "").slice(0,65536));
    let raw = ""; for (const b of bytes) raw += String.fromCharCode(b);
    dioxus.send({kind: "paste", data: btoa(raw)});
}, true);
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
    else if (msg.kind === "destroy") { clearInterval(timer); x.destroy(el); break; }
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
    moonkale_ext_api::crash::use_effect(move || {
        let (seq, cmd) = *ws.shell.commands.read();
        let mut handled = sessions.handled;
        if seq <= *handled.peek() {
            return;
        }
        handled.set(seq);
        // The frame resolves `NewTerminal` to an implementation (Milestone 12).
        if cmd == Some(Command::NewTerminalIn("xterm")) {
            let cwd = ws.processes.terminal_cwd.peek().clone();
            moonkale_ext_api::crash::spawn(start_session(ws, sessions, cwd));
        }
    });
    // Sessions started elsewhere — the `ssh` of a remote folder (Milestone
    // 11) — become tabs here, so their prompts can be answered.
    moonkale_ext_api::crash::use_effect(move || {
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
    let _ = (sessions.frame)();

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
                                onclick: moonkale_ext_api::crash::event(move |_| sessions.active.set(Some(id))),
                                "{title}"
                                span { class: "mk-term-close", title: t!(ws, L, "terminal-close"), onclick: moonkale_ext_api::crash::event(move |e: dioxus::prelude::MouseEvent| {
                                    e.stop_propagation();
                                    sessions.close(id);
                                }), "✕" }
                            }
                        }
                    }
                }
                moonkale_ext_api::icons::ToolbarActions {
                    primary: rsx! { button { class: "mk-term-tab mk-term-new", title: t!(ws, L, "terminal-new"), disabled: !available,
                    onclick: moonkale_ext_api::crash::event(move |_| { ws.processes.terminal_cwd.set(None); moonkale_ext_api::crash::spawn(start_session(ws, sessions, None)); }),
                    "+"
                } },

                if active.is_some() {
                    button { class: "mk-term-tab mk-term-trace", title: t!(ws, L, "terminal-trace-title"),
                        onclick: moonkale_ext_api::crash::event(move |_| { let mut t = sessions.trace_tick; t += 1; }),
                        {t!(ws, L, "terminal-trace")}
                    }
                }
                button { class: "mk-term-sharing", aria_pressed: sessions.paused(),
                    onclick: moonkale_ext_api::crash::event(move |_| sessions.toggle_sharing()),
                    if sessions.paused() { {t!(ws, L, "terminal-resume-sharing")} } else { {t!(ws, L, "terminal-pause-sharing")} }
                }
                span { class: "mk-term-spacer" }
                moonkale_ext_api::icons::PanelSettingsButton {}

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
                                SessionView { key: "{id.0}", ws, session: s.clone(), relay, visible: active == Some(id), trace_tick: sessions.trace_tick, frame: sessions.frame }
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
    frame: Signal<u64>,
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
        mut frame,
    } = props;
    let id = session.borrow().id;
    let element_id = format!("mk-term-{}", id.0);
    let mut eval: Signal<Option<Eval>> = use_signal(|| None);
    let mut private = use_signal(|| session.borrow().backend.private_input());
    let mut paste: Signal<Option<Vec<u8>>> = use_signal(|| None);
    let session_paste = session.clone();
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
            let relay_in = relay.clone();
            // JS → Rust: keystrokes, resizes, links.
            moonkale_ext_api::crash::spawn(async move {
                loop {
                    match rx.recv::<FromJs>().await {
                        Ok(FromJs::Tick) => {
                            let value = session_in.borrow().backend.private_input();
                            if *private.peek() != value {
                                if !value {
                                    paste.set(None);
                                }
                                private.set(value);
                                frame += 1;
                            }
                        }
                        Ok(FromJs::Paste { data }) => {
                            if let Ok(bytes) =
                                base64::engine::general_purpose::STANDARD.decode(data)
                            {
                                if session_in.borrow().backend.private_input() {
                                    paste.set(Some(bytes));
                                } else {
                                    session_in.borrow().backend.write(&bytes);
                                }
                            }
                        }
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
                                moonkale_ext_api::crash::spawn(open_link(ws, link.path));
                            }
                        }
                        Ok(FromJs::Text { text }) => {
                            if session_in.borrow().backend.private_input()
                                || relay_in.borrow().sharing_paused()
                            {
                                continue;
                            }
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
            moonkale_ext_api::crash::spawn(async move {
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
        moonkale_ext_api::crash::use_effect(move || {
            let tick = *trace_tick.read();
            if tick == *seen.peek() || !visible {
                return;
            }
            seen.set(tick);
            if !*private.peek() {
                if let Some(ev) = eval.peek().as_ref() {
                    let _ = ev.send(ToJs::Text);
                }
            }
        });
    }

    use_drop(move || {
        if let Some(ev) = eval.peek().as_ref() {
            let _ = ev.send(ToJs::Destroy);
        }
    });

    rsx! {
        if visible && private() {
            span { class: "mk-term-private", role: "status", aria_label: t!(ws, L, "terminal-password-prompt"), "🔒 " {t!(ws, L, "terminal-password-prompt")} }
        }
        if visible && paste.read().is_some() {
            div { class: "mk-term-confirm", role: "alertdialog", onkeydown: move |e: KeyboardEvent| { if e.key() == Key::Escape { e.stop_propagation(); e.prevent_default(); paste.set(None); } }, aria_label: t!(ws, L, "terminal-confirm-paste"),
                p { {t!(ws, L, "terminal-confirm-paste")} }
                button { class: "mk-btn", onclick: move |_| { let bytes = paste.write().take(); if let Some(bytes) = bytes { let session = session_paste.borrow(); if session.backend.private_input() { session.backend.write(&bytes); } } }, {t!(ws, L, "terminal-paste")} }
                button { class: "mk-btn", onmounted: move |event: MountedEvent| { moonkale_ext_api::crash::spawn(async move { let _ = event.data().set_focus(true).await; }); }, onclick: move |_| paste.set(None), {t!(ws, L, "terminal-cancel")} }
            }
        }
        div {
            id: "{element_id}",
            class: "mk-term-session",
            class: if !visible { "mk-term-hidden" },
            onmounted: moonkale_ext_api::crash::event(mount),
            onclick: moonkale_ext_api::crash::event(move |_| { if let Some(ev) = eval.peek().as_ref() { let _ = ev.send(ToJs::Focus); } }),
        }
    }
}

/// Ctrl+click on `path:line`: open the file through the workspace.
async fn open_link(ws: Workspace, path: String) {
    let _ = ws.open_relative_path(&path).await;
}

/// The content stays a snapshot, but privacy can revoke it while Overview is open.
#[component]
pub(crate) fn PrivatePreview(ws: Workspace, sessions: Sessions) -> Element {
    let mut text = use_signal(|| sessions.preview().unwrap_or_default());
    let mut mode = use_signal(|| (sessions.private(), sessions.paused()));
    moonkale_ext_api::crash::use_future(move || async move {
        loop {
            futures_timer::Delay::new(std::time::Duration::from_millis(100)).await;
            let next = (sessions.private(), sessions.paused());
            if next != *mode.peek() {
                text.set(String::new());
                mode.set(next);
            }
        }
    });
    let (private, paused) = mode();
    let body = if private {
        t!(ws, L, "terminal-password-prompt")
    } else if paused {
        t!(ws, L, "terminal-sharing-paused")
    } else {
        text()
    };
    rsx! { {moonkale_ext_api::preview::text(&body)} }
}
