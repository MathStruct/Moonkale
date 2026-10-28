//! The icon set, and the buttons toolbars are made of (2026-10-14).
//!
//! Inline SVG on a 24-unit viewbox, stroked with `currentColor` so themes
//! colour them; an unknown name draws a dot. Toolbars use [`IconButton`]:
//! a symbol to click, its description (and shortcut) on hover, and the
//! same text hidden for screen readers and tests (ADR-0027 "Simple by
//! default": symbols over labels, shortcuts in plain sight).

use dioxus::prelude::*;

/// An icon by name.
#[component]
pub fn Icon(name: &'static str) -> Element {
    let path: &str = match name {
        "files" => "M3 6a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z",
        "search" => "M11 4a7 7 0 1 1 0 14 7 7 0 0 1 0-14zM20 20l-4-4",
        "links" => "M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1.5 1.5M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1.5-1.5",
        "git" => "M6 3v18M6 9a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM18 15a3 3 0 1 0 0 6 3 3 0 0 0 0-6zM18 15V9a3 3 0 0 0-3-3h-3",
        "history" => "M3 12a9 9 0 1 0 3-6.7M3 4v5h5M12 7v5l3 2",
        "agent" => "M12 3v3M8 6h8a3 3 0 0 1 3 3v6a3 3 0 0 1-3 3H8a3 3 0 0 1-3-3V9a3 3 0 0 1 3-3zM9 12h.01M15 12h.01M9 18v2M15 18v2",
        "terminal" => "M4 5h16v14H4zM7 9l3 3-3 3M12 15h5",
        "graph" => "M6 6a2 2 0 1 0 0-4 2 2 0 0 0 0 4zM18 8a2 2 0 1 0 0-4 2 2 0 0 0 0 4zM12 20a2 2 0 1 0 0-4 2 2 0 0 0 0 4zM7.5 5.2l9 1.6M7.2 7.5l4 9M16.8 8l-4 8",
        "settings" => "M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z",
        "table" => "M3 5h18v14H3zM3 10h18M3 15h18M9 5v14M15 5v14",
        "flow" => "M4 6h5v4H4zM15 14h5v4h-5zM9 8h3a2 2 0 0 1 2 2v4a2 2 0 0 0 2 2",
        "image" => "M4 5h16v14H4zM8 11a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3zM20 15l-5-5-8 8",
        "puzzle" => "M10 4h4v3a2 2 0 1 0 0 3v3h3a2 2 0 1 1 0 3h-3v4h-4v-3a2 2 0 1 1 0-3v-3H7a2 2 0 1 1 0-3h3z",
        "presence" => "M9 11a4 4 0 1 0 0-8 4 4 0 0 0 0 8zM3 21v-1a6 6 0 0 1 12 0v1M17 4a4 4 0 0 1 0 7M21 21v-1a6 6 0 0 0-4-5.6",
        "more" => "M5 12h.01M12 12h.01M19 12h.01",
        "editor" => "M4 4h16v16H4zM8 9h8M8 12h8M8 15h5",
        // Source kinds (spec 009).
        "folder" => "M3 6a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z",
        "folder-remote" => "M3 6a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v3M3 8v8a2 2 0 0 0 2 2h6M14 17a3 3 0 0 1 3-3h1a3 3 0 0 1 0 6h-1M20 17a3 3 0 0 0-3-3",
        "repo" => "M7 3v18M7 8a2.5 2.5 0 1 0 0-5 2.5 2.5 0 0 0 0 5zM17 16a2.5 2.5 0 1 0 0 5 2.5 2.5 0 0 0 0-5zM17 16V9a2 2 0 0 0-2-2h-3",
        "database" => "M12 3c4.4 0 8 1.3 8 3s-3.6 3-8 3-8-1.3-8-3 3.6-3 8-3zM4 6v12c0 1.7 3.6 3 8 3s8-1.3 8-3V6M4 12c0 1.7 3.6 3 8 3s8-1.3 8-3",
        "graphdb" => "M6 7a2 2 0 1 0 0-4 2 2 0 0 0 0 4zM18 7a2 2 0 1 0 0-4 2 2 0 0 0 0 4zM12 21a2 2 0 1 0 0-4 2 2 0 0 0 0 4zM8 5h8M7 7l4 10M17 7l-4 10",
        "kv" => "M14 3a5 5 0 1 0 3.5 8.6L21 15l-2 2-2-2-2 2-1.5-1.5 3.5-3.5A5 5 0 0 0 14 3zM14 9a1 1 0 1 0 0-2 1 1 0 0 0 0 2z",
        "data" => "M3 5h18v14H3zM3 10h18M9 5v14",
        "api" => "M9 3v4M15 3v4M6 7h12v4a6 6 0 0 1-12 0zM12 17v4",
        "index" => "M4 6h16M4 12h10M4 18h16",
        "lock" => "M6 11V8a6 6 0 0 1 12 0v3M5 11h14v10H5z",
        // Toolbars (2026-10-14: icons, with the description on hover).
        "save" => "M5 3h11l3 3v15H5zM8 3v5h7V3M8 21v-7h8v7",
        "reload" => "M20 11a8 8 0 1 0-2.3 5.7M20 4v7h-7",
        "wrap" => "M4 6h16M4 12h13a3 3 0 0 1 0 6h-4M15 16l-2 2 2 2M4 18h5",
        "swap" => "M7 7h12l-3-3M17 17H5l3 3",
        "layout" => "M4 4h6v6H4zM14 14h6v6h-6zM10 7h4a2 2 0 0 1 2 2v5",
        "fit" => "M4 9V4h5M15 4h5v5M20 15v5h-5M9 20H4v-5",
        "zoom-in" => "M11 4a7 7 0 1 1 0 14 7 7 0 0 1 0-14zM20 20l-4-4M8 11h6M11 8v6",
        "zoom-out" => "M11 4a7 7 0 1 1 0 14 7 7 0 0 1 0-14zM20 20l-4-4M8 11h6",
        "actual-size" => "M4 5h16v14H4zM8 9v6M12 10.5v.01M12 13.5v.01M16 9v6",
        "code" => "M9 7l-5 5 5 5M15 7l5 5-5 5",
        "close" => "M6 6l12 12M18 6L6 18",
        _ => "M12 12m-2 0a2 2 0 1 0 4 0a2 2 0 1 0-4 0",
    };
    rsx! {
        svg { class: "mk-icon", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "1.8", stroke_linecap: "round", stroke_linejoin: "round", "aria-hidden": "true",
            path { d: "{path}" }
        }
    }
}

/// A toolbar button: the icon; on hover its `description` (or its
/// `label`) with `shortcut` after it ("Save (Ctrl+S)"); and `label`,
/// hidden, as its text, so screen readers name it and tests find it by its
/// words. `pressed` makes it a toggle
/// (`aria-pressed`, the `mk-btn-on` style).
#[component]
pub fn IconButton(
    icon: &'static str,
    label: String,
    #[props(default)] description: Option<String>,
    #[props(default)] shortcut: Option<String>,
    #[props(default)] command: Option<String>,
    #[props(default)] pressed: Option<bool>,
    #[props(default)] disabled: bool,
    #[props(default)] class: String,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    let text = description.clone().unwrap_or_else(|| label.clone());
    let shortcut = command
        .as_deref()
        .map(|id| crate::command_hint(id, "").shortcut)
        .or(shortcut)
        .filter(|key| !key.is_empty());
    let tip = match &shortcut {
        Some(k) => format!("{text} ({k})"),
        None => text,
    };
    let on = pressed == Some(true);
    let class = format!(
        "mk-btn mk-icon-btn{}{}",
        if on { " mk-btn-on" } else { "" },
        if class.is_empty() {
            String::new()
        } else {
            format!(" {class}")
        }
    );
    rsx! {
        button {
            class: "{class}",
            r#type: "button",
            title: "{tip}",
            "data-command": command,
            "data-shortcut": shortcut,
            disabled,
            aria_pressed: pressed.map(|p| p.to_string()),
            onclick: move |e| onclick.call(e),
            Icon { name: icon }
            span { class: "mk-sr-only", "{label}" }
        }
    }
}

/// What the shell tells a panel about its extension's settings
/// (2026-10-14): whether there are any, the gear's tooltip, and the
/// switch for the settings popover the shell draws over the panel.
#[derive(Clone, PartialEq)]
pub struct PanelSettings {
    /// The extension has settings to show.
    pub available: bool,
    /// The popover is open.
    pub open: Signal<bool>,
    /// The gear's tooltip ("Settings for Code editor").
    pub label: String,
}

/// The gear in a panel's toolbar: opens its extension's settings next to
/// the panel. Draws nothing when the extension has none (or outside a
/// shell panel).
#[component]
pub fn PanelSettingsButton() -> Element {
    let Some(ctx) = try_use_context::<PanelSettings>() else {
        return rsx! {};
    };
    if !ctx.available {
        return rsx! {};
    }
    let mut open = ctx.open;
    rsx! {
        IconButton {
            icon: "settings",
            label: ctx.label.clone(),
            pressed: Some(open()),
            class: "mk-panel-settings-btn".to_string(),
            onclick: move |_| open.toggle(),
        }
    }
}

/// Owner of the current panel's chrome, supplied by the shell.
#[derive(Clone)]
pub struct ChromeOwner(pub crate::Manifest);

/// Whether the user has chosen to display this extension's chrome.
/// Core chrome always appears; Detailed explicitly offers the full surface.
pub fn chrome_visible(ws: crate::Workspace, manifest: &crate::Manifest) -> bool {
    !manifest.optional
        || ws.settings.resolved.read().density == crate::settings::Density::Detailed
        || chrome_accepted(ws, manifest.id)
}

/// Moonkale's graph and terminal are today's UI, not added chrome (ADR-0027
/// trims the rail to seven, it doesn't empty it; Settings, Extensions and More
/// take three places). They start accepted; a decline still hides them.
const DEFAULT_CHROME: &[&str] = &["dev.moonkale.editor-graph", "dev.moonkale.editor-terminal"];

/// The user's chrome choice for an extension, or its default.
pub fn chrome_accepted(ws: crate::Workspace, id: &str) -> bool {
    ws.settings
        .user
        .read()
        .chrome
        .get(id)
        .copied()
        .unwrap_or(DEFAULT_CHROME.contains(&id))
}

/// Whether the user still has to be offered this extension's chrome.
pub fn chrome_undecided(ws: crate::Workspace, id: &str) -> bool {
    !DEFAULT_CHROME.contains(&id) && !ws.settings.user.read().chrome.contains_key(id)
}

/// Shared toolbar actions: one primary action and a flat overflow in Simple.
/// Detailed lays out all actions. Unaccepted extension actions stay in the
/// overflow, so declining chrome never removes a capability.
#[component]
pub fn ToolbarActions(
    #[props(default = VNode::empty())] primary: Element,
    children: Element,
) -> Element {
    let ws = use_context::<crate::Workspace>();
    let owner = try_use_context::<ChromeOwner>();
    let detailed = ws.settings.resolved.read().density == crate::settings::Density::Detailed;
    let accepted = owner.as_ref().is_none_or(|o| chrome_visible(ws, &o.0));
    rsx! {
        crate::Stylesheet { href: asset!("/assets/chrome.css") }
        if detailed {
            {primary}
            {children}
        } else {
            if accepted { {primary.clone()} }
            ToolbarMenu {
                if !accepted { {primary} }
                {children}
            }
        }
    }
}

/// Flat, keyboard/touch accessible overflow. Escape returns focus to its trigger.
#[component]
pub fn ToolbarMenu(children: Element) -> Element {
    let ws = use_context::<crate::Workspace>();
    let mut open = use_signal(|| false);
    let mut trigger = use_signal(|| None::<std::rc::Rc<MountedData>>);
    let label = crate::t!(ws, crate::L, "chrome-more");
    let id = use_hook(|| format!("mk-more-{}", uuid::Uuid::new_v4()));
    let listen_id = id.clone();
    use_hook(move || {
        let mut events = document::eval(&format!(
            r#"
            const id = {};
            const close = e => {{
                const menu = document.getElementById(id);
                if (!menu) {{ document.removeEventListener('click', close); return; }}
                if (menu.open && (!menu.contains(e.target) || e.target.closest('button'))) dioxus.send(true);
            }};
            document.addEventListener('click', close);
        "#,
            serde_json::to_string(&listen_id).unwrap_or_default()
        ));
        spawn(async move {
            while events.recv::<bool>().await.is_ok() {
                open.set(false);
            }
        });
    });
    rsx! {
        crate::Stylesheet { href: asset!("/assets/chrome.css") }
        details { id: "{id}", class: "mk-toolbar-more mk-native-more", open: open(),
            onkeydown: move |event: KeyboardEvent| {
                if event.key() == Key::Escape {
                    event.prevent_default(); event.stop_propagation(); open.set(false);
                    if let Some(target) = trigger.peek().clone() {
                        spawn(async move { let _ = target.set_focus(true).await; });
                    }
                }
            },
            summary { class: "mk-btn mk-icon-btn", title: "{label}", aria_expanded: open().to_string(),
                onmounted: move |event| trigger.set(Some(event.data())),
                onclick: move |event| { event.prevent_default(); open.toggle(); },
                Icon { name: "more" }
                span { class: "mk-sr-only", "{label}" }
            }
            div { class: "mk-toolbar-more-items mk-native-more-items", {children} }
        }
    }
}
