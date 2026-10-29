//! Quiet secondary Git workflows, shared by overflow and palette.
use crate::everyday_types::*;
use crate::panel::{self, GitState};
use crate::types::{GitRequest, GitResponse};
use crate::L;
use dioxus::prelude::*;
use moonkale_ext_api::{t, Workspace};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Tab {
    Branches,
    Undo,
    Stash,
    Blame,
}
pub fn actions(ws: Workspace) -> Vec<(&'static str, String, Tab)> {
    vec![
        ("git.branches", t!(ws, L, "git-branches"), Tab::Branches),
        ("git.undo", t!(ws, L, "git-undo"), Tab::Undo),
        ("git.stash", t!(ws, L, "git-stash"), Tab::Stash),
        ("git.blame", t!(ws, L, "git-blame"), Tab::Blame),
    ]
}
#[component]
pub fn EverydayPanel(mut ws: Workspace, mut state: GitState, tab: Tab) -> Element {
    let mut data = use_signal(EverydayState::default);
    let mut name = use_signal(String::new);
    let mut branch = use_signal(String::new);
    let mut other = use_signal(String::new);
    let mut upstream = use_signal(String::new);
    let mut commit = use_signal(String::new);
    let mut stash = use_signal(String::new);
    let mut stash_first = use_signal(|| false);
    let mut untracked = use_signal(|| false);
    let mut mode = use_signal(|| ResetMode::Mixed);
    let mut path = use_signal(|| {
        ws.active_document()
            .map(|(_, d)| d)
            .map(|d| d.peek().node.native_key.clone())
            .unwrap_or_default()
    });
    let mut blame = use_signal(Vec::<BlameLine>::new);
    let mut review = use_signal(|| None::<(EverydayAction, EverydayPlan)>);
    let mut output = use_signal(String::new);
    let mut busy = use_signal(|| false);
    moonkale_ext_api::crash::use_effect(move || {
        let _ = state.epoch.read();
        moonkale_ext_api::crash::spawn(async move {
            match panel::run(ws, GitRequest::EverydayState).await {
                Ok(GitResponse::EverydayState(d)) => {
                    if !d.branches.iter().any(|r| r.name == *branch.peek()) {
                        branch.set(
                            d.branches
                                .first()
                                .map(|r| r.name.clone())
                                .unwrap_or_default(),
                        );
                    }
                    if !d.branches.iter().any(|r| r.name == *other.peek()) {
                        other.set(
                            d.branches
                                .last()
                                .map(|r| r.name.clone())
                                .unwrap_or_default(),
                        );
                    }
                    if !d.upstreams.contains(&upstream.peek()) {
                        upstream.set(d.upstreams.first().cloned().unwrap_or_default());
                    }
                    if !d.stashes.iter().any(|r| r.oid == *stash.peek()) {
                        stash.set(d.stashes.first().map(|s| s.oid.clone()).unwrap_or_default());
                    }
                    if !d.reflog.iter().any(|r| r.oid == *commit.peek()) {
                        commit.set(d.reflog.first().map(|s| s.oid.clone()).unwrap_or_default());
                    }
                    data.set(d);
                }
                Err(e) => output.set(e),
                _ => {}
            }
        });
    });
    moonkale_ext_api::crash::use_effect(move || {
        ws.focus_element(if review.read().is_some() {
            "mk-git-everyday-confirm"
        } else {
            "mk-git-everyday-first"
        });
    });
    let mut send = move |request: GitRequest| {
        if *busy.peek() {
            return;
        }
        busy.set(true);
        let action_root = ws.folder_root();
        let blame_doc = ws
            .active_document()
            .map(|(id, d)| (id, d.peek().version, d.peek().node.native_key.clone()));
        let live_blame = matches!(&request, GitRequest::Blame { commit: None, .. });
        let action = match &request {
            GitRequest::Everyday { action, .. } => Some(action.clone()),
            _ => None,
        };
        moonkale_ext_api::crash::spawn(async move {
            let response = panel::run(ws, request).await;
            if ws.folder_root() != action_root {
                busy.set(false);
                return;
            }
            match response {
                Ok(GitResponse::EverydayPlan(p)) => review.set(action.map(|a| (a, p))),
                Ok(GitResponse::Blame(rows)) => {
                    let marks = rows
                        .iter()
                        .map(|r| moonkale_ext_api::workspace::GutterMark {
                            line: r.line,
                            label: r.commit.chars().take(8).collect(),
                            title: format!("{}\n{}\n{}", r.commit, r.author, r.summary),
                        })
                        .collect();
                    if live_blame {
                        if let Some((id, doc)) = ws.active_document().filter(|(_, d)| {
                            d.peek().node.native_key == *path.peek()
                                && d.peek().node.source.as_str().strip_prefix("folder:")
                                    == action_root.as_deref()
                        }) {
                            if !doc.peek().dirty()
                                && blame_doc.as_ref().is_some_and(|(before, version, key)| {
                                    *before == id
                                        && *version == doc.peek().version
                                        && *key == *path.peek()
                                })
                            {
                                ws.contrib.gutter_marks.with_mut(|m| {
                                    m.insert(id, (doc.peek().version, marks));
                                });
                            }
                        }
                    }
                    blame.set(rows);
                    output.set(String::new());
                }
                Ok(GitResponse::Done(s)) => {
                    output.set(s);
                    review.set(None);
                    state.bump();
                }
                Ok(GitResponse::Diff(s)) => {
                    output.set(s);
                }
                Ok(_) => {}
                Err(e) => {
                    output.set(e);
                    review.set(None);
                    state.bump();
                }
            }
            busy.set(false);
        });
    };
    let mut act = move |action| {
        send(GitRequest::Everyday {
            action,
            expected: None,
        })
    };
    let content = data.read().clone();
    rsx! {
        section {class:"mk-git-everyday", "data-tab":"{tab:?}", aria_busy:*busy.read(),
            onkeydown:moonkale_ext_api::crash::event(move |e:KeyboardEvent| {if e.key()==Key::Escape {e.stop_propagation();e.prevent_default();review.set(None);state.everyday.set(None);ws.focus_element("mk-git-sync");}}),
            div {class:"mk-git-group-head",
                strong { {actions(ws).into_iter().find(|(_,_,t)|*t==tab).map(|(_,label,_)|label).unwrap_or_default()} }
                button {class:"mk-btn",onclick:moonkale_ext_api::crash::event(move |_| {state.everyday.set(None);ws.focus_element("mk-git-sync");}),{t!(ws,L,"git-close-tools")} }
            }
            if let Some((action,plan))=review.read().clone(){
                div {class:"mk-git-review",role:"group",aria_label:t!(ws,L,"git-review-everyday"),
                    p {{t!(ws,L,"git-review-everyday")} }
                    pre {"{plan.summary}"}
                    button {id:"mk-git-everyday-confirm",class:"mk-btn mk-btn-danger",disabled:*busy.read(),onclick:moonkale_ext_api::crash::event(move |_|send(GitRequest::Everyday{action:action.clone(),expected:Some(plan.clone())})),{t!(ws,L,"git-confirm")} }
                    button {class:"mk-btn",onclick:moonkale_ext_api::crash::event(move |_|review.set(None)),{t!(ws,L,"git-cancel")} }
                }
            }else{
                if tab==Tab::Branches {
                    label {{t!(ws,L,"git-branch-select")}
                        select {id:"mk-git-everyday-first",class:"mk-input",value:branch(),onchange:moonkale_ext_api::crash::event(move|e:FormEvent|branch.set(e.value())),for r in &content.branches{option{value:r.name.clone(),"{r.name}"}}}
                    }
                    label { input {r#type:"checkbox",checked:stash_first(),onchange:moonkale_ext_api::crash::event(move|e:FormEvent|stash_first.set(e.checked()))} {t!(ws,L,"git-stash-before-switch")} }
                    button {class:"mk-btn",onclick:moonkale_ext_api::crash::event(move |_|act(EverydayAction::SwitchBranch{name:branch(),stash:stash_first()})),{t!(ws,L,"git-switch-branch")} }
                    label {{t!(ws,L,"git-new-branch-name")} input {class:"mk-input mk-git-branch-name",value:name(),oninput:moonkale_ext_api::crash::event(move|e:FormEvent|name.set(e.value()))} }
                    label {{t!(ws,L,"git-commit-select")} select {class:"mk-input mk-git-commit-select",value:commit(),onchange:moonkale_ext_api::crash::event(move|e:FormEvent|commit.set(e.value())),for r in &content.reflog{option{value:r.oid.clone(),"{r.oid} {r.subject}"}}} }
                    button {class:"mk-btn",disabled:name.read().is_empty(),onclick:moonkale_ext_api::crash::event(move |_|act(EverydayAction::CreateBranch{name:name(),commit:commit()})),{t!(ws,L,"git-create-branch")} }
                    button {class:"mk-btn",disabled:name.read().is_empty(),onclick:moonkale_ext_api::crash::event(move |_|act(EverydayAction::RenameBranch{name:branch(),to:name()})),{t!(ws,L,"git-rename-branch")} }
                    button {class:"mk-btn",onclick:moonkale_ext_api::crash::event(move |_|act(EverydayAction::DeleteBranch{name:branch()})),{t!(ws,L,"git-delete-branch")} }
                    label {{t!(ws,L,"git-upstream-select")} select {class:"mk-input mk-git-upstream-select",value:upstream(),onchange:moonkale_ext_api::crash::event(move|e:FormEvent|upstream.set(e.value())),for r in &content.upstreams{option{value:r.clone(),"{r}"}}} }
                    button {class:"mk-btn",onclick:moonkale_ext_api::crash::event(move |_|act(EverydayAction::Upstream{name:branch(),upstream:upstream()})),{t!(ws,L,"git-set-upstream")} }
                    label {{t!(ws,L,"git-compare-with")} select {class:"mk-input mk-git-compare-select",value:other(),onchange:moonkale_ext_api::crash::event(move|e:FormEvent|other.set(e.value())),for r in &content.branches{option{value:r.name.clone(),"{r.name}"}}} }
                    button {class:"mk-btn",onclick:moonkale_ext_api::crash::event(move |_|send(GitRequest::Compare{left:branch(),right:other()})),{t!(ws,L,"git-compare")} }
                }
                if tab==Tab::Undo {
                    label {{t!(ws,L,"git-commit-select")} select {id:"mk-git-everyday-first",class:"mk-input mk-git-commit-select",value:commit(),onchange:moonkale_ext_api::crash::event(move|e:FormEvent|commit.set(e.value())),for r in &content.reflog{option{value:r.oid.clone(),"{r.oid} {r.selector}: {r.subject}"}}} }
                    button {class:"mk-btn",onclick:moonkale_ext_api::crash::event(move |_|act(EverydayAction::Revert{commit:commit()})),{t!(ws,L,"git-revert")} }
                    button {class:"mk-btn",onclick:moonkale_ext_api::crash::event(move |_|act(EverydayAction::CherryPick{commit:commit()})),{t!(ws,L,"git-cherry-pick")} }
                    label {{t!(ws,L,"git-reset-mode")} select {class:"mk-input mk-git-reset-mode",value:format!("{:?}",mode()),onchange:moonkale_ext_api::crash::event(move |e:FormEvent|mode.set(match e.value().as_str(){"Soft"=>ResetMode::Soft,"Hard"=>ResetMode::Hard,_=>ResetMode::Mixed})),option{value:"Soft",{t!(ws,L,"git-reset-soft")}} option{value:"Mixed",{t!(ws,L,"git-reset-mixed")}} option{value:"Hard",{t!(ws,L,"git-reset-hard")}}} }
                    button {class:"mk-btn",onclick:moonkale_ext_api::crash::event(move |_|act(EverydayAction::Reset{commit:commit(),mode:mode()})),{t!(ws,L,"git-reset")} }
                    label {{t!(ws,L,"git-new-branch-name")} input {class:"mk-input mk-git-branch-name",value:name(),oninput:moonkale_ext_api::crash::event(move |e:FormEvent|name.set(e.value()))} }
                    button {class:"mk-btn",disabled:name.read().is_empty(),onclick:moonkale_ext_api::crash::event(move |_|act(EverydayAction::Restore{commit:commit(),name:name()})),{t!(ws,L,"git-reflog-restore")} }
                }
                if tab==Tab::Stash {
                    label {{t!(ws,L,"git-stash-message")} input {id:"mk-git-everyday-first",class:"mk-input mk-git-stash-message",value:name(),oninput:moonkale_ext_api::crash::event(move|e:FormEvent|name.set(e.value()))} }
                    label {input{r#type:"checkbox",class:"mk-git-stash-untracked",checked:untracked(),onchange:moonkale_ext_api::crash::event(move|e:FormEvent|untracked.set(e.checked()))} {t!(ws,L,"git-include-untracked")} }
                    button {class:"mk-btn",onclick:moonkale_ext_api::crash::event(move |_|act(EverydayAction::StashPush{message:name(),untracked:untracked()})),{t!(ws,L,"git-stash-push")} }
                    label {{t!(ws,L,"git-stash-select")} select {class:"mk-input mk-git-stash-select",value:stash(),onchange:moonkale_ext_api::crash::event(move|e:FormEvent|stash.set(e.value())),for r in &content.stashes{option{value:r.oid.clone(),"{r.selector}: {r.subject}"}}} }
                    button {class:"mk-btn",disabled:content.stashes.is_empty(),onclick:moonkale_ext_api::crash::event(move |_|send(GitRequest::StashShow{oid:stash()})),{t!(ws,L,"git-stash-show")} }
                    button {class:"mk-btn",disabled:content.stashes.is_empty(),onclick:moonkale_ext_api::crash::event(move |_|act(EverydayAction::StashApply{oid:stash()})),{t!(ws,L,"git-stash-apply")} }
                    button {class:"mk-btn",disabled:content.stashes.is_empty(),onclick:moonkale_ext_api::crash::event(move |_|act(EverydayAction::StashPop{oid:stash()})),{t!(ws,L,"git-stash-pop")} }
                    button {class:"mk-btn",disabled:content.stashes.is_empty(),onclick:moonkale_ext_api::crash::event(move |_|act(EverydayAction::StashDrop{oid:stash()})),{t!(ws,L,"git-stash-drop")} }
                }
                if tab==Tab::Blame {
                    label {{t!(ws,L,"git-blame-path")} input {id:"mk-git-everyday-first",class:"mk-input mk-git-blame-path",value:path(),oninput:moonkale_ext_api::crash::event(move|e:FormEvent|path.set(e.value()))} }
                    button {class:"mk-btn",onclick:moonkale_ext_api::crash::event(move |_|send(GitRequest::Blame{path:path(),commit:None})),{t!(ws,L,"git-blame")} }
                    button {class:"mk-btn",onclick:moonkale_ext_api::crash::event(move |_|{if let Some((id,_))=ws.active_document(){ws.contrib.gutter_marks.with_mut(|m|{m.remove(&id);});}blame.set(Vec::new());}),{t!(ws,L,"git-blame-clear")} }
                    for (i,row) in blame.read().iter().enumerate(){
                        div {key:"{i}",class:"mk-git-blame-row",title:"{row.commit}\n{row.author}\n{row.summary}","{row.line+1}: {row.author} — {row.summary} ",
                            if let (Some(c),Some(p))=(&row.previous,&row.previous_path){
                                button {class:"mk-btn",onclick:moonkale_ext_api::crash::event({let c=c.clone();let p=p.clone();move |_|{path.set(p.clone());send(GitRequest::Blame{path:p.clone(),commit:Some(c.clone())});}}),{t!(ws,L,"git-blame-previous")} }
                            }
                        }
                    }
                }
            }
            if !output.read().is_empty(){pre{class:"mk-git-everyday-output",role:"status","{output}"}}
        }
    }
}
