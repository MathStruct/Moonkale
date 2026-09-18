//! `TitleBar` — VS Code-style: menus on the left, the window title in the
//! middle, and (desktop) minimize / maximize / close on the right. The empty
//! areas are a drag region; double-click toggles maximize.

use crate::frame::WindowControls;
use dioxus::prelude::*;
use moonkale_ext_api::{Command, Workspace};

const TITLEBAR_CSS: Asset = asset!("/assets/styling/titlebar.css");

#[derive(Clone, Copy, PartialEq)]
enum Item {
    Cmd {
        label: &'static str,
        shortcut: &'static str,
        cmd: Command,
    },
    Sep,
    /// Platform-only entries are given as callbacks so `ui` stays generic.
    Native {
        label: &'static str,
        kind: Native,
    },
}

#[derive(Clone, Copy, PartialEq)]
enum Native {
    DevTools,
    Exit,
}

fn menus(controls: Option<WindowControls>) -> Vec<(&'static str, Vec<Item>)> {
    let desktop = controls.is_some();
    let mut file = vec![
        Item::Cmd {
            label: "Open Folder…",
            shortcut: "Ctrl+O",
            cmd: Command::OpenFolder,
        },
        Item::Sep,
        Item::Cmd {
            label: "Save",
            shortcut: "Ctrl+S",
            cmd: Command::Save,
        },
        Item::Cmd {
            label: "Close Editor",
            shortcut: "Ctrl+W",
            cmd: Command::CloseEditor,
        },
    ];
    if desktop {
        file.push(Item::Sep);
        file.push(Item::Native {
            label: "Exit",
            kind: Native::Exit,
        });
    }
    let mut view = vec![
        Item::Cmd {
            label: "New Window",
            shortcut: "Ctrl+Shift+N",
            cmd: Command::NewWindow,
        },
        Item::Cmd {
            label: "New Terminal",
            shortcut: "Ctrl+`",
            cmd: Command::NewTerminal,
        },
        Item::Sep,
        Item::Cmd {
            label: "Reset Layout",
            shortcut: "",
            cmd: Command::ResetLayout,
        },
    ];
    if controls.and_then(|c| c.devtools).is_some() {
        view.push(Item::Sep);
        view.push(Item::Native {
            label: "Toggle Developer Tools",
            kind: Native::DevTools,
        });
    }
    vec![
        ("File", file),
        (
            "Edit",
            vec![
                Item::Cmd {
                    label: "Undo",
                    shortcut: "Ctrl+Z",
                    cmd: Command::Undo,
                },
                Item::Cmd {
                    label: "Redo",
                    shortcut: "Ctrl+Y",
                    cmd: Command::Redo,
                },
            ],
        ),
        ("View", view),
        (
            "Help",
            vec![Item::Cmd {
                label: "About Moonkale",
                shortcut: "",
                cmd: Command::About,
            }],
        ),
    ]
}

#[component]
pub fn TitleBar(controls: Option<WindowControls>) -> Element {
    let mut ws = use_context::<Workspace>();
    let mut open: Signal<Option<&'static str>> = use_signal(|| None);

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
        document::Stylesheet { href: TITLEBAR_CSS }
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
                span { class: "mk-titlebar-logo", "◐" }
                for (name, items) in menus(controls) {
                    div { class: "mk-menu",
                        button {
                            class: "mk-menu-button",
                            class: if open() == Some(name) { "mk-menu-button-open" },
                            r#type: "button",
                            "aria-haspopup": "true",
                            "aria-expanded": if open() == Some(name) { "true" } else { "false" },
                            onclick: move |_| open.set(if open() == Some(name) { None } else { Some(name) }),
                            onmouseenter: move |_| { if open().is_some() { open.set(Some(name)) } },
                            "{name}"
                        }
                        if open() == Some(name) {
                            div { class: "mk-menu-popup", role: "menu",
                                for item in items {
                                    match item {
                                        Item::Sep => rsx! { div { class: "mk-menu-sep" } },
                                        Item::Cmd { label, shortcut, cmd } => rsx! {
                                            button { class: "mk-menu-item", role: "menuitem", r#type: "button",
                                                onclick: move |_| { open.set(None); ws.dispatch(cmd); },
                                                span { "{label}" }
                                                span { class: "mk-menu-shortcut", "{shortcut}" }
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
