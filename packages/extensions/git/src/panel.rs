//! The Changes panel and the diff view.

use dioxus::prelude::*;
use moonkale_ext_api::git::{Commit, GitRequest, GitResponse, StatusEntry};
use moonkale_ext_api::prelude::*;
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
    /// Bumped to ask for a refresh (commands, saves, file operations).
    pub epoch: Signal<u64>,
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
            epoch: Signal::new_in_scope(0, ScopeId::ROOT),
        }
    }

    pub fn bump(&self) {
        let mut e = self.epoch;
        e.with_mut(|v| *v += 1);
    }
}

async fn run(ws: Workspace, req: GitRequest) -> Result<GitResponse, String> {
    let git = ws.git().ok_or("git is not available on this platform")?;
    let root = ws.folder_root().ok_or("open a folder first")?;
    git(root, req).await
}

/// Fetch status + log and publish the decorations.
pub async fn refresh(mut ws: Workspace, mut state: GitState) {
    if ws.folder_root().is_none() {
        state.status.set(None);
        ws.vcs_status.set(HashMap::new());
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
            let map: HashMap<String, (char, char)> = entries
                .iter()
                .map(|e| (e.path.clone(), (e.index, e.worktree)))
                .collect();
            if *ws.vcs_status.peek() != map {
                ws.vcs_status.set(map);
            }
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
            ws.vcs_status.set(HashMap::new());
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
    ws.set_status(format!("History: {} commits", log.len()));
}

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
    let available = ws.git().is_some();

    // Refresh when the folder changes, when files or documents change, and
    // when asked (`epoch`).
    let mut seen: Signal<(u64, u64, u64, usize)> = use_signal(|| (u64::MAX, 0, 0, 0));
    use_effect(move || {
        let key = (
            *state.epoch.read(),
            *ws.fs_epoch.read(),
            *ws.graph_epoch.read(),
            ws.sources.read().len(),
        );
        if *seen.peek() == key {
            return;
        }
        seen.set(key);
        if available {
            spawn(async move { refresh(ws, state).await });
        }
    });

    let act = move |req: GitRequest| {
        spawn(async move {
            state.busy.set(true);
            match run(ws, req).await {
                Ok(GitResponse::Done(msg)) => ws.set_status(msg),
                Ok(GitResponse::Unavailable(m)) | Err(m) => ws.set_status(format!("git: {m}")),
                Ok(_) => {}
            }
            state.busy.set(false);
            refresh(ws, state).await;
        });
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
        ws.dispatch(Command::ShowPanel(Box::leak(
            format!("git-diff:{key}").into_boxed_str(),
        )));
        spawn(async move {
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
        document::Stylesheet { href: CSS }
        div { class: "mk-git",
            if !available {
                p { class: "mk-muted", "Git is not available on this platform." }
            } else {
                match status {
                    None => rsx! { p { class: "mk-muted", "Open a folder that is a git repository." } },
                    Some(Err(e)) => rsx! { p { class: "mk-muted", "{e}" } },
                    Some(Ok(st)) => {
                        let staged: Vec<StatusEntry> = st.entries.iter().filter(|e| e.staged()).cloned().collect();
                        let unstaged: Vec<StatusEntry> = st.entries.iter().filter(|e| e.unstaged()).cloned().collect();
                        let staged_paths: Vec<String> = staged.iter().map(|e| e.path.clone()).collect();
                        let unstaged_paths: Vec<String> = unstaged.iter().map(|e| e.path.clone()).collect();
                        rsx! {
                            div { class: "mk-git-head",
                                span { class: "mk-git-branch", title: "{st.upstream.clone().unwrap_or_default()}", "⎇ {st.branch.clone().unwrap_or_else(|| \"(detached)\".into())}" }
                                if st.ahead > 0 { span { class: "mk-muted", " ↑{st.ahead}" } }
                                if st.behind > 0 { span { class: "mk-muted", " ↓{st.behind}" } }
                                span { class: "mk-git-spacer" }
                                button { class: "mk-btn", disabled: busy, onclick: move |_| state.bump(), title: "Refresh", "↻" }
                                button { class: "mk-btn", disabled: busy || log.is_empty(), onclick: move |_| { spawn(async move { show_history(ws, state).await; }); }, title: "Draw the last commits in the Graph panel", "Graph" }
                            }
                            div { class: "mk-git-commit",
                                textarea { id: COMMIT_ID, class: "mk-input", rows: 2, placeholder: "Commit message (Ctrl+Enter)", value: "{message}",
                                    oninput: move |e| message.set(e.value()),
                                    onkeydown: move |e| {
                                        if e.key() == Key::Enter && (e.modifiers().ctrl() || e.modifiers().meta()) {
                                            e.prevent_default();
                                            let m = message.peek().clone();
                                            if !m.trim().is_empty() && !staged_paths.is_empty() { message.set(String::new()); act(GitRequest::Commit { message: m }); }
                                        }
                                        e.stop_propagation();
                                    },
                                }
                                button { class: "mk-btn mk-btn-on", disabled: busy || staged.is_empty() || message.read().trim().is_empty(),
                                    onclick: move |_| { let m = message.peek().clone(); message.set(String::new()); act(GitRequest::Commit { message: m }); },
                                    "Commit {staged.len()} staged"
                                }
                            }
                            div { class: "mk-git-group",
                                div { class: "mk-git-group-head",
                                    span { "Staged ({staged.len()})" }
                                    if !staged.is_empty() { button { class: "mk-btn mk-btn-mini", disabled: busy, onclick: { let p = staged_paths.clone(); move |_| act(GitRequest::Unstage { paths: p.clone() }) }, "unstage all" } }
                                }
                                for e in staged.iter() {
                                    {
                                        let (p1, p2) = (e.path.clone(), e.path.clone());
                                        let letter = e.index;
                                        rsx! {
                                            div { key: "s:{e.path}", class: "mk-git-entry", "data-status": "{letter}", title: "{status_word(letter)}",
                                                onclick: move |_| open_diff(p1.clone(), true),
                                                span { class: "mk-git-letter mk-git-{status_word(letter)}", "{letter}" }
                                                span { class: "mk-git-path", "{e.path}" }
                                                span { class: "mk-git-actions",
                                                    button { class: "mk-btn mk-btn-mini", disabled: busy, title: "Unstage", onclick: move |ev| { ev.stop_propagation(); act(GitRequest::Unstage { paths: vec![p2.clone()] }) }, "−" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            div { class: "mk-git-group",
                                div { class: "mk-git-group-head",
                                    span { "Changes ({unstaged.len()})" }
                                    if !unstaged.is_empty() { button { class: "mk-btn mk-btn-mini", disabled: busy, onclick: { let p = unstaged_paths.clone(); move |_| act(GitRequest::Stage { paths: p.clone() }) }, "stage all" } }
                                }
                                for e in unstaged.iter() {
                                    {
                                        let (p1, p2, p3) = (e.path.clone(), e.path.clone(), e.path.clone());
                                        let letter = if e.index == '?' { '?' } else { e.worktree };
                                        rsx! {
                                            div { key: "w:{e.path}", class: "mk-git-entry", "data-status": "{letter}", title: "{status_word(letter)}",
                                                onclick: move |_| open_diff(p1.clone(), false),
                                                span { class: "mk-git-letter mk-git-{status_word(letter)}", "{letter}" }
                                                span { class: "mk-git-path", "{e.path}" }
                                                span { class: "mk-git-actions",
                                                    button { class: "mk-btn mk-btn-mini", disabled: busy, title: "Stage", onclick: move |ev| { ev.stop_propagation(); act(GitRequest::Stage { paths: vec![p2.clone()] }) }, "+" }
                                                    button { class: "mk-btn mk-btn-mini mk-btn-danger", disabled: busy, title: "Discard changes (untracked files are deleted)", onclick: move |ev| { ev.stop_propagation(); act(GitRequest::Discard { path: p3.clone() }) }, "✕" }
                                                }
                                            }
                                        }
                                    }
                                }
                                if st.entries.is_empty() { p { class: "mk-muted", "Working tree clean." } }
                            }
                            div { class: "mk-git-group mk-git-log",
                                div { class: "mk-git-group-head", span { "Log" } }
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
        return rsx! { div { class: "mk-git-diff", p { class: "mk-muted", "Closed." } } };
    };
    let path = view.path.clone();
    rsx! {
        document::Stylesheet { href: CSS }
        div { class: "mk-git-diff",
            div { class: "mk-git-diff-head",
                span { class: "mk-git-path", "{view.path}" }
                span { class: "mk-muted", if view.staged { " staged vs HEAD" } else { " working tree vs index" } }
                span { class: "mk-git-spacer" }
                button { class: "mk-btn", onclick: move |_| {
                    let p = path.clone();
                    spawn(async move { if let Ok(n) = ws.open_relative_path(&p).await { let _ = ws.reveal(n, 0, 0).await; } });
                }, "Open file" }
            }
            match &view.text {
                None => rsx! { p { class: "mk-muted", "Loading…" } },
                Some(Err(e)) => rsx! { p { class: "mk-explorer-error", "{e}" } },
                Some(Ok(t)) if t.trim().is_empty() => rsx! { p { class: "mk-muted", "No differences." } },
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
                                rsx! { span { key: "{i}", class: "mk-diff-line {class}", "{line}\n" } }
                            }
                        }
                    }
                },
            }
        }
    }
}
