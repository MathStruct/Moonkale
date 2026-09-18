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
        LayoutNode::tile("side", ["explorer", "search", "links"]),
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

#[component]
pub fn Shell() -> Element {
    let mut ws = use_context::<Workspace>();
    let exts: Rc<Vec<Box<dyn Extension>>> = use_context::<Extensions_>().0;
    // Controlled layout so "View → Reset Layout" can put it back.
    let mut layout = use_signal(default_layout);

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
        if ws.settings_folder.peek().is_none() {
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
                let placements: Vec<PanelPlacement> = exts
                    .iter()
                    .filter(|e| ext_settings.is_enabled(&e.manifest()))
                    .flat_map(|e| e.panels(ws))
                    .map(|c| PanelPlacement::new(PanelId::from(c.id.as_str()), TileId::from(c.home.tile_id())))
                    .collect();
                let mut next = layout.peek().clone();
                next.reconcile(&placements);
                if next.activate(&PanelId::from(id)) {
                    layout.set(next);
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
    for (i, ext) in exts.iter().enumerate() {
        if !ext_settings.is_enabled(&ext.manifest()) {
            continue;
        }
        for c in ext.panels(ws) {
            owner.insert(c.id.clone(), i);
            if c.node.is_some() && c.node == active_node {
                active_panel = Some(PanelId::from(c.id.as_str()));
            }
            let mut panel = Panel::new(
                c.id.as_str(),
                c.title.as_str(),
                c.home.tile_id(),
                ext.render(&c.id, ws),
            )
            .with_closable(c.closable);
            if c.dirty {
                panel = panel.with_tab_accessory(
                    rsx! { span { class: "mk-tab-dirty", "aria-label": "Unsaved changes" } },
                );
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

    rsx! {
        document::Stylesheet { href: SHELL_CSS }
        div { class: "mk-shell",
            Workbench {
                rail: rsx! {
                    ActivityRail {
                        ActivityButton { label: "Explorer", icon: rsx! { FilesIcon {} }, active: true, onclick: move |_| {} }
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

#[component]
fn FilesIcon() -> Element {
    rsx! {
        svg { class: "mk-icon", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2",
            path { d: "M3 6a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" }
        }
    }
}
