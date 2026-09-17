//! A dummy code-editor workbench: dockable, tabbed, resizable panels built on
//! `dioxus-workbench`. All panel content is placeholder text for now.

mod panels;

use dioxus::prelude::*;
use dioxus_workbench::prelude::*;

use panels::{Explorer, Outline, Problems, Search, SourceFile, Terminal};

const EDITOR_CSS: Asset = asset!("/assets/styling/editor.css");

/// Top-level activities selectable from the rail. Each one brings a
/// side panel to the front.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Activity {
    Explorer,
    Search,
    Outline,
}

impl Activity {
    fn panel(self) -> &'static str {
        match self {
            Activity::Explorer => "explorer",
            Activity::Search => "search",
            Activity::Outline => "outline",
        }
    }
}

/// The canonical first-open arrangement. Tile and split ids are layout
/// positions; keep them stable if layouts are ever persisted.
///
/// ```text
/// ┌──────────┬────────────────────────┬─────────┐
/// │ explorer │ main.rs  lib.rs        │ outline │
/// │ search   │                        │         │
/// │          ├────────────────────────┤         │
/// │          │ terminal  problems     │         │
/// └──────────┴────────────────────────┴─────────┘
/// ```
fn default_layout() -> PanelLayout {
    PanelLayout::new(LayoutNode::split(
        "root",
        SplitAxis::Horizontal,
        0.2,
        LayoutNode::tile("side", ["explorer", "search"]),
        LayoutNode::split(
            "center-and-right",
            SplitAxis::Horizontal,
            0.78,
            LayoutNode::split(
                "main-rows",
                SplitAxis::Vertical,
                0.7,
                LayoutNode::tile("main", ["main-rs", "lib-rs"]),
                LayoutNode::tile("bottom", ["terminal", "problems"]),
            ),
            LayoutNode::tile("right", ["outline"]),
        ),
    ))
}

#[component]
pub fn EditorWorkbench() -> Element {
    let mut activity = use_signal(|| Activity::Explorer);
    // Edge-triggered: the workspace applies a new value once, then leaves the
    // user's tab choice alone.
    let mut active_panel = use_signal(|| None::<PanelId>);
    let mut status = use_signal(|| "Ready".to_string());

    let mut select = move |next: Activity| {
        activity.set(next);
        active_panel.set(Some(PanelId::from(next.panel())));
    };

    let panels = vec![
        Panel::new("explorer", "Explorer", "side", rsx! { Explorer {} }),
        Panel::new("search", "Search", "side", rsx! { Search {} }),
        Panel::new(
            "main-rs",
            "main.rs",
            "main",
            rsx! { SourceFile { name: "main.rs" } },
        )
        .with_tab_icon(rsx! { FileIcon {} })
        .with_tab_accessory(rsx! {
            span { class: "editor-dirty-dot", "aria-label": "Unsaved changes" }
        }),
        Panel::new(
            "lib-rs",
            "lib.rs",
            "main",
            rsx! { SourceFile { name: "lib.rs" } },
        )
        .with_tab_icon(rsx! { FileIcon {} }),
        Panel::new("terminal", "Terminal", "bottom", rsx! { Terminal {} })
            .with_class("editor-panel-terminal"),
        Panel::new("problems", "Problems", "bottom", rsx! { Problems {} }),
        Panel::new("outline", "Outline", "right", rsx! { Outline {} }),
    ];

    rsx! {
        document::Link { rel: "stylesheet", href: EDITOR_CSS }

        div { class: "editor-root",
            Workbench {
                rail: rsx! {
                    ActivityRail {
                        ActivityButton {
                            label: "Explorer",
                            icon: rsx! { FilesIcon {} },
                            active: activity() == Activity::Explorer,
                            onclick: move |_| select(Activity::Explorer),
                        }
                        ActivityButton {
                            label: "Search",
                            icon: rsx! { SearchIcon {} },
                            active: activity() == Activity::Search,
                            onclick: move |_| select(Activity::Search),
                        }
                        ActivityButton {
                            label: "Outline",
                            icon: rsx! { OutlineIcon {} },
                            active: activity() == Activity::Outline,
                            onclick: move |_| select(Activity::Outline),
                        }
                        ActivityButton {
                            label: "Settings",
                            icon: rsx! { GearIcon {} },
                            bottom: true,
                            onclick: move |_| status.set("Settings are not implemented yet".into()),
                        }
                    }
                },
                status: rsx! {
                    StatusBar {
                        left: rsx! {
                            StatusItem { StatusDot { tone: StatusTone::Good } "rust-analyzer" }
                            StatusItem { "main" }
                        },
                        message: rsx! { StatusMessage { "{status}" } },
                        right: rsx! {
                            StatusItem { "Ln 12, Col 4" }
                            StatusItem { "UTF-8" }
                            StatusItem { tone: StatusTone::Accent, "Rust" }
                        },
                    }
                },
                PanelWorkspace {
                    panels,
                    initial_layout: default_layout(),
                    reset_layout: default_layout(),
                    active_panel: active_panel(),
                    on_panel_activate: move |id: PanelId| {
                        status.set(format!("Activated {id:?}"));
                    },
                    on_layout_change: move |_| status.set("Layout changed".into()),
                }
            }
        }
    }
}

// Tiny inline icons so the rail and tabs have something to show without an
// icon-font dependency.

#[component]
fn FilesIcon() -> Element {
    rsx! {
        svg { class: "editor-icon", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2",
            path { d: "M3 6a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" }
        }
    }
}

#[component]
fn SearchIcon() -> Element {
    rsx! {
        svg { class: "editor-icon", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2",
            circle { cx: "11", cy: "11", r: "7" }
            path { d: "m20 20-3.5-3.5" }
        }
    }
}

#[component]
fn OutlineIcon() -> Element {
    rsx! {
        svg { class: "editor-icon", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2",
            path { d: "M4 6h16M8 12h12M12 18h8" }
        }
    }
}

#[component]
fn GearIcon() -> Element {
    rsx! {
        svg { class: "editor-icon", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2",
            circle { cx: "12", cy: "12", r: "3" }
            path { d: "M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z" }
        }
    }
}

#[component]
fn FileIcon() -> Element {
    rsx! {
        svg { class: "editor-icon editor-icon-small", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2",
            path { d: "M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z" }
            path { d: "M14 3v5h5" }
        }
    }
}
