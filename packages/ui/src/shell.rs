//! `Shell` — extensions in, workbench out.

use crate::frame::Extensions_;
use dioxus::prelude::*;
use dioxus_workbench::prelude::*;
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
    let exts: Rc<Vec<Box<dyn Extension>>> = use_context::<Extensions_>().0;
    // Controlled layout so "View → Reset Layout" can put it back.
    let mut layout = use_signal(default_layout);
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
        let folder = ws.settings_folder.read().clone();
        if folder.is_none() || *restored_for.peek() == folder {
            return;
        }
        restored_for.set(folder);
        if let Some(encoded) = ws.settings.peek().layout.clone() {
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
        if ws.settings_folder.peek().is_none() || *narrow.peek() {
            return;
        }
        let encoded = next.encode();
        if encoded == ws.settings_workspace.peek().layout {
            return;
        }
        spawn(async move {
            ws.update_workspace_settings(|f| f.layout = encoded).await;
        });
    };

    // Shell-level commands.
    let exts_for_commands = exts.clone();
    use_effect(move || {
        let exts = &exts_for_commands;
        let (_, cmd) = *ws.commands.read();
        match cmd {
            Some(Command::ResetLayout) => {
                layout.set(default_layout());
                if ws.settings_folder.peek().is_some() {
                    spawn(async move {
                        ws.update_workspace_settings(|f| f.layout = None).await;
                    });
                }
            }
            Some(Command::ShowPanel(id)) => {
                // Our copy of the layout only learns about attached panels
                // through mutations, so reconcile with the current
                // contributions first (the workbench does the same on click).
                ws.closed_panels.with_mut(|c| {
                    c.remove(id);
                });
                let ext_settings = ws.settings.peek().extensions.clone();
                let is_narrow = *narrow.peek();
                let mut target = if is_narrow { phone_layout } else { layout };
                let mut next = target.peek().clone();
                let closed = ws.closed_panels.peek().clone();
                let placements: Vec<PanelPlacement> = exts
                    .iter()
                    .filter(|e| ext_settings.is_enabled(&e.manifest()))
                    .flat_map(|e| e.panels(ws))
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
                let home = exts
                    .iter()
                    .flat_map(|e| e.panels(ws))
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
            }
            Some(Command::OpenFolder) => {
                spawn(async move {
                    if let Err(e) = ws.open_folder_dialog().await {
                        ws.set_status(format!("Open folder failed: {e}"));
                    }
                });
            }
            // Ctrl+B / Ctrl+J (spec 009): hide every panel of the tile and
            // remember them; the next toggle shows them again.
            Some(Command::ToggleSide) | Some(Command::ToggleBottom) => {
                let tile = if matches!(cmd, Some(Command::ToggleSide)) { "side" } else { "bottom" };
                let remembered = ws.hidden_tiles.peek().get(tile).cloned().unwrap_or_default();
                let ext_settings = ws.settings.peek().extensions.clone();
                let closed = ws.closed_panels.peek().clone();
                let visible: Vec<String> = exts
                    .iter()
                    .filter(|e| ext_settings.is_enabled(&e.manifest()))
                    .flat_map(|e| e.panels(ws))
                    .filter(|c| c.node.is_none() && !closed.contains(&c.id) && c.home.tile_id() == tile)
                    .map(|c| c.id)
                    .collect();
                if !visible.is_empty() {
                    ws.hidden_tiles.with_mut(|h| { h.insert(tile.to_string(), visible.clone()); });
                    ws.closed_panels.with_mut(|c| c.extend(visible));
                } else if !remembered.is_empty() {
                    ws.hidden_tiles.with_mut(|h| { h.remove(tile); });
                    let first = remembered[0].clone();
                    ws.closed_panels.with_mut(|c| { for id in &remembered { c.remove(id); } });
                    ws.show_panel(&first);
                } else {
                    ws.set_status(format!("Nothing to show in the {tile} area"));
                }
            }
            Some(Command::SaveAll) => {
                let dirty: Vec<moonkale_core::NodeId> = ws
                    .documents
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
                    ws.set_status(format!("Saved {n} document(s)"));
                });
            }
            Some(Command::CloseAllEditors) => {
                let clean: Vec<moonkale_core::NodeId> = ws
                    .documents
                    .peek()
                    .iter()
                    .filter(|(_, d)| !d.peek().dirty())
                    .map(|(id, _)| *id)
                    .collect();
                let kept = ws.documents.peek().len() - clean.len();
                for id in clean {
                    ws.close_node(id);
                }
                let views: Vec<moonkale_core::NodeId> = ws.views.peek().iter().map(|n| n.id).collect();
                for id in views {
                    ws.close_node(id);
                }
                if kept > 0 {
                    ws.set_status(format!("{kept} unsaved document(s) left open"));
                }
            }
            Some(Command::CloseFolder) => {
                let first = ws
                    .sources
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
                    None => ws.set_status("No folder is open"),
                }
            }
            Some(Command::Docs) => {
                let _ = dioxus::document::eval("window.open('https://mathstruct.github.io/Moonkale/', '_blank');");
            }
            Some(Command::About) => ws.set_status("Moonkale 0.1.0 — graph-native code and knowledge editor · mathstruct.github.io/Moonkale"),
            _ => {}
        }
    });

    // Collect panels from every extension. `panels()` reads workspace
    // signals, so this render re-runs when documents open/close or turn dirty.
    let mut panels = Vec::new();
    let mut owner: HashMap<String, usize> = HashMap::new();
    let mut active_panel: Option<PanelId> = None;
    let active_node = *ws.active.read();
    let is_narrow = *narrow.read();
    // Disabled extensions (Settings → Extensions) contribute nothing.
    let ext_settings = ws.settings.read().extensions.clone();
    // Block libraries for the flow editor, from the enabled extensions.
    {
        let libs: Vec<moonkale_ext_api::flow::FlowLibrary> = exts
            .iter()
            .filter(|e| ext_settings.is_enabled(&e.manifest()))
            .flat_map(|e| e.flow_libraries())
            .collect();
        if *ws.flow_libraries.peek() != libs {
            ws.flow_libraries.set(libs);
        }
    }
    // Panel ids present this render, for the phone bar.
    let mut present: Vec<String> = Vec::new();
    // Static panels the user closed (spec 011) contribute nothing.
    let closed = ws.closed_panels.read().clone();
    // Whether a contribution is a static panel (no document behind it).
    let mut is_static: HashMap<String, bool> = HashMap::new();
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
    for (i, ext) in exts.iter().enumerate() {
        if !ext_settings.is_enabled(&ext.manifest()) {
            continue;
        }
        for c in ext.panels(ws) {
            if let (None, Some(act)) = (&c.node, &c.activity) {
                activities.push((c.id.clone(), act.clone()));
            }
            if c.node.is_none() && closed.contains(&c.id) {
                continue;
            }
            owner.insert(c.id.clone(), i);
            is_static.insert(c.id.clone(), c.node.is_none());
            present.push(c.id.clone());
            if c.node.is_some() && c.node == active_node {
                active_panel = Some(PanelId::from(c.id.as_str()));
                editor_panel = Some(c.id.clone());
            }
            let home = home_tile(is_narrow, c.home);
            let mut panel =
                Panel::new(c.id.as_str(), c.title.as_str(), home, ext.render(&c.id, ws))
                    .with_closable(c.closable)
                    .with_home_zone(home_zone(&current_layout, home));
            // Tab accessories: the unsaved dot, and the git status letter of
            // the document's file (Milestone 7).
            let vcs = c
                .node
                .and_then(|n| ws.document(n))
                .and_then(|d| {
                    let key = d.peek().node.native_key.clone();
                    ws.vcs_status.read().get(&key).map(|(i, w)| {
                        if *i == '?' {
                            '?'
                        } else if *w != '.' {
                            *w
                        } else {
                            *i
                        }
                    })
                })
                .filter(|c| *c != '.');
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
            if c.dirty || vcs.is_some() || !here.is_empty() {
                let dirty = c.dirty;
                panel = panel.with_tab_accessory(rsx! {
                    if dirty { span { class: "mk-tab-dirty", "aria-label": "Unsaved changes" } }
                    if let Some(l) = vcs { span { class: "mk-tab-vcs", "data-status": "{l}", "{l}" } }
                    for m in here.iter() {
                        span { class: "mk-tab-presence", title: "{m.name} has this open", "{m.initials()}" }
                    }
                });
            }
            panels.push(panel);
        }
    }

    let on_close = {
        let exts = exts.clone();
        move |id: PanelId| {
            let id_str = id.to_string();
            if let Some(&i) = owner.get(&id_str) {
                exts[i].on_panel_closed(&id_str, ws);
            }
            // A static panel stays closed until View → Show / the activity
            // bar / the palette brings it back (spec 011); documents close
            // through their extension.
            if is_static.get(&id_str).copied().unwrap_or(false) {
                ws.closed_panels.with_mut(|c| {
                    c.insert(id_str);
                });
            }
        }
    };

    let status = ws.status.read().clone();
    let _ = ws.presence.read();
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
    let lsp = ws.lsp_status.read().clone();
    let windows = ws.peers.read().len() + 1;
    let window_id = ws.window.read().to_string();
    let source_name = ws
        .sources
        .read()
        .first()
        .map(|s| s.descriptor.display_name.clone());
    // The first source's accent (spec 009), matching its Explorer stripe.
    let source_color: Option<&'static str> = ws
        .sources
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
    let (phone_main, phone_more): (Vec<BarEntry>, Vec<BarEntry>) = if is_narrow {
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
                    "Editor".into(),
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
    let others_count = others.len() as u32;

    rsx! {
        moonkale_ext_api::Stylesheet { href: SHELL_CSS }
        div { class: if is_narrow { "mk-shell mk-narrow" } else { "mk-shell" }, onmounted: start_narrow_watch,
            Workbench {
                rail: rsx! {
                    if !is_narrow {
                        ActivityRail {
                            for (id, label, icon, active, badge, bottom) in rail.iter().cloned() {
                                ActivityButton {
                                    id: format!("mk-rail-{id}"),
                                    label: label.clone(),
                                    title: format!("{label} — click again to hide"),
                                    icon: rsx! {
                                        span { class: "mk-rail-icon",
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
                                                ws.closed_panels.with_mut(|c| { c.insert(id.clone()); });
                                            } else {
                                                ws.show_panel(&id);
                                            }
                                        }
                                    },
                                }
                            }
                            // Presence: who else is here (spec 009).
                            ActivityButton {
                                id: "mk-rail-presence".to_string(),
                                label: "People".to_string(),
                                title: if others_names.is_empty() { "Nobody else is here".to_string() } else { format!("Here: {others_names}") },
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
                                    move |_| ws.set_status(if names.is_empty() { "Nobody else is looking at this folder".to_string() } else { format!("Here: {names}") })
                                },
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
                                {source_name.clone().unwrap_or_else(|| "No folder open".into())}
                            }
                        },
                        message: rsx! { StatusMessage { "{status}" } },
                        right: rsx! {
                            if let Some(l) = lsp {
                                StatusItem { title: "Language server", "{l}" }
                            }
                            if !others.is_empty() {
                                StatusItem { title: "{others_names} — in this folder too",
                                    span { class: "mk-presence", "data-count": "{others.len()}",
                                        "👥 "
                                        for (initials, title) in others_badges.iter() {
                                            span { class: "mk-presence-badge", title: "{title}", "{initials}" }
                                        }
                                    }
                                }
                            }
                            StatusItem { title: "This window: {window_id}. Other windows of this session are counted once they answer.",
                                if windows > 1 { "{windows} windows" } else { "1 window" }
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
                            layout: phone_layout,
                            reset_layout: narrow_layout(),
                            active_panel,
                            on_panel_close: on_close,
                            on_layout_change: move |_| {},
                        }
                        if more_open() {
                            div { class: "mk-phone-more", onclick: move |_| more_open.set(false),
                                for (id, label, icon, active, badge) in phone_more.iter().cloned() {
                                    button {
                                        class: if active { "mk-phone-btn mk-active" } else { "mk-phone-btn" },
                                        "data-panel": "{id}",
                                        onclick: { let mut show = show.clone(); let id = id.clone(); move |_| { more_open.set(false); ws.show_panel(&id); show(&id) } },
                                        span { class: "mk-rail-icon", crate::icons::Icon { name: icon } if badge > 0 { span { class: "mk-badge", "{badge}" } } }
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
                                    onclick: { let mut show = show.clone(); let id = id.clone(); move |_| { ws.show_panel(&id); show(&id) } },
                                    span { class: "mk-rail-icon", crate::icons::Icon { name: icon } if badge > 0 { span { class: "mk-badge", "{badge}" } } }
                                    span { "{label}" }
                                }
                            }
                            if !phone_more.is_empty() {
                                button {
                                    class: if more_open() { "mk-phone-btn mk-active" } else { "mk-phone-btn" },
                                    "data-panel": "more",
                                    onclick: move |_| more_open.toggle(),
                                    span { class: "mk-rail-icon", crate::icons::Icon { name: "more" } }
                                    span { "More" }
                                }
                            }
                        }
                    }
                } else {
                    PanelWorkspace {
                        panels,
                        layout,
                        reset_layout: default_layout(),
                        active_panel,
                        on_panel_close: on_close,
                        on_layout_change,
                    }
                }
            }
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

/// Reports `matchMedia("(max-width: <px>px)")` now and on every change.
const NARROW_WATCH: &str = r#"
function watch(px) {
    const mq = window.matchMedia(`(max-width: ${px}px)`);
    dioxus.send(mq.matches);
    mq.addEventListener("change", (e) => dioxus.send(e.matches));
}
for (;;) { await dioxus.recv(); }
"#;
