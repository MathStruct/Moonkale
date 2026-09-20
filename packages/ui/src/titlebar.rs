//! `TitleBar` — VS Code-style: menus on the left, the window title in the
//! middle, and (desktop) minimize / maximize / close on the right. The empty
//! areas are a drag region; double-click toggles maximize.

use crate::frame::WindowControls;
use dioxus::prelude::*;
use moonkale_ext_api::{Command, Workspace};

const TITLEBAR_CSS: Asset = asset!("/assets/styling/titlebar.css");
const LOGO: Asset = asset!("/assets/icon32.png");

#[derive(Clone, PartialEq)]
enum Item {
    /// A registry command: the shortcut shown is the effective keybinding.
    Cmd {
        label: &'static str,
        id: &'static str,
    },
    /// A direct command outside the registry (parameterised ones).
    Direct {
        label: &'static str,
        cmd: Command,
    },
    Sep,
    /// Platform-only entries are given as callbacks so `ui` stays generic.
    Native {
        label: &'static str,
        kind: Native,
    },
    /// A recent folder (label is the path).
    Recent(usize, String),
    /// A registry command known only at runtime (Show <panel>, extension menus).
    Dyn {
        label: String,
        id: String,
    },
}

#[derive(Clone, Copy, PartialEq)]
enum Native {
    DevTools,
    Exit,
}

fn menus(
    controls: Option<WindowControls>,
    recent: &[String],
    flow_enabled: bool,
    registry: &crate::commands::Registry,
) -> Vec<(String, Vec<Item>)> {
    let desktop = controls.is_some();
    let mut file = vec![
        Item::Cmd {
            label: "New File…",
            id: "file.new",
        },
        Item::Cmd {
            label: "Open Folder…",
            id: "workspace.openFolder",
        },
        Item::Cmd {
            label: "Close Folder",
            id: "workspace.closeFolder",
        },
    ];
    if flow_enabled {
        file.push(Item::Direct {
            label: "New Flow…",
            cmd: Command::NewFile(
                "untitled.flow.json",
                "{\n  \"version\": 1,\n  \"blocks\": [],\n  \"wires\": []\n}\n",
            ),
        });
    }
    for (i, path) in recent.iter().take(8).enumerate() {
        file.push(Item::Recent(i, path.clone()));
    }
    file.extend([
        Item::Sep,
        Item::Cmd {
            label: "Settings…",
            id: "view.settings",
        },
        Item::Sep,
        Item::Cmd {
            label: "Save",
            id: "file.save",
        },
        Item::Cmd {
            label: "Save All",
            id: "file.saveAll",
        },
        Item::Cmd {
            label: "Close All Editors",
            id: "editor.closeAll",
        },
        Item::Cmd {
            label: "Close Editor",
            id: "editor.close",
        },
    ]);
    if desktop {
        file.push(Item::Sep);
        file.push(Item::Native {
            label: "Exit",
            kind: Native::Exit,
        });
    }
    let mut view = vec![
        Item::Cmd {
            label: "Command Palette…",
            id: "view.palette",
        },
        Item::Cmd {
            label: "Go to File…",
            id: "view.quickOpen",
        },
        Item::Sep,
    ];
    // Show <panel> for every static panel, from the registry (spec 009).
    let mut shows: Vec<&crate::commands::Entry> = registry
        .entries
        .iter()
        .filter(|e| e.id.starts_with("view.panel."))
        .collect();
    shows.sort_by(|a, b| a.title.cmp(&b.title));
    for e in shows {
        view.push(Item::Dyn {
            label: e.title.trim_start_matches("View: ").to_string(),
            id: e.id.clone(),
        });
    }
    view.extend([
        Item::Sep,
        Item::Cmd {
            label: "Toggle Side Bar",
            id: "view.toggleSide",
        },
        Item::Cmd {
            label: "Toggle Bottom Panel",
            id: "view.toggleBottom",
        },
        Item::Cmd {
            label: "Toggle Word Wrap",
            id: "editor.toggleWrap",
        },
        Item::Cmd {
            label: "Fold All",
            id: "editor.foldAll",
        },
        Item::Cmd {
            label: "Unfold All",
            id: "editor.unfoldAll",
        },
        Item::Sep,
        Item::Cmd {
            label: "New Window",
            id: "view.newWindow",
        },
        Item::Cmd {
            label: "New Terminal",
            id: "view.newTerminal",
        },
        Item::Sep,
        Item::Cmd {
            label: "Reset Layout",
            id: "view.resetLayout",
        },
    ]);
    if controls.and_then(|c| c.devtools).is_some() {
        view.push(Item::Sep);
        view.push(Item::Native {
            label: "Toggle Developer Tools",
            kind: Native::DevTools,
        });
    }
    let edit = vec![
        Item::Cmd {
            label: "Undo",
            id: "edit.undo",
        },
        Item::Cmd {
            label: "Redo",
            id: "edit.redo",
        },
        Item::Sep,
        Item::Cmd {
            label: "Find",
            id: "editor.find",
        },
        Item::Cmd {
            label: "Replace",
            id: "editor.replace",
        },
        Item::Cmd {
            label: "Find in Workspace…",
            id: "search.workspace",
        },
        Item::Sep,
        Item::Cmd {
            label: "Rename Symbol",
            id: "editor.rename",
        },
        Item::Cmd {
            label: "Code Actions",
            id: "editor.codeActions",
        },
        Item::Cmd {
            label: "Go to Definition",
            id: "editor.definition",
        },
        Item::Cmd {
            label: "Find References",
            id: "editor.references",
        },
        Item::Cmd {
            label: "Toggle Comment",
            id: "editor.toggleComment",
        },
    ];
    let mut out: Vec<(String, Vec<Item>)> = vec![
        ("File".into(), file),
        ("Edit".into(), edit),
        ("View".into(), view),
    ];
    // Extension menus (spec 009): commands whose title category ("Git: …",
    // "Agent: …") is not one of the built-in menus form their own menu.
    let builtin = ["File", "Edit", "View", "Help", "Search", "Go to File…"];
    let mut groups: Vec<(String, Vec<Item>)> = Vec::new();
    for e in registry.entries.iter() {
        if e.id.starts_with("view.panel.") {
            continue;
        }
        let Some((cat, label)) = e.title.split_once(": ") else {
            continue;
        };
        if builtin.contains(&cat) {
            continue;
        }
        let item = Item::Dyn {
            label: label.to_string(),
            id: e.id.clone(),
        };
        match groups.iter_mut().find(|(c, _)| c == cat) {
            Some((_, items)) => items.push(item),
            None => groups.push((cat.to_string(), vec![item])),
        }
    }
    out.extend(groups);
    out.push((
        "Help".into(),
        vec![
            Item::Cmd {
                label: "About Moonkale",
                id: "help.about",
            },
            Item::Cmd {
                label: "Keyboard Shortcuts",
                id: "view.settings",
            },
            Item::Direct {
                label: "Documentation",
                cmd: Command::Docs,
            },
        ],
    ));
    out
}

#[component]
pub fn TitleBar(controls: Option<WindowControls>) -> Element {
    let mut ws = use_context::<Workspace>();
    let registry = use_context::<crate::commands::CommandRegistry>();
    let mut open: Signal<Option<String>> = use_signal(|| None);

    let title = {
        let active = ws.active_document().map(|(_, d)| {
            let d = d.read();
            format!("{}{}", if d.dirty() { "● " } else { "" }, d.node.native_key)
        });
        let folder = ws
            .sources
            .read()
            .first()
            .map(|s| s.descriptor.display_name.clone());
        match (active, folder) {
            (Some(a), Some(f)) => format!("{a} — {f} — Moonkale"),
            (None, Some(f)) => format!("{f} — Moonkale"),
            _ => "Moonkale".to_string(),
        }
    };

    let run_native = move |kind: Native| {
        if let Some(c) = controls {
            match kind {
                Native::Exit => c.close.call(()),
                Native::DevTools => {
                    if let Some(d) = c.devtools {
                        d.call(())
                    }
                }
            }
        }
    };

    rsx! {
        moonkale_ext_api::Stylesheet { href: TITLEBAR_CSS }
        div {
            class: "mk-titlebar",
            role: "menubar",
            // Empty areas drag the window; double-click toggles maximize.
            onmousedown: move |e| {
                if e.data().trigger_button() == Some(dioxus::html::input_data::MouseButton::Primary) {
                    if let Some(c) = controls { c.drag.call(()) }
                }
            },
            ondoubleclick: move |_| { if let Some(c) = controls { c.toggle_maximize.call(()) } },
            div { class: "mk-titlebar-left", onmousedown: |e| e.stop_propagation(), ondoubleclick: |e| e.stop_propagation(),
                img { class: "mk-titlebar-logo", src: LOGO, alt: "Moonkale", width: "16", height: "16", draggable: false }
                for (name, items) in {
                    let reg = registry.read();
                    menus(
                        controls,
                        &ws.settings.read().recent_folders,
                        ws.settings.read().extensions.enabled.iter().any(|e| e == "dev.moonkale.editor-flow"),
                        &reg,
                    )
                } {
                    div { class: "mk-menu", key: "{name}",
                        button {
                            class: "mk-menu-button",
                            class: if open().as_deref() == Some(name.as_str()) { "mk-menu-button-open" },
                            r#type: "button",
                            "aria-haspopup": "true",
                            "aria-expanded": if open().as_deref() == Some(name.as_str()) { "true" } else { "false" },
                            onclick: { let name = name.clone(); move |_| open.set(if open().as_deref() == Some(name.as_str()) { None } else { Some(name.clone()) }) },
                            onmouseenter: { let name = name.clone(); move |_| { if open().is_some() { open.set(Some(name.clone())) } } },
                            "{name}"
                        }
                        if open().as_deref() == Some(name.as_str()) {
                            div { class: "mk-menu-popup", role: "menu",
                                for item in items {
                                    match item {
                                        Item::Sep => rsx! { div { class: "mk-menu-sep" } },
                                        Item::Cmd { label, id } => rsx! {
                                            button { class: "mk-menu-item", role: "menuitem", r#type: "button",
                                                onclick: move |_| { open.set(None); crate::commands::run(id, ws); },
                                                span { "{label}" }
                                                span { class: "mk-menu-shortcut", "{registry.read().shortcut(id)}" }
                                            }
                                        },
                                        Item::Direct { label, cmd } => rsx! {
                                            button { class: "mk-menu-item", role: "menuitem", r#type: "button",
                                                onclick: move |_| { open.set(None); ws.dispatch(cmd); },
                                                span { "{label}" }
                                                span { class: "mk-menu-shortcut" }
                                            }
                                        },
                                        Item::Recent(i, path) => rsx! {
                                            button { class: "mk-menu-item mk-menu-recent", role: "menuitem", r#type: "button", title: "{path}",
                                                onclick: move |_| { open.set(None); ws.dispatch(Command::OpenRecent(i)); },
                                                span { class: "mk-menu-recent-path", "{path}" }
                                            }
                                        },
                                        Item::Dyn { label, id } => rsx! {
                                            button { class: "mk-menu-item", role: "menuitem", r#type: "button",
                                                onclick: { let id = id.clone(); move |_| { open.set(None); crate::commands::run(&id, ws); } },
                                                span { "{label}" }
                                                span { class: "mk-menu-shortcut", "{registry.read().shortcut(&id)}" }
                                            }
                                        },
                                        Item::Native { label, kind } => rsx! {
                                            button { class: "mk-menu-item", role: "menuitem", r#type: "button",
                                                onclick: move |_| { open.set(None); run_native(kind); },
                                                span { "{label}" }
                                            }
                                        },
                                    }
                                }
                            }
                        }
                    }
                }
            }
            div { class: "mk-titlebar-title", "{title}" }
            if let Some(c) = controls {
                div { class: "mk-window-controls", onmousedown: |e| e.stop_propagation(), ondoubleclick: |e| e.stop_propagation(),
                    button { class: "mk-wc", title: "Minimize", onclick: move |_| c.minimize.call(()), MinIcon {} }
                    button { class: "mk-wc", title: "Maximize", onclick: move |_| c.toggle_maximize.call(()), MaxIcon {} }
                    button { class: "mk-wc mk-wc-close", title: "Close", onclick: move |_| c.close.call(()), CloseIcon {} }
                }
            }
        }
        // Click anywhere else closes an open menu.
        if open().is_some() {
            div { class: "mk-menu-backdrop", onclick: move |_| open.set(None), onmousedown: |e| e.stop_propagation() }
        }
    }
}

#[component]
fn MinIcon() -> Element {
    rsx! { svg { width: "10", height: "10", view_box: "0 0 10 10", path { d: "M0 5h10", stroke: "currentColor", stroke_width: "1" } } }
}
#[component]
fn MaxIcon() -> Element {
    rsx! { svg { width: "10", height: "10", view_box: "0 0 10 10", rect { x: "0.5", y: "0.5", width: "9", height: "9", fill: "none", stroke: "currentColor", stroke_width: "1" } } }
}
#[component]
fn CloseIcon() -> Element {
    rsx! { svg { width: "10", height: "10", view_box: "0 0 10 10", path { d: "M0 0l10 10M10 0L0 10", stroke: "currentColor", stroke_width: "1.2" } } }
}
