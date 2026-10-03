//! Inline SVG icons for the activity bar, the phone bar and the Explorer's
//! source rows (spec 009). Names are what `Activity::icon` and
//! `source_icon` hand out; an unknown name draws a dot. 24-unit viewbox,
//! stroked with `currentColor` so themes colour them.

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
        _ => "M12 12m-2 0a2 2 0 1 0 4 0a2 2 0 1 0-4 0",
    };
    rsx! {
        svg { class: "mk-icon", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "1.8", stroke_linecap: "round", stroke_linejoin: "round", "aria-hidden": "true",
            path { d: "{path}" }
        }
    }
}

/// The icon and a human label for a source (spec 009): by family, refined
/// by the `Custom` name the drivers use (`"git"`, `"data"`, …) and by the id
/// prefix (`ssh:`, `repo:`, `api:`).
pub fn source_icon(d: &moonkale_core::SourceDescriptor) -> (&'static str, &'static str) {
    use moonkale_core::SourceFamily as F;
    let id = d.id.as_str();
    match &d.family {
        F::Folder if id.starts_with("ssh:") || id.starts_with("remote:") => {
            ("folder-remote", "remote folder")
        }
        F::Folder => ("folder", "folder"),
        F::Sql if d.display_name.contains("data files") || id.starts_with("data:") => {
            ("data", "data folder")
        }
        F::Sql => ("database", "database"),
        F::Graph => ("graphdb", "graph database"),
        F::KeyValue => ("kv", "key-value store"),
        F::Index => ("index", "index"),
        F::Remote => ("folder-remote", "remote"),
        F::Custom(name) if name == "git" || name == "repo" => ("repo", "repository"),
        F::Custom(name) if name == "api" => ("api", "API"),
        F::Custom(_) => ("puzzle", "source"),
    }
}

/// A stable accent colour for a source: one of eight palette entries chosen
/// by a hash of the id, so it is the same on every machine (spec 009; the
/// user-chosen colour of a project overrides it later).
pub fn source_color(id: &str) -> &'static str {
    const PALETTE: [&str; 8] = [
        "#4fb3e8", "#e0a84a", "#8fd18f", "#e07a7a", "#b48fe0", "#5fd1c1", "#e08fc7", "#c9c95a",
    ];
    let mut h: u32 = 2166136261;
    for b in id.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(16777619);
    }
    PALETTE[(h % 8) as usize]
}
