use dioxus::prelude::*;
use moonkale_core::{Source, SourceError};
use std::sync::Arc;
use ui::{OpenFolderFuture, Shell, ShellConfig};

/// Native build: open the folder in-process with `FolderSource`.
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
pub fn Home() -> Element {
    rsx! {
        div { id: "home",
            Shell { config: ShellConfig { extensions: ui::default_extensions, open_folder: open_local } }
        }
    }
}
