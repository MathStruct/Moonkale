//! Backlinks / outgoing links of the active document, from the index.

use dioxus::prelude::*;
use moonkale_core::{Direction, EdgeKind, Node, NodeId, NodeKind, Query};
use moonkale_ext_api::Workspace;

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
        let active = *ws.active.read();
        let _epoch = *ws.graph_epoch.read();
        let _sources = ws.sources.read().len();
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
        document::Stylesheet { href: CSS }
        div { class: "mk-links",
            match (title, links()) {
                (None, _) => rsx! { p { class: "mk-links-empty", "Open a document to see what links to it." } },
                (Some(t), None) => rsx! { p { class: "mk-links-empty", "No index for {t} — open a folder to build one." } },
                (Some(t), Some(l)) => rsx! {
                    div { class: "mk-links-title", "{t}" }
                    LinkSection { ws, heading: "Backlinks", nodes: l.backlinks, empty: "Nothing links here yet." }
                    LinkSection { ws, heading: "Outgoing", nodes: l.outgoing, empty: "No links in this document." }
                },
            }
        }
    }
}

#[component]
fn LinkSection(
    ws: Workspace,
    heading: &'static str,
    nodes: Vec<Node>,
    empty: &'static str,
) -> Element {
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
                                title: if phantom { "Unresolved link: no file with this name yet" } else { "{node.native_key}" },
                                onclick: move |_| {
                                    let n = n.clone();
                                    if n.kind == NodeKind::File {
                                        spawn(async move {
                                            if let Err(e) = ws.open_node(n).await { ws2.set_status(e.to_string()); }
                                        });
                                    } else {
                                        ws2.set_status(format!("{} does not exist yet", n.label));
                                    }
                                },
                                span { class: "mk-links-label", "{node.label}" }
                                if !phantom { span { class: "mk-links-path", "{node.native_key}" } }
                            }
                        }
                    }
                }
            }
        }
    }
}
