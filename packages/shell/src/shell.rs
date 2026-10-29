//! `Shell` — extensions in, workbench out.

use crate::frame::Extensions_;
use crate::L;
use dioxus::prelude::*;
use dioxus_workbench::prelude::*;
use moonkale_ext_api::t;
use moonkale_ext_api::{Command, Extension, Workspace};
use std::collections::HashMap;
use std::rc::Rc;

const SHELL_CSS: Asset = asset!("/assets/styling/shell.css");

fn default_layout() -> PanelLayout {
    PanelLayout::new(LayoutNode::split(
        "root",
        SplitAxis::Horizontal,
        0.22,
        LayoutNode::tile("side", ["explorer", "search", "links", "git", "history"]),
        LayoutNode::split(
            "main-right",
            SplitAxis::Horizontal,
            0.7,
            LayoutNode::split(
                "main-rows",
                SplitAxis::Vertical,
                0.72,
                LayoutNode::empty_tile("main"),
                LayoutNode::tile("bottom", ["terminal"]),
            ),
            LayoutNode::tile("right", ["agent"]),
        ),
    ))
}

/// Everything in one tile: the phone-sized shell (Milestone 6).
fn narrow_layout() -> PanelLayout {
    PanelLayout::new(LayoutNode::empty_tile("main"))
}

/// Below this width the shell collapses to one tile plus a bottom bar.
pub const NARROW_MAX_PX: u32 = 700;

#[component]
pub fn Shell() -> Element {
    let mut ws = use_context::<Workspace>();
    let mut status_folder = use_context_provider(|| Signal::new(None::<moonkale_core::SourceId>));
    use_effect(move || {
        if let Some((_, doc)) = ws.active_document() {
            status_folder.set(Some(doc.peek().node.source.clone()));
        }
    });
    let cache_status = use_resource(move || {
        let indices: Vec<_> = ws
            .sources
            .open
            .read()
            .iter()
            .filter(|s| s.descriptor.family == moonkale_core::SourceFamily::Index)
            .filter_map(|s| {
                let open = ws.sources.open.read();
                let folder = open.iter().find(|f| {
                    f.descriptor.family == moonkale_core::SourceFamily::Folder
                        && s.descriptor.id.as_str() == format!("index:{}", f.descriptor.id)
                })?;
                Some((
                    s.source.clone(),
                    s.descriptor.truth,
                    folder.descriptor.clone(),
                ))
            })
            .collect();
        let _epoch = *ws.sources.graph_epoch.read();
        let _status = *ws.sources.code_graph_status_epoch.read();
        let force_memory = ws.settings.resolved.read().code_graph
            == moonkale_ext_api::settings::CodeGraphSource::InMemory;
        let selected = status_folder.read().clone();
        async move {
            let mut folders = Vec::new();
            for (index, truth, folder) in indices {
                let mut messages = Vec::new();
                messages.push(if truth == moonkale_core::SourceTruth::Store {
                    t!(ws, L, "status-truth-store")
                } else {
                    t!(ws, L, "status-truth-files")
                });
                if truth == moonkale_core::SourceTruth::Store {
                    messages.push(t!(ws, L, "status-store-read-only"));
                    messages.push(t!(ws, L, "status-code-memory"));
                    folders.push((folder, messages.join(" · ")));
                    continue;
                }
                if force_memory {
                    messages.push(t!(ws, L, "status-code-memory"));
                    folders.push((folder, messages.join(" · ")));
                    continue;
                }
                let reason = match index
                    .query(moonkale_core::Query::Text {
                        dialect: "code-graph-status".into(),
                        text: String::new(),
                    })
                    .await
                {
                    Ok(result) => result
                        .table
                        .and_then(|t| t.rows.first().and_then(|r| r.first()).cloned())
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "cache status unavailable".into()),
                    Err(_) => "Sophia unavailable in this build".into(),
                };
                messages.push(if reason.is_empty() {
                    t!(ws, L, "status-code-cache")
                } else {
                    t!(ws, L, "status-code-fallback", reason = reason)
                });
                folders.push((folder, messages.join(" · ")));
            }
            let message = folders
                .iter()
                .find(|(folder, _)| Some(&folder.id) == selected.as_ref())
                .or_else(|| folders.first())
                .map(|(_, message)| message.clone())
                .unwrap_or_default();
            let tooltip = folders
                .iter()
                .map(|(folder, message)| format!("{}: {message}", folder.display_name))
                .collect::<Vec<_>>()
                .join("\n");
            (message, tooltip)
        }
    });
    let exts: Rc<Vec<Box<dyn Extension>>> = use_context::<Extensions_>().0;
    // Controlled layout so "View → Reset Layout" can put it back.
    let mut layout = use_signal(default_layout);
    use_effect(move || {
        if *ws.crashes.safe.read() {
            layout.set(default_layout());
        }
    });
    // Phone-sized shell: one tile, a bottom bar, nothing persisted. The
    // media query decides; both platforms start wide so hydration matches.
    let mut narrow = use_signal(|| false);
    let mut phone_layout = use_signal(narrow_layout);
    // Started from `onmounted` (an event, so the webview is up — P-047);
    // an eval created in a hook was silently lost on desktop.
    let mut watching = use_signal(|| false);
    let start_narrow_watch = move |_| {
        if *watching.peek() {
            return;
        }
        watching.set(true);
        let mut ev = document::eval(&format!("watch({NARROW_MAX_PX});\n{NARROW_WATCH}"));
        spawn(async move {
            loop {
                match ev.recv::<bool>().await {
                    Ok(v) => {
                        if *narrow.peek() != v {
                            tracing::info!("shell: narrow = {v}");
                            narrow.set(v);
                        }
                    }
                    Err(dioxus::document::EvalError::Serialization(_)) => continue,
                    Err(_) => break,
                }
            }
        });
    };

    // Layout persistence: restore the workspace's layout when its settings
    // load; save every settled change into `.moonkale/settings.json`.
    let mut restored_for: Signal<Option<moonkale_core::SourceId>> = use_signal(|| None);
    use_effect(move || {
        let folder = ws.settings.folder.read().clone();
        if *ws.saved_sessions.switching.read()
            || *ws.crashes.safe.read()
            || folder.is_none()
            || *restored_for.peek() == folder
        {
            return;
        }
        restored_for.set(folder);
        if let Some(encoded) = ws.settings.resolved.peek().layout.clone() {
            match PanelLayout::decode(&encoded) {
                Some(saved) => {
                    tracing::info!("settings: restoring layout");
                    layout.set(saved);
                }
                None => tracing::warn!("settings: saved layout could not be decoded"),
            }
        }
    });
    let on_layout_change = move |next: PanelLayout| {
        if *ws.saved_sessions.switching.peek()
            || *ws.crashes.safe.peek()
            || ws.settings.folder.peek().is_none()
            || *narrow.peek()
        {
            return;
        }
        let encoded = next.encode();
        if encoded == ws.settings.workspace.peek().layout {
            return;
        }
        spawn(async move {
            ws.update_workspace_settings(|f| f.layout = encoded).await;
        });
    };

    // Shell-level commands.
    // Each command is handled once, by its sequence number: the branches
    // read workspace signals (`contributions` reads the documents), and a
    // re-run for one of those must not replay the last command — a dirty
    // dot re-ran `ShowPanel` and sent the foreground tab back (P-183).
    let exts_for_commands = exts.clone();
    let mut handled_command = use_signal(|| None::<u64>);
    use_effect(move || {
        let exts = &exts_for_commands;
        let (seq, cmd) = *ws.shell.commands.read();
        if *handled_command.peek() == Some(seq) {
            return;
        }
        handled_command.set(Some(seq));
        match cmd {
            Some(Command::SplitEditor) => {
                let mut target = if *narrow.peek() { phone_layout } else { layout };
                let mut next = target.peek().clone();
                let closed = ws.shell.closed_panels.peek().clone();
                let settings = ws.settings.resolved.peek().extensions.clone();
                let placements: Vec<_> = contributions(exts, &settings, ws, false)
                    .into_iter()
                    .map(|(_, contribution)| contribution)
                    .filter(|contribution| {
                        contribution.node.is_some() || !closed.contains(&contribution.id)
                    })
                    .map(|contribution| {
                        let home = home_tile(*narrow.peek(), contribution.home);
                        PanelPlacement::new(PanelId::from(contribution.id), TileId::from(home))
                            .with_zone(home_zone(&next, home))
                    })
                    .collect();
                next.reconcile(&placements);
                let active = *ws.docs.active.peek();
                let panel = active.map(|id| PanelId::from(format!("editor:{id}")));
                if let Some(tile) = panel.and_then(|panel| next.tile_for_panel(&panel)) {
                    if next.split_active(&tile, DockZone::Right) {
                        target.set(next);
                    }
                }
            }
            Some(Command::ResetLayout) => {
                layout.set(default_layout());
                if !*ws.crashes.safe.peek() && ws.settings.folder.peek().is_some() {
                    spawn(async move {
                        ws.update_workspace_settings(|f| f.layout = None).await;
                    });
                }
            }
            Some(Command::ShowPanel(id)) => {
                // Our copy of the layout only learns about attached panels
                // through mutations, so reconcile with the current
                // contributions first (the workbench does the same on click).
                ws.shell.closed_panels.with_mut(|c| {
                    c.remove(id);
                });
                let ext_settings = ws.settings.resolved.peek().extensions.clone();
                let is_narrow = *narrow.peek();
                let mut target = if is_narrow { phone_layout } else { layout };
                let mut next = target.peek().clone();
                let closed = ws.shell.closed_panels.peek().clone();
                let placements: Vec<PanelPlacement> = contributions(exts, &ext_settings, ws, false)
                    .into_iter()
                    .map(|(_, c)| c)
                    .filter(|c| c.node.is_some() || !closed.contains(&c.id))
                    .map(|c| {
                        let home = home_tile(is_narrow, c.home);
                        PanelPlacement::new(PanelId::from(c.id.as_str()), TileId::from(home))
                            .with_zone(home_zone(&next, home))
                    })
                    .collect();
                let before = next.split_ids();
                next.reconcile(&placements);
                // A home tile that had been pruned (every panel in it closed,
                // spec 011) comes back as a fresh edge split at 0.5: give it
                // the default proportion of that edge.
                let home = contributions(exts, &ext_settings, ws, false)
                    .into_iter()
                    .map(|(_, c)| c)
                    .find(|c| c.id == id)
                    .map(|c| home_tile(is_narrow, c.home));
                if let Some(home) = home {
                    for sid in next.split_ids() {
                        if !before.contains(&sid) {
                            let ratio = match home {
                                "side" => 0.22,
                                "bottom" => 0.72,
                                "right" => 0.7,
                                _ => 0.5,
                            };
                            next.set_split_ratio(&sid, ratio);
                        }
                    }
                }
                if next.activate(&PanelId::from(id)) {
                    target.set(next);
                }
                let _ = document::eval(&format!(
                    "window.moonkaleFocusPanel?.({});",
                    serde_json::to_string(id).unwrap_or_default()
                ));
            }
            Some(Command::OpenFolder) => {
                if !ws.has_folder_dialog() {
                    ws.show_panel("explorer");
                    dioxus_workbench::focus_after_render("mk-explorer-path");
                    return;
                }
                spawn(async move {
                    if let Err(e) = ws.open_folder_dialog().await {
                        ws.set_status(t!(
                            ws,
                            L,
                            "status-open-folder-failed",
                            error = e.to_string()
                        ));
                    }
                });
            }
            // Ctrl+B / Ctrl+J (spec 009): hide every panel of the tile and
            // remember them; the next toggle shows them again.
            Some(Command::ToggleSide) | Some(Command::ToggleBottom) => {
                let tile = if matches!(cmd, Some(Command::ToggleSide)) {
                    "side"
                } else {
                    "bottom"
                };
                let remembered = ws
                    .shell
                    .hidden_tiles
                    .peek()
                    .get(tile)
                    .cloned()
                    .unwrap_or_default();
                let ext_settings = ws.settings.resolved.peek().extensions.clone();
                let closed = ws.shell.closed_panels.peek().clone();
                let visible: Vec<String> = contributions(exts, &ext_settings, ws, false)
                    .into_iter()
                    .map(|(_, c)| c)
                    .filter(|c| {
                        c.node.is_none() && !closed.contains(&c.id) && c.home.tile_id() == tile
                    })
                    .map(|c| c.id)
                    .collect();
                if !visible.is_empty() {
                    ws.shell.hidden_tiles.with_mut(|h| {
                        h.insert(tile.to_string(), visible.clone());
                    });
                    ws.shell.closed_panels.with_mut(|c| c.extend(visible));
                } else if !remembered.is_empty() {
                    ws.shell.hidden_tiles.with_mut(|h| {
                        h.remove(tile);
                    });
                    let first = remembered[0].clone();
                    ws.shell.closed_panels.with_mut(|c| {
                        for id in &remembered {
                            c.remove(id);
                        }
                    });
                    ws.show_panel(&first);
                } else {
                    ws.set_status(t!(ws, L, "status-nothing-in-tile", tile = tile.to_string()));
                }
            }
            Some(Command::SaveAll) => {
                let dirty: Vec<moonkale_core::NodeId> = ws
                    .docs
                    .open
                    .peek()
                    .iter()
                    .filter(|(_, d)| d.peek().dirty())
                    .map(|(id, _)| *id)
                    .collect();
                spawn(async move {
                    let mut n = 0;
                    for id in dirty {
                        if ws.save(id).await.is_ok() {
                            n += 1;
                        }
                    }
                    ws.set_status(t!(ws, L, "status-saved", n = n));
                });
            }
            Some(Command::CloseEditor) => {
                let panel = ws.links.focused.peek().clone();
                if !panel.as_ref().is_some_and(|p| ws.close_content_panel(p)) {
                    if let Some(node) = *ws.docs.active.peek() {
                        ws.close_node(node);
                    }
                }
            }
            Some(Command::CloseAllEditors) => {
                let clean: Vec<moonkale_core::NodeId> = ws
                    .docs
                    .open
                    .peek()
                    .iter()
                    .filter(|(_, d)| !d.peek().dirty())
                    .map(|(id, _)| *id)
                    .collect();
                let kept = ws.docs.open.peek().len() - clean.len();
                for id in clean {
                    ws.close_node(id);
                }
                let views: Vec<moonkale_core::NodeId> =
                    ws.docs.views.peek().iter().map(|n| n.id).collect();
                for id in views {
                    ws.close_node(id);
                }
                if kept > 0 {
                    ws.set_status(t!(ws, L, "status-kept-open", n = kept));
                }
            }
            Some(Command::CloseFolder) => {
                let first = ws
                    .sources
                    .open
                    .peek()
                    .iter()
                    .find(|s| s.descriptor.family == moonkale_core::SourceFamily::Folder)
                    .map(|s| s.descriptor.id.clone());
                match first {
                    Some(id) => {
                        spawn(async move {
                            let _ = ws.close_source(&id).await;
                        });
                    }
                    None => ws.set_status(t!(ws, L, "status-no-folder")),
                }
            }
            Some(Command::Docs) => {
                let _ = dioxus::document::eval(
                    "window.open('https://mathstruct.org/Moonkale/', '_blank');",
                );
            }
            Some(Command::About) => {
                ws.set_status(t!(ws, L, "status-about", version = moonkale_core::VERSION))
            }
            _ => {}
        }
    });

    let mut tab_navigation = use_signal(|| None::<moonkale_core::NodeId>);

    let mut placed_links = use_signal(std::collections::BTreeSet::<String>::new);
    use_effect({
        let exts = exts.clone();
        move || {
            if *ws.saved_sessions.switching.read() {
                return;
            }
            let saved_layout = ws
                .settings
                .workspace
                .peek()
                .layout
                .as_deref()
                .and_then(PanelLayout::decode);
            let mut changed_layout = None;
            let groups = ws.links.groups.read().clone();
            let session = ws.saved_sessions.active.read().clone();
            for group in &groups {
                let key = format!("{session}:{}", group.id);
                if placed_links.peek().contains(&key) {
                    continue;
                }
                if group.agent.is_some() {
                    // Attach agent panels in the background. Reconciliation below
                    // preserves every existing tile's active tab and DOM focus.
                    placed_links.with_mut(|ids| {
                        ids.insert(key);
                    });
                    continue;
                }
                let lead = group
                    .members
                    .iter()
                    .find(|(_, r, _)| *r == moonkale_ext_api::links::Role::Lead)
                    .map(|(p, _, _)| PanelId::from(p.as_str()));
                let graph = group
                    .members
                    .iter()
                    .find(|(p, _, _)| p.starts_with("graph:"))
                    .map(|(p, _, _)| PanelId::from(p.as_str()));
                let (Some(lead), Some(graph)) = (lead, graph) else {
                    continue;
                };
                let mut target = if *narrow.peek() { phone_layout } else { layout };
                let mut next = target.peek().clone();
                let already_placed = next.tile_for_panel(&graph).is_some();
                let settings = ws.settings.resolved.peek().extensions.clone();
                let placements = contributions(&exts, &settings, ws, false)
                    .into_iter()
                    .map(|(_, c)| {
                        PanelPlacement::new(
                            PanelId::from(c.id),
                            TileId::from(home_tile(*narrow.peek(), c.home)),
                        )
                    })
                    .collect::<Vec<_>>();
                next.reconcile(&placements);
                let retained = saved_layout
                    .as_ref()
                    .is_some_and(|saved| saved.tile_for_panel(&graph).is_some());
                if !already_placed || !retained {
                    // Keep the new pair beside the previous pair, so both
                    // leads remain visible instead of sharing one tab strip.
                    if let Some(other) = groups
                        .iter()
                        .filter(|g| {
                            g.id != group.id
                                && placed_links.peek().contains(&format!("{session}:{}", g.id))
                        })
                        .flat_map(|g| &g.members)
                        .find(|(p, _, _)| {
                            p.starts_with("graph:")
                                && next.tile_for_panel(&PanelId::from(p.as_str())).is_some()
                        })
                    {
                        if let Some(tile) = next.tile_for_panel(&PanelId::from(other.0.as_str())) {
                            next.dock_panel(&lead, &tile, DockZone::Right);
                        }
                    }
                    if let Some(tile) = next.tile_for_panel(&lead) {
                        next.dock_panel(&graph, &tile, DockZone::Right);
                    }
                    balance_link_tiles(&mut next.root);
                    next.activate(&lead);
                    changed_layout = Some(next.clone());
                    target.set(next);
                    ws.links.focused.set(Some(lead.to_string()));
                    ws.focus_element(&format!("wb-tab-{}", lead.as_str().replace(':', "-")));
                }
                placed_links.with_mut(|ids| {
                    ids.insert(key);
                });
            }
            if let Some(next) = changed_layout {
                on_layout_change(next);
            }
        }
    });

    // Collect panels from every extension. `panels()` reads workspace
    // signals, so this render re-runs when documents open/close or turn dirty.
    let mut panels = Vec::new();
    let mut owner: HashMap<String, usize> = HashMap::new();
    let mut active_panel: Option<PanelId> = None;
    let active_node = *ws.docs.active.read();
    let linked_activation = ws
        .links
        .focused
        .peek()
        .as_ref()
        .and_then(|panel| ws.panel_group(panel))
        .filter(|group| Some(group.subject.node) == active_node)
        .and_then(|group| {
            group
                .members
                .into_iter()
                .find(|(_, role, _)| *role == moonkale_ext_api::links::Role::Lead)
                .map(|(panel, _, _)| panel)
        });
    // Activation is a navigation edge. Workbench remounts on layout changes;
    // replaying the last document here would override its retained active tabs.
    let activate_document = use_document_activation(active_node);
    let is_narrow = *narrow.read();
    // Disabled extensions (Settings → Extensions) contribute nothing.
    let ext_settings = ws.settings.resolved.read().extensions.clone();
    // Block libraries for the flow editor, from the enabled extensions.
    {
        let libs: Vec<moonkale_ext_api::flow::FlowLibrary> = exts
            .iter()
            .filter(|e| ext_settings.is_enabled(&e.manifest()) && ws.crashes.allows(&e.manifest()))
            .flat_map(|e| e.flow_libraries())
            .collect();
        if *ws.contrib.flow_libraries.peek() != libs {
            ws.contrib.flow_libraries.set(libs);
        }
    }
    // Panel ids present this render, for the phone bar.
    let mut present: Vec<String> = Vec::new();
    // Static panels the user closed (spec 011) contribute nothing.
    let closed = ws.shell.closed_panels.read().clone();
    // Whether a contribution is a static panel (no document behind it).
    let mut is_static: HashMap<String, bool> = HashMap::new();
    let mut document_nodes = HashMap::new();
    let current_layout = if is_narrow {
        phone_layout.read().clone()
    } else {
        layout.read().clone()
    };
    // The panel of the most recently active document, for the phone bar's
    // "Editor" button.
    let mut editor_panel: Option<String> = None;
    // Activity-bar entries (spec 009): every static panel that declares one,
    // closed or not — the bar is how a closed panel comes back.
    let mut activities: Vec<(String, moonkale_ext_api::Activity)> = Vec::new();
    let mut offered_activities = Vec::new();
    let detailed =
        ws.settings.resolved.read().density == moonkale_ext_api::settings::Density::Detailed;
    for (i, c) in contributions(&exts, &ext_settings, ws, true) {
        {
            if let (None, Some(act)) = (&c.node, &c.activity) {
                if moonkale_ext_api::icons::chrome_visible(ws, &exts[i].manifest()) {
                    activities.push((c.id.clone(), act.clone()));
                } else {
                    offered_activities.push((c.id.clone(), act.clone()));
                }
            }
            if c.node.is_none() && closed.contains(&c.id) {
                continue;
            }
            owner.insert(c.id.clone(), i);
            if let Some(node) = c.node {
                document_nodes.insert(c.id.clone(), node);
            }
            is_static.insert(c.id.clone(), c.node.is_none());
            present.push(c.id.clone());
            if c.node.is_some()
                && c.node == active_node
                && linked_activation.as_ref().is_none_or(|id| id == &c.id)
            {
                if activate_document {
                    active_panel = Some(PanelId::from(c.id.as_str()));
                }
                editor_panel = Some(c.id.clone());
            }
            let home = home_tile(is_narrow, c.home);
            let content = rsx! { ExtensionPanel {
                key: "{c.id}", ws, index: i, panel_id: c.id.clone(), title: c.title.clone()
            } };
            let mut panel = Panel::new(c.id.as_str(), c.title.as_str(), home, content)
                .with_closable(c.closable)
                .with_home_zone(home_zone(&current_layout, home));
            let link = ws.panel_group(&c.id);
            if let Some(group) = &link {
                panel = panel.with_class(format!("mk-link-{}", group.colour));
            }
            // Tab accessories: the unsaved dot, and the git status letter of
            // the document's file (Milestone 7).
            let vcs = c.node.and_then(|n| ws.document(n)).and_then(|d| {
                let key = d.peek().node.native_key.clone();
                ws.contrib.file_marks.read().get(&key).cloned()
            });
            // Others looking at this document (presence, Milestone 8).
            let here: Vec<moonkale_ext_api::presence::Member> =
                match c.node.and_then(|n| ws.document(n)) {
                    Some(d) => {
                        let key = d.peek().node.native_key.clone();
                        ws.others()
                            .into_iter()
                            .filter(|m| m.active.as_deref() == Some(key.as_str()))
                            .collect()
                    }
                    None => Vec::new(),
                };
            if c.dirty || vcs.is_some() || !here.is_empty() || link.is_some() || c.id == "graph" {
                let dirty = c.dirty;
                panel = panel.with_tab_accessory(rsx! {
                    if let Some(group) = link { span { class: "mk-link-marker mk-link-{group.colour}", "{group.marker()}" if let Some(name) = group.agent { " {name}" } } }
                    else if c.id == "graph" { span { class: "mk-link-marker", title: t!(ws, L, "link-active"), "↯" } }
                    if dirty { span { class: "mk-tab-dirty", "aria-label": "Unsaved changes" } }
                    if let Some(m) = vcs { span { class: "mk-tab-mark {m.class}", "data-status": "{m.letter}", title: "{m.title}", "{m.letter}" } }
                    for m in here.iter() {
                        span { class: "mk-tab-presence", title: "{m.name} has this open", "{m.initials()}" }
                    }
                });
            }
            panels.push(panel);
        }
    }

    // The workbench activates newly attached panels by default. Reconcile
    // background document loads here and retain each tile's chosen tab.
    let agent_panels = ws
        .links
        .groups
        .peek()
        .iter()
        .filter(|g| g.agent.is_some())
        .flat_map(|g| g.members.iter().map(|(id, _, _)| id.clone()))
        .collect();
    let display_request = ws.links.display_request.read().clone();
    let focused_panel = ws.links.focused.peek().clone();
    if let Some(next) = attach_background_panels(
        &current_layout,
        &panels,
        &document_nodes,
        active_node,
        &agent_panels,
        display_request.as_deref(),
        focused_panel.as_deref(),
    ) {
        if is_narrow {
            phone_layout.set(next);
        } else {
            layout.set(next);
        }
    }

    if display_request.is_some() {
        ws.links.display_request.set(None);
    }

    // Document navigation opens/focuses its concrete editor. Retained-tab
    // keyboard navigation keeps focus on the tab so another arrow can follow.
    let focus_editor = editor_panel.clone();
    let focused_document = use_hook(|| Rc::new(std::cell::Cell::new(None)));
    use_effect(move || {
        let active = *ws.docs.active.read();
        if focused_document.replace(active) == active {
            return;
        }
        let from_tab = tab_navigation.peek().is_some() && *tab_navigation.peek() == active;
        tab_navigation.set(None);
        if active.is_some() && !from_tab {
            if let Some(id) = &focus_editor {
                let _ = document::eval(&format!(
                    "window.moonkaleFocusPanel?.({}, true);",
                    serde_json::to_string(id).unwrap_or_default()
                ));
            }
        }
    });

    // A user-selected retained tab is foreground navigation too. Programmatic
    // activation during restore does not call the workbench's event callback.
    let overview_nodes = document_nodes.clone();
    let on_activate = move |id: PanelId| {
        if ws.links.focused.peek().as_deref() != Some(id.as_str()) {
            ws.links.focused.set(Some(id.to_string()));
        }
        if let Some(node) = document_nodes
            .get(id.as_str())
            .and_then(|node| ws.open_node_by_id(*node))
        {
            // Focus restoration can reactivate the already-active document's
            // retained tab. Do not launch a redundant async open that could
            // complete after that document has subsequently been closed.
            if *ws.docs.active.peek() == Some(node.id) {
                return;
            }
            tab_navigation.set(Some(node.id));
            spawn(async move {
                let _ = ws.open_node(node).await;
            });
        }
    };

    let overview_owner = owner.clone();
    let on_close = {
        let exts = exts.clone();
        move |id: PanelId| {
            let id_str = id.to_string();
            if ws.close_content_panel(&id_str) {
                return;
            }
            ws.close_linked_panel(&id_str);
            if let Some(&i) = owner.get(&id_str) {
                exts[i].on_panel_closed(&id_str, ws);
            }
            // A static panel stays closed until View → Show / the activity
            // bar / the palette brings it back (spec 011); documents close
            // through their extension.
            if is_static.get(&id_str).copied().unwrap_or(false) {
                ws.shell.closed_panels.with_mut(|c| {
                    c.insert(id_str);
                });
            }
        }
    };

    let mut overview_tabs = panels
        .iter()
        .map(|p| {
            crate::overview::Tab::new(
                ws,
                p.id.to_string(),
                p.title.clone(),
                overview_owner.get(p.id.as_str()).copied(),
                overview_nodes.get(p.id.as_str()).copied(),
            )
        })
        .collect::<Vec<_>>();
    let recent_tabs = use_hook(|| {
        Rc::new(std::cell::RefCell::new(
            crate::overview::RecentTabs::default(),
        ))
    });
    recent_tabs.borrow_mut().sort(
        &mut overview_tabs,
        active_node,
        ws.links.focused.read().clone(),
    );
    let mut overview_close = on_close.clone();
    let mut overview_activate = on_activate.clone();
    let overview_exts = exts.clone();
    let overview_switch = move |(id, beside): (String, bool)| {
        let mut target = if *narrow.peek() { phone_layout } else { layout };
        let mut next = target.peek().clone();
        let placements: Vec<_> = contributions(
            &overview_exts,
            &ws.settings.resolved.peek().extensions,
            ws,
            false,
        )
        .into_iter()
        .filter(|(_, c)| c.node.is_some() || !ws.shell.closed_panels.peek().contains(&c.id))
        .map(|(_, c)| {
            PanelPlacement::new(
                PanelId::from(c.id.as_str()),
                TileId::from(home_tile(*narrow.peek(), c.home)),
            )
        })
        .collect();
        next.reconcile(&placements);
        let panel = PanelId::from(id.as_str());
        next.activate(&panel);
        if beside {
            if let Some(tile) = next.tile_for_panel(&panel) {
                next.split_active(&tile, DockZone::Right);
            }
        }
        target.set(next);
        overview_activate(PanelId::from(id.as_str()));
    };
    let link_panels: Vec<_> = panels
        .iter()
        .map(|p| (p.id.to_string(), p.title.clone()))
        .collect();
    let status = ws.shell.status.read().clone();
    let _ = ws.session.presence.read();
    let others = ws.others();
    let others_names = others
        .iter()
        .map(|m| m.name.clone())
        .collect::<Vec<_>>()
        .join(", ");
    let others_badges: Vec<(String, String)> = others
        .iter()
        .map(|m| {
            (
                m.initials(),
                format!(
                    "{}{}{}",
                    m.name,
                    m.active
                        .as_ref()
                        .map(|a| format!(" · {a}"))
                        .unwrap_or_default(),
                    m.line.map(|l| format!(":{}", l + 1)).unwrap_or_default()
                ),
            )
        })
        .collect();
    let lsp = ws.processes.lsp_status.read().clone();
    // The remote session (Milestone 11): "⇅ host:path · connected".
    let remote = ws
        .remote
        .ssh
        .read()
        .as_ref()
        .map(|r| (r.label(), r.phase.label()));
    let server = ws.remote.server.read().as_ref().map(|(u, _)| u.clone());
    let windows = ws.session.peers.read().len() + 1;
    let window_id = ws.session.window.read().to_string();
    let source_name = ws
        .sources
        .open
        .read()
        .first()
        .map(|s| s.descriptor.display_name.clone());
    // The first source's accent (spec 009), matching its Explorer stripe.
    let source_color: Option<&'static str> = ws
        .sources
        .open
        .read()
        .first()
        .map(|s| crate::icons::source_color(s.descriptor.id.as_str()));
    let active_doc = active_node.and_then(|n| ws.document(n)).map(|d| {
        let d = d.read();
        (d.node.native_key.clone(), d.dirty())
    });

    // Phone bar: which panel is in front, and the buttons that have a panel.
    // Our copy of the phone layout only learns about attached panels through
    // mutations (the workbench renders a reconciled copy), so reconcile with
    // this render's panels both to read the front tab and before activating.
    let placements: Vec<PanelPlacement> = present
        .iter()
        .map(|id| PanelPlacement::new(PanelId::from(id.as_str()), TileId::from("main")))
        .collect();
    let front = {
        let mut l = phone_layout.read().clone();
        l.reconcile(&placements);
        l.tile(&TileId::from("main"))
            .and_then(|t| t.active.clone())
            .map(|p| p.to_string())
    };
    let placements = Rc::new(placements);
    let show = move |id: &str| {
        let mut next = phone_layout.peek().clone();
        next.reconcile(&placements);
        if next.activate(&PanelId::from(id)) {
            phone_layout.set(next);
        }
    };
    activities.sort_by(|a, b| a.1.order.cmp(&b.1.order).then_with(|| a.0.cmp(&b.0)));
    // Which activity is "current": the one whose panel is the front tab of
    // its tile (desktop), or the front panel (phone).
    let front_of = |id: &str| -> bool {
        if is_narrow {
            front.as_deref() == Some(id)
        } else {
            let l = current_layout.clone();
            l.tile_for_panel(&PanelId::from(id))
                .and_then(|t| l.tile(&t).and_then(|tile| tile.active.clone()))
                .is_some_and(|p| p.to_string() == id)
                && !closed.contains(id)
        }
    };
    // Phone bar (spec 009): the same registry — Files, Search, Editor, then
    // the rest in order, Settings last; entries past six go into "More".
    // Phone bar (spec 009): primary entries in order (the editor after
    // Search) fill the bar up to five; secondary ones and the overflow go to
    // the More sheet.
    type BarEntry = (String, String, &'static str, bool, u32);
    let (phone_main, mut phone_more): (Vec<BarEntry>, Vec<BarEntry>) = if is_narrow {
        let mut main = Vec::new();
        let mut more = Vec::new();
        let mut primary = 0;
        let editor_front = front.as_deref().is_some_and(|f| f.starts_with("editor:"));
        for (id, act) in activities.iter() {
            let entry = (
                id.clone(),
                act.label.clone(),
                act.icon,
                front_of(id),
                act.badge,
            );
            if act.phone_secondary || primary >= 5 {
                more.push(entry);
            } else {
                main.push(entry);
                primary += 1;
            }
            if act.order == 20 {
                main.push((
                    editor_panel.clone().unwrap_or_default(),
                    t!(ws, L, "editor"),
                    "editor",
                    editor_front,
                    0,
                ));
            }
        }
        (main, more)
    } else {
        (Vec::new(), Vec::new())
    };
    if is_narrow {
        phone_more.extend(offered_activities.iter().map(|(id, act)| {
            (
                id.clone(),
                act.label.clone(),
                act.icon,
                front_of(id),
                act.badge,
            )
        }));
    }
    let mut more_open = use_signal(|| false);
    let rail: Vec<(String, String, &'static str, bool, u32, bool)> = if is_narrow {
        Vec::new()
    } else {
        activities
            .iter()
            .map(|(id, act)| {
                (
                    id.clone(),
                    act.label.clone(),
                    act.icon,
                    front_of(id),
                    act.badge,
                    act.order >= 900,
                )
            })
            .collect()
    };
    let mut rail = rail;
    let mut rail_more = Vec::new();
    if !detailed {
        // Reserve one place for More; presence moves into that sheet. Entries
        // pinned to the bottom (Settings, Extensions) stay; the top ones are trimmed.
        let (top, bottom): (Vec<_>, Vec<_>) = rail.into_iter().partition(|e| !e.5);
        let room = 6usize.saturating_sub(bottom.len());
        let mut top = top;
        if top.len() > room {
            rail_more = top.split_off(room);
        }
        rail = top.into_iter().chain(bottom).collect();
        for (id, act) in offered_activities {
            rail_more.push((id, act.label, act.icon, false, act.badge, false));
        }
    }
    let mut rail_more_open = use_signal(|| false);
    let others_count = others.len() as u32;
    // Only "dark" has tokens so far (theme.css); spec 030 adds the others.
    let theme = ws.settings.resolved.read().theme.clone();

    rsx! {
        moonkale_ext_api::Stylesheet { href: SHELL_CSS }
        crate::linked_panels::LinkedPanels { ws, panels: link_panels }
        crate::palette::Palette { tabs: overview_tabs, on_switch: overview_switch, on_close: move |id: String| overview_close(PanelId::from(id.as_str())) }
        div { class: if is_narrow { "mk-shell mk-narrow" } else { "mk-shell" }, "data-theme": "{theme}", onmounted: start_narrow_watch,
            Workbench {
                rail: rsx! {
                    if !is_narrow {
                        ActivityRail {
                            for (id, label, icon, active, badge, bottom) in rail.iter().cloned() {
                                ActivityButton {
                                    id: format!("mk-rail-{id}"),
                                    label: label.clone(),
                                    title: moonkale_ext_api::command_hint(&if id == "settings" { "view.settings".to_string() } else { format!("view.panel.{id}") }, t!(ws, L, "rail-title", label = label.clone())).title,
                                    icon: rsx! {
                                        span { class: "mk-rail-icon",
                                            "data-shortcut": moonkale_ext_api::command_hint(&if id == "settings" { "view.settings".to_string() } else { format!("view.panel.{id}") }, "").shortcut,
                                            crate::icons::Icon { name: icon }
                                            if badge > 0 {
                                                span { class: "mk-badge", "data-count": "{badge}", if badge > 99 { "99+" } else { "{badge}" } }
                                            }
                                        }
                                    },
                                    active,
                                    bottom,
                                    onclick: {
                                        let id = id.clone();
                                        move |_| {
                                            // Click on the current entry hides it (spec 009);
                                            // otherwise show (reopening if closed).
                                            if active {
                                                ws.shell.closed_panels.with_mut(|c| { c.insert(id.clone()); });
                                            } else {
                                                ws.show_panel(&id);
                                            }
                                        }
                                    },
                                }
                            }
                            if !detailed {
                                ActivityButton {
                                    id: "mk-rail-more".to_string(), label: t!(ws, L, "more"), title: t!(ws, L, "chrome-more"),
                                    icon: rsx! { crate::icons::Icon { name: "more" } }, active: rail_more_open(), bottom: true,
                                    onclick: move |_| rail_more_open.toggle(),
                                }
                            }
                            if detailed {
                            // Presence: who else is here (spec 009).
                            ActivityButton {
                                id: "mk-rail-presence".to_string(),
                                label: t!(ws, L, "people"),
                                title: if others_names.is_empty() { t!(ws, L, "people-nobody") } else { t!(ws, L, "people-here", names = others_names.clone()) },
                                icon: rsx! {
                                    span { class: "mk-rail-icon",
                                        crate::icons::Icon { name: "presence" }
                                        if others_count > 0 { span { class: "mk-badge", "{others_count}" } }
                                    }
                                },
                                active: false,
                                bottom: true,
                                onclick: {
                                    let names = others_names.clone();
                                    move |_| ws.set_status(if names.is_empty() { t!(ws, L, "people-nobody-folder") } else { t!(ws, L, "people-here", names = names.clone()) })
                                },
                            }
                            }
                        }
                        if rail_more_open() && !detailed {
                            div { class: "mk-rail-overflow", role: "dialog", aria_label: t!(ws, L, "chrome-more"), tabindex: "-1",
                                onmounted: move |e| { spawn(async move { let _ = e.set_focus(true).await; }); },
                                onkeydown: move |e| { if e.key() == Key::Escape { e.stop_propagation(); rail_more_open.set(false); } },
                                for (id, label, _, _, _, _) in rail_more.iter().cloned() {
                                    button { id: "mk-rail-{id}", class: "mk-btn", key: "{id}", "data-panel": id.clone(),
                                        onclick: move |_| { ws.show_panel(&id); rail_more_open.set(false); },
                                        {moonkale_ext_api::command_hint(&format!("view.panel.{id}"), label).title}
                                    }
                                }
                                button { class: "mk-btn", onclick: { let names = others_names.clone(); move |_| { ws.set_status(if names.is_empty() { t!(ws, L, "people-nobody") } else { t!(ws, L, "people-here", names = names.clone()) }); rail_more_open.set(false); } }, {t!(ws, L, "people")} }
                            }
                        }
                    }
                },
                status: rsx! {
                    StatusBar {
                        left: rsx! {
                            StatusItem {
                                if let Some(color) = source_color {
                                    span { class: "mk-source-dot", style: "background: {color};" }
                                } else {
                                    StatusDot { tone: StatusTone::Neutral }
                                }
                                {source_name.clone().unwrap_or_else(|| t!(ws, L, "status-no-folder-open"))}
                            }
                        },
                        message: rsx! { StatusMessage { if status.is_empty() { {t!(ws, L, "status-ready")} } else { "{status}" } } },
                        right: rsx! {
                            if let Some((message, tooltip)) = cache_status.read().as_ref().filter(|(s, _)| !s.is_empty()) {
                                StatusItem { title: tooltip.clone(), span { class: "mk-status-folder", "{message}" } }
                            }
                            if let Some(url) = server.clone() {
                                StatusItem { title: t!(ws, L, "status-server-client"),
                                    span { class: "mk-status-remote", "⇅ {url}" }
                                }
                            }
                            if let Some((label, phase)) = remote.filter(|_| server.is_none() || detailed) {
                                StatusItem { title: t!(ws, L, "status-remote", phase = phase.to_string()),
                                    span { class: "mk-status-remote", "⇅ {label} · {phase}" }
                                }
                            }
                            if detailed {
                            if let Some(l) = lsp {
                                StatusItem { title: t!(ws, L, "status-lsp"), "{l}" }
                            }
                            if !others.is_empty() {
                                StatusItem { title: t!(ws, L, "status-others", names = others_names.clone()),
                                    span { class: "mk-presence", "data-count": "{others.len()}",
                                        "👥 "
                                        for (initials, title) in others_badges.iter() {
                                            span { class: "mk-presence-badge", title: "{title}", "{initials}" }
                                        }
                                    }
                                }
                            }
                            StatusItem { title: t!(ws, L, "status-window-title", id = window_id.to_string()),
                                {t!(ws, L, "status-windows", n = windows)}
                            }
                            }
                            if let Some((path, dirty)) = active_doc {
                                StatusItem { tone: if dirty { StatusTone::Caution } else { StatusTone::Neutral },
                                    if dirty { "● " } "{path}"
                                }
                            }
                        },
                    }
                },
                // Two branches so each mode keeps its own controlled signal
                // (the workbench requires the same signal for its lifetime).
                if is_narrow {
                    div { class: "mk-phone",
                        PanelWorkspace {
                            panels,
                            strings: workbench_strings(ws),
                            layout: phone_layout,
                            reset_layout: narrow_layout(),
                            active_panel,
                            on_panel_close: on_close,
                            on_panel_activate: on_activate,
                            on_layout_change: move |_| {},
                        }
                        if more_open() {
                            div { class: "mk-phone-more", onclick: move |_| more_open.set(false),
                                for (id, label, icon, active, badge) in phone_more.iter().cloned() {
                                    button {
                                        class: if active { "mk-phone-btn mk-active" } else { "mk-phone-btn" },
                                        "data-panel": "{id}",
                                        title: moonkale_ext_api::command_hint(&if id == "settings" { "view.settings".to_string() } else { format!("view.panel.{id}") }, label.clone()).title,
                                        onclick: { let mut show = show.clone(); let id = id.clone(); move |_| { more_open.set(false); ws.show_panel(&id); show(&id) } },
                                        span { class: "mk-rail-icon",
                                            "data-shortcut": moonkale_ext_api::command_hint(&if id == "settings" { "view.settings".to_string() } else { format!("view.panel.{id}") }, "").shortcut, crate::icons::Icon { name: icon } if badge > 0 { span { class: "mk-badge", "{badge}" } } }
                                        span { "{label}" }
                                    }
                                }
                            }
                        }
                        nav { class: "mk-phone-bar", "aria-label": "Panels",
                            for (id, label, icon, active, badge) in phone_main.iter().cloned() {
                                button {
                                    class: if active { "mk-phone-btn mk-active" } else { "mk-phone-btn" },
                                    disabled: id.is_empty(),
                                    "data-panel": "{id}",
                                        title: moonkale_ext_api::command_hint(&if id == "settings" { "view.settings".to_string() } else { format!("view.panel.{id}") }, label.clone()).title,
                                    onclick: { let mut show = show.clone(); let id = id.clone(); move |_| { ws.show_panel(&id); show(&id) } },
                                    span { class: "mk-rail-icon",
                                            "data-shortcut": moonkale_ext_api::command_hint(&if id == "settings" { "view.settings".to_string() } else { format!("view.panel.{id}") }, "").shortcut, crate::icons::Icon { name: icon } if badge > 0 { span { class: "mk-badge", "{badge}" } } }
                                    span { "{label}" }
                                }
                            }
                            if !phone_more.is_empty() {
                                button {
                                    class: if more_open() { "mk-phone-btn mk-active" } else { "mk-phone-btn" },
                                    "data-panel": "more",
                                    onclick: move |_| more_open.toggle(),
                                    span { class: "mk-rail-icon", crate::icons::Icon { name: "more" } }
                                    span { {t!(ws, L, "more")} }
                                }
                            }
                        }
                    }
                } else {
                    PanelWorkspace {
                        panels,
                        strings: workbench_strings(ws),
                        layout,
                        reset_layout: default_layout(),
                        active_panel,
                        on_panel_close: on_close,
                        on_panel_activate: on_activate,
                        on_layout_change,
                    }
                }
            }
        }
    }
}

/// Mount beneath the owning extension's guard, so callbacks and tasks
/// created by descendants inherit the same identity.
#[component]
fn ExtensionPanel(ws: Workspace, index: usize, panel_id: String, title: String) -> Element {
    let exts = use_context::<Extensions_>().0;
    let ext = &exts[index];
    let manifest = ext.manifest();
    use_context_provider(|| moonkale_ext_api::links::PanelOrigin(panel_id.clone()));
    use_context_provider(|| moonkale_ext_api::icons::ChromeOwner(manifest.clone()));
    let id = manifest.id;
    let guard = use_context_provider(|| moonkale_ext_api::crash::ExtensionGuard::new(ws, id));
    let render_id = ws
        .linked_content(&panel_id)
        .map(|node| ws.content_panel(node))
        .unwrap_or_else(|| panel_id.clone());
    let content = guard
        .run(|| ext.render(&render_id, ws))
        .unwrap_or_else(VNode::empty);
    let content =
        rsx! { div { key: "{panel_id}:{render_id}", class: "mk-linked-content", {content} } };
    let settings = guard
        .run(|| ext.settings(ws, moonkale_ext_api::SettingsTarget::User))
        .flatten();
    guarded(
        ws,
        title,
        content,
        settings,
        ext.manifest().name.to_string(),
    )
}

// Balance only splits whose two children contain linked panels. The rail,
// sidebar, terminal and manually placed unrelated tiles retain their ratios.
fn balance_link_tiles(node: &mut LayoutNode) -> usize {
    match node {
        LayoutNode::Tile(tile) => usize::from(
            tile.panels
                .iter()
                .any(|p| p.as_str().starts_with("lead:") || p.as_str().starts_with("graph:")),
        ),
        LayoutNode::Split {
            first,
            second,
            ratio,
            ..
        } => {
            let a = balance_link_tiles(first);
            let b = balance_link_tiles(second);
            if a > 0 && b > 0 {
                *ratio = a as f64 / (a + b) as f64;
            }
            a + b
        }
    }
}

/// Where a panel first appears: its declared home, or the single tile of
/// the phone-sized shell.
/// How a panel joins its home when that tile is gone from the layout (all
/// its panels were closed and the tile was pruned): dock at the edge the
/// home stands for, so the side bar comes back on the left, the bottom
/// panel at the bottom. An existing home tile takes it as a tab.
fn home_zone(layout: &PanelLayout, home: &str) -> DockZone {
    if layout.tile(&TileId::from(home)).is_some() {
        return DockZone::Center;
    }
    match home {
        "side" => DockZone::Left,
        "bottom" => DockZone::Bottom,
        "right" => DockZone::Right,
        _ => DockZone::Center,
    }
}

fn home_tile(narrow: bool, home: moonkale_ext_api::PanelHome) -> &'static str {
    if narrow {
        "main"
    } else {
        home.tile_id()
    }
}

/// Remember navigation outside the workbench subtree that remounts on resize.
fn use_document_activation(active: Option<moonkale_core::NodeId>) -> bool {
    let last = use_hook(|| Rc::new(std::cell::Cell::new(None)));
    last.replace(active) != active
}

/// Reports `matchMedia("(max-width: <px>px)")` now and on every change.
const NARROW_WATCH: &str = r#"
function watch(px) {
    const mq = window.matchMedia(`(max-width: ${px}px)`);
    const report = () => {
        // Minimized desktop windows can report an unusably small viewport.
        if (window.innerWidth >= 100 && window.innerHeight >= 100)
            dioxus.send(mq.matches);
    };
    report();
    window.addEventListener("resize", report);
    mq.addEventListener("change", report);
}
for (;;) { await dioxus.recv(); }
"#;

/// The panels the enabled extensions contribute, as `(extension index,
/// contribution)`. A document or view panel (`node: Some`) is kept only from
/// the extension that wins its node (`Extension::claims`, Milestone 18 phase
/// 2): the highest claim, a tie going to `Workspace::preferred_editor`. Only
/// claimants compete: a contribution from an extension that does not claim
/// the node is always kept.
///
/// `tracked`: `true` in the shell's render (a change of the user's editor
/// choice must re-render it), `false` in event handlers and effects, which
/// must not subscribe to the settings.
pub(crate) fn contributions(
    exts: &[Box<dyn Extension>],
    enabled: &moonkale_ext_api::ExtensionsSettings,
    ws: Workspace,
    tracked: bool,
) -> Vec<(usize, moonkale_ext_api::PanelContribution)> {
    let active: Vec<usize> = (0..exts.len())
        .filter(|&i| {
            enabled.is_enabled(&exts[i].manifest()) && ws.crashes.allows(&exts[i].manifest())
        })
        .collect();
    // node → (the extensions that claim it, the winner)
    let mut claims: HashMap<moonkale_core::NodeId, (Vec<usize>, Option<usize>)> = HashMap::new();
    let mut out = Vec::new();
    for &i in &active {
        // P-151: an untracked caller must not subscribe to the language
        // through the panels' translated titles.
        let guard = moonkale_ext_api::crash::ExtensionGuard::new(ws, exts[i].manifest().id);
        let panels = guard
            .run(|| {
                if tracked {
                    exts[i].panels(ws)
                } else {
                    moonkale_ext_api::workspace::untracked(|| exts[i].panels(ws))
                }
            })
            .unwrap_or_default();
        for c in panels {
            if let Some(node) = c.node {
                let (claimants, won) = claims
                    .entry(node)
                    .or_insert_with(|| claim_winner(exts, &active, ws, node, tracked));
                // Only a claimant can lose: an extension showing a node it
                // does not claim (an SVG opened as text from the image
                // viewer's *Source*) keeps its panel.
                if claimants.contains(&i) && won.is_some_and(|w| w != i) {
                    continue;
                }
            }
            out.push((i, c));
        }
    }
    let views = if tracked {
        ws.links.contents.read().clone()
    } else {
        ws.links.contents.peek().clone()
    };
    let effective: Vec<_> = views
        .keys()
        .map(|id| {
            let node = if tracked {
                ws.linked_content(id)
            } else {
                moonkale_ext_api::workspace::untracked(|| ws.linked_content(id))
            };
            (id.clone(), node)
        })
        .collect();
    out.into_iter()
        .flat_map(|(i, c)| {
            let aliases: Vec<_> = effective
                .iter()
                .filter(|(_, node)| c.node.is_some() && *node == c.node)
                .map(|(id, _)| id)
                .collect();
            if aliases.is_empty() {
                if c.node
                    .is_some_and(|node| ws.links.agent_documents.peek().contains(&node))
                {
                    return Vec::new();
                }
                // A linked instance can currently display a different document.
                if views.contains_key(&c.id) {
                    Vec::new()
                } else {
                    vec![(i, c)]
                }
            } else {
                let preserve_person = aliases.iter().all(|id| id.starts_with("lead:agent:"))
                    && c.node
                        .is_some_and(|node| !ws.links.agent_documents.peek().contains(&node));
                let mut panels: Vec<_> = aliases
                    .into_iter()
                    .map(|id| {
                        let mut alias = c.clone();
                        alias.id = id.clone();
                        (i, alias)
                    })
                    .collect();
                if preserve_person {
                    panels.insert(0, (i, c));
                }
                panels
            }
        })
        .collect()
}

fn claim_winner(
    exts: &[Box<dyn Extension>],
    active: &[usize],
    ws: Workspace,
    node: moonkale_core::NodeId,
    tracked: bool,
) -> (Vec<usize>, Option<usize>) {
    let Some(n) = ws.open_node_by_id(node) else {
        return (Vec::new(), None);
    };
    let claims: Vec<(usize, u8)> = active
        .iter()
        .filter_map(|&i| exts[i].claims(&n).map(|p| (i, p)))
        .collect();
    let claimants = claims.iter().map(|c| c.0).collect();
    let Some(top) = claims.iter().map(|c| c.1).max() else {
        return (claimants, None);
    };
    let tied: Vec<usize> = claims.iter().filter(|c| c.1 == top).map(|c| c.0).collect();
    let won = if tied.len() == 1 {
        Some(tied[0])
    } else {
        let preferred = if tracked {
            ws.preferred_editor(node)
        } else {
            ws.preferred_editor_untracked(node)
        };
        tied.iter()
            .copied()
            .find(|&i| exts[i].manifest().id == preferred)
            .or(tied.first().copied())
    };
    (claimants, won)
}

/// The workbench chrome's own words (tab tooltips, empty groups, splitters)
/// in the user's language (spec 030).
fn workbench_strings(ws: Workspace) -> dioxus_workbench::WorkbenchStrings {
    dioxus_workbench::WorkbenchStrings {
        tab_group_label: t!(ws, L, "wb-tab-group"),
        tab_group_labelled: t!(ws, L, "wb-tab-group-labelled"),
        tab_hint: t!(ws, L, "wb-tab-hint"),
        close_tab: t!(ws, L, "wb-close-tab"),
        empty_group_label: t!(ws, L, "wb-empty-group"),
        empty_tile_title: t!(ws, L, "wb-empty-title"),
        empty_tile_hint: t!(ws, L, "wb-empty-hint"),
        empty_tile_keys: [
            ("workspace.openFolder", t!(ws, L, "menu-open-folder")),
            ("view.quickOpen", t!(ws, L, "menu-go-to-file")),
            ("view.palette", t!(ws, L, "menu-palette")),
            ("search.workspace", t!(ws, L, "menu-find-workspace")),
        ]
        .map(|(id, label)| moonkale_ext_api::command_hint(id, label).title)
        .join("\n"),
        split_right_label: t!(ws, L, "wb-split-right"),
        split_right_hint: t!(ws, L, "wb-split-right-hint"),
        split_down_label: t!(ws, L, "wb-split-down"),
        split_down_hint: t!(ws, L, "wb-split-down-hint"),
        close_empty_label: t!(ws, L, "wb-close-empty"),
        close_empty_hint: t!(ws, L, "wb-close-empty-hint"),
        splitter_label: t!(ws, L, "wb-splitter"),
        splitter_hint: t!(ws, L, "wb-splitter-hint"),
    }
}

/// An extension's panel inside an error boundary (2026-10-14): a panic or an
/// error while it renders shows a notice with **Reload** in that panel,
/// instead of blanking the window. Native builds catch render panics
/// (Dioxus turns them into errors); the web client is built with
/// `panic = "abort"`, where only a page reload recovers. The session's
/// saved state and bounded unsaved-buffer recovery use the store (ADR-0026).
pub fn guarded(
    ws: Workspace,
    title: String,
    content: Element,
    settings: Option<Element>,
    extension: String,
) -> Element {
    #[cfg(not(target_arch = "wasm32"))]
    remember_panics();
    rsx! { PanelFrame { ws, title, content, settings, extension } }
}

/// The frame around every extension panel: its error boundary, and its
/// extension's settings as a popover next to it (2026-10-14). The panel's
/// toolbar shows the gear (`moonkale_ext_api::icons::PanelSettingsButton`)
/// through the context provided here; the popover holds the same settings
/// the Extensions panel shows under the extension's row, for the user
/// scope.
#[component]
fn PanelFrame(
    ws: Workspace,
    title: String,
    content: Element,
    settings: Option<Element>,
    extension: String,
) -> Element {
    let mut open = use_signal(|| false);
    let available = settings.is_some();
    let label = t!(ws, L, "panel-settings", name = extension.clone());
    use_context_provider(|| moonkale_ext_api::icons::PanelSettings {
        available,
        open,
        label,
    });
    let boundary = boundary(ws, title.clone(), content);
    rsx! {
        div { class: "mk-panel-frame",
            {boundary}
            if open() {
                if let Some(section) = settings {
                    div {
                        class: "mk-panel-settings",
                        role: "dialog",
                        aria_label: t!(ws, L, "panel-settings", name = extension.clone()),
                        // Dialogs take the focus (ADR-0027 §5), so Escape
                        // closes it straight away.
                        tabindex: "-1",
                        onmounted: move |e| { spawn(async move { let _ = e.data().set_focus(true).await; }); },
                        onkeydown: move |e| {
                            if e.key() == Key::Escape {
                                e.stop_propagation();
                                open.set(false);
                            }
                        },
                        div { class: "mk-panel-settings-head",
                            span { {t!(ws, L, "panel-settings", name = extension.clone())} }
                            moonkale_ext_api::icons::IconButton {
                                icon: "close",
                                label: t!(ws, L, "panel-settings-close"),
                                onclick: move |_| open.set(false),
                            }
                        }
                        div { class: "mk-panel-settings-body", {section} }
                        button { class: "mk-btn mk-panel-settings-all", r#type: "button",
                            onclick: move |_| { open.set(false); ws.dispatch(Command::ShowPanel("extensions")); },
                            {t!(ws, L, "panel-settings-all")}
                        }
                    }
                }
            }
        }
    }
}

/// The error boundary of [`guarded`].
fn boundary(ws: Workspace, title: String, content: Element) -> Element {
    rsx! {
        ErrorBoundary {
            handle_error: move |errors: ErrorContext| {
                // Dioxus keeps a render panic's payload private; the hook
                // below kept its message.
                let detail = LAST_PANIC.with(|last| last.borrow_mut().take())
                    .or_else(|| errors.error().map(|e| e.to_string()))
                    .unwrap_or_default();
                rsx! {
                    div { class: "mk-ext-crash", role: "alert",
                        p { {t!(ws, L, "ext-crashed", name = title.clone())} }
                        if !detail.is_empty() { pre { class: "mk-ext-crash-detail", "{detail}" } }
                        button { class: "mk-btn", onclick: move |_| errors.clear_errors(),
                            {t!(ws, L, "ext-crash-reload")}
                        }
                    }
                }
            },
            {content}
        }
    }
}

// The last render panic's message and location, local to this UI thread.
thread_local! {
    static LAST_PANIC: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

/// Once per process: a panic hook that keeps the message for [`guarded`],
/// then runs the hook that was there before (the default prints it).
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn remember_panics() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if moonkale_ext_api::crash::is_guarded() {
                return;
            }
            let msg = info
                .payload()
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| info.payload().downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "a panic".into());
            let at = info
                .location()
                .map(|l| format!(" at {}:{}", l.file(), l.line()))
                .unwrap_or_default();
            let _ = LAST_PANIC.try_with(|last| {
                if let Ok(mut last) = last.try_borrow_mut() {
                    *last = Some(format!("{msg}{at}"));
                }
            });
            previous(info);
        }));
    });
}

fn attach_background_panels(
    layout: &PanelLayout,
    panels: &[Panel],
    nodes: &HashMap<String, moonkale_core::NodeId>,
    active: Option<moonkale_core::NodeId>,
    agent_panels: &std::collections::HashSet<String>,
    display_request: Option<&str>,
    focused: Option<&str>,
) -> Option<PanelLayout> {
    if display_request.is_none()
        && !panels.iter().any(|panel| {
            (agent_panels.contains(panel.id.as_str())
                || nodes
                    .get(panel.id.as_str())
                    .is_some_and(|node| Some(*node) != active))
                && layout.tile_for_panel(&panel.id).is_none()
        })
    {
        return None;
    }
    fn actives(node: &LayoutNode, selected: &mut Vec<PanelId>) {
        match node {
            LayoutNode::Tile(tile) => selected.extend(tile.active.clone()),
            LayoutNode::Split { first, second, .. } => {
                actives(first, selected);
                actives(second, selected);
            }
        }
    }
    let background: std::collections::HashSet<_> = panels
        .iter()
        .filter(|panel| {
            (agent_panels.contains(panel.id.as_str())
                || nodes
                    .get(panel.id.as_str())
                    .is_some_and(|node| Some(*node) != active))
                && layout.tile_for_panel(&panel.id).is_none()
        })
        .map(|panel| panel.id.clone())
        .collect();
    // Controlled layouts can lag panels attached only by the workbench's
    // derived render tree. Reconstruct that previous registry before saving
    // selected tabs, rather than treating an empty canonical tile as visible.
    let previous: Vec<_> = panels
        .iter()
        .filter(|panel| !background.contains(&panel.id))
        .map(|panel| {
            PanelPlacement::new(panel.id.clone(), panel.home.clone()).with_zone(panel.home_zone)
        })
        .collect();
    let before = layout.reconciled(&previous);
    let mut selected = Vec::new();
    actives(&before.root, &mut selected);
    let placements: Vec<_> = panels
        .iter()
        .map(|panel| {
            PanelPlacement::new(panel.id.clone(), panel.home.clone()).with_zone(panel.home_zone)
        })
        .collect();
    let mut next = layout.reconciled(&placements);
    for panel in &selected {
        next.activate(panel);
    }
    // A split would remount the person's editor and lose in-flight input.
    // Show only in an existing tile away from keyboard focus. On a one-tile
    // screen the new view stays a background tab, available for the person.
    let focus_tile = focused
        .and_then(|p| before.tile_for_panel(&PanelId::from(p)))
        .or_else(|| {
            selected
                .iter()
                .find(|p| nodes.get(p.as_str()).is_some_and(|n| Some(*n) == active))
                .and_then(|p| before.tile_for_panel(p))
        });
    let destination = before
        .tile_for_panel(&PanelId::from("agent"))
        .filter(|tile| Some(tile) != focus_tile.as_ref())
        .or_else(|| {
            selected
                .iter()
                .rev()
                .filter_map(|p| before.tile_for_panel(p))
                .find(|tile| Some(tile) != focus_tile.as_ref())
        });
    if let Some(request) = display_request.filter(|id| agent_panels.contains(*id)) {
        let panel = PanelId::from(request);
        if let Some(tile) = destination {
            if next.tile_for_panel(&panel).as_ref() != Some(&tile) {
                next.dock_panel(&panel, &tile, DockZone::Center);
            }
            next.activate(&panel);
        }
    }
    (next != *layout).then_some(next)
}

#[cfg(test)]
mod responsive_tests {
    use super::*;
    thread_local! {
        static WIDTH: std::cell::Cell<Option<Signal<bool>>> = const { std::cell::Cell::new(None) };
        static WIDE: std::cell::Cell<Option<Signal<PanelLayout>>> = const { std::cell::Cell::new(None) };
        static REQUESTS: std::cell::RefCell<Vec<bool>> = const { std::cell::RefCell::new(Vec::new()) };
    }
    fn app() -> Element {
        let narrow = use_signal(|| false);
        WIDTH.with(|width| width.set(Some(narrow)));
        let node = use_hook(|| {
            moonkale_core::NodeId::derive(&moonkale_core::SourceId("fixture".into()), "Cargo.lock")
        });
        let request = use_document_activation(Some(node));
        REQUESTS.with(|requests| requests.borrow_mut().push(request));
        let wide = use_signal(|| {
            PanelLayout::new(LayoutNode::split(
                "root",
                SplitAxis::Horizontal,
                0.25,
                LayoutNode::tile("side", ["explorer", "search"]),
                LayoutNode::tile("main", ["graph", "document"]),
            ))
        });
        let phone = use_signal(|| {
            PanelLayout::new(LayoutNode::tile(
                "main",
                ["graph", "document", "explorer", "search"],
            ))
        });
        WIDE.with(|value| value.set(Some(wide)));
        let active = request.then(|| PanelId::from("document"));
        let panels = ["graph", "document", "explorer", "search"]
            .into_iter()
            .map(|id| {
                Panel::new(
                    id,
                    id,
                    if narrow() || matches!(id, "graph" | "document") {
                        "main"
                    } else {
                        "side"
                    },
                    rsx! {div { "{id}" }},
                )
            })
            .collect::<Vec<_>>();
        rsx! {
            if narrow() { PanelWorkspace { panels, layout: phone, active_panel: active } }
            else { PanelWorkspace { panels, layout: wide, active_panel: active } }
        }
    }
    #[test]
    fn layout_switch_does_not_replay_document_navigation() {
        REQUESTS.with(|requests| requests.borrow_mut().clear());
        let mut dom = VirtualDom::new(app);
        dom.rebuild_in_place();
        dom.render_immediate_to_vec();
        // The user foregrounds Graph while the most recent document remains
        // Cargo.lock, and selects Search in the other tab group.
        dom.in_scope(ScopeId::ROOT, || {
            WIDE.with(|value| {
                let mut layout = value.get().unwrap();
                layout.with_mut(|layout| {
                    layout.activate(&PanelId::from("graph"));
                    layout.activate(&PanelId::from("search"));
                });
            })
        });
        dom.render_immediate_to_vec();
        for narrow in [true, false, true, false] {
            dom.in_scope(ScopeId::ROOT, || {
                WIDTH.with(|width| width.get().unwrap().set(narrow))
            });
            dom.render_immediate_to_vec();
        }
        dom.in_scope(ScopeId::ROOT, || {
            WIDE.with(|value| {
                let layout = value.get().unwrap();
                assert_eq!(
                    layout.peek().tile(&TileId::from("main")).unwrap().active,
                    Some(PanelId::from("graph"))
                );
                assert_eq!(
                    layout.peek().tile(&TileId::from("side")).unwrap().active,
                    Some(PanelId::from("search"))
                );
            })
        });
        REQUESTS.with(|requests| {
            let requests = requests.borrow();
            assert!(requests[0]);
            assert!(requests[1..].iter().all(|request| !request));
        });
    }
}

#[cfg(test)]
mod background_tests {
    use super::*;

    #[test]
    fn agent_display_keeps_the_typing_tile_mounted() {
        let node = moonkale_core::NodeId::fresh("person");
        let layout = PanelLayout::new(LayoutNode::split(
            "root",
            SplitAxis::Horizontal,
            0.7,
            LayoutNode::tile("main", ["person"]),
            LayoutNode::tile("right", ["agent"]),
        ));
        let panels = [
            Panel::new("person", "Person", "main", rsx! {div {}}),
            Panel::new("agent", "Agent", "right", rsx! {div {}}),
            Panel::new("graph:agent:test", "Agent graph", "right", rsx! {div {}}),
        ];
        let nodes = HashMap::from([("person".to_string(), node)]);
        let owned = std::collections::HashSet::from(["graph:agent:test".to_string()]);
        let next = attach_background_panels(
            &layout,
            &panels,
            &nodes,
            Some(node),
            &owned,
            Some("graph:agent:test"),
            Some("person"),
        )
        .unwrap();
        match next.root {
            LayoutNode::Split {
                first,
                second,
                ratio,
                ..
            } => {
                assert_eq!(ratio, 0.7);
                let LayoutNode::Tile(person) = *first else {
                    panic!("typing tile was split")
                };
                let LayoutNode::Tile(agent) = *second else {
                    panic!("display tile was split")
                };
                assert_eq!(person.active, Some(PanelId::from("person")));
                assert_eq!(agent.active, Some(PanelId::from("graph:agent:test")));
            }
            _ => panic!("workbench root changed"),
        }
    }
    #[test]
    fn background_attachment_preserves_each_tiles_selected_tab() {
        let node = moonkale_core::NodeId::fresh("background-alpha");
        let layout = PanelLayout::new(LayoutNode::split(
            "root",
            SplitAxis::Horizontal,
            0.5,
            LayoutNode::tile("main", ["readme"]),
            LayoutNode::tile("side", ["search"]),
        ));
        let panels = vec![
            Panel::new("readme", "Readme", "main", rsx! {}),
            Panel::new("search", "Search", "side", rsx! {}),
            Panel::new("alpha", "Alpha", "main", rsx! {}),
        ];
        let nodes = HashMap::from([("alpha".into(), node)]);
        let next = attach_background_panels(
            &layout,
            &panels,
            &nodes,
            None,
            &Default::default(),
            None,
            None,
        )
        .unwrap();
        assert_eq!(
            next.tile(&TileId::from("main")).unwrap().active,
            Some(PanelId::from("readme"))
        );
        assert_eq!(
            next.tile(&TileId::from("side")).unwrap().active,
            Some(PanelId::from("search"))
        );
        assert!(next.tile_for_panel(&PanelId::from("alpha")).is_some());
        assert!(attach_background_panels(
            &next,
            &panels,
            &nodes,
            None,
            &Default::default(),
            None,
            None
        )
        .is_none());
        assert!(attach_background_panels(
            &layout,
            &panels,
            &nodes,
            Some(node),
            &Default::default(),
            None,
            None
        )
        .is_none());
    }
}
