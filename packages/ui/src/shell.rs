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
                let ext_settings = ws.settings.peek().extensions.clone();
                let is_narrow = *narrow.peek();
                let placements: Vec<PanelPlacement> = exts
                    .iter()
                    .filter(|e| ext_settings.is_enabled(&e.manifest()))
                    .flat_map(|e| e.panels(ws))
                    .map(|c| PanelPlacement::new(PanelId::from(c.id.as_str()), TileId::from(home_tile(is_narrow, c.home))))
                    .collect();
                let mut target = if is_narrow { phone_layout } else { layout };
                let mut next = target.peek().clone();
                next.reconcile(&placements);
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
    // The panel of the most recently active document, for the phone bar's
    // "Editor" button.
    let mut editor_panel: Option<String> = None;
    for (i, ext) in exts.iter().enumerate() {
        if !ext_settings.is_enabled(&ext.manifest()) {
            continue;
        }
        for c in ext.panels(ws) {
            owner.insert(c.id.clone(), i);
            present.push(c.id.clone());
            if c.node.is_some() && c.node == active_node {
                active_panel = Some(PanelId::from(c.id.as_str()));
                editor_panel = Some(c.id.clone());
            }
            let mut panel = Panel::new(
                c.id.as_str(),
                c.title.as_str(),
                home_tile(is_narrow, c.home),
                ext.render(&c.id, ws),
            )
            .with_closable(c.closable);
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
    let bar: Vec<(String, &'static str, bool)> = if is_narrow {
        let mut v = Vec::new();
        for (id, label) in [("explorer", "Files"), ("search", "Search")] {
            if present.iter().any(|p| p == id) {
                v.push((id.to_string(), label, front.as_deref() == Some(id)));
            }
        }
        let editor_front = front.as_deref().is_some_and(|f| f.starts_with("editor:"));
        v.push((
            editor_panel.clone().unwrap_or_default(),
            "Editor",
            editor_front,
        ));
        for (id, label) in [
            ("graph", "Graph"),
            ("terminal", "Terminal"),
            ("agent", "Agent"),
            ("settings", "Settings"),
        ] {
            if present.iter().any(|p| p == id) {
                v.push((id.to_string(), label, front.as_deref() == Some(id)));
            }
        }
        v
    } else {
        Vec::new()
    };

    rsx! {
        document::Stylesheet { href: SHELL_CSS }
        div { class: if is_narrow { "mk-shell mk-narrow" } else { "mk-shell" }, onmounted: start_narrow_watch,
            Workbench {
                rail: rsx! {
                    if !is_narrow {
                        ActivityRail {
                            ActivityButton { label: "Explorer", icon: rsx! { FilesIcon {} }, active: true, onclick: move |_| {} }
                        }
                    }
                },
                status: rsx! {
                    StatusBar {
                        left: rsx! {
                            StatusItem {
                                StatusDot { tone: if source_name.is_some() { StatusTone::Good } else { StatusTone::Neutral } }
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
                        nav { class: "mk-phone-bar", "aria-label": "Panels",
                            for (id, label, active) in bar {
                                button {
                                    class: if active { "mk-phone-btn mk-active" } else { "mk-phone-btn" },
                                    disabled: id.is_empty(),
                                    "data-panel": "{id}",
                                    onclick: { let mut show = show.clone(); let id = id.clone(); move |_| show(&id) },
                                    "{label}"
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

#[component]
fn FilesIcon() -> Element {
    rsx! {
        svg { class: "mk-icon", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2",
            path { d: "M3 6a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" }
        }
    }
}
