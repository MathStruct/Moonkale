//! The panel: a tab strip of sessions and one Dioxus-rendered screen per
//! session. No JavaScript of ours: the screen is `vt100`, sizes come from
//! Dioxus's `MountedData::get_client_rect`, keys from `onkeydown`.

use crate::keys::encode;
use dioxus::html::geometry::WheelDelta;
use dioxus::prelude::*;
use moonkale_ext_api::prelude::*;
use moonkale_terminal::{links, Output, Session, SessionId, TerminalBackend};
use std::cell::RefCell;
use std::rc::Rc;

pub const PANEL_ID: &str = "terminal-native";
const CSS: Asset = asset!("/assets/terminal-native.css");
const SCROLLBACK: usize = 5_000;

/// A session with its screen.
pub struct NativeSession {
    pub id: SessionId,
    pub title: String,
    pub backend: Box<dyn TerminalBackend>,
    pub parser: vt100::Parser,
    /// Output stream, taken by the pump once.
    output: Option<Output>,
    /// Bumped per output chunk: the view re-renders.
    pub frame: Signal<u64>,
}

/// Sessions compare by id (a prop of `SessionView`).
impl PartialEq for NativeSession {
    fn eq(&self, o: &Self) -> bool {
        self.id == o.id
    }
}

impl NativeSession {
    pub fn new(session: Session) -> Self {
        let Session {
            id, title, backend, ..
        } = session;
        Self {
            id,
            title,
            backend,
            parser: vt100::Parser::new(24, 80, SCROLLBACK),
            output: None,
            frame: Signal::new_in_scope(0, ScopeId::ROOT),
        }
    }
}

/// Sessions shared by the extension and every mounted panel instance.
#[derive(Clone, Copy)]
pub struct Sessions {
    pub list: Signal<Vec<Rc<RefCell<NativeSession>>>>,
    pub active: Signal<Option<SessionId>>,
}

impl PartialEq for Sessions {
    fn eq(&self, o: &Self) -> bool {
        self.list == o.list && self.active == o.active
    }
}

pub struct NativeTerminalExtension {
    sessions: Sessions,
}

impl Default for NativeTerminalExtension {
    fn default() -> Self {
        Self::new()
    }
}

impl NativeTerminalExtension {
    pub fn new() -> Self {
        Self {
            sessions: Sessions {
                list: Signal::new_in_scope(Vec::new(), ScopeId::ROOT),
                active: Signal::new_in_scope(None, ScopeId::ROOT),
            },
        }
    }
}

impl Extension for NativeTerminalExtension {
    fn manifest(&self) -> Manifest {
        Manifest::opt_in(
            "dev.moonkale.editor-terminal-native",
            "Terminal (Rust)",
            "A terminal panel without JavaScript: a vt100 screen drawn by Dioxus. Same shells as the xterm.js panel; pick one under Settings → Terminal.",
        )
        .with_permissions(&["run-commands"])
    }

    fn panels(&self, _ws: Workspace) -> Vec<PanelContribution> {
        vec![PanelContribution {
            id: PANEL_ID.into(),
            title: "Terminal (Rust)".into(),
            home: PanelHome::Bottom,
            closable: true,
            dirty: false,
            node: None,
            activity: Some(
                Activity::new("terminal", 71, "Terminal (Rust)")
                    .badge(self.sessions.list.read().len() as u32),
            ),
        }]
    }

    fn render(&self, _panel_id: &str, ws: Workspace) -> Element {
        rsx! { NativeTerminalPanel { ws, sessions: self.sessions } }
    }

    // Milestone 13: the same chooser setting as the xterm.js panel.
    // Which terminal opens is Settings → Which extension (Milestone 15).
}

async fn start_session(mut ws: Workspace, mut sessions: Sessions, cwd: Option<String>) {
    let Some(spawn_fn) = ws.spawn_terminal() else {
        ws.set_status("Terminals are not available on this platform");
        return;
    };
    match spawn_fn(cwd.clone(), 80, 24).await {
        Ok(backend) => {
            let id = SessionId::fresh();
            let title = backend.title();
            let session = NativeSession::new(Session {
                id,
                title,
                cwd,
                backend,
            });
            sessions
                .list
                .with_mut(|l| l.push(Rc::new(RefCell::new(session))));
            sessions.active.set(Some(id));
        }
        Err(e) => ws.set_status(format!("Could not start a terminal: {e}")),
    }
}

#[component]
pub fn NativeTerminalPanel(ws: Workspace, sessions: Sessions) -> Element {
    let mut sessions = sessions;
    // View → New Terminal, routed to this implementation (Milestone 12).
    use_effect(move || {
        let (_, cmd) = *ws.commands.read();
        if cmd == Some(Command::NewTerminalIn("native")) {
            let cwd = ws.terminal_cwd.peek().clone();
            spawn(start_session(ws, sessions, cwd));
        }
    });
    let list = sessions.list.read().clone();
    let active = *sessions.active.read();
    let available = ws.spawn_terminal().is_some();
    rsx! {
        moonkale_ext_api::Stylesheet { href: CSS }
        div { class: "mk-tn",
            div { class: "mk-tn-tabs",
                for s in list.iter() {
                    {
                        let id = s.borrow().id;
                        let title = s.borrow().title.clone();
                        rsx! {
                            span { key: "{id.0}", class: if active == Some(id) { "mk-tn-tab mk-tn-tab-active" } else { "mk-tn-tab" },
                                onclick: move |_| sessions.active.set(Some(id)),
                                "{title}"
                                button { class: "mk-tn-close", title: "Close", onclick: move |e| {
                                    e.stop_propagation();
                                    sessions.list.with_mut(|l| l.retain(|s| s.borrow().id != id));
                                    if sessions.active.peek().as_ref() == Some(&id) {
                                        let next = sessions.list.peek().last().map(|s| s.borrow().id);
                                        sessions.active.set(next);
                                    }
                                }, "×" }
                            }
                        }
                    }
                }
                button { class: "mk-tn-new", disabled: !available, title: "New terminal (Rust renderer)",
                    onclick: move |_| { ws.terminal_cwd.set(None); spawn(start_session(ws, sessions, None)); }, "+" }
            }
            if list.is_empty() {
                p { class: "mk-muted mk-tn-empty", if available { "No terminal yet — press + or use View → New Terminal." } else { "Terminals are not available on this platform." } }
            }
            for s in list.iter() {
                {
                    let id = s.borrow().id;
                    rsx! { SessionView { key: "{id.0}", ws, session: s.clone(), visible: active == Some(id) } }
                }
            }
        }
    }
}

/// A cell run: text with one style.
#[derive(Clone, PartialEq)]
struct Run {
    text: String,
    class: String,
    style: String,
}

fn color_css(c: vt100::Color, fg: bool) -> (Option<u8>, Option<String>) {
    match c {
        vt100::Color::Default => (None, None),
        vt100::Color::Idx(i) if i < 16 => (Some(i), None),
        vt100::Color::Idx(i) => {
            let (r, g, b) = idx_rgb(i);
            (
                None,
                Some(format!(
                    "{}:rgb({r},{g},{b});",
                    if fg { "color" } else { "background" }
                )),
            )
        }
        vt100::Color::Rgb(r, g, b) => (
            None,
            Some(format!(
                "{}:rgb({r},{g},{b});",
                if fg { "color" } else { "background" }
            )),
        ),
    }
}

/// The xterm 256-colour cube and greys for indices 16..=255.
fn idx_rgb(i: u8) -> (u8, u8, u8) {
    if i >= 232 {
        let v = 8 + (i - 232) * 10;
        return (v, v, v);
    }
    let i = i.saturating_sub(16);
    let step = |n: u8| if n == 0 { 0 } else { 55 + n * 40 };
    (step(i / 36), step((i / 6) % 6), step(i % 6))
}

/// The visible screen as rows of runs; the cursor cell carries `mk-tn-cursor`.
fn rows_of(screen: &vt100::Screen, show_cursor: bool) -> Vec<Vec<Run>> {
    let (rows, cols) = screen.size();
    let (cr, cc) = screen.cursor_position();
    let cursor_on = show_cursor && !screen.hide_cursor() && screen.scrollback() == 0;
    let mut out = Vec::with_capacity(rows as usize);
    for r in 0..rows {
        let mut runs: Vec<Run> = Vec::new();
        let mut c = 0u16;
        while c < cols {
            let Some(cell) = screen.cell(r, c) else {
                break;
            };
            if cell.is_wide_continuation() {
                c += 1;
                continue;
            }
            let mut class = String::new();
            let mut style = String::new();
            let (fg_idx, fg_css) = color_css(cell.fgcolor(), true);
            let (bg_idx, bg_css) = color_css(cell.bgcolor(), false);
            if let Some(i) = fg_idx {
                class.push_str(&format!(" mk-tn-fg{i}"));
            }
            if let Some(i) = bg_idx {
                class.push_str(&format!(" mk-tn-bg{i}"));
            }
            if let Some(s) = fg_css {
                style.push_str(&s);
            }
            if let Some(s) = bg_css {
                style.push_str(&s);
            }
            if cell.bold() {
                class.push_str(" mk-tn-b");
            }
            if cell.dim() {
                class.push_str(" mk-tn-dim");
            }
            if cell.italic() {
                class.push_str(" mk-tn-i");
            }
            if cell.underline() {
                class.push_str(" mk-tn-u");
            }
            if cell.inverse() {
                class.push_str(" mk-tn-inv");
            }
            if cursor_on && r == cr && c == cc {
                class.push_str(" mk-tn-cursor");
            }
            let text = if cell.has_contents() {
                cell.contents().to_string()
            } else {
                " ".to_string()
            };
            let is_cursor = cursor_on && r == cr && c == cc;
            match runs.last_mut() {
                Some(last) if last.class == class && last.style == style && !is_cursor => {
                    last.text.push_str(&text)
                }
                _ => runs.push(Run { text, class, style }),
            }
            c += 1;
        }
        // Trailing blanks are not worth spans.
        while let Some(last) = runs.last() {
            if last.class.is_empty() && last.style.is_empty() && last.text.trim().is_empty() {
                runs.pop();
            } else {
                break;
            }
        }
        out.push(runs);
    }
    out
}

#[component]
fn SessionView(ws: Workspace, session: Rc<RefCell<NativeSession>>, visible: bool) -> Element {
    let frame = session.borrow().frame;
    let mut mounted: Signal<Option<Rc<MountedData>>> = use_signal(|| None);
    let mut cell_size: Signal<(f64, f64)> = use_signal(|| (8.0, 17.0));
    let mut probe: Signal<Option<Rc<MountedData>>> = use_signal(|| None);
    let mut focused = use_signal(|| false);

    // Output → screen; started once per session from `onmounted` (P-047).
    let pump = {
        let session = session.clone();
        move || {
            let taken = session.borrow_mut().output.take();
            let output = match taken {
                Some(o) => Some(o),
                None => session.borrow_mut().backend.take_output(),
            };
            let Some(mut output) = output else {
                return;
            };
            let session = session.clone();
            spawn(async move {
                use futures_util::StreamExt;
                while let Some(chunk) = output.next().await {
                    let mut s = session.borrow_mut();
                    s.parser.process(&chunk);
                    let mut f = s.frame;
                    drop(s);
                    f += 1;
                }
                let mut s = session.borrow_mut();
                s.parser.process(b"\r\n[process ended]\r\n");
                let mut f = s.frame;
                drop(s);
                f += 1;
            });
        }
    };

    // Measure the container and the cell probe → rows × cols.
    let measure = {
        let session = session.clone();
        move || {
            let (Some(m), Some(p)) = (mounted.peek().clone(), probe.peek().clone()) else {
                return;
            };
            let session = session.clone();
            spawn(async move {
                let (Ok(rect), Ok(pr)) = (m.get_client_rect().await, p.get_client_rect().await)
                else {
                    return;
                };
                let cw = (pr.size.width / 10.0).max(1.0);
                let ch = pr.size.height.max(1.0);
                cell_size.set((cw, ch));
                let cols = ((rect.size.width - 8.0) / cw).floor().max(10.0) as u16;
                let rows = ((rect.size.height - 8.0) / ch).floor().max(3.0) as u16;
                let mut s = session.borrow_mut();
                if s.parser.screen().size() != (rows, cols) {
                    s.parser.screen_mut().set_size(rows, cols);
                    s.backend.resize(cols, rows);
                    let mut f = s.frame;
                    drop(s);
                    f += 1;
                }
            });
        }
    };

    let _ = frame();
    let (rows, app_cursor) = {
        let s = session.borrow();
        (
            rows_of(s.parser.screen(), focused()),
            s.parser.screen().application_cursor(),
        )
    };
    let s_key = session.clone();
    let s_click = session.clone();
    let s_wheel = session.clone();
    let measure_mount = measure.clone();
    let measure_resize = measure.clone();
    let measure_tick = measure.clone();
    let measure_probe = measure;
    let (cw, ch) = cell_size();

    rsx! {
        div {
            class: if visible { "mk-tn-screen" } else { "mk-tn-screen mk-tn-hidden" },
            class: if focused() { "mk-tn-focused" },
            tabindex: 0,
            onmounted: move |e| {
                mounted.set(Some(e.data()));
                pump();
                measure_mount();
                // The webview does not always deliver `onresize` for a tile
                // that grows or shrinks later (P-112): measure on a timer too.
                let measure_tick = measure_tick.clone();
                spawn(async move {
                    loop {
                        crate::sleep_ms(600).await;
                        measure_tick();
                    }
                });
            },
            onresize: move |_| measure_resize(),
            onfocus: move |_| focused.set(true),
            onblur: move |_| focused.set(false),
            onkeydown: move |e: KeyboardEvent| {
                let m = e.modifiers();
                let key = e.key().to_string();
                // Leave the browser's own copy/paste and the app's palette alone.
                if m.ctrl() && m.shift() {
                    return;
                }
                // Control bytes come from Ctrl on every platform; Cmd on a Mac is
                // the app's (copy, palette), never a control character (spec 027).
                if m.meta() && !m.ctrl() {
                    return;
                }
                if let Some(bytes) = encode(&key, m.ctrl(), m.alt(), m.shift(), app_cursor) {
                    e.prevent_default();
                    e.stop_propagation();
                    let s = s_key.borrow();
                    s.backend.write(&bytes);
                    if s.parser.screen().scrollback() != 0 {
                        drop(s);
                        s_key.borrow_mut().parser.screen_mut().set_scrollback(0);
                    }
                }
            },
            onwheel: move |e: WheelEvent| {
                let dy = match e.delta() {
                    WheelDelta::Pixels(v) => v.y / ch,
                    WheelDelta::Lines(v) => v.y,
                    WheelDelta::Pages(v) => v.y * 20.0,
                };
                let lines = dy.round() as i64;
                if lines == 0 { return; }
                e.prevent_default();
                let mut s = s_wheel.borrow_mut();
                let cur = s.parser.screen().scrollback() as i64;
                let next = (cur - lines).clamp(0, SCROLLBACK as i64) as usize;
                s.parser.screen_mut().set_scrollback(next);
                let mut f = s.frame;
                drop(s);
                f += 1;
            },
            // Ten `M`s: the cell size is one tenth of this span's box.
            span { class: "mk-tn-probe", onmounted: move |e| { probe.set(Some(e.data())); measure_probe(); }, "MMMMMMMMMM" }
            div { class: "mk-tn-grid", style: "--mk-tn-cw: {cw}px; --mk-tn-ch: {ch}px;",
                for (r, runs) in rows.iter().enumerate() {
                    div { key: "{r}", class: "mk-tn-row",
                        // Ctrl+click on a row: open the first file link in it.
                        onclick: {
                            let s_click = s_click.clone();
                            move |e: MouseEvent| {
                                if !moonkale_ext_api::keys::primary(&e.modifiers()) { return; }
                                let text = {
                                    let s = s_click.borrow();
                                    let screen = s.parser.screen();
                                    let (_, cols) = screen.size();
                                    screen.contents_between(r as u16, 0, r as u16, cols)
                                };
                                if let Some(link) = links::find(&text).into_iter().next() {
                                    spawn(async move { let _ = ws.open_relative_path(&link.path).await; });
                                }
                            }
                        },
                        for (i, run) in runs.iter().enumerate() {
                            span { key: "{i}", class: "mk-tn-run{run.class}", style: "{run.style}", "{run.text}" }
                        }
                    }
                }
            }
        }
    }
}
