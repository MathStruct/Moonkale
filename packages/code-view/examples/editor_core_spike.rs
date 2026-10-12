//! Run the editor-core prototype as a standalone Dioxus app.

use moonkale_code_view::editor_core_spike::EditorCoreSpike;

fn main() {
    dioxus::launch(EditorCoreSpike);
}
