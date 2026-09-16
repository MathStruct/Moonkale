use dioxus::prelude::*;
use ui::EditorWorkbench;

#[component]
pub fn Home() -> Element {
    rsx! {
        div { id: "home",
            EditorWorkbench {}
        }
    }
}
