//! Open-tab metadata and lazy, static extension previews. No source reads.
use crate::frame::Extensions_;
use crate::L;
use dioxus::prelude::*;
use moonkale_core::NodeId;
use moonkale_ext_api::{t, Workspace};
use std::{cell::RefCell, collections::VecDeque, rc::Rc};
#[derive(Clone, Default)]
struct PreviewCache(Rc<RefCell<VecDeque<(String, u64, Element)>>>);

#[derive(Clone, PartialEq)]
pub(crate) struct Tab {
    pub movable: bool,
    pub window: Option<moonkale_ext_api::WindowId>,
    pub id: String,
    pub title: String,
    pub path: String,
    pub kind: String,
    pub state: String,
    pub group: String,
    pub marker: String,
    pub colour: u8,
    pub owner: Option<usize>,
    pub node: Option<NodeId>,
    pub dirty: bool,
    pub mark: String,
    pub search_text: String,
    pub source_colour: String,
    pub presence: String,
}
impl Tab {
    pub fn new(
        ws: Workspace,
        id: String,
        title: String,
        owner: Option<usize>,
        node: Option<NodeId>,
    ) -> Self {
        let group = ws.panel_group(&id);
        let path = node
            .and_then(|n| ws.open_node_by_id(n))
            .map(|n| n.native_key)
            .unwrap_or_default();
        let dirty = node
            .and_then(|n| ws.document(n))
            .is_some_and(|d| d.peek().dirty());
        let mark = ws
            .contrib
            .file_marks
            .peek()
            .get(&path)
            .map(|m| m.letter.to_string())
            .unwrap_or_default();
        let search_text = node
            .and_then(|n| ws.document(n))
            .filter(|d| moonkale_ext_api::preview::allowed_path(&d.peek().node.native_key))
            .map(|d| moonkale_ext_api::preview::bounded_text(&d.peek().text))
            .unwrap_or_default();
        let source_colour = node
            .and_then(|n| ws.open_node_by_id(n))
            .map(|n| crate::icons::source_color(n.source.as_str()).to_string())
            .unwrap_or_default();
        let presence = ws
            .others()
            .iter()
            .filter(|m| m.active.as_deref() == Some(&path))
            .take(8)
            .map(|m| m.name.clone())
            .collect::<Vec<_>>()
            .join(", ");
        let kind = if id.starts_with("graph") {
            "graph"
        } else if path.ends_with(".flow.json") {
            "flow"
        } else if path.ends_with(".md") {
            "markdown"
        } else if id.starts_with("editor") || id.starts_with("lead:") {
            "code"
        } else {
            id.split(':').next().unwrap_or(&id)
        };
        Self {
            // Only graph views currently have a source-backed transfer protocol.
            // Other followers can be docked locally; held query data is local.
            movable: node.is_some()
                || (id.starts_with("graph:") && ws.links.views.peek().contains_key(&id)),
            window: None,
            state: if dirty {
                t!(ws, L, "overview-unsaved")
            } else {
                String::new()
            },
            search_text,
            source_colour,
            presence,
            kind: kind.into(),
            id,
            title,
            path,
            group: group.as_ref().map(|g| g.id.clone()).unwrap_or_default(),
            marker: group.as_ref().map(|g| g.marker()).unwrap_or_default(),
            colour: group.as_ref().map_or(0, |g| g.colour),
            owner,
            node,
            dirty,
            mark,
        }
    }
    pub fn remote(
        ws: Workspace,
        window: moonkale_ext_api::WindowId,
        tab: moonkale_ext_api::window_panels::WindowTab,
    ) -> Self {
        let group = tab
            .group
            .as_ref()
            .and_then(|id| ws.links.groups.peek().iter().find(|g| &g.id == id).cloned());
        Self {
            movable: tab.movable,
            window: Some(window.clone()),
            id: format!("window:{}:{}", window.0, tab.panel),
            title: tab.title,
            path: tab.path,
            kind: tab.category,
            group: tab.group.unwrap_or_default(),
            marker: group.as_ref().map(|g| g.marker()).unwrap_or_default(),
            colour: group.as_ref().map_or(0, |g| g.colour),
            owner: None,
            node: None,
            dirty: tab.dirty,
            state: if tab.dirty {
                t!(ws, L, "overview-unsaved")
            } else {
                String::new()
            },
            mark: String::new(),
            search_text: String::new(),
            source_colour: String::new(),
            presence: String::new(),
        }
    }
    pub fn remote_panel(&self) -> Option<&str> {
        self.id
            .strip_prefix(&format!("window:{}:", self.window.as_ref()?.0))
    }
    pub fn matches(&self, query: &str) -> bool {
        let q = query.to_lowercase();
        [&self.title, &self.path, &self.kind, &self.search_text]
            .iter()
            .any(|s| s.to_lowercase().contains(&q))
    }
}
#[derive(Default)]
pub(crate) struct RecentTabs {
    last: Option<(Option<NodeId>, Option<String>)>,
    order: Vec<String>,
}
impl RecentTabs {
    pub fn sort(&mut self, tabs: &mut [Tab], node: Option<NodeId>, focused: Option<String>) {
        let changed_node = self.last.as_ref().is_none_or(|last| last.0 != node);
        if self.last.as_ref() != Some(&(node, focused.clone())) {
            let active = if changed_node && node.is_some() {
                tabs.iter()
                    .find(|t| {
                        t.node.is_some() && t.node == node && focused.as_deref() == Some(&t.id)
                    })
                    .or_else(|| tabs.iter().find(|t| t.node.is_some() && t.node == node))
                    .map(|t| t.id.clone())
            } else {
                focused.clone()
            };
            if let Some(active) = active {
                self.order.retain(|id| id != &active);
                self.order.insert(0, active);
            }
            self.last = Some((node, focused));
        }
        self.order.retain(|id| tabs.iter().any(|t| t.id == *id));
        tabs.sort_by_key(|t| {
            self.order
                .iter()
                .position(|id| *id == t.id)
                .unwrap_or(usize::MAX)
        });
    }
}

/// Stable clusters preserve the supplied recency order within each group.
pub(crate) fn grouped(tabs: &[Tab], query: &str) -> Vec<Tab> {
    type Cluster = ((Option<moonkale_ext_api::WindowId>, String), Vec<Tab>);
    let mut groups: Vec<Cluster> = Vec::new();
    for tab in tabs.iter().filter(|t| t.matches(query)) {
        let key = (tab.window.clone(), tab.group.clone());
        if let Some((_, members)) = groups.iter_mut().find(|(g, _)| *g == key) {
            members.push(tab.clone());
        } else {
            groups.push((key, vec![tab.clone()]));
        }
    }
    groups.sort_by_key(|((window, group), _)| {
        (window.as_ref().map(|w| w.0.clone()), group.is_empty())
    });
    groups.into_iter().flat_map(|(_, tabs)| tabs).collect()
}

#[component]
pub(crate) fn Preview(tab: Tab) -> Element {
    let ws = use_context::<Workspace>();
    let exts = use_context::<Extensions_>().0;
    let identity = tab
        .owner
        .map(|i| exts[i].manifest().id)
        .unwrap_or("overview");
    let guard = use_context_provider(|| moonkale_ext_api::crash::ExtensionGuard::new(ws, identity));
    use_context_provider(|| moonkale_ext_api::links::PanelOrigin(tab.id.clone()));
    let mut visible = use_signal(|| false);
    let cache = use_hook(move || ws.shared_state(PreviewCache::default));
    let id = format!("mk-preview-{}", tab.id);
    let mount_id = id.clone();
    // Mount an extension only when its card is on screen. Previews are a
    // snapshot for this opening. Offscreen components unmount, and removal
    // disconnects both observers; scrolling never subscribes to editor state.
    let preview = use_memo(move || {
        if !visible() {
            return None;
        }
        let index = tab.owner?;
        let revision = tab
            .node
            .and_then(|node| ws.docs.editor_sessions.peek().get(&node).copied())
            .map(|s| s.peek().snapshot().revision.0);
        let cache_key = format!(
            "{}:{}:{}:{}",
            ws.saved_sessions.active.peek(),
            ws.lang(),
            index,
            tab.id
        );
        if let Some(revision) = revision {
            if let Some((_, _, element)) = cache
                .0
                .borrow()
                .iter()
                .find(|(id, r, _)| *id == cache_key && *r == revision)
            {
                return Some(element.clone());
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        let started = std::time::Instant::now();
        #[cfg(target_arch = "wasm32")]
        let started = js_sys::Date::now();
        let result = moonkale_ext_api::workspace::untracked(|| {
            exts[index].preview(
                &tab.id,
                ws,
                moonkale_ext_api::preview::PreviewSize::new(256, 120),
            )
        });
        #[cfg(not(target_arch = "wasm32"))]
        let elapsed = started.elapsed().as_secs_f64() * 1000.;
        #[cfg(target_arch = "wasm32")]
        let elapsed = js_sys::Date::now() - started;
        if elapsed > 4. {
            return None;
        }
        if let (Some(revision), Some(element)) = (revision, &result) {
            let mut entries = cache.0.borrow_mut();
            entries.retain(|(id, _, _)| *id != cache_key);
            if entries.len() >= 64 {
                entries.pop_front();
            }
            entries.push_back((cache_key, revision, element.clone()));
        }
        result
    });
    rsx! {
        div { id, class: "mk-overview-preview", onmounted: move |_| {
            let id = serde_json::to_string(&mount_id).unwrap_or_default();
            spawn(async move {
                let mut eval = document::eval(&format!(r#"
                    const el=document.getElementById({id});
                    if(el) {{
                        const visibility=new IntersectionObserver(entries=>{{
                            for(const entry of entries) dioxus.send(entry.isIntersecting);
                        }});
                        visibility.observe(el);
                        await new Promise(resolve=>{{
                            const removal=new MutationObserver(()=>{{
                                if(!el.isConnected) {{ visibility.disconnect(); removal.disconnect(); resolve(); }}
                            }});
                            removal.observe(document.body,{{childList:true,subtree:true}});
                        }});
                    }}
                "#));
                while let Ok(on_screen) = eval.recv::<bool>().await { visible.set(on_screen); }
            });
        },
            if let Some(content) = preview() {
                ErrorBoundary { handle_error: move |_errors: ErrorContext| { guard.failed(); rsx! { span { "◇" } } }, {content} }
            }
            else { span { "◇" } }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tab(title: &str, group: &str) -> Tab {
        Tab {
            movable: true,
            window: None,
            id: title.into(),
            title: title.into(),
            path: format!("notes/{title}"),
            kind: "markdown".into(),
            state: String::new(),
            group: group.into(),
            marker: String::new(),
            colour: 0,
            owner: None,
            node: None,
            dirty: false,
            mark: String::new(),
            search_text: String::new(),
            source_colour: String::new(),
            presence: String::new(),
        }
    }
    #[test]
    fn linked_cards_stay_adjacent_and_ungrouped_follow() {
        let tabs = [
            tab("Other", ""),
            tab("Note", "a"),
            tab("Second", "b"),
            tab("Graph", "a"),
        ];
        assert_eq!(
            grouped(&tabs, "")
                .iter()
                .map(|t| t.title.as_str())
                .collect::<Vec<_>>(),
            ["Note", "Graph", "Second", "Other"]
        );
    }
    #[test]
    fn recency_keeps_selected_panels_first_and_forgets_closed_tabs() {
        let mut tabs = [tab("A", ""), tab("B", ""), tab("C", "")];
        let mut recent = RecentTabs::default();
        recent.sort(&mut tabs, None, Some("B".into()));
        recent.sort(&mut tabs, None, Some("C".into()));
        assert_eq!(
            tabs.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(),
            ["C", "B", "A"]
        );
        let mut remaining = [tab("A", "")];
        recent.sort(&mut remaining, None, Some("A".into()));
        assert_eq!(recent.order, ["A"]);
    }
    #[test]
    fn filter_matches_title_path_and_kind() {
        let tabs = [tab("Alpha.md", "a"), tab("Beta.md", "")];
        assert_eq!(grouped(&tabs, "ALP").len(), 1);
        assert_eq!(grouped(&tabs, "notes/").len(), 2);
        assert_eq!(grouped(&tabs, "markdown").len(), 2);
        assert!(grouped(&tabs, "unmatched").is_empty());
    }
}
