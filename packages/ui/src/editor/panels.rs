//! Placeholder contents for each workbench panel.

use dioxus::prelude::*;

const MAIN_RS: &str = r#"use dioxus::prelude::*;

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    let mut count = use_signal(|| 0);

    rsx! {
        h1 { "Count: {count}" }
        button { onclick: move |_| *count.write() += 1, "+" }
    }
}
"#;

const LIB_RS: &str = r#"//! Shared library code.

pub fn greet(name: &str) -> String {
    format!("Hello, {name}!")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greets() {
        assert_eq!(greet("Moonkale"), "Hello, Moonkale!");
    }
}
"#;

#[component]
pub fn SourceFile(name: String) -> Element {
    let source = match name.as_str() {
        "main.rs" => MAIN_RS,
        _ => LIB_RS,
    };

    rsx! {
        div { class: "editor-source",
            for (index, line) in source.lines().enumerate() {
                div { class: "editor-line",
                    span { class: "editor-gutter", "{index + 1}" }
                    span { class: "editor-code", "{line}" }
                }
            }
        }
    }
}

#[component]
pub fn Explorer() -> Element {
    rsx! {
        ul { class: "editor-tree",
            li { class: "editor-tree-dir", "moonkale/"
                ul {
                    li { class: "editor-tree-dir", "src/"
                        ul {
                            li { class: "editor-tree-file editor-tree-active", "main.rs" }
                            li { class: "editor-tree-file", "lib.rs" }
                        }
                    }
                    li { class: "editor-tree-file", "Cargo.toml" }
                    li { class: "editor-tree-file", "README.md" }
                }
            }
        }
    }
}

#[component]
pub fn Search() -> Element {
    let mut query = use_signal(String::new);

    rsx! {
        div { class: "editor-search",
            input {
                class: "editor-search-input",
                placeholder: "Search",
                value: "{query}",
                oninput: move |e| query.set(e.value()),
            }
            if query().is_empty() {
                p { class: "editor-muted", "Type to search across files." }
            } else {
                p { class: "editor-muted", "No results for \"{query}\" (search is a stub)." }
            }
        }
    }
}

#[component]
pub fn Outline() -> Element {
    rsx! {
        ul { class: "editor-tree",
            li { class: "editor-tree-file", "fn main" }
            li { class: "editor-tree-file", "fn App" }
        }
    }
}

#[component]
pub fn Terminal() -> Element {
    rsx! {
        pre { class: "editor-terminal",
            "$ cargo build\n"
            "   Compiling moonkale v0.1.0\n"
            "    Finished `dev` profile in 1.42s\n"
            "$ "
            span { class: "editor-cursor" }
        }
    }
}

#[component]
pub fn Problems() -> Element {
    rsx! {
        ul { class: "editor-problems",
            li {
                span { class: "editor-problem-warn", "warning" }
                " unused variable: `count` "
                span { class: "editor-muted", "main.rs:10" }
            }
            li {
                span { class: "editor-problem-info", "info" }
                " consider running `cargo fmt` "
                span { class: "editor-muted", "lib.rs:1" }
            }
        }
    }
}
