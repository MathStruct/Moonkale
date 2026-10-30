//! The Changes panel and the diff view.

use crate::types::{
    Commit, ConflictFile, GitRequest, GitResponse, StatusEntry, SyncAction, SyncPlan,
};
use crate::L;
use dioxus::prelude::*;
use moonkale_ext_api::prelude::*;
use moonkale_ext_api::{t, FileMark};
use std::collections::HashMap;

const CSS: Asset = asset!("/assets/git.css");
pub const COMMIT_ID: &str = "mk-git-commit-message";

#[derive(Clone, PartialEq)]
pub struct DiffView {
    pub path: String,
    pub staged: bool,
    pub text: Option<Result<String, String>>,
}

impl DiffView {
    pub fn key(&self) -> String {
        format!("{}{}", if self.staged { "s:" } else { "w:" }, self.path)
    }
}

#[derive(Clone, PartialEq)]
pub struct Status {
    pub branch: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub entries: Vec<StatusEntry>,
}

/// Root-scope signals shared by the panels.
#[derive(Clone, Copy)]
pub struct GitState {
    pub status: Signal<Option<Result<Status, String>>>,
    pub log: Signal<Vec<Commit>>,
    pub diffs: Signal<Vec<DiffView>>,
    pub busy: Signal<bool>,
    pub output: Signal<String>,
    pub plan: Signal<Option<(SyncAction, SyncPlan)>>,
    pub conflicts: Signal<Vec<ConflictFile>>,
    pub operation: Signal<Option<String>>,
    /// Bumped to ask for a refresh (commands, saves, file operations).
    pub epoch: Signal<u64>,
    pub everyday: Signal<Option<crate::everyday_panel::Tab>>,
    pub discard: Signal<Option<String>>,
}

impl PartialEq for GitState {
    fn eq(&self, o: &Self) -> bool {
        self.status == o.status && self.log == o.log && self.diffs == o.diffs
    }
}

impl GitState {
    pub fn new() -> Self {
        Self {
            status: Signal::new_in_scope(None, ScopeId::ROOT),
            log: Signal::new_in_scope(Vec::new(), ScopeId::ROOT),
            diffs: Signal::new_in_scope(Vec::new(), ScopeId::ROOT),
            busy: Signal::new_in_scope(false, ScopeId::ROOT),
            output: Signal::new_in_scope(String::new(), ScopeId::ROOT),
            plan: Signal::new_in_scope(None, ScopeId::ROOT),
            conflicts: Signal::new_in_scope(Vec::new(), ScopeId::ROOT),
            operation: Signal::new_in_scope(None, ScopeId::ROOT),
            discard: Signal::new_in_scope(None, ScopeId::ROOT),
            everyday: Signal::new_in_scope(None, ScopeId::ROOT),
            epoch: Signal::new_in_scope(0, ScopeId::ROOT),
        }
    }

    pub fn bump(&self) {
        let mut e = self.epoch;
        e.with_mut(|v| *v += 1);
    }
}

pub(super) async fn run(ws: Workspace, req: GitRequest) -> Result<GitResponse, String> {
    let git = ws
        .service::<crate::GitRunner>()
        .map(|r| r.0)
        .ok_or_else(|| t!(ws, L, "git-unavailable"))?;
    let root = ws
        .folder_root()
        .ok_or_else(|| t!(ws, L, "git-open-folder"))?;
    git(root, req).await
}

/// Fetch status + log and publish the decorations.
pub async fn refresh(mut ws: Workspace, mut state: GitState) {
    if ws.folder_root().is_none() {
        state.status.set(None);
        ws.contrib.file_marks.set(HashMap::new());
        return;
    }
    match run(ws, GitRequest::Status).await {
        Ok(GitResponse::Status {
            branch,
            upstream,
            ahead,
            behind,
            entries,
        }) => {
            let map: HashMap<String, FileMark> = entries
                .iter()
                .filter_map(|e| mark(e.index, e.worktree).map(|m| (e.path.clone(), m)))
                .collect();
            if *ws.contrib.file_marks.peek() != map {
                ws.contrib.file_marks.set(map);
            }
            // An index diff ceases to exist when its last staged change is
            // removed. Retire its tab instead of leaving an empty staged view
            // in front of the working-tree diff for the same path.
            state.diffs.with_mut(|diffs| {
                diffs.retain(|diff| {
                    !diff.staged
                        || entries
                            .iter()
                            .any(|entry| entry.path == diff.path && entry.staged())
                });
            });
            state.status.set(Some(Ok(Status {
                branch,
                upstream,
                ahead,
                behind,
                entries,
            })));
        }
        Ok(GitResponse::Unavailable(msg)) => {
            state.status.set(Some(Err(msg)));
            ws.contrib.file_marks.set(HashMap::new());
            return;
        }
        Ok(_) => {}
        Err(e) => {
            state.status.set(Some(Err(e)));
            return;
        }
    }
    if let Ok(GitResponse::Log(log)) = run(ws, GitRequest::Log { limit: 30 }).await {
        state.log.set(log);
    }
    match run(ws, GitRequest::Conflicts).await {
        Ok(GitResponse::Conflicts { operation, files }) => {
            state.operation.set(operation);
            state.conflicts.set(files);
        }
        Err(e) => state.output.set(e),
        _ => {}
    }
    // Open diff views follow the working tree.
    let open: Vec<DiffView> = state.diffs.peek().clone();
    for d in open {
        if let Ok(GitResponse::Diff(text)) = run(
            ws,
            GitRequest::Diff {
                path: d.path.clone(),
                staged: d.staged,
            },
        )
        .await
        {
            state.diffs.with_mut(|v| {
                if let Some(x) = v.iter_mut().find(|x| x.key() == d.key()) {
                    x.text = Some(Ok(text));
                }
            });
        }
    }
}

pub fn act(mut ws: Workspace, mut state: GitState, req: GitRequest) {
    // Refresh may retire the originating diff tab. Finish the operation and
    // release busy even if its component unmounts during that refresh.
    moonkale_ext_api::crash::spawn_forever(async move {
        if *state.busy.peek() {
            return;
        }
        state.busy.set(true);
        state.output.set(t!(ws, L, "git-working"));
        let root = ws.folder_root();
        let action_root = root.clone();
        moonkale_ext_api::crash::spawn(async move {
            loop {
                #[cfg(target_arch = "wasm32")]
                gloo_timers::future::TimeoutFuture::new(250).await;
                #[cfg(not(target_arch = "wasm32"))]
                futures_timer::Delay::new(std::time::Duration::from_millis(250)).await;
                if !*state.busy.peek() || ws.folder_root() != root {
                    break;
                }
                if let Ok(GitResponse::Diff(text)) = run(ws, GitRequest::Progress).await {
                    if *state.busy.peek() && !text.is_empty() {
                        state.output.set(text);
                    }
                }
            }
        });
        let sync_action = match &req {
            GitRequest::Sync { action, .. } => Some(*action),
            _ => None,
        };
        let is_commit = matches!(req, GitRequest::Commit { .. });
        let message = match &req {
            GitRequest::Commit { message } => message.clone(),
            _ => String::new(),
        };
        let response = run(ws, req).await;
        if ws.folder_root() != action_root {
            state.plan.set(None);
            state.busy.set(false);
            refresh(ws, state).await;
            return;
        }
        match response {
            Ok(GitResponse::Done(msg)) => {
                state.output.set(msg.clone());
                ws.set_status(msg.clone());
                if is_commit {
                    // `[main abc1234] subject` → checkpoint in the entity log (Milestone 8).
                    let hash = msg
                        .split_once(']')
                        .and_then(|(head, _)| head.rsplit(' ').next())
                        .unwrap_or("")
                        .to_string();
                    ws.record(moonkale_core::EventKind::Checkpoint {
                        commit: hash,
                        message: message.lines().next().unwrap_or("").to_string(),
                    });
                }
            }
            Ok(GitResponse::SyncPlan(p)) => {
                state.output.set(String::new());
                state.plan.set(sync_action.map(|a| (a, p)));
            }
            Ok(GitResponse::Unavailable(m)) | Err(m) => {
                state.output.set(m.clone());
                ws.set_status(t!(ws, L, "git-error", error = m))
            }
            Ok(_) => {}
        }
        refresh(ws, state).await;
        state.busy.set(false);
    });
}

/// Draw the last commits in the Graph panel.
pub async fn show_history(mut ws: Workspace, state: GitState) {
    let log = match run(ws, GitRequest::Log { limit: 200 }).await {
        Ok(GitResponse::Log(l)) => l,
        Ok(GitResponse::Unavailable(m)) | Err(m) => {
            ws.set_status(m);
            return;
        }
        Ok(_) => return,
    };
    let _ = state;
    let Some(root) = ws.folder_root() else { return };
    let unique = ws.next_unique();
    ws.add_source(std::sync::Arc::new(crate::history::GitHistorySource::new(
        &root, &log, unique,
    )));
    ws.dispatch(Command::ShowPanel("graph"));
    ws.set_status(t!(ws, L, "git-history-status", n = log.len()));
}

/// The status letter as a word in the user's language (tooltips).
fn status_label(ws: Workspace, c: char) -> String {
    match c {
        'M' => t!(ws, L, "git-modified"),
        'A' => t!(ws, L, "git-added"),
        'D' => t!(ws, L, "git-deleted"),
        'R' => t!(ws, L, "git-renamed"),
        'C' => t!(ws, L, "git-copied"),
        '?' => t!(ws, L, "git-untracked"),
        'U' => t!(ws, L, "git-conflict"),
        _ => String::new(),
    }
}

/// The status letter as a CSS class suffix.
fn status_word(c: char) -> &'static str {
    match c {
        'M' => "modified",
        'A' => "added",
        'D' => "deleted",
        'R' => "renamed",
        'C' => "copied",
        '?' => "untracked",
        'U' => "conflict",
        _ => "",
    }
}

#[component]
pub fn ChangesPanel(ws: Workspace, state: GitState) -> Element {
    let mut state = state;
    let mut ws = ws;
    let mut message = use_signal(String::new);
    let available = ws.service::<crate::GitRunner>().is_some();

    // Refresh when the folder changes, when files or documents change, and
    // when asked (`epoch`).
    let mut seen_root: Signal<Option<String>> = use_signal(|| None);
    let mut seen: Signal<(u64, u64, u64, usize)> = use_signal(|| (u64::MAX, 0, 0, 0));
    moonkale_ext_api::crash::use_effect(move || {
        let root = ws.folder_root();
        if *seen_root.peek() != root {
            seen_root.set(root);
            state.plan.set(None);
            state.discard.set(None);
            state.everyday.set(None);
            ws.contrib.gutter_marks.with_mut(|m| m.clear());
            state.conflicts.set(Vec::new());
            state.operation.set(None);
            state.output.set(String::new());
        }
        let key = (
            *state.epoch.read(),
            *ws.sources.fs_epoch.read(),
            *ws.sources.graph_epoch.read(),
            ws.sources.open.read().len(),
        );
        if *seen.peek() == key {
            return;
        }
        seen.set(key);
        if available {
            moonkale_ext_api::crash::spawn(async move { refresh(ws, state).await });
        }
    });

    let mut act = move |req: GitRequest| {
        if let GitRequest::Discard { path } = req {
            state.discard.set(Some(path));
        } else {
            act(ws, state, req);
        }
    };
    let mut open_diff = move |path: String, staged: bool| {
        let view = DiffView {
            path: path.clone(),
            staged,
            text: None,
        };
        let key = view.key();
        if !state.diffs.peek().iter().any(|d| d.key() == key) {
            state.diffs.with_mut(|v| v.push(view));
        }
        ws.show_panel(&format!("git-diff:{key}"));
        moonkale_ext_api::crash::spawn(async move {
            let res = match run(ws, GitRequest::Diff { path, staged }).await {
                Ok(GitResponse::Diff(t)) => Ok(t),
                Ok(GitResponse::Unavailable(m)) | Err(m) => Err(m),
                Ok(_) => Err("unexpected reply".into()),
            };
            state.diffs.with_mut(|v| {
                if let Some(d) = v.iter_mut().find(|d| d.key() == key) {
                    d.text = Some(res);
                }
            });
        });
    };

    let status = state.status.read().clone();
    let busy = *state.busy.read();
    let log = state.log.read().clone();

    rsx! {
        moonkale_ext_api::Stylesheet { href: CSS }
        div { class: "mk-git",
            if !available {
                p { class: "mk-muted", {t!(ws, L, "git-unavailable-panel")} }
            } else {
                match status {
                    None => rsx! { p { class: "mk-muted", {t!(ws, L, "git-no-repo")} } },
                    Some(Err(e)) => rsx! { p { class: "mk-muted", "{e}" } },
                    Some(Ok(st)) => {
                        let staged: Vec<StatusEntry> = st.entries.iter().filter(|e| e.staged()).cloned().collect();
                        let unstaged: Vec<StatusEntry> = st.entries.iter().filter(|e| e.unstaged()).cloned().collect();
                        let staged_paths: Vec<String> = staged.iter().map(|e| e.path.clone()).collect();
                        let unstaged_paths: Vec<String> = unstaged.iter().map(|e| e.path.clone()).collect();
                        rsx! {
                            div { class: "mk-git-head",
                                span { class: "mk-git-branch", title: "{st.upstream.clone().unwrap_or_default()}", "⎇ " {st.branch.clone().unwrap_or_else(|| t!(ws, L, "git-detached"))} }
                                if st.ahead > 0 { span { class: "mk-muted", " ↑{st.ahead}" } }
                                if st.behind > 0 { span { class: "mk-muted", " ↓{st.behind}" } }
                                span { class: "mk-git-spacer" }
                                moonkale_ext_api::icons::ToolbarActions {
                    primary: rsx! { button { class: "mk-btn", disabled: busy, onclick: moonkale_ext_api::crash::event(move |_| act(GitRequest::Sync { action: SyncAction::Sync, expected: None })), id: "mk-git-sync", "data-command": "git.sync", "data-shortcut": moonkale_ext_api::command_hint("git.sync", "").shortcut, title: moonkale_ext_api::command_hint("git.sync", t!(ws, L, "git-sync")).title, {t!(ws, L, "git-sync")} } },
                                for (id, label, tab) in crate::everyday_panel::actions(ws) {
                                    button { key: "{id}", class: "mk-btn", "data-command": id, "data-shortcut": moonkale_ext_api::command_hint(id, "").shortcut, title: moonkale_ext_api::command_hint(id, label.clone()).title,
                                        onclick: moonkale_ext_api::crash::event(move |_| state.everyday.set(Some(tab))), "{label}"
                                    }
                                }
                                for (id, label, action) in sync_actions(ws) {
                                    button { key: "{id}", class: "mk-btn", disabled: busy, "data-command": "{id}", "data-shortcut": moonkale_ext_api::command_hint(id, "").shortcut, title: moonkale_ext_api::command_hint(id, label.clone()).title,
                                        onclick: moonkale_ext_api::crash::event(move |_| act(GitRequest::Sync { action, expected: None })), "{label}"
                                    }
                                }
                                button { class: "mk-btn", disabled: busy, onclick: moonkale_ext_api::crash::event(move |_| state.bump()), title: moonkale_ext_api::command_hint("git.refresh", t!(ws, L, "git-refresh")).title, "data-command": "git.refresh", "data-shortcut": moonkale_ext_api::command_hint("git.refresh", "").shortcut, "↻" },

                                button { class: "mk-btn", disabled: busy || log.is_empty(), onclick: moonkale_ext_api::crash::event(move |_| { moonkale_ext_api::crash::spawn(async move { show_history(ws, state).await; }); }), title: moonkale_ext_api::command_hint("git.history", t!(ws, L, "git-graph-title")).title, "data-command": "git.history", "data-shortcut": moonkale_ext_api::command_hint("git.history", "").shortcut, {t!(ws, L, "git-graph")} }

                }
            }
                            if let Some(path) = state.discard.read().clone() {
                                div { class: "mk-git-discard-review", role: "group",
                                    p { {t!(ws,L,"git-discard-confirm",path=path.clone())} }
                                    button { id: "mk-git-discard-confirm", class: "mk-btn mk-btn-danger",
                                        onmounted: moonkale_ext_api::crash::event(move |e:Event<MountedData>| {let data=e.data();moonkale_ext_api::crash::spawn(async move {let _=data.set_focus(true).await;});}),
                                        onclick: moonkale_ext_api::crash::event(move |_| {state.discard.set(None);self::act(ws,state,GitRequest::Discard{path:path.clone()});}), {t!(ws,L,"git-confirm")} }
                                    button {class:"mk-btn", onclick: moonkale_ext_api::crash::event(move |_|state.discard.set(None)),{t!(ws,L,"git-cancel")} }
                                }
                            }
                            SyncView { ws, state }
                            if let Some(tab) = *state.everyday.read() {
                                crate::everyday_panel::EverydayPanel { key: "{tab:?}", ws, state, tab }
                            }
                            div { class: "mk-git-commit",
                                textarea { id: COMMIT_ID, class: "mk-input", rows: 2, placeholder: t!(ws, L, "git-message"), value: "{message}",
                                    oninput: moonkale_ext_api::crash::event(move |e: dioxus::prelude::FormEvent| message.set(e.value())),
                                    onkeydown: moonkale_ext_api::crash::event(move |e: dioxus::prelude::KeyboardEvent| {
                                        if e.key() == Key::Enter && moonkale_ext_api::keys::primary(&e.modifiers()) {
                                            e.prevent_default();
                                            let m = message.peek().clone();
                                            if !m.trim().is_empty() && !staged_paths.is_empty() { message.set(String::new()); act(GitRequest::Commit { message: m }); }
                                        }
                                        e.stop_propagation();
                                    }),
                                }
                                button { class: "mk-btn mk-btn-on", disabled: busy || staged.is_empty() || message.read().trim().is_empty(),
                                    onclick: moonkale_ext_api::crash::event(move |_| { let m = message.peek().clone(); message.set(String::new()); act(GitRequest::Commit { message: m }); }),
                                    {t!(ws, L, "git-commit-staged", n = staged.len())}
                                }
                            }
                            div { class: "mk-git-group",
                                div { class: "mk-git-group-head",
                                    span { {t!(ws, L, "git-staged", n = staged.len())} }
                                    if !staged.is_empty() { button { class: "mk-btn mk-btn-mini", disabled: busy, onclick: moonkale_ext_api::crash::event({ let p = staged_paths.clone(); move |_| act(GitRequest::Unstage { paths: p.clone() }) }), {t!(ws, L, "git-unstage-all")} } }
                                }
                                for e in staged.iter() {
                                    {
                                        let (p1, p2) = (e.path.clone(), e.path.clone());
                                        let letter = e.index;
                                        rsx! {
                                            div { key: "s:{e.path}", class: "mk-git-entry", role: "button", tabindex: 0, "data-mode": "staged", "data-path": "{e.path}", "data-status": "{letter}", title: status_label(ws, letter),
                                                onkeydown: moonkale_ext_api::crash::event({let p=p1.clone();move |ev:KeyboardEvent| {if ev.key()==Key::Enter || ev.key()==Key::Character(" ".into()) {ev.prevent_default();open_diff(p.clone(),true);}}}),
                                                onclick: moonkale_ext_api::crash::event(move |_| open_diff(p1.clone(), true)),
                                                span { class: "mk-git-letter mk-git-{status_word(letter)}", "{letter}" }
                                                span { class: "mk-git-path", "{e.path}" }
                                                span { class: "mk-git-actions", onkeydown: moonkale_ext_api::crash::event(move |e:KeyboardEvent|{if e.key()==Key::Enter || e.key()==Key::Character(" ".into()){e.stop_propagation();}}),
                                                    button { class: "mk-btn mk-btn-mini", disabled: busy, title: t!(ws, L, "git-unstage"), onclick: moonkale_ext_api::crash::event(move |ev: dioxus::prelude::MouseEvent| { ev.stop_propagation(); act(GitRequest::Unstage { paths: vec![p2.clone()] }) }), "−" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            div { class: "mk-git-group",
                                div { class: "mk-git-group-head",
                                    span { {t!(ws, L, "git-changes-n", n = unstaged.len())} }
                                    if !unstaged.is_empty() { button { class: "mk-btn mk-btn-mini", disabled: busy, onclick: moonkale_ext_api::crash::event({ let p = unstaged_paths.clone(); move |_| act(GitRequest::Stage { paths: p.clone() }) }), {t!(ws, L, "git-stage-all")} } }
                                }
                                for e in unstaged.iter() {
                                    {
                                        let (p1, p2, p3) = (e.path.clone(), e.path.clone(), e.path.clone());
                                        let letter = if e.index == '?' { '?' } else { e.worktree };
                                        rsx! {
                                            div { key: "w:{e.path}", class: "mk-git-entry", role: "button", tabindex: 0, "data-mode": "worktree", "data-path": "{e.path}", "data-status": "{letter}", title: status_label(ws, letter),
                                                onkeydown: moonkale_ext_api::crash::event({let p=p1.clone();move |ev:KeyboardEvent| {if ev.key()==Key::Enter || ev.key()==Key::Character(" ".into()) {ev.prevent_default();open_diff(p.clone(),false);}}}),
                                                onclick: moonkale_ext_api::crash::event(move |_| open_diff(p1.clone(), false)),
                                                span { class: "mk-git-letter mk-git-{status_word(letter)}", "{letter}" }
                                                span { class: "mk-git-path", "{e.path}" }
                                                span { class: "mk-git-actions", onkeydown: moonkale_ext_api::crash::event(move |e:KeyboardEvent|{if e.key()==Key::Enter || e.key()==Key::Character(" ".into()){e.stop_propagation();}}),
                                                    button { class: "mk-btn mk-btn-mini", disabled: busy, title: t!(ws, L, "git-stage"), onclick: moonkale_ext_api::crash::event(move |ev: dioxus::prelude::MouseEvent| { ev.stop_propagation(); act(GitRequest::Stage { paths: vec![p2.clone()] }) }), "+" }
                                                    button { class: "mk-btn mk-btn-mini mk-btn-danger", disabled: busy, title: t!(ws, L, "git-discard"), onclick: moonkale_ext_api::crash::event(move |ev: dioxus::prelude::MouseEvent| { ev.stop_propagation(); act(GitRequest::Discard { path: p3.clone() }) }), "✕" }
                                                }
                                            }
                                        }
                                    }
                                }
                                if st.entries.is_empty() { p { class: "mk-muted", {t!(ws, L, "git-clean")} } }
                            }
                            div { class: "mk-git-group mk-git-log",
                                div { class: "mk-git-group-head", span { {t!(ws, L, "git-log")} } }
                                for c in log.iter() {
                                    div { key: "{c.hash}", class: "mk-git-commit-row", title: "{c.hash}\n{c.author} · {c.date}\n{c.files.join(\"\\n\")}",
                                        span { class: "mk-git-hash", "{c.short}" }
                                        span { class: "mk-git-subject", "{c.subject}" }
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
pub fn DiffPanel(ws: Workspace, state: GitState, view_key: String) -> Element {
    let view = state
        .diffs
        .read()
        .iter()
        .find(|d| d.key() == view_key)
        .cloned();
    let Some(view) = view else {
        return rsx! { div { class: "mk-git-diff", p { class: "mk-muted", {t!(ws, L, "git-closed")} } } };
    };
    let path = view.path.clone();
    let shared_diff = std::rc::Rc::new(
        view.text
            .as_ref()
            .and_then(|r| r.as_ref().ok())
            .cloned()
            .unwrap_or_default(),
    );
    let mut hunk_rows = std::collections::HashMap::<usize, Vec<usize>>::new();
    let mut current_hunk = None;
    for (i, line) in shared_diff.lines().enumerate() {
        if line.starts_with("@@ ") {
            current_hunk = Some(i);
        } else if line.starts_with('+') || line.starts_with('-') {
            if let Some(hunk) = current_hunk {
                hunk_rows.entry(hunk).or_default().push(i);
            }
        }
    }
    rsx! {
        moonkale_ext_api::Stylesheet { href: CSS }
        div { class: "mk-git-diff", "data-path": "{view.path}", "data-mode": if view.staged { "staged" } else { "worktree" },
            div { class: "mk-git-diff-head",
                span { class: "mk-git-path", "{view.path}" }
                span { class: "mk-muted", if view.staged { {t!(ws, L, "git-staged-vs-head")} } else { {t!(ws, L, "git-worktree-vs-index")} } }
                span { class: "mk-git-spacer" }
                button { class: "mk-btn", onclick: moonkale_ext_api::crash::event(move |_| {
                    let p = path.clone();
                    moonkale_ext_api::crash::spawn(async move { if let Ok(n) = ws.open_relative_path(&p).await { let _ = ws.reveal(n, 0, 0).await; } });
                }), {t!(ws, L, "git-open-file")} }
            }
            match &view.text {
                None => rsx! { p { class: "mk-muted", {t!(ws, L, "git-loading")} } },
                Some(Err(e)) => rsx! { p { class: "mk-explorer-error", "{e}" } },
                Some(Ok(t)) if t.trim().is_empty() => rsx! { p { class: "mk-muted", {t!(ws, L, "git-no-diff")} } },
                Some(Ok(t)) => rsx! {
                    pre { class: "mk-git-diff-body",
                        for (i, line) in t.lines().enumerate() {
                            {
                                let class = if line.starts_with("+++") || line.starts_with("---") { "mk-diff-file" }
                                    else if line.starts_with('+') { "mk-diff-add" }
                                    else if line.starts_with('-') { "mk-diff-del" }
                                    else if line.starts_with("@@") { "mk-diff-hunk" }
                                    else if line.starts_with("diff ") || line.starts_with("index ") { "mk-diff-meta" }
                                    else { "mk-diff-ctx" };
                                let selected = if class == "mk-diff-hunk" { hunk_rows.get(&i).cloned().unwrap_or_default() } else { vec![i] };
                                let expected = shared_diff.clone();
                                let stage_path = view.path.clone();
                                let staged = view.staged;
                                rsx! { span { key: "{i}", class: "mk-diff-line {class}",
                                    if matches!(class, "mk-diff-hunk" | "mk-diff-add" | "mk-diff-del") {
                                        button { class: "mk-btn mk-git-stage-selection", "data-row": "{i}", "data-kind": if class == "mk-diff-hunk" { "hunk" } else { "line" },
                                            disabled: *state.busy.read(),
                                            aria_label: if view.staged { t!(ws,L,"git-unstage-selection") } else { t!(ws,L,"git-stage-selection") },
                                            title: if view.staged { t!(ws,L,"git-unstage-selection") } else { t!(ws,L,"git-stage-selection") },
                                            onclick: moonkale_ext_api::crash::event(move |_| act(ws,state,GitRequest::StageSelection{path:stage_path.clone(),staged,expected:expected.as_ref().clone(),lines:selected.clone()})),
                                            if view.staged { "−" } else { "+" }
                                        }
                                    }
                                    "{line}\n"
                                } }
                            }
                        }
                    }
                },
            }
        }
    }
}

/// Git's mark for a file from its index and worktree status letters (the
/// shell draws it; the classes are styled in the shell's stylesheet).
fn mark(index: char, worktree: char) -> Option<FileMark> {
    let c = if index == '?' {
        '?'
    } else if worktree != '.' {
        worktree
    } else {
        index
    };
    let (class, title) = match c {
        'M' => ("mk-vcs-modified", "modified"),
        'A' | '?' => ("mk-vcs-added", "added"),
        'D' => ("mk-vcs-deleted", "deleted"),
        'R' | 'C' => ("mk-vcs-renamed", "renamed"),
        'U' => ("mk-vcs-conflict", "conflict"),
        _ => return None,
    };
    Some(FileMark::new(c, class, title))
}

pub fn sync_actions(ws: Workspace) -> Vec<(&'static str, String, SyncAction)> {
    vec![
        ("git.fetch", t!(ws, L, "git-fetch"), SyncAction::Fetch),
        ("git.pull", t!(ws, L, "git-pull"), SyncAction::Pull),
        ("git.push", t!(ws, L, "git-push"), SyncAction::Push),
        (
            "git.push-lease",
            t!(ws, L, "git-push-lease"),
            SyncAction::PushLease,
        ),
        ("git.publish", t!(ws, L, "git-publish"), SyncAction::Publish),
    ]
}

#[component]
fn SyncView(ws: Workspace, mut state: GitState) -> Element {
    moonkale_ext_api::crash::use_effect(move || {
        if state.plan.read().is_some() && !*state.busy.read() {
            ws.focus_element("mk-git-confirm");
        }
    });
    let busy = *state.busy.read();
    let plan = state.plan.read().clone();
    let files = state.conflicts.read().clone();
    let operation = state.operation.read().clone();
    rsx! {
        div { class: "mk-git-sync", aria_busy: busy,
            if let Some((action, p)) = plan {
                div { class: "mk-git-plan", onkeydown: moonkale_ext_api::crash::event(move |e: KeyboardEvent| {
                    if e.key() == Key::Escape { e.prevent_default(); e.stop_propagation(); state.plan.set(None); ws.focus_element("mk-git-sync"); }
                }), role: "region", aria_label: t!(ws, L, "git-review"),
                    p { {t!(ws, L, "git-review")} " {p.branch} → {p.remote} ({p.target}) ↑{p.ahead} ↓{p.behind}" }
                    if p.dirty { p { {t!(ws, L, "git-dirty")} } }
                    if p.ahead > 0 && p.behind > 0 { p { {t!(ws, L, "git-diverged")} } }
                    if action == SyncAction::PushLease { p { {t!(ws, L, "git-lease-warning")} " {p.remote_oid}" } }
                    button { class: "mk-btn", disabled: busy, onmounted: move |_| ws.focus_element("mk-git-confirm"), id: "mk-git-confirm",
                        onclick: moonkale_ext_api::crash::event(move |_| { state.plan.set(None); act(ws, state, GitRequest::Sync { action, expected: Some(p.clone()) }); }), {t!(ws, L, "git-proceed")} }
                    button { class: "mk-btn", onclick: moonkale_ext_api::crash::event(move |_| { state.plan.set(None); ws.focus_element("mk-git-sync"); }), {t!(ws, L, "git-cancel")} }
                }
            }
            if busy { p { role: "status", {t!(ws, L, "git-working")} } }
            if !state.output.read().is_empty() { pre { class: "mk-git-output", role: "status", "{state.output}" } }
            if state.output.read().contains("rejected") || state.output.read().contains("non-fast-forward") {
                button { class: "mk-btn", disabled: busy, onclick: moonkale_ext_api::crash::event(move |_| act(ws, state, GitRequest::Sync { action: SyncAction::Pull, expected: None })), {t!(ws, L, "git-pull")} }
            }
            if let Some(op) = operation {
                p { {t!(ws, L, "git-operation")} " {op}" }
                if op == "rebase" { p { {t!(ws, L, "git-rebase-sides")} } }
                for file in files {
                    details { key: "{file.path}:{file.hunks}:{file.original}", open: true, class: "mk-git-conflict-view",
                        summary { "{file.path}" }
                        if file.can_delete {
                            button { class: "mk-btn", disabled: busy,
                                onclick: moonkale_ext_api::crash::event({ let file = file.clone(); move |_| act(ws, state, GitRequest::ResolveDeletion { file: file.clone() }) }), {t!(ws, L, "git-keep-deletion")}
                            }
                        }
                        crate::conflict::ThreeWayEditor { ws, base: file.base.clone(), ours: file.ours.clone(), theirs: file.theirs.clone(), hunks: file.hunks.clone(), original: file.original.clone(), busy,
                            onresolve: move |result| act(ws, state, GitRequest::Resolve { file: file.clone(), result })
                        }
                    }
                }
                button { class: "mk-btn", "data-command": "git.continue", "data-shortcut": moonkale_ext_api::command_hint("git.continue", "").shortcut, title: moonkale_ext_api::command_hint("git.continue", t!(ws, L, "git-continue")).title, disabled: busy || !state.conflicts.read().is_empty(), onclick: moonkale_ext_api::crash::event(move |_| act(ws, state, GitRequest::Continue)), {t!(ws, L, "git-continue")} }
                button { class: "mk-btn", "data-command": "git.abort", "data-shortcut": moonkale_ext_api::command_hint("git.abort", "").shortcut, title: moonkale_ext_api::command_hint("git.abort", t!(ws, L, "git-abort")).title, disabled: busy, onclick: moonkale_ext_api::crash::event(move |_| act(ws, state, GitRequest::Abort)), {t!(ws, L, "git-abort")} }
            }
        }
    }
}
