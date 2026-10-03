//! Backlinks / outgoing links of the active document, from the index.

use crate::L;
use dioxus::prelude::*;
use moonkale_core::{Direction, EdgeKind, Node, NodeId, NodeKind, Query};
use moonkale_ext_api::{t, Workspace};

const CSS: Asset = asset!("/assets/links.css");

#[derive(Clone, PartialEq, Default)]
struct Links {
    backlinks: Vec<Node>,
    outgoing: Vec<Node>,
}

async fn load(ws: Workspace, node: NodeId) -> Option<Links> {
    let index = ws.index()?;
    let res = index
        .source
        .query(Query::Neighbours {
            node,
            depth: 1,
            direction: Direction::Both,
        })
        .await
        .ok()?;
    let by_id = |id: NodeId| res.nodes.iter().find(|n| n.id == id).cloned();
    let mut links = Links::default();
    for e in res.edges.iter().filter(|e| e.kind == EdgeKind::Links) {
        if e.to == node {
            if let Some(n) = by_id(e.from) {
                links.backlinks.push(n);
            }
        } else if e.from == node {
            if let Some(n) = by_id(e.to) {
                links.outgoing.push(n);
            }
        }
    }
    links
        .backlinks
        .sort_by(|a, b| a.native_key.cmp(&b.native_key));
    links
        .outgoing
        .sort_by(|a, b| a.native_key.cmp(&b.native_key));
    Some(links)
}

#[component]
pub fn LinksPanel(ws: Workspace) -> Element {
    let mut links: Signal<Option<Links>> = use_signal(|| None);
    let mut for_node: Signal<Option<NodeId>> = use_signal(|| None);

    use_effect(move || {
        let active = *ws.docs.active.read();
        let _epoch = *ws.sources.graph_epoch.read();
        let _sources = ws.sources.open.read().len();
        match active {
            Some(node) => {
                spawn(async move {
                    let l = load(ws, node).await;
                    for_node.set(Some(node));
                    links.set(l);
                });
            }
            None => {
                for_node.set(None);
                links.set(None);
            }
        }
    });

    let title = for_node().and_then(|id| ws.document(id).map(|d| d.read().node.label.clone()));

    rsx! {
        moonkale_ext_api::Stylesheet { href: CSS }
        div { class: "mk-links",
            match (title, links()) {
                (None, _) => rsx! { p { class: "mk-links-empty", {t!(ws, L, "links-no-document")} } },
                (Some(title), None) => rsx! { p { class: "mk-links-empty", {t!(ws, L, "links-no-index", name = title)} } },
                (Some(title), Some(l)) => rsx! {
                    div { class: "mk-links-title", "{title}" }
                    LinkSection { ws, heading: t!(ws, L, "links-backlinks"), nodes: l.backlinks, empty: t!(ws, L, "links-backlinks-empty") }
                    LinkSection { ws, heading: t!(ws, L, "links-outgoing"), nodes: l.outgoing, empty: t!(ws, L, "links-outgoing-empty") }
                },
            }
        }
    }
}

#[component]
fn LinkSection(ws: Workspace, heading: String, nodes: Vec<Node>, empty: String) -> Element {
    rsx! {
        div { class: "mk-links-section",
            div { class: "mk-links-heading", "{heading} " span { class: "mk-links-count", "{nodes.len()}" } }
            if nodes.is_empty() {
                p { class: "mk-links-empty", "{empty}" }
            }
            ul { class: "mk-links-list",
                for node in nodes {
                    {
                        let n = node.clone();
                        let phantom = node.kind == NodeKind::Page;
                        let mut ws2 = ws;
                        rsx! {
                            li { key: "{node.id}",
                                class: if phantom { "mk-links-item mk-links-phantom" } else { "mk-links-item" },
                                title: if phantom { t!(ws, L, "links-unresolved") } else { node.native_key.clone() },
                                onclick: move |_| {
                                    let n = n.clone();
                                    if n.kind == NodeKind::File {
                                        spawn(async move {
                                            if let Err(e) = ws.open_node(n).await { ws2.set_status(e.to_string()); }
                                        });
                                    } else {
                                        ws2.set_status(t!(ws2, L, "links-missing", name = n.label.clone()));
                                    }
                                },
                                span { class: "mk-links-label", "{node.label}" }
                                if !phantom { span { class: "mk-links-path", "{node.native_key}" } }
                                if phantom {
                                    // Spec 012: an unresolved link becomes a page next to the linking document.
                                    button { class: "mk-btn mk-links-create", title: t!(ws, L, "links-create-title"),
                                        onclick: {
                                            let target = node.label.clone();
                                            move |e: MouseEvent| {
                                                e.stop_propagation();
                                                let target = target.clone();
                                                spawn(async move {
                                                    let Some((_, d)) = ws.active_document() else { return };
                                                    let from = d.peek().node.clone();
                                                    let _ = ws.follow_wiki(&from, &target, true).await;
                                                });
                                            }
                                        },
                                        {t!(ws, L, "links-create")}
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
