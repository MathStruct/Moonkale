//! Agent-owned display panels and quiet offers. Content never grants authority.
use crate::{markdown::Markdown, L};
use dioxus::prelude::*;
use moonkale_core::{
    Edge, EdgeKind, Node, NodeId, NodeKind, Query, QueryResult, SourceId, Table, Value, Version,
};
use moonkale_ext_api::{
    agent_display::*,
    links::{Follow, LinkGroup, LinkView, Role, Subject},
    t, Workspace,
};
use moonkale_llm::ToolCall;
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

use moonkale_ext_api::agent_display::DisplayAction as Action;
#[derive(Clone, PartialEq)]
struct Offer {
    tool: String,
    outer: String,
    owner: String,
    name: String,
    action: Action,
}
#[derive(Clone, PartialEq)]
pub(crate) struct Display {
    pub outer: String,
    pub name: String,
    pub content: Option<Canvas>,
}
#[derive(Clone, Copy)]
pub(crate) struct Displays {
    pub panels: Signal<BTreeMap<String, Display>>,
    offers: Signal<Vec<Offer>>,
    budgets: Signal<BTreeMap<String, Rc<RefCell<DisplayBudget>>>>,
}
impl Displays {
    pub fn get(ws: Workspace) -> Self {
        ws.shared_state(|| Self {
            panels: Signal::new_in_scope(BTreeMap::new(), ScopeId::ROOT),
            offers: Signal::new_in_scope(Vec::new(), ScopeId::ROOT),
            budgets: Signal::new_in_scope(BTreeMap::new(), ScopeId::ROOT),
        })
    }
}
fn group_id(outer: &str, owner: &str, name: &str) -> String {
    format!(
        "agent:{}",
        NodeId::derive(
            &SourceId::new(outer),
            &serde_json::to_string(&(owner, name)).unwrap()
        )
    )
}
fn synthetic(source: &SourceId, key: &str, label: &str, kind: NodeKind) -> Node {
    Node {
        id: NodeId::derive(source, key),
        source: source.clone(),
        kind,
        label: label.into(),
        native_key: key.into(),
        content: None,
        version: Version(0),
        props: Default::default(),
    }
}

pub(crate) async fn request(
    ws: Workspace,
    outer: String,
    owner: String,
    name: String,
    call: ToolCall,
) -> Result<String, String> {
    if *ws.saved_sessions.active.peek() != outer {
        return Err("session changed".into());
    }
    if ws
        .settings
        .resolved
        .peek()
        .policy
        .denied_tools
        .iter()
        .any(|name| name == &call.name)
    {
        return Err("display tool denied in this session".into());
    }
    let budget_key = NodeId::derive(&SourceId::new(&outer), &owner).to_string();
    let mut state = Displays::get(ws);
    // The input adapter retains only a timestamp, never keys or text.
    let (now, typing): (u64, bool) = document::eval(r#"dioxus.send([Math.floor(performance.now()), !!window.__mkAgentComposing || performance.now() < (window.__mkAgentTypingUntil || 0)]);"#)
        .recv().await.map_err(|_| "display activity unavailable")?;
    if *ws.saved_sessions.active.peek() != outer {
        return Err("session changed".into());
    }
    if !state.budgets.peek().contains_key(&budget_key) {
        if state.budgets.peek().len() >= 64 {
            return Err("too many agent display sessions".into());
        }
        state.budgets.with_mut(|all| {
            all.insert(
                budget_key.clone(),
                Rc::new(RefCell::new(DisplayBudget::default())),
            );
        });
    }
    state
        .budgets
        .peek()
        .get(&budget_key)
        .unwrap()
        .borrow_mut()
        .charge(now)?;
    let action = parse_action(&call.name, call.input)?;
    let offer = Offer {
        tool: call.name.clone(),
        outer,
        owner,
        name,
        action,
    };
    let setting = ws.settings.resolved.peek().agent.open_views;
    let focused_table = match &offer.action {
        Action::Canvas(Canvas::Table { .. }) => {
            let group = group_id(&offer.outer, &offer.owner, &offer.name);
            let owner = NodeId::derive(&SourceId::new(group), "canvas");
            ws.links.focused.peek().as_deref() == Some(format!("table:{owner}").as_str())
        }
        _ => false,
    };
    if setting == OpenViews::Ask || setting == OpenViews::WhenIdle && typing || focused_table {
        if state.offers.peek().len() >= 8 {
            return Err("agent display offer queue is full".into());
        }
        state.offers.with_mut(|offers| offers.push(offer));
        return Ok("Offered to the person; waits for Show without taking focus".into());
    }
    apply(ws, offer).await?;
    Ok("Shown in your marked link group without taking focus".into())
}

async fn apply(mut ws: Workspace, offer: Offer) -> Result<(), String> {
    if *ws.saved_sessions.active.peek() != offer.outer {
        return Err("session changed".into());
    }
    // Recheck denials at acceptance, so a queued offer cannot outlive policy.
    let denied = ws.settings.resolved.peek().policy.denied_tools.clone();
    if denied.iter().any(|name| name == &offer.tool) {
        return Err("display tool denied in this session".into());
    }
    let key = group_id(&offer.outer, &offer.owner, &offer.name);
    let panel = format!("agent-canvas:{key}");
    let source = SourceId::new(&key);
    let owner = NodeId::derive(&source, "canvas");
    let mut subject = Subject::new(owner);
    let mut content_panel = None;
    let wants_graph = matches!(
        &offer.action,
        Action::Canvas(Canvas::Graph { .. }) | Action::Reveal(_)
    );
    let mut state = Displays::get(ws);
    match &offer.action {
        Action::Canvas(content) => {
            content.validate()?;
            let mut result = QueryResult::default();
            match content {
                Canvas::Markdown { .. } => {}
                Canvas::Table { columns, rows } => {
                    result.table = Some(Table {
                        columns: columns.clone(),
                        rows: rows
                            .iter()
                            .map(|r| r.iter().cloned().map(Value::Text).collect())
                            .collect(),
                        truncated: false,
                    });
                    let node = synthetic(&source, "canvas", &offer.name, NodeKind::Table);
                    if !ws.docs.views.peek().iter().any(|n| n.id == owner) {
                        ws.docs.views.with_mut(|views| views.push(node));
                    }
                    content_panel = Some(format!("table:{owner}"));
                }
                Canvas::Graph { nodes, edges } => {
                    result.nodes = nodes
                        .iter()
                        .map(|n| {
                            let kind = match n.style {
                                ThemeStyle::Default => NodeKind::Vertex,
                                ThemeStyle::Accent => NodeKind::Symbol,
                                ThemeStyle::Success => NodeKind::File,
                                ThemeStyle::Warning => NodeKind::Table,
                            };
                            synthetic(&source, &n.id, &n.label, kind)
                        })
                        .collect();
                    result.edges = edges
                        .iter()
                        .map(|e| Edge {
                            source: source.clone(),
                            from: NodeId::derive(&source, &e.from),
                            to: NodeId::derive(&source, &e.to),
                            kind: EdgeKind::Links,
                            props: Default::default(),
                        })
                        .collect();
                }
            }
            ws.links.results.with_mut(|all| {
                all.insert(owner, Rc::new(result));
            });
            state.panels.with_mut(|all| {
                all.insert(
                    key.clone(),
                    Display {
                        outer: offer.outer.clone(),
                        name: offer.name.clone(),
                        content: Some(content.clone()),
                    },
                );
            });
        }
        Action::Reveal(range) => {
            let source = ws
                .source(&SourceId::new(&range.source))
                .ok_or("unknown source")?;
            let node_id = range.node.parse().map_err(|_| "invalid node")?;
            let node = source
                .query(Query::Node(node_id))
                .await
                .map_err(|_| "node unavailable")?
                .nodes
                .into_iter()
                .find(|n| n.id == node_id)
                .ok_or("unknown node")?;
            if ws.document(node_id).is_none() {
                ws.links.agent_documents.with_mut(|ids| {
                    ids.insert(node_id);
                });
            }
            ws.load_node_guarded(node, || *ws.saved_sessions.active.peek() == offer.outer)
                .await
                .map_err(|_| "node unavailable")?;
            if *ws.saved_sessions.active.peek() != offer.outer {
                return Err("session changed".into());
            }
            let group = ws.links.groups.peek().iter().find(|g| g.id == key).cloned();
            let focused = ws.links.focused.peek().clone();
            let retained = group.as_ref().and_then(|g| {
                g.members
                    .iter()
                    .find(|(id, _, follow)| {
                        focused.as_deref() != Some(id.as_str())
                            && id.starts_with("lead:agent:")
                            && matches!(follow, Follow::Pinned(subject) if subject.node == node_id)
                    })
                    .map(|(id, _, _)| id.clone())
            });
            if retained.is_none() && group.as_ref().is_some_and(|g| g.members.len() >= 16) {
                return Err("agent view limit (16)".into());
            }
            let lead = retained.unwrap_or_else(|| format!("lead:{key}:{}", uuid::Uuid::new_v4()));
            ws.links.contents.with_mut(|all| {
                all.insert(lead.clone(), node_id);
            });
            ws.links.reveals.with_mut(|all| {
                all.insert(lead.clone(), range.clone());
            });
            subject = Subject::new(node_id);
            content_panel = Some(lead);
            state.panels.with_mut(|all| {
                all.entry(key.clone()).or_insert(Display {
                    outer: offer.outer.clone(),
                    name: offer.name.clone(),
                    content: None,
                });
            });
        }
        Action::Select(selection) => {
            if !ws.links.views.peek().contains_key(&format!("graph:{key}")) {
                return Err("show a graph first".into());
            }
            let group = ws
                .links
                .groups
                .peek()
                .iter()
                .find(|g| g.id == key)
                .cloned()
                .ok_or("show a graph first")?;
            let result = ws.links.results.peek().get(&group.subject.node).cloned();
            let mut selected = Vec::new();
            for id in &selection.nodes {
                if let Some(result) = &result {
                    let node = result
                        .nodes
                        .iter()
                        .find(|n| n.native_key == *id || n.id.to_string() == *id)
                        .ok_or("selection is outside your graph")?;
                    selected.push(node.id.to_string());
                } else {
                    let node: NodeId =
                        id.parse().map_err(|_| "context selection needs node ids")?;
                    selected.push(node.to_string());
                }
            }
            ws.select_linked(&format!("graph:{key}"), selected);
            ws.links.display_request.set(Some(format!("graph:{key}")));
            return Ok(());
        }
    }
    if wants_graph {
        ws.links.views.with_mut(|all| {
            all.entry(format!("graph:{key}")).or_insert(LinkView {
                camera: None,
                detached: subject.clone(),
            });
        });
    }
    let primary = content_panel.unwrap_or_else(|| {
        if wants_graph {
            format!("graph:{key}")
        } else {
            panel
        }
    });
    let show = primary.clone();
    ws.links.groups.with_mut(|all| {
        let follow = if primary.starts_with("lead:agent:") {
            Follow::Pinned(subject.clone())
        } else {
            Follow::Group
        };
        let mut members = vec![(primary.clone(), Role::Lead, follow)];
        if wants_graph && !primary.starts_with("graph:") {
            members.push((format!("graph:{key}"), Role::Follower, Follow::Group));
        }
        if let Some(group) = all.iter_mut().find(|g| g.id == key) {
            for member in members {
                if !group.members.iter().any(|m| m.0 == member.0) {
                    group.members.push(member);
                }
            }
            for (id, role, _) in &mut group.members {
                *role = if id == &primary {
                    Role::Lead
                } else {
                    Role::Follower
                };
            }
            group.subject = subject;
        } else {
            let number = all
                .iter()
                .map(|g| g.number)
                .max()
                .unwrap_or(0)
                .saturating_add(1);
            all.push(LinkGroup {
                id: key,
                agent: Some(offer.name),
                colour: ((number - 1) % 8 + 1) as u8,
                number,
                members,
                subject: subject.clone(),
                history: vec![subject],
                cursor: 0,
            });
        }
    });
    ws.shell.closed_panels.with_mut(|closed| {
        closed.remove(&show);
    });
    ws.links.display_request.set(Some(show));
    Ok(())
}

#[component]
pub(crate) fn Offers(mut ws: Workspace) -> Element {
    let mut state = Displays::get(ws);
    let outer = ws.saved_sessions.active.read().clone();
    let offers = state.offers.read().clone();
    rsx! {
        for (index, offer) in offers.into_iter().enumerate().filter(|(_,o)| o.outer == outer) {
            div { class: "mk-agent-display-offer", key: "{index}",
                span { {t!(ws, L, "agent-display-offer", name = offer.name.clone())} }
                button { class: "mk-agent-display-show", onclick: move |_| {
                    state.offers.with_mut(|all| { if index < all.len() { all.remove(index); } });
                    let offer = offer.clone();
                    moonkale_ext_api::crash::spawn(async move { if let Err(error) = apply(ws, offer).await { ws.set_status(error); } });
                }, {t!(ws, L, "agent-display-show")} }
                button { onclick: move |_| { state.offers.with_mut(|all| { if index < all.len() { all.remove(index); } }); }, {t!(ws, L, "agent-display-dismiss")} }
            }
        }
    }
}

#[component]
pub(crate) fn CanvasPanel(ws: Workspace, group: String) -> Element {
    let state = Displays::get(ws);
    let display = state.panels.read().get(&group).cloned();
    rsx! { div { class: "mk-agent-canvas",
        if let Some(display) = display {
            p { class: "mk-muted", "{display.name}" }
            if let Some(Canvas::Markdown { text }) = display.content { Markdown { text } }
        }
    } }
}
