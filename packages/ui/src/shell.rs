//! `Shell` — extensions in, workbench out.

use dioxus::prelude::*;
use dioxus_workbench::prelude::*;
use moonkale_ext_api::{Extension, OpenFolder, Workspace};
use std::collections::HashMap;
use std::rc::Rc;

const SHELL_CSS: Asset = asset!("/assets/styling/shell.css");

/// Builds the extension list. A plain `fn` so it can be a prop.
pub type Extensions = fn() -> Vec<Box<dyn Extension>>;

/// What the platform hands the shell. Set once at startup; comparing
/// function pointers is meaningless, so two configs are always "equal" and
/// the shell never re-mounts because of them.
#[derive(Clone, Copy)]
pub struct ShellConfig {
    pub extensions: Extensions,
    pub open_folder: OpenFolder,
}

impl PartialEq for ShellConfig {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

fn default_layout() -> PanelLayout {
    PanelLayout::new(LayoutNode::split(
        "root",
        SplitAxis::Horizontal,
        0.22,
        LayoutNode::tile("side", ["explorer"]),
        LayoutNode::empty_tile("main"),
    ))
}

#[component]
pub fn Shell(config: ShellConfig) -> Element {
    let ws = use_hook(|| Workspace::new(config.open_folder));
    let exts: Rc<Vec<Box<dyn Extension>>> = use_hook(|| Rc::new((config.extensions)()));

    // Collect panels from every extension. `panels()` reads workspace
    // signals, so this render re-runs when documents open/close or turn dirty.
    let mut panels = Vec::new();
    let mut owner: HashMap<String, usize> = HashMap::new();
    let mut active_panel: Option<PanelId> = None;
    let active_node = *ws.active.read();
    for (i, ext) in exts.iter().enumerate() {
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
                    initial_layout: default_layout(),
                    reset_layout: default_layout(),
                    active_panel,
                    on_panel_close: on_close,
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
