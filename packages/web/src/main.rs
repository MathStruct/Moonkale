//! Web entrypoint. The folder lives on the server: `RemoteSource` over the
//! `api` server functions. No native window, so no `WindowControls`.

use dioxus::prelude::*;
use moonkale_core::Source;
use std::sync::Arc;
use ui::{Frame, OpenFolderFuture, Shell, ShellConfig, WorkspaceConfig};

const FAVICON: Asset = asset!("/assets/favicon.ico");
const MAIN_CSS: Asset = asset!("/assets/main.css");

fn main() {
    dioxus::launch(App);
}

fn open_remote(path: String) -> OpenFolderFuture {
    Box::pin(async move {
        api::RemoteSource::open_folder(&path)
            .await
            .map(|s| Arc::new(s) as Arc<dyn Source>)
    })
}

#[component]
fn App() -> Element {
    rsx! {
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        Frame {
            config: ShellConfig {
                extensions: ui::default_extensions,
                workspace: WorkspaceConfig { open_folder: open_remote, pick_folder: None },
            },
            Shell {}
        }
    }
}
