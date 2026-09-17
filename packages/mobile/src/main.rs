//! Mobile entrypoint. Reads folders in-process (the app sandbox until the
//! Storage Access Framework lands — see markdown/packaging/Android.md); no
//! native dialog yet.

use dioxus::prelude::*;
use moonkale_core::{Source, SourceError};
use std::sync::Arc;
use ui::{Frame, OpenFolderFuture, Shell, ShellConfig, WorkspaceConfig};

const MAIN_CSS: Asset = asset!("/assets/main.css");

fn main() {
    dioxus::launch(App);
}

fn open_local(path: String) -> OpenFolderFuture {
    Box::pin(async move {
        let path = if path.trim().is_empty() {
            ".".to_string()
        } else {
            path
        };
        moonkale_project_fs::FolderSource::open(&path)
            .map(|s| Arc::new(s) as Arc<dyn Source>)
            .map_err(SourceError::from)
    })
}

#[component]
fn App() -> Element {
    rsx! {
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        Frame {
            config: ShellConfig {
                extensions: ui::default_extensions,
                workspace: WorkspaceConfig { open_folder: open_local, pick_folder: None },
            },
            Shell {}
        }
    }
}
