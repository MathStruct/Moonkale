use dioxus::prelude::*;
use moonkale_core::Source;
use std::sync::Arc;
use ui::{OpenFolderFuture, Shell, ShellConfig};

/// On the web the folder lives on the server: open it through the `api`
/// server functions and talk to it via `RemoteSource`.
fn open_remote(path: String) -> OpenFolderFuture {
    Box::pin(async move {
        api::RemoteSource::open_folder(&path)
            .await
            .map(|s| Arc::new(s) as Arc<dyn Source>)
    })
}

#[component]
pub fn Home() -> Element {
    rsx! {
        div { id: "home",
            Shell { config: ShellConfig { extensions: ui::default_extensions, open_folder: open_remote } }
        }
    }
}
